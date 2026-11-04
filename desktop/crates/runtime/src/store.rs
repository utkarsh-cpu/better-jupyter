//! Read-only content-addressed blob access. Every read verifies the payload hash.
//!
//! This mirrors `kernelledger.blobs.BlobStore` reads: no hash cache, bounded
//! streaming verification, and closure traversal only through checkpoint
//! manifests and experimental artifact/state documents.
use anyhow::Result;
use kernelledger_protocol::{BUFFER_SIZE, ProtocolError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub fn coded(code: &str, message: impl Into<String>, details: Value) -> anyhow::Error {
    ProtocolError {
        code: code.into(),
        message: message.into(),
        retryable: false,
        details,
    }
    .into()
}

/// Python `ValueError`, reported by the helper as `invalid_request`.
pub fn invalid(message: impl Into<String>) -> anyhow::Error {
    coded("invalid_request", message, json!({}))
}

pub fn is_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn check_hash(digest: &str) -> Result<&str> {
    if is_hex64(digest) {
        Ok(digest)
    } else {
        Err(invalid("invalid blob hash"))
    }
}

/// Comparable file identity. Windows omits change time, matching the Python store.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Identity {
    size: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    unix: (u64, u64, i64, i64),
}
fn identity(metadata: &fs::Metadata) -> Identity {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Identity {
        size: metadata.len(),
        modified: metadata.modified().ok(),
        #[cfg(unix)]
        unix: (
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        ),
    }
}

/// Operation-local verification evidence. Never retained across requests.
pub struct Verified {
    pub size: u64,
    path: PathBuf,
    identity: Identity,
}
impl Verified {
    pub fn check_current(&self, digest: &str) -> Result<()> {
        if identity(&fs::metadata(&self.path)?) != self.identity {
            return Err(invalid(format!(
                "blob changed after verification: {digest}"
            )));
        }
        Ok(())
    }
}

pub struct BlobStore {
    root: PathBuf,
}
impl BlobStore {
    pub fn new(state: &Path) -> Self {
        Self {
            root: state.join("blobs"),
        }
    }

    pub fn open(&self, digest: &str) -> Result<File> {
        Ok(File::open(self.root.join(check_hash(digest)?))?)
    }

