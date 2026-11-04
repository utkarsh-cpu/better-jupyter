//! Bounded incremental pipe decoding and one output budget for pipe/protocol text.
//! Attribution is taken when a decoded chunk is recorded. Output outside an
//! active execution is retained as an explicitly unattributed session event.
use crate::writer::{Mutation, Writer};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    io::Read,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const BUFFER: usize = 16 * 1024;
const STARTUP_LIMIT: usize = 64 * 1024;
pub enum Readiness {
    Pending,
    Data(usize),
    Eof,
}
pub trait Pipe: Read + Send + 'static {
    fn available(&self) -> std::io::Result<Readiness>;
}
#[cfg(windows)]
impl<T: Read + Send + 'static + std::os::windows::io::AsRawHandle> Pipe for T {
    fn available(&self) -> std::io::Result<Readiness> {
        let mut count = 0;
        let ok = unsafe {
            windows_sys::Win32::System::Pipes::PeekNamedPipe(
                self.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut count,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(109) {
                return Ok(Readiness::Eof);
            }
            return Err(error);
        }
        Ok(if count == 0 {
            Readiness::Pending
        } else {
            Readiness::Data(count as usize)
        })
    }
}
#[cfg(unix)]
impl<T: Read + Send + 'static + std::os::fd::AsRawFd> Pipe for T {
    fn available(&self) -> std::io::Result<Readiness> {
        let mut fd = libc::pollfd {
            fd: self.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut fd, 1, 0) };
        if result < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(if result == 0 {
            Readiness::Pending
        } else if fd.revents & libc::POLLHUP != 0 && fd.revents & libc::POLLIN == 0 {
            Readiness::Eof
        } else {
            Readiness::Data(BUFFER)
        })
    }
}

#[derive(Default)]
struct Decoder {
    pending: Vec<u8>,
}
impl Decoder {
    fn decode(&mut self, bytes: &[u8], eof: bool) -> String {
        self.pending.extend_from_slice(bytes);
        let mut text = String::new();
        let mut offset = 0;
        while offset < self.pending.len() {
            match std::str::from_utf8(&self.pending[offset..]) {
                Ok(valid) => {
                    text.push_str(valid);
                    offset = self.pending.len();
                }
                Err(error) => {
                    let end = offset + error.valid_up_to();
                    text.push_str(std::str::from_utf8(&self.pending[offset..end]).unwrap());
                    offset = end;
                    if let Some(count) = error.error_len() {
                        text.push('\u{fffd}');
                        offset += count;
                    } else {
                        break;
                    }
                }
            }
        }
        self.pending.drain(..offset);
        if eof && !self.pending.is_empty() {
            text.push('\u{fffd}');
            self.pending.clear();
        }
        text
    }
}
#[derive(Default)]
struct Budget {
    recorded: usize,
    dropped: usize,
}
impl Budget {
    fn take<'a>(&mut self, text: &'a str, limit: usize) -> &'a str {
        let mut count = text.len().min(limit.saturating_sub(self.recorded));
        while !text.is_char_boundary(count) {
            count -= 1;
        }
        self.recorded = self.recorded.saturating_add(count);
        self.dropped = self.dropped.saturating_add(text.len() - count);
        &text[..count]
    }
}
struct State {
    ledger: Writer,
    session: Option<String>,
    active: Option<String>,
    limit: usize,
    execution: Budget,
    idle: Budget,
    startup: Vec<(String, String)>,
    startup_bytes: usize,
    startup_dropped: usize,
    error: Option<String>,
}
impl State {
    fn record(&mut self, kind: &str, text: &str, pipe: bool, unattributed: bool) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let Some(session) = &self.session else {
            let mut count = text
                .len()
                .min(STARTUP_LIMIT.saturating_sub(self.startup_bytes));
            while !text.is_char_boundary(count) {
                count -= 1;
            }
            self.startup_dropped = self.startup_dropped.saturating_add(text.len() - count);
            if count > 0 {
                self.startup.push((kind.into(), text[..count].into()));
                self.startup_bytes += count;
            }
            return Ok(());
        };
        let active = self.active.as_ref().filter(|_| !unattributed);
        let mut payload = json!({"text":if active.is_some() {self.execution.take(text,self.limit)} else {self.idle.take(text,self.limit)}});
        if payload["text"] == "" {
            return Ok(());
        }
        if pipe {
            payload["capture"] = "pipe".into();
        }
        if let Some(execution) = active {
            self.ledger.call(Mutation::Event {
                execution: execution.clone(),
                kind: kind.into(),
                payload,
            })?;
        } else {
            payload["cell_attribution"] = "unattributed".into();
            self.ledger.call(Mutation::SessionEvent {
                session: session.clone(),
                kind: kind.into(),
                payload,
            })?;
        }
        Ok(())
    }
    fn fail(&mut self, error: impl std::fmt::Display) {
        if self.error.is_none() {
            self.error = Some(error.to_string());
        }
    }
}

