//! Read and authenticate runtime hints; registry files contain no tokens.
use super::*;
pub fn directory() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
            .join("KernelLedger/runtimes")
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
            })
            .join("kernelledger/runtimes")
    }
}
pub fn list(identity: &ClientIdentity) -> Result<Value> {
    let mut entries = Vec::new();
    if !directory().exists() {
        return Ok(json!({"items":entries}));
    }
    for path in fs::read_dir(directory())? {
        let path = path?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let mut info: Value = match fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        {
            Some(info) => info,
            None => json!({"state":"stale","registry_path":path}),
        };
        if let Some(project) = info["project_path"].as_str() {
            match RuntimeClient::connect(Path::new(project), identity, "control")
                .and_then(|mut client| client.request("runtime.status", json!({})))
            {
                Ok(status) => {
                    info["state"] = json!("verified");
                    info["runtime"] = status;
                }
                Err(_) => info["state"] = json!("stale"),
            }
        }
        entries.push(info);
    }
    Ok(json!({"items":entries}))
}
