//! Headless feasibility probe. No ledger writer or production runtime service.
pub mod platform;
use anyhow::{Context, Result, bail, ensure};
use kernelledger_protocol::{BUFFER_SIZE, copy_blob, hash, read_worker, write_worker};
use platform::{FileLock, WorkerTree, python_command};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process::Stdio,
    thread,
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;
use uuid::Uuid;

pub struct ProbeOptions<'a> {
    pub project: &'a Path,
    pub python: &'a Path,
    pub worker: &'a Path,
    pub mib: usize,
    pub child_pid_file: Option<&'a Path>,
}

struct Transport {
    socket: TcpStream,
    blobs: BTreeMap<String, NamedTempFile>,
    bytes: u64,
}
impl Transport {
    fn message(&mut self) -> Result<Value> {
        loop {
            let value = read_worker(&mut self.socket)?;
            match value["kind"].as_str().unwrap_or("") {
                "blob" => {
                    let mut file = NamedTempFile::new()?;
                    self.bytes += copy_blob(&mut self.socket, &mut file, &value)?;
                    self.blobs.insert(
                        value["sha256"].as_str().context("missing digest")?.into(),
                        file,
                    );
                }
                "envelope" => {
                    let file = self
                        .blobs
                        .get_mut(value["blob"].as_str().context("missing envelope digest")?)
                        .context("missing envelope blob")?;
                    // This is an explicit spike limit, not a production protocol change.
                    ensure!(
                        file.as_file().metadata()?.len() <= 16 * 1024 * 1024,
                        "probe envelope exceeds 16 MiB"
                    );
                    file.seek(SeekFrom::Start(0))?;
                    let value: Value = serde_json::from_reader(file)?;
                    ensure!(value["schema"] == 1, "unsupported envelope schema");
                    return Ok(value);
                }
                _ => return Ok(value),
            }
        }
    }

    fn execute(&mut self, source: &str) -> Result<()> {
        write_worker(
            &mut self.socket,
            &json!({"schema":1,"kind":"execute", "exec_id":Uuid::new_v4().to_string(),"cell_id":null,"source":source,"source_sha256":hash(source.as_bytes())}),
        )?;
        loop {
            let message = self.message()?;
            if message["kind"] == "status" && message["state"] == "idle" {
                ensure!(
                    message["outcome"] == "ok",
                    "probe validation execution failed"
                );
                return Ok(());
            }
        }
    }
}

