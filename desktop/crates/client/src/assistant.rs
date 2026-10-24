//! Independent assistant transport. Provider waits never block autosave or runtime polling.
use super::*;
use crate::service::Event;
use std::sync::{Arc, Mutex, mpsc};

pub struct AssistantService {
    pub requests: mpsc::Sender<(String, Value)>,
    process: Arc<Mutex<Option<Child>>>,
}

impl AssistantService {
    pub fn new(project: PathBuf, python: PathBuf, events: mpsc::Sender<Event>) -> Self {
        let (requests, receive) = mpsc::channel::<(String, Value)>();
        let process = Arc::new(Mutex::new(None::<Child>));
        let child = process.clone();
        std::thread::spawn(move || {
            let mut transport: Option<(ChildStdin, ChildStdout)> = None;
            while let Ok((method, mut fields)) = receive.recv() {
                let result = (|| -> Result<()> {
                    if method == "credential.set" || method == "credential.delete" {
                        let provider = fields["provider"].as_str().unwrap_or("openai");
                        ensure!(
                            ["openai", "openai-compatible"].contains(&provider),
                            "Unknown provider"
                        );
                        // keyring's explicit target keeps CLI and desktop credentials interoperable.
                        let target = format!("KernelLedger/assistant/{provider}");
                        let entry =
                            keyring::Entry::new_with_target(&target, "KernelLedger", provider)?;
                        if method == "credential.set" {
                            entry.set_password(fields["credential"].as_str().unwrap_or(""))?;
                            fields["credential"] = Value::Null;
                        } else {
                            match entry.delete_credential() {
                                Ok(()) | Err(keyring::Error::NoEntry) => {}
                                Err(error) => return Err(error.into()),
                            }
                        }
                        transport = None;
                        if let Some(mut old) = child.lock().unwrap().take() {
                            let _ = old.kill();
                            let _ = old.wait();
                        }
                        let _ = events.send(Event::AssistantDone {
                            method: method.clone(),
                            value: json!({"stored":true}),
                        });
                        return Ok(());
                    }
                    let alive = child
                        .lock()
                        .unwrap()
                        .as_mut()
                        .is_some_and(|p| p.try_wait().ok().flatten().is_none());
                    if !alive {
                        transport = None;
                    }
                    if transport.is_none() {
                        let mut command = std::process::Command::new(&python);
                        command
                            .args(["-I", "-m", "kernelledger.assistant", "--project"])
                            .arg(&project)
                            .stdin(Stdio::piped())
                            .stdout(Stdio::piped())
                            .stderr(Stdio::null());
                        // Assistant credentials are passed in the hello only. Never inherit API key env vars.
                        command
                            .env_remove("KL_AI_API_KEY")
                            .env_remove("OPENAI_API_KEY");
                        hide_console(&mut command);
                        let mut spawned = command.spawn()?;
                        let mut stdin = spawned.stdin.take().unwrap();
                        let mut stdout = spawned.stdout.take().unwrap();
                        *child.lock().unwrap() = Some(spawned);
                        let mut hello =
                            json!({"kind":"hello", "api":"kernelledger.assistant", "versions":[1]});
                        // Read settings outside the notebook. No secret or key is ever persisted here.
                        let config_path = assistant_settings_path();
                        let config: Value = fs::read(&config_path)
                            .ok()
                            .and_then(|b| serde_json::from_slice(&b).ok())
                            .unwrap_or(Value::Null);
                        if let Some(provider) = config["provider"]
                            .as_str()
                            .filter(|p| ["openai", "openai-compatible"].contains(p))
                        {
                            let target = format!("KernelLedger/assistant/{provider}");
                            let entry =
                                keyring::Entry::new_with_target(&target, "KernelLedger", provider)?;
                            match entry.get_password() {
                                Ok(secret) => hello["credential"] = json!(secret),
                                Err(keyring::Error::NoEntry) => {}
                                Err(error) => return Err(error.into()),
                            }
                        }
                        write_frame(&mut stdin, &hello)?;
                        hello["credential"] = Value::Null;
                        let welcome = checked_response(read_frame(&mut stdout)?)?;
                        ensure!(
                            welcome["kind"] == "welcome" && welcome["version"] == 1,
                            "Unsupported assistant protocol"
                        );
                        transport = Some((stdin, stdout));
                    }
                    let (stdin, stdout) = transport.as_mut().unwrap();
                    fields["method"] = json!(method);
                    let request_id = Uuid::new_v4().to_string();
                    fields["request_id"] = json!(request_id);
                    write_frame(stdin, &fields)?;
                    loop {
                        let frame = checked_response(read_frame(&mut *stdout)?)?;
                        ensure!(
                            frame["request_id"] == request_id,
                            "Assistant request id mismatch"
                        );
                        match frame["kind"].as_str() {
                            Some("delta") => {
                                let _ = events.send(Event::AssistantDelta(frame));
                            }
                            Some("proposal") => {
                                let _ = events
                                    .send(Event::AssistantProposal(frame["proposal"].clone()));
                            }
                            Some("done") => {
                                let _ = events.send(Event::AssistantDone {
                                    method: method.clone(),
                                    value: frame["result"].clone(),
                                });
                                break;
                            }
                            _ => bail!("Invalid assistant frame"),
                        }
                    }
                    // Provider changes take effect with credentials loaded for that provider.
                    if method == "settings.update" {
                        transport = None;
                        if let Some(mut old) = child.lock().unwrap().take() {
                            let _ = old.kill();
                            let _ = old.wait();
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    transport = None;
                    if let Some(mut old) = child.lock().unwrap().take() {
                        let _ = old.kill();
                        let _ = old.wait();
                    }
                    let _ = events.send(Event::AssistantError {
                        method,
                        message: if error.downcast_ref::<keyring::Error>().is_some() {
                            "OS credential store is unavailable or the credential cannot be decoded"
                                .into()
                        } else {
                            error.to_string()
                        },
                    });
                }
            }
            if let Some(mut old) = child.lock().unwrap().take() {
                let _ = old.kill();
                let _ = old.wait();
            }
        });
        Self { requests, process }
    }

    pub fn cancel(&self) {
        if let Some(process) = self.process.lock().unwrap().as_mut() {
            let _ = process.kill();
        }
    }
}
impl Drop for AssistantService {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn assistant_settings_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("KernelLedger/assistant.json")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            })
            .join("kernelledger/assistant.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_stream_fixture_round_trips_bounded_frames() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/desktop/assistant/frames.json"
        ))
        .unwrap();
        assert_eq!(fixture["api"], "kernelledger.assistant");
        assert_eq!(fixture["version"], 1);
        for frame in fixture["frames"].as_array().unwrap() {
            let mut encoded = Vec::new();
            write_frame(&mut encoded, frame).unwrap();
            let decoded = read_frame(encoded.as_slice()).unwrap();
            assert_eq!(decoded, *frame);
            assert_eq!(decoded["request_id"], "request-1");
        }
    }
}
