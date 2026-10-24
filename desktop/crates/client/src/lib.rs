//! Blocking transports used only by background executors, never by GPUI render code.
use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::Duration,
};
use uuid::Uuid;
pub mod assistant;
pub mod output;
pub mod preferences;
pub mod registry;
pub mod service;

pub use kernelledger_protocol::{API, FRAME_LIMIT, ProtocolError, hash, read_frame, write_frame};
pub fn checked_response(message: Value) -> Result<Value> {
    if message["kind"] == "error" {
        return Err(serde_json::from_value::<ProtocolError>(message["error"].clone())?.into());
    }
    Ok(message)
}

#[derive(Clone)]
pub struct ClientIdentity {
    pub id: Uuid,
    pub name: String,
}
impl ClientIdentity {
    pub fn load(profile: &Path) -> Result<Self> {
        fs::create_dir_all(profile)?;
        let path = profile.join("client-id");
        let id = if path.exists() {
            Uuid::parse_str(fs::read_to_string(path)?.trim())?
        } else {
            let id = Uuid::new_v4();
            fs::write(path, id.to_string())?;
            id
        };
        Ok(Self {
            id,
            name: "KernelLedger Desktop".into(),
        })
    }
    pub fn hello(&self, role: &str, token: Option<&str>) -> Value {
        let mut hello = json!({"kind":"hello","api":API,"versions":[1],"client_id":self.id,"client_name":self.name,"role":role});
        if let Some(token) = token {
            hello["token"] = json!(token);
        }
        hello
    }
}

pub struct RuntimeClient {
    stream: TcpStream,
    pub welcome: Value,
}
impl RuntimeClient {
    pub fn connect(project: &Path, identity: &ClientIdentity, role: &str) -> Result<Self> {
        let info: Value = serde_json::from_slice(&fs::read(project.join(".kl/supervisor.json"))?)?;
        let port = info["desktop_port"].as_u64().ok_or_else(|| {
            anyhow::anyhow!("Runtime has no desktop protocol; upgrade KernelLedger")
        })?;
        let address: SocketAddr = format!("127.0.0.1:{port}").parse()?;
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let token = fs::read_to_string(project.join(".kl/token"))?;
        write_frame(&mut stream, &identity.hello(role, Some(&token)))?;
        let welcome = checked_response(read_frame(&mut stream)?)?;
        ensure!(
            welcome["kind"] == "welcome" && welcome["version"] == 1,
            "Unsupported desktop protocol"
        );
        // No execution-level timeout; the caller reconciles a disconnected request by its key.
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        Ok(Self { stream, welcome })
    }
    pub fn request(&mut self, method: &str, mut fields: Value) -> Result<Value> {
        fields["method"] = json!(method);
        fields["request_id"] = json!(Uuid::new_v4());
        write_frame(&mut self.stream, &fields)?;
        loop {
            let response = self.next_message()?;
            if response["kind"] == "pong" {
                continue;
            }
            return Ok(checked_response(response)?["result"].clone());
        }
    }
    pub fn next_message(&mut self) -> Result<Value> {
        loop {
            let message = read_frame(&mut self.stream)?;
            if message["kind"] == "ping" {
                write_frame(&mut self.stream, &json!({"kind":"pong"}))?;
                continue;
            }
            return checked_response(message);
        }
    }
    pub fn subscribe(&mut self, after: Option<u64>) -> Result<()> {
        let mut request = json!({"method":"subscribe"});
        if let Some(after) = after {
            request["after_change_seq"] = json!(after);
        }
        write_frame(&mut self.stream, &request)
    }
    pub fn follow(&mut self, exec_id: &str, after: u64) -> Result<()> {
        write_frame(
            &mut self.stream,
            &json!({"method":"execution.follow","exec_id":exec_id,"after_seq":after}),
        )
    }
    pub fn blob(&mut self, sha256: &str) -> Result<Vec<u8>> {
        write_frame(
            &mut self.stream,
            &json!({"method":"blob.read","sha256":sha256}),
        )?;
        let mut bytes = Vec::new();
        loop {
            let header = self.next_message()?;
            ensure!(header["kind"] == "blob", "Expected binary blob frame");
            let size = header["byte_size"].as_u64().unwrap_or(0) as usize;
            ensure!(
                size <= FRAME_LIMIT
                    && bytes.len() as u64 == header["offset"].as_u64().unwrap_or(u64::MAX),
                "Invalid blob chunk"
            );
            let start = bytes.len();
            bytes.resize(start + size, 0);
            self.stream.read_exact(&mut bytes[start..])?;
            if header["final"] == true {
                ensure!(
                    hash(&bytes) == sha256 && header["total_size"] == bytes.len(),
                    "Blob integrity check failed"
                );
                return Ok(bytes);
            }
        }
    }
}

