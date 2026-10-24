//! Versioned worker resource embedded at build time. No repository lookup at runtime.
use anyhow::Result;
use std::collections::VecDeque;
use std::io::Write;

const VALIDATION_BYTES: usize = 4 * 1024 * 1024;
const VALIDATION_ENTRIES: usize = 64;

/// Pure validation results for exact manifest input bytes. These are not blob
/// verification results: storage is rehashed on every request. Owner lifetime
/// fixes the embedded resource and Python interpreter. Custom workers bypass it.
#[derive(Default)]
pub struct ValidationCache {
    entries: VecDeque<(Vec<u8>, Vec<u8>)>,
    bytes: usize,
}
impl ValidationCache {
    pub fn get(&mut self, input: &[u8]) -> Option<Vec<u8>> {
        let position = self.entries.iter().position(|(key, _)| key == input)?;
        let entry = self.entries.remove(position)?;
        let output = entry.1.clone();
        self.entries.push_back(entry);
        Some(output)
    }
    pub fn insert(&mut self, input: &[u8], output: &[u8]) {
        let size = input.len() + output.len();
        if size > VALIDATION_BYTES {
            return;
        }
        while self.entries.len() >= VALIDATION_ENTRIES || self.bytes + size > VALIDATION_BYTES {
            let Some((key, value)) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= key.len() + value.len();
        }
        self.bytes += size;
        self.entries.push_back((input.to_vec(), output.to_vec()));
    }
}

pub const WORKER: &[u8] = include_bytes!("../../../../src/kernelledger/worker.py");

/// Execute the exact trusted validator and canonical encoder without importing
/// the worker's unrelated execution/codec modules. No notebook code is loaded.
/// Unknown future dependencies fail closed instead of bypassing validation.
pub const VALIDATOR_COMMAND: &str = r#"
import ast, base64, json, re, sys
from typing import Any, cast
with open(sys.argv[1], encoding='utf-8') as source:
    tree = ast.parse(source.read(), filename=sys.argv[1])
names = {'validate_checkpoint', 'json_bytes'}
functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
assert len(functions) == len(names) and {node.name for node in functions} == names
exec(compile(ast.Module(body=functions, type_ignores=[]), sys.argv[1], 'exec'), globals())
manifest = json.load(sys.stdin)
validate_checkpoint(manifest)
sys.stdout.buffer.write(json_bytes(manifest))
"#;

pub fn extract() -> Result<tempfile::TempDir> {
    let directory = tempfile::Builder::new()
        .prefix("kl-native-resource-")
        .tempdir()?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.path().join("worker.py"))?;
    file.write_all(WORKER)?;
    file.sync_all()?;
    Ok(directory)
}

pub fn identity() -> serde_json::Value {
    serde_json::json!({"name":"worker.py","worker_protocol":1,"version":env!("CARGO_PKG_VERSION"),"sha256":kernelledger_protocol::hash(WORKER),"byte_size":WORKER.len(),"source":"embedded"})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validation_cache_requires_exact_bytes_and_has_hard_entry_and_byte_limits() {
        let mut cache = ValidationCache::default();
        cache.insert(b"valid", b"canonical");
        assert_eq!(cache.get(b"valid").unwrap(), b"canonical");
        assert!(cache.get(b"invalid").is_none());
        for index in 0..VALIDATION_ENTRIES + 1 {
            cache.insert(index.to_string().as_bytes(), &vec![0; 70000]);
        }
        assert!(cache.entries.len() <= VALIDATION_ENTRIES && cache.bytes <= VALIDATION_BYTES);
        assert!(cache.get(b"valid").is_none());
        let count = cache.entries.len();
        cache.insert(&vec![0; VALIDATION_BYTES + 1], &[]);
        assert_eq!(cache.entries.len(), count);
    }
}
