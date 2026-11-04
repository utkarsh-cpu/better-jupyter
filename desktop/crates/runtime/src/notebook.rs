//! Tracked-source lookup used by output views and the native source service.
//!
//! Mirrors `Notebook.read`, `Notebook.cell`, `Notebook.source`, and
//! `source_hash`. Mutation and journal recovery live in `crate::source`.
use anyhow::{Result, bail, ensure};
use serde_json::Value;
use std::{collections::HashSet, fs, path::Path};

const DEVICES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub(crate) fn extension(kind: &str) -> Option<&'static str> {
    match kind {
        "code" => Some(".py"),
        "markdown" => Some(".md"),
        "raw" => Some(".raw"),
        _ => None,
    }
}

pub fn valid_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_alphanumeric()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
        && !DEVICES.contains(&value.to_uppercase().as_str())
}

pub(crate) fn filename(value: &Value) -> Result<()> {
    let Some(name) = value.as_str() else {
        bail!("invalid attachment filename")
    };
    ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.chars().any(|c| "/\\\0:".contains(c))
            && name.trim_end_matches(['.', ' ']) == name
            && !DEVICES.contains(&name.split('.').next().unwrap_or("").to_uppercase().as_str()),
        "invalid attachment filename"
    );
    Ok(())
}

pub fn canonical(text: &str) -> String {
    text.strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

pub fn source_hash(text: &str) -> String {
    kernelledger_protocol::hash(canonical(text).as_bytes())
}

fn canonical_uuid(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(c),
        })
}

/// Validated manifest, as `Notebook.read(validate=True)`.
pub fn read(root: &Path) -> Result<Value> {
    let data: Value = serde_json::from_str(&fs::read_to_string(root.join("notebook.json"))?)?;
    ensure!(
        data.get("schema") == Some(&Value::from(1)),
        "unsupported notebook schema"
    );
    ensure!(
        data.get("id")
            .and_then(Value::as_str)
            .is_some_and(canonical_uuid),
        "invalid notebook UUID"
    );
    ensure!(
        data.get("metadata").is_some_and(Value::is_object)
            && data.get("cells").is_some_and(Value::is_array),
        "invalid notebook manifest"
    );
    let mut seen = HashSet::new();
    for cell in data["cells"].as_array().into_iter().flatten() {
        let cell = cell
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("invalid cell"))?;
        let id = cell.get("id").and_then(Value::as_str).unwrap_or("");
        ensure!(
            valid_id(id) && seen.insert(id.to_lowercase()),
            "invalid or duplicate cell id"
        );
        ensure!(
            cell.get("type")
                .and_then(Value::as_str)
                .and_then(extension)
                .is_some(),
            "invalid cell type"
        );
        let names = match cell.get("attachments") {
            None => Vec::new(),
            Some(Value::Array(names)) => names.clone(),
            Some(_) => bail!("attachments must be a list"),
        };
        let mut lowered = HashSet::new();
        for name in &names {
            let text = name
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("invalid attachment"))?;
            lowered.insert(text.to_lowercase());
        }
        ensure!(lowered.len() == names.len(), "duplicate attachment names");
        names.iter().try_for_each(filename)?;
    }
    Ok(data)
}

/// Current canonical source hash for a tracked cell.
pub fn current_source_hash(root: &Path, cell_id: &str) -> Result<String> {
    let data = read(root)?;
    let cell = data["cells"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|cell| cell["id"] == cell_id)
        .ok_or_else(|| anyhow::anyhow!("unknown cell: {cell_id}"))?;
    let id = cell["id"].as_str().unwrap_or("");
    ensure!(valid_id(id), "invalid cell id");
    let ext = extension(cell["type"].as_str().unwrap_or("")).unwrap_or(".py");
    let bytes = fs::read(root.join("cells").join(format!("{id}{ext}")))?;
    Ok(source_hash(&canonical(&String::from_utf8(bytes)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_canonical_source() {
        assert!(valid_id("a1_b-c"));
        assert!(!valid_id("con"));
        assert!(!valid_id("_a"));
        assert!(!valid_id(&"a".repeat(65)));
        assert_eq!(canonical("\u{feff}a\r\nb\rc"), "a\nb\nc");
        assert!(filename(&Value::from("ok.png")).is_ok());
        for bad in ["", "..", "a/b", "x.", "NUL.txt", "a:b"] {
            assert!(filename(&Value::from(bad)).is_err(), "{bad}");
        }
    }
}