    /// Hash an open file from offset zero in fixed buffers.
    pub fn verify_file(&self, file: &mut File, digest: &str) -> Result<Verified> {
        check_hash(digest)?;
        file.seek(SeekFrom::Start(0))?;
        let before = identity(&file.metadata()?);
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; BUFFER_SIZE];
        let mut size = 0u64;
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            size += count as u64;
        }
        let after = identity(&file.metadata()?);
        if before != after || format!("{:x}", hasher.finalize()) != digest {
            return Err(invalid(format!("blob hash mismatch: {digest}")));
        }
        let verified = Verified {
            size,
            path: self.root.join(digest),
            identity: after,
        };
        verified.check_current(digest)?;
        Ok(verified)
    }

    /// Durable publication, matching `BlobStore.put`: temp file, fsync, then rename.
    /// An existing digest is rehashed and left in place. Callers must not use this
    /// from the read-only helper.
    pub fn publish(&self, data: &[u8], expected: Option<&str>) -> Result<String> {
        let digest = kernelledger_protocol::hash(data);
        if let Some(expected) = expected {
            let expected = check_hash(expected)?;
            if expected != digest {
                return Err(invalid("blob hash mismatch"));
            }
        }
        let target = self.root.join(&digest);
        if target.exists() {
            self.get(&digest)?;
            return Ok(digest);
        }
        let temporary = self
            .root
            .parent()
            .ok_or_else(|| invalid("blob directory has no parent"))?
            .join("tmp");
        fs::create_dir_all(&self.root)?;
        fs::create_dir_all(&temporary)?;
        let mut file = tempfile::NamedTempFile::new_in(&temporary)?;
        file.write_all(data)?;
        file.as_file().sync_all()?;
        match file.persist_noclobber(&target) {
            Ok(_) => {}
            Err(error) if target.exists() => {
                drop(error);
                self.get(&digest)?;
                return Ok(digest);
            }
            Err(error) => return Err(error.error.into()),
        }
        #[cfg(unix)]
        {
            File::open(&self.root)?.sync_all()?;
        }
        self.get(&digest)?;
        Ok(digest)
    }

    /// Receive and publish a worker binary frame with one bounded buffer.
    pub fn receive(&self, reader: impl Read, header: &Value) -> Result<String> {
        let digest = header["sha256"]
            .as_str()
            .ok_or_else(|| invalid("missing blob hash"))?;
        check_hash(digest)?;
        let temporary = self
            .root
            .parent()
            .ok_or_else(|| invalid("missing state path"))?
            .join("tmp");
        fs::create_dir_all(&temporary)?;
        fs::create_dir_all(&self.root)?;
        let mut file = tempfile::NamedTempFile::new_in(temporary)?;
        kernelledger_protocol::copy_blob(reader, &mut file, header)?;
        file.as_file().sync_all()?;
        let target = self.root.join(digest);
        match file.persist_noclobber(&target) {
            Ok(_) => {}
            Err(error) if target.exists() => {
                drop(error);
                let mut stored = self.open(digest)?;
                self.verify_file(&mut stored, digest)?;
            }
            Err(error) => return Err(error.error.into()),
        }
        #[cfg(unix)]
        File::open(&self.root)?.sync_all()?;
        Ok(digest.to_owned())
    }

    /// Whole-payload read for small structured blobs (sources, manifests).
    pub fn get(&self, digest: &str) -> Result<Vec<u8>> {
        let mut file = self.open(digest)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        if kernelledger_protocol::hash(&data) != digest {
            return Err(invalid(format!("blob hash mismatch: {digest}")));
        }
        Ok(data)
    }

    /// Verify each reachable blob once; parse only potential JSON objects.
    pub fn closure(&self, seeds: &[String]) -> Result<BTreeSet<String>> {
        Ok(self.verified_closure(seeds)?.into_keys().collect())
    }

    /// Verification evidence lives only for this operation. Publication checks
    /// every identity again under its transaction; no cross-request hash cache.
    pub fn verified_closure(&self, seeds: &[String]) -> Result<BTreeMap<String, Verified>> {
        let mut pending: Vec<String> = seeds.to_vec();
        let mut seen = BTreeMap::new();
        while let Some(digest) = pending.pop() {
            if seen.contains_key(&digest) {
                continue;
            }
            let mut file = self.open(&digest)?;
            let verified = self.verify_file(&mut file, &digest)?;
            if !json_object(&mut file)? {
                seen.insert(digest, verified);
                continue;
            }
            file.seek(SeekFrom::Start(0))?;
            let mut data = Vec::new();
            file.read_to_end(&mut data)?;
            let decoded = decode_json(&data);
            verified.check_current(&digest)?;
            seen.insert(digest, verified);
            let Ok(value) = decoded else {
                continue;
            };
            if let Value::Object(map) = &value {
                let schema = map.get("schema");
                let manifest = schema.is_some_and(python_equals_one)
                    && map.contains_key("objects")
                    && map.contains_key("bindings");
                let experimental = matches!(
                    schema.and_then(Value::as_str),
                    Some(
                        "kernelledger.experimental.artifact/1"
                            | "kernelledger.experimental.state/1"
                    )
                );
                if manifest || experimental {
                    references(&value, &mut pending);
                }
            }
        }
        Ok(seen)
    }
}

/// Python `value == 1`, which is also true for `1.0` and `True`.
fn python_equals_one(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() == Some(1.0),
        _ => false,
    }
}

/// Port of `kernelledger.blobs.references`.
pub fn references(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let reference_key = matches!(
                    key.as_str(),
                    "blob" | "manifest_sha256" | "source_sha256" | "blob_sha256"
                ) || (key == "source"
                    && matches!(
                        map.get("k").and_then(Value::as_str),
                        Some("class" | "function")
                    ));
                match child.as_str() {
                    Some(text) if reference_key && is_hex64(text) => out.push(text.into()),
                    _ if key != "value" => references(child, out),
                    _ => {}
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| references(item, out)),
        _ => {}
    }
}

