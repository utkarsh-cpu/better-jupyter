//! Blocking listener threads wake the coordinator through a bounded channel.
//! No timer polling on the connection acceptance path.
use super::{CLIENT_LIMIT, Shared};
use anyhow::Result;
use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Arc, atomic::Ordering, mpsc},
    thread,
    time::Duration,
};

pub(super) struct Acceptors {
    pub receive: mpsc::Receiver<(TcpStream, bool)>,
    shared: Arc<Shared>,
    addresses: Vec<SocketAddr>,
    threads: Vec<thread::JoinHandle<()>>,
}
impl Acceptors {
    pub fn start(listeners: &[(&TcpListener, bool)], shared: Arc<Shared>) -> Result<Self> {
        let (send, receive) = mpsc::sync_channel(CLIENT_LIMIT);
        let mut owner = Self {
            receive,
            shared,
            addresses: Vec::new(),
            threads: Vec::new(),
        };
        for (listener, desktop) in listeners {
            let listener = listener.try_clone()?;
            listener.set_nonblocking(false)?;
            owner.addresses.push(listener.local_addr()?);
            let shared = owner.shared.clone();
            let send = send.clone();
            let desktop = *desktop;
            owner
                .threads
                .push(
                    thread::Builder::new()
                        .name("kl-accept".into())
                        .spawn(move || {
                            while !shared.stopping.load(Ordering::Acquire) {
                                match listener.accept() {
                                    Ok((socket, _)) => {
                                        if shared.stopping.load(Ordering::Acquire) {
                                            break;
                                        }
                                        // An overflowing connection has not authenticated or
                                        // admitted effects. Drop it instead of blocking stop.
                                        let _ = send.try_send((socket, desktop));
                                    }
                                    Err(error)
                                        if error.kind() == std::io::ErrorKind::Interrupted =>
                                    {
                                        continue;
                                    }
                                    Err(error) => {
                                        *shared.stop_error.lock().unwrap() =
                                            Some(format!("listener failed: {error}"));
                                        shared.stopping.store(true, Ordering::Release);
                                        break;
                                    }
                                }
                            }
                        })?,
                );
        }
        Ok(owner)
    }
}
impl Drop for Acceptors {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Release);
        // Wake accept even when no external client is connecting. The stop bit
        // is checked before forwarding this socket, so it cannot admit work.
        for address in &self.addresses {
            let _ = TcpStream::connect_timeout(address, Duration::from_millis(100));
        }
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        while self.receive.try_recv().is_ok() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocking_accept_wakes_and_idle_shutdown_joins_both_listeners() {
        let legacy = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let desktop = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let shared = Arc::new(Shared::default());
        let acceptors =
            Acceptors::start(&[(&legacy, false), (&desktop, true)], shared.clone()).unwrap();
        let _client = TcpStream::connect(desktop.local_addr().unwrap()).unwrap();
        assert!(
            acceptors
                .receive
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .1
        );
        let (done, finished) = mpsc::sync_channel(1);
        thread::spawn(move || {
            drop(acceptors);
            done.send(()).unwrap();
        });
        finished
            .recv_timeout(Duration::from_secs(2))
            .expect("idle acceptors did not wake on shutdown");
        assert!(shared.stopping.load(Ordering::Acquire));
    }
}
