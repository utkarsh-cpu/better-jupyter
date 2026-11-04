//! Python-compatible registry hints and cleanup tied to an exact runtime instance.
use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn directory() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        let base = match std::env::var_os("LOCALAPPDATA") {
            Some(value) => PathBuf::from(value),
            None => PathBuf::from(std::env::var_os("USERPROFILE").context("missing user profile")?)
                .join("AppData/Local"),
        };
        Ok(base.join("KernelLedger/runtimes"))
    }
    #[cfg(unix)]
    {
        let base = match std::env::var_os("XDG_STATE_HOME") {
            Some(value) => PathBuf::from(value),
            None => PathBuf::from(std::env::var_os("HOME").context("missing home directory")?)
                .join(".local/state"),
        };
        Ok(base.join("kernelledger/runtimes"))
    }
}
pub fn entry(root: &Path, directory: &Path) -> PathBuf {
    let canonical = crate::display_path(root);
    let canonical = if cfg!(windows) {
        canonical.to_lowercase()
    } else {
        canonical
    };
    directory.join(format!(
        "{}.json",
        kernelledger_protocol::hash(canonical.as_bytes())
    ))
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("publication parent")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
pub struct Publication {
    paths: Vec<PathBuf>,
    instance: Value,
}
impl Publication {
    pub fn publish(root: &Path, info: &Value) -> Result<Self> {
        Self::publish_to(root, &directory()?, info)
    }
    fn publish_to(root: &Path, directory: &Path, info: &Value) -> Result<Self> {
        let mut publication = Self {
            paths: Vec::new(),
            instance: info
                .get("instance_id")
                .cloned()
                .filter(Value::is_string)
                .context("runtime instance id")?,
        };
        let bytes = serde_json::to_vec(info)?;
        // The project hint is the startup readiness point used by existing CLI
        // clients, so publish it only after the registry entry is durable.
        for path in [entry(root, directory), root.join(".kl/supervisor.json")] {
            publication.paths.push(path.clone());
            atomic(&path, &bytes)?;
        }
        Ok(publication)
    }
    pub fn cleanup(&mut self) -> Result<()> {
        for path in &self.paths {
            let bytes = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            let info: Value = serde_json::from_slice(&bytes)?;
            // Never erase a successor's metadata, including during failed startup.
            if info.get("instance_id") == Some(&self.instance) {
                fs::remove_file(path)?;
            }
        }
        self.paths.clear();
        Ok(())
    }
}
impl Drop for Publication {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn cleanup_removes_only_the_publishing_instance() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let registry = directory.path().join("registry");
        let old = Publication::publish_to(&root, &registry, &json!({"instance_id":"old"})).unwrap();
        let new = Publication::publish_to(&root, &registry, &json!({"instance_id":"new"})).unwrap();
        drop(old);
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(root.join(".kl/supervisor.json")).unwrap())
                .unwrap()["instance_id"],
            "new"
        );
        assert!(entry(&root, &registry).is_file());
        drop(new);
        assert!(!root.join(".kl/supervisor.json").exists());
        assert!(!entry(&root, &registry).exists());
    }
}