/// Inspect only the first non-whitespace code point, including JSON BOMs.
fn json_object(file: &mut File) -> Result<bool> {
    file.seek(SeekFrom::Start(0))?;
    let mut prefix = Vec::with_capacity(4);
    Read::by_ref(file).take(4).read_to_end(&mut prefix)?;
    let boms: [(&[u8], usize, bool); 5] = [
        (&[0xFF, 0xFE, 0, 0], 4, false),
        (&[0, 0, 0xFE, 0xFF], 4, true),
        (&[0xFF, 0xFE], 2, false),
        (&[0xFE, 0xFF], 2, true),
        (&[0xEF, 0xBB, 0xBF], 1, false),
    ];
    let (mut width, mut big, mut offset) = (1usize, false, 0usize);
    if let Some((marker, unit, endian)) = boms.iter().find(|(m, _, _)| prefix.starts_with(m)) {
        (width, big, offset) = (*unit, *endian, marker.len());
    } else if prefix.len() >= 2 && prefix[0] == 0 {
        width = if prefix.len() == 4 && prefix[1] == 0 {
            4
        } else {
            2
        };
        big = true;
    } else if prefix.len() >= 2 && prefix[1] == 0 {
        width = if prefix.len() == 4 && prefix[2..] == [0, 0] {
            4
        } else {
            2
        };
    }
    file.seek(SeekFrom::Start(offset as u64))?;
    let mut buffer = vec![0; BUFFER_SIZE];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            return Ok(false);
        }
        let chunk = &buffer[..count];
        // Matches Python: each chunk is examined from its own start.
        for unit in chunk.chunks(width) {
            let codepoint = unit.iter().enumerate().fold(0u64, |acc, (i, b)| {
                if big {
                    (acc << 8) | *b as u64
                } else {
                    acc | ((*b as u64) << (8 * i))
                }
            });
            if !matches!(codepoint, 9 | 10 | 13 | 32) {
                return Ok(codepoint == 123);
            }
        }
    }
}

/// Python `json.loads(bytes)`: detect UTF-8/16/32 and BOMs, then parse.
pub fn decode_json(data: &[u8]) -> Result<Value> {
    let text = decode_text(data)?;
    serde_json::from_str(&text).map_err(|e| invalid(e.to_string()))
}