pub fn probe(options: ProbeOptions<'_>) -> Result<Value> {
    ensure!(
        (1..=1024).contains(&options.mib),
        "probe MiB must be between 1 and 1024"
    );
    ensure!(
        options.worker.is_absolute() && options.worker.is_file(),
        "worker must be an existing absolute file"
    );
    let project = options.project.canonicalize()?;
    let _lock = FileLock::acquire(&project.join(".kl/lock"))?;
    // Source-lock compatibility is also independently tested with lock-probe.
    let _source = FileLock::acquire(&project.join(".kl/source.lock"))?;
    let temporary = tempfile::tempdir()?;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let mut command = python_command(options.python)?;
    command
        .args(["-I", "-X", "utf8"])
        .arg(options.worker)
        .arg(&project)
        .env("KL_WORKER_PORT", listener.local_addr()?.port().to_string())
        .env("KL_WORKER_TOKEN", &token)
        .env("KL_TRAINING_TEMP_ROOT", temporary.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = Instant::now();
    let mut worker = WorkerTree::spawn(&mut command)?;
    // Drain output pipes in fixed buffers. Notebook output uses the protocol.
    let out = worker.child.stdout.take().context("missing stdout")?;
    let err = worker.child.stderr.take().context("missing stderr")?;
    fn drain(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let _ = std::io::copy(&mut pipe, &mut std::io::sink());
        })
    }
    let stdout = drain(out);
    let stderr = drain(err);
    let deadline = started + Duration::from_secs(20);
    let (socket, hello) = loop {
        ensure!(Instant::now() < deadline, "worker handshake timed out");
        if let Some(status) = worker.child.try_wait()? {
            bail!("worker exited before readiness: {status}");
        }
        match listener.accept() {
            Ok((mut socket, _)) => {
                // Winsock accepted sockets inherit the listener's nonblocking mode.
                socket.set_nonblocking(false)?;
                socket.set_read_timeout(Some(Duration::from_secs(2)))?;
                socket.set_write_timeout(Some(Duration::from_secs(30)))?;
                if let Ok(hello) = read_worker(&mut socket) {
                    // Tokens are unpredictable and never written to diagnostics.
                    if hello["kind"] == "hello" && hello["token"].as_str() == Some(&token) {
                        break (socket, hello);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5))
            }
            Err(e) => return Err(e.into()),
        }
    };
    socket.set_read_timeout(Some(Duration::from_secs(60)))?;
    let mut transport = Transport {
        socket,
        blobs: BTreeMap::new(),
        bytes: 0,
    };
    ensure!(
        transport.message()?["kind"] == "ready",
        "worker was not ready"
    );
    let ready_seconds = started.elapsed().as_secs_f64();
    let mut source = format!(
        "payload=b'x'*({}*1024**2)\nprint('native probe ready')",
        options.mib
    );
    if let Some(path) = options.child_pid_file {
        let path_literal = serde_json::to_string(&path.to_string_lossy())?;
        source.push_str(&format!("\nimport subprocess,sys,pathlib\nchild=subprocess.Popen([sys.executable,'-c','import time; time.sleep(120)'])\npathlib.Path({path_literal}).write_text(str(child.pid))\ndel child"));
    }
    write_worker(
        &mut transport.socket,
        &json!({"schema":1,"kind":"execute","exec_id":Uuid::new_v4().to_string(),"cell_id":null,"source":source,"source_sha256":hash(source.as_bytes())}),
    )?;
    let mut output_seen = false;
    loop {
        let message = transport.message()?;
        if message["kind"] == "stdout"
            && message["text"]
                .as_str()
                .is_some_and(|s| s.contains("native probe ready"))
        {
            output_seen = true;
        }
        if message["kind"] == "status" && message["state"] == "idle" {
            ensure!(message["outcome"] == "ok", "probe execution failed");
            break;
        }
    }
    ensure!(output_seen, "worker stdout missing");
    let checkpoint_start = Instant::now();
    write_worker(
        &mut transport.socket,
        &json!({"schema":1,"kind":"checkpoint"}),
    )?;
    let offer = transport.message()?;
    ensure!(
        offer["kind"] == "checkpoint_offer" && offer["manifest"].is_object(),
        "checkpoint probe refused or failed: {}",
        offer["kind"]
    );
    ensure!(
        transport.bytes >= (options.mib * 1024 * 1024) as u64,
        "large blob was not transferred"
    );
    let checkpoint_seconds = checkpoint_start.elapsed().as_secs_f64();
    let worker_pid = worker.child.id();
    let received_bytes = transport.bytes;
    transport.execute("del payload")?;
    // Reuse precisely the bytes verified on receipt for the reverse transfer.
    let restore_start = Instant::now();
    for (digest, file) in &mut transport.blobs {
        let size = file.as_file().metadata()?.len();
        write_worker(
            &mut transport.socket,
            &json!({"schema":1,"kind":"blob","sha256":digest,"byte_size":size}),
        )?;
        file.seek(SeekFrom::Start(0))?;
        let mut buffer = vec![0; BUFFER_SIZE];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            transport.socket.write_all(&buffer[..n])?;
        }
    }
    write_worker(
        &mut transport.socket,
        &json!({"schema":1,"kind":"restore","manifest":offer["manifest"]}),
    )?;
    let restored = transport.message()?;
    ensure!(
        restored["kind"] == "restore_result",
        "missing restore result"
    );
    ensure!(
        transport.message()?["kind"] == "ready",
        "worker not ready after restore"
    );
    transport.execute(&format!("assert payload == b'x'*({}*1024**2)", options.mib))?;
    let restore_seconds = restore_start.elapsed().as_secs_f64();
    worker.terminate()?;
    drop(transport);
    stdout
        .join()
        .map_err(|_| anyhow::anyhow!("stdout drain failed"))?;
    stderr
        .join()
        .map_err(|_| anyhow::anyhow!("stderr drain failed"))?;
    Ok(
        json!({"mode":"feasibility_probe", "production_backend":false, "ledger_writes":false,
        "python":hello["python"], "worker_pid":worker_pid, "mib":options.mib,
        "ready_seconds":ready_seconds, "checkpoint_seconds":checkpoint_seconds,
        "restore_seconds":restore_seconds, "received_bytes":received_bytes,
        "buffer_bytes":BUFFER_SIZE, "process_tree_terminated":true,
        "limitations":["no durability or operation admission", "restore reuses probe worker", "not a production latency comparison"]}),
    )
}

pub fn diagnostics() -> Value {
    json!({"implementation":"rust", "version":env!("CARGO_PKG_VERSION"), "mode":"feasibility_only",
        "supervisor":false, "offline_helper":false, "native_gpu_capture":false,
        "worker_protocol":1, "ledger_schema_written":null})
}

pub fn run(args: &[String]) -> Result<Value> {
    let Some(mode) = args.first() else {
        return Ok(diagnostics());
    };
    if mode == "diagnostics" {
        return Ok(diagnostics());
    }
    let value = |flag: &str| -> Result<&str> {
        let i = args
            .iter()
            .position(|arg| arg == flag)
            .with_context(|| format!("missing {flag}"))?;
        args.get(i + 1)
            .map(String::as_str)
            .with_context(|| format!("missing value for {flag}"))
    };
    match mode.as_str() {
        "lock-probe" => {
            let _lock = FileLock::acquire(Path::new(value("--path")?))?;
            println!("{{\"locked\":true}}");
            std::io::stdout().flush()?;
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            Ok(json!({"released":true}))
        }
        "probe" => {
            let child_file = args
                .iter()
                .position(|arg| arg == "--child-pid-file")
                .map(|_| value("--child-pid-file"))
                .transpose()?;
            probe(ProbeOptions {
                project: Path::new(value("--project")?),
                python: Path::new(value("--python")?),
                worker: Path::new(value("--worker")?),
                mib: value("--mib")?.parse()?,
                child_pid_file: child_file.map(Path::new),
            })
        }
        _ => bail!(
            "unsupported mode; available: diagnostics, probe, lock-probe (no production supervisor/helper)"
        ),
    }
}