pub struct Capture {
    state: Arc<Mutex<State>>,
    readers: Vec<thread::JoinHandle<()>>,
    closing: Arc<AtomicBool>,
}
impl Capture {
    pub fn start(
        stdout: impl Pipe,
        stderr: impl Pipe,
        ledger: Writer,
        limit: usize,
    ) -> Result<Self> {
        let state = Arc::new(Mutex::new(State {
            ledger,
            session: None,
            active: None,
            limit,
            execution: Budget::default(),
            idle: Budget::default(),
            startup: Vec::new(),
            startup_bytes: 0,
            startup_dropped: 0,
            error: None,
        }));
        let mut capture = Self {
            state,
            readers: Vec::new(),
            closing: Arc::new(AtomicBool::new(false)),
        };
        capture.spawn(stdout, "stdout")?;
        capture.spawn(stderr, "stderr")?;
        Ok(capture)
    }
    fn spawn(&mut self, mut pipe: impl Pipe, kind: &'static str) -> Result<()> {
        let state = self.state.clone();
        let closing = self.closing.clone();
        self.readers.push(
            thread::Builder::new()
                .name(format!("kl-{kind}-capture"))
                .spawn(move || {
                    let mut decoder = Decoder::default();
                    let mut buffer = [0u8; BUFFER];
                    let mut deadline = None;
                    loop {
                        let finishing = closing.load(Ordering::Acquire);
                        if finishing {
                            deadline.get_or_insert_with(|| Instant::now() + Duration::from_secs(2));
                        }
                        let available = match pipe.available() {
                            Ok(value) => value,
                            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                            Err(e) => {
                                state.lock().unwrap().fail(e);
                                break;
                            }
                        };
                        let count = match available {
                            Readiness::Pending if !finishing => {
                                thread::sleep(Duration::from_millis(2));
                                continue;
                            }
                            Readiness::Pending | Readiness::Eof => 0,
                            Readiness::Data(count) => count.min(BUFFER),
                        };
                        let forced = deadline.is_some_and(|deadline| Instant::now() >= deadline);
                        let (text, eof) = match if count == 0 || forced {
                            Ok(0)
                        } else {
                            pipe.read(&mut buffer[..count])
                        } {
                            Ok(count) => (decoder.decode(&buffer[..count], count == 0), count == 0),
                            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                            Err(e) => {
                                state.lock().unwrap().fail(e);
                                break;
                            }
                        };
                        let mut guard = state.lock().unwrap();
                        if guard.error.is_none()
                            && let Err(error) = guard.record(kind, &text, true, false)
                        {
                            guard.fail(error);
                        }
                        if eof {
                            break;
                        }
                    }
                })?,
        );
        Ok(())
    }
    pub fn session(&self, session: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.session = Some(session.into());
        let startup = std::mem::take(&mut state.startup);
        state.startup_bytes = 0;
        state.idle.dropped = state.startup_dropped;
        for (kind, text) in startup {
            state.record(&kind, &text, true, false)?;
        }
        drop(state);
        self.check()
    }
    pub fn begin(&self, execution: &str) -> Result<()> {
        self.check()?;
        let mut state = self.state.lock().unwrap();
        ensure!(
            state.active.is_none(),
            "previous output attribution is still active"
        );
        state.active = Some(execution.into());
        state.execution = Budget::default();
        Ok(())
    }
    pub fn protocol(&self, kind: &str, payload: &Value) -> Result<()> {
        self.check()?;
        let text = payload["text"]
            .as_str()
            .context("stream text is required")?;
        self.state.lock().unwrap().record(kind, text, false, false)
    }
    pub fn unattributed(&self, kind: &str, payload: &Value) -> Result<()> {
        self.check()?;
        let text = payload["text"]
            .as_str()
            .context("stream text is required")?;
        self.state.lock().unwrap().record(kind, text, false, true)
    }
    pub fn finish(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if let Some(execution) = state.active.take()
            && state.execution.dropped > 0
        {
            state.ledger.call(Mutation::Event {
                execution,
                kind: "truncated".into(),
                payload: json!({"dropped_bytes":state.execution.dropped}),
            })?;
        }
        drop(state);
        self.check()
    }
    pub fn check(&self) -> Result<()> {
        let state = self.state.lock().unwrap();
        ensure!(
            state.error.is_none(),
            "worker output capture failed: {}",
            state.error.as_deref().unwrap_or("")
        );
        Ok(())
    }
    /// Terminate the process tree first. Bounded draining also handles inherited
    /// writer handles that remain open after termination.
    pub fn close(&mut self) -> Result<()> {
        self.closing.store(true, Ordering::Release);
        for reader in self.readers.drain(..) {
            reader
                .join()
                .map_err(|_| anyhow::anyhow!("pipe capture panicked"))?;
        }
        self.finish()?;
        let mut state = self.state.lock().unwrap();
        if let Some(session) = &state.session
            && state.idle.dropped > 0
        {
            state.ledger.call(Mutation::SessionEvent {session:session.clone(),kind:"truncated".into(),payload:json!({"dropped_bytes":state.idle.dropped,"cell_attribution":"unattributed"})})?;
            state.idle.dropped = 0;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Pending;
    impl Read for Pending {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("an unreadable pipe must not block in read")
        }
    }
    impl Pipe for Pending {
        fn available(&self) -> std::io::Result<Readiness> {
            Ok(Readiness::Pending)
        }
    }
    #[test]
    fn closing_does_not_wait_for_an_inherited_pipe_handle_to_reach_eof() {
        let directory = tempfile::tempdir().unwrap();
        let writer = Writer::start(directory.path()).unwrap();
        let mut capture = Capture::start(Pending, Pending, writer.clone(), 32).unwrap();
        let started = Instant::now();
        capture.close().unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(capture);
        drop(writer);
        assert!(Writer::start(directory.path()).is_ok());
    }
    #[test]
    fn incremental_utf8_preserves_split_characters_and_replaces_invalid_bytes() {
        let mut decoder = Decoder::default();
        let mut output = String::new();
        for byte in "🙂é\n".as_bytes() {
            output.push_str(&decoder.decode(&[*byte], false));
        }
        assert_eq!(output, "🙂é\n");
        assert_eq!(decoder.decode(&[0xff, 0xe2], false), "\u{fffd}");
        assert_eq!(decoder.decode(&[], true), "\u{fffd}");
        assert!(decoder.pending.is_empty());
    }
    #[test]
    fn budget_never_splits_unicode_or_forgets_dropped_bytes() {
        let mut budget = Budget::default();
        assert_eq!(budget.take("a🙂", 4), "a");
        assert_eq!(budget.take("bcde", 4), "bcd");
        assert_eq!(budget.recorded, 4);
        assert_eq!(budget.dropped, 5);
    }
}