fn decode_text(b: &[u8]) -> Result<String> {
    let units = |data: &[u8], width: usize, big: bool| -> Result<Vec<u32>> {
        if !data.len().is_multiple_of(width) {
            return Err(invalid("truncated data"));
        }
        Ok(data
            .chunks(width)
            .map(|c| {
                c.iter().enumerate().fold(0u32, |acc, (i, b)| {
                    if big {
                        (acc << 8) | *b as u32
                    } else {
                        acc | ((*b as u32) << (8 * i))
                    }
                })
            })
            .collect())
    };
    let utf16 = |data: &[u8], big: bool| -> Result<String> {
        let words: Vec<u16> = units(data, 2, big)?.into_iter().map(|u| u as u16).collect();
        String::from_utf16(&words).map_err(|_| invalid("invalid UTF-16 data"))
    };
    let utf32 = |data: &[u8], big: bool| -> Result<String> {
        units(data, 4, big)?
            .into_iter()
            .map(|u| char::from_u32(u).ok_or_else(|| invalid("invalid UTF-32 data")))
            .collect()
    };
    if b.starts_with(&[0, 0, 0xFE, 0xFF]) {
        return utf32(&b[4..], true);
    }
    if b.starts_with(&[0xFF, 0xFE, 0, 0]) {
        return utf32(&b[4..], false);
    }
    if b.starts_with(&[0xFE, 0xFF]) {
        return utf16(&b[2..], true);
    }
    if b.starts_with(&[0xFF, 0xFE]) {
        return utf16(&b[2..], false);
    }
    if b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(b[3..].to_vec()).map_err(|_| invalid("invalid UTF-8 data"));
    }
    if b.len() >= 4 {
        if b[0] == 0 {
            return if b[1] != 0 {
                utf16(b, true)
            } else {
                utf32(b, true)
            };
        }
        if b[1] == 0 {
            return if b[2] != 0 || b[3] != 0 {
                utf16(b, false)
            } else {
                utf32(b, false)
            };
        }
    } else if b.len() == 2 {
        if b[0] == 0 {
            return utf16(b, true);
        }
        if b[1] == 0 {
            return utf16(b, false);
        }
    }
    String::from_utf8(b.to_vec()).map_err(|_| invalid("invalid UTF-8 data"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_local_evidence_rejects_changes_and_next_request_rehashes() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path());
        let payload = store.publish(b"binary payload", None).unwrap();
        let manifest=store.publish(&serde_json::to_vec(&json!({"schema":1,"objects":{},"bindings":{"a":{"blob":payload},"b":{"blob":payload}}})).unwrap(),None).unwrap();
        let evidence = store
            .verified_closure(std::slice::from_ref(&manifest))
            .unwrap();
        assert_eq!(evidence.len(), 2);
        fs::write(
            dir.path().join("blobs").join(&payload),
            b"corrupted and longer",
        )
        .unwrap();
        assert!(evidence[&payload].check_current(&payload).is_err());
        assert!(store.verified_closure(&[manifest]).is_err());
    }

    #[test]
    fn references_follow_python_rules() {
        let h = "a".repeat(64);
        let value = json!({"objects":[{"k":"function","source":h},{"k":"str","source":"b".repeat(64)},
            {"value":{"blob":"c".repeat(64)}},{"blob":"short","x":{"blob_sha256":"d".repeat(64)}}]});
        let mut out = Vec::new();
        references(&value, &mut out);
        out.sort();
        assert_eq!(out, vec!["a".repeat(64), "d".repeat(64)]);
    }

    #[test]
    fn json_encodings_match_python_detection() {
        assert_eq!(decode_json(b"{\"a\":1}").unwrap(), json!({"a":1}));
        assert_eq!(decode_json(b"\xEF\xBB\xBF{}").unwrap(), json!({}));
        let utf16: Vec<u8> = "{\"a\":2}"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert_eq!(decode_json(&utf16).unwrap(), json!({"a":2}));
        let utf32: Vec<u8> = "[3]"
            .chars()
            .flat_map(|c| (c as u32).to_be_bytes())
            .collect();
        assert_eq!(decode_json(&utf32).unwrap(), json!([3]));
        assert!(decode_json(b"\xFF{}").is_err());
    }

    #[test]
    fn verification_detects_corruption_and_missing() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path());
        fs::create_dir_all(dir.path().join("blobs")).unwrap();
        let digest = kernelledger_protocol::hash(b"{\"schema\":1,\"objects\":[],\"bindings\":{}}");
        fs::write(
            dir.path().join("blobs").join(&digest),
            b"{\"schema\":1,\"objects\":[],\"bindings\":{}}",
        )
        .unwrap();
        assert_eq!(
            store.closure(std::slice::from_ref(&digest)).unwrap().len(),
            1
        );
        fs::write(dir.path().join("blobs").join(&digest), b"tampered").unwrap();
        assert!(store.get(&digest).is_err());
        let mut file = store.open(&digest).unwrap();
        assert!(store.verify_file(&mut file, &digest).is_err());
        let missing = store.open(&"0".repeat(64)).unwrap_err();
        assert!(missing.downcast_ref::<std::io::Error>().is_some());
        assert!(store.open("../x").is_err());
    }

    #[test]
    fn publish_is_content_addressed_and_does_not_replace_existing() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::new(dir.path());
        let digest = store.publish(b"payload", None).unwrap();
        assert_eq!(digest, kernelledger_protocol::hash(b"payload"));
        assert_eq!(
            fs::read(dir.path().join("blobs").join(&digest)).unwrap(),
            b"payload"
        );
        assert!(dir.path().join("tmp").is_dir());
        let again = store.publish(b"payload", Some(&digest)).unwrap();
        assert_eq!(again, digest);
        assert!(store.publish(b"payload", Some(&"ab".repeat(32))).is_err());
        assert!(store.publish(b"other", Some(&digest)).is_err());
    }
}