pub struct HelperClient {
    process: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    pub welcome: Value,
}
impl HelperClient {
    pub fn start(python: &Path, project: &Path, identity: &ClientIdentity) -> Result<Self> {
        let mut command = Command::new(python);
        command
            .args(["-I", "-m", "kernelledger.desktop.helper", "--project"])
            .arg(project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        fs::create_dir_all(project.join(".kl"))?;
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(project.join(".kl/helper.log"))?;
        command.stderr(log);
        hide_console(&mut command);
        let mut process = command.spawn()?;
        let mut stdin = process
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Helper stdin unavailable"))?;
        let mut stdout = process
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Helper stdout unavailable"))?;
        write_frame(&mut stdin, &identity.hello("control", None))?;
        let welcome = checked_response(read_frame(&mut stdout)?)?;
        ensure!(
            welcome["kind"] == "welcome" && welcome["version"] == 1,
            "Unsupported helper protocol"
        );
        Ok(Self {
            process,
            stdin,
            stdout,
            welcome,
        })
    }
    pub fn request(&mut self, method: &str, mut fields: Value) -> Result<Value> {
        fields["method"] = json!(method);
        fields["request_id"] = json!(Uuid::new_v4());
        write_frame(&mut self.stdin, &fields)?;
        let response = checked_response(read_frame(&mut self.stdout)?)?;
        Ok(response["result"].clone())
    }
    pub fn blob(&mut self, digest: &str) -> Result<Vec<u8>> {
        let mut result = Vec::new();
        loop {
            let page = self.blob_range(digest, result.len() as u64, FRAME_LIMIT)?;
            if page.bytes.is_empty() && result.len() as u64 >= page.total {
                break;
            }
            ensure!(!page.bytes.is_empty(), "Empty blob chunk");
            result.extend(page.bytes);
            if result.len() as u64 >= page.total {
                break;
            }
        }
        ensure!(hash(&result) == digest, "Blob integrity check failed");
        Ok(result)
    }
    pub fn blob_range(
        &mut self,
        digest: &str,
        offset: u64,
        length: usize,
    ) -> Result<output::BlobPage> {
        let length = length.clamp(1, FRAME_LIMIT);
        write_frame(
            &mut self.stdin,
            &json!({"method":"blob.read","sha256":digest,"offset":offset,"length":length}),
        )?;
        let header = checked_response(read_frame(&mut self.stdout)?)?;
        ensure!(header["kind"] == "blob", "Expected blob frame");
        let size = header["byte_size"].as_u64().unwrap_or(u64::MAX) as usize;
        ensure!(size <= FRAME_LIMIT, "Invalid blob size");
        ensure!(
            header["offset"].as_u64().unwrap_or(u64::MAX) == offset,
            "Blob offset mismatch"
        );
        let mut bytes = vec![0; size];
        if size > 0 {
            self.stdout.read_exact(&mut bytes)?;
        }
        let total = header["total_size"].as_u64().unwrap_or(u64::MAX);
        if header["verified"] == true {
            ensure!(
                hash(&bytes) == digest && total == bytes.len() as u64,
                "Blob integrity check failed"
            );
        }
        Ok(output::BlobPage {
            bytes,
            offset,
            total,
        })
    }
}
impl Drop for HelperClient {
    fn drop(&mut self) {
        // The helper owns no computation; dropping it never stops a supervisor.
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
pub fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW.
        // The group flag stays so job-object shutdown still owns the tree.
        command.creation_flags(0x00000008 | 0x00000200 | 0x08000000);
    }
    #[cfg(not(windows))]
    let _ = command;
}
pub fn profile_path() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
            .join("KernelLedger/Desktop")
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
            })
            .join("kernelledger/desktop")
    }
}

pub fn all_pages(mut call: impl FnMut(Value) -> Result<Value>) -> Result<Vec<Value>> {
    let mut items = Vec::new();
    let mut cursor = Value::Null;
    loop {
        let page = call(json!({"page_size":500,"cursor":cursor}))?;
        items.extend(
            page["items"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Invalid list response"))?
                .iter()
                .cloned(),
        );
        cursor = page["cursor"].clone();
        if cursor.is_null() {
            return Ok(items);
        }
    }
}

pub fn start_runtime(python: &Path, project: &Path, worker_python: Option<&Path>) -> Result<()> {
    let mut command = Command::new(python);
    command
        .args(["-I", "-m", "kernelledger", "-C"])
        .arg(project)
        .args(["serve", "--launch", "desktop"]);
    if std::env::var_os("KL_PRIVATE_RUNTIME").is_some() {
        command.args(["--distribution", "private"]);
    }
    if let Some(worker) = worker_python {
        command.arg("--python").arg(worker);
    }
    fs::create_dir_all(project.join(".kl"))?;
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(project.join(".kl/supervisor.log"))?;
    command
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    hide_console(&mut command);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    for _ in 0..150 {
        if project.join(".kl/supervisor.json").exists() {
            return Ok(());
        }
        if child.try_wait()?.is_some() {
            bail!("Runtime startup failed. See .kl/supervisor.log");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    bail!("Runtime startup still pending. Reconnect to inspect its outcome")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn rejects_length_before_allocating() {
        assert!(read_frame(Cursor::new(((FRAME_LIMIT + 1) as u32).to_be_bytes())).is_err());
    }
    #[test]
    fn shared_fixture_frames() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/desktop/contract.json"
        ))
        .unwrap();
        for message in fixture["frames"].as_array().unwrap() {
            let mut bytes = Vec::new();
            write_frame(&mut bytes, message).unwrap();
            assert_eq!(&read_frame(Cursor::new(bytes)).unwrap(), message);
        }
    }
    #[test]
    fn structured_errors_survive_transport() {
        let error = checked_response(json!({"kind":"error","error":{"code":"source_conflict","message":"changed","retryable":false,"details":{}}})).unwrap_err();
        assert_eq!(
            error.downcast_ref::<ProtocolError>().unwrap().code,
            "source_conflict"
        );
    }
}
