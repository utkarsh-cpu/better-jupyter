//! Transport-neutral framing. The Python catalogs remain the contract authority.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

pub const API: &str = "kernelledger.desktop";
pub const FRAME_LIMIT: usize = 1_048_576;
pub const WORKER_INLINE: usize = 65_536;
pub const BINARY_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
pub const BUFFER_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: Value,
}
impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ProtocolError {}

pub fn write_frame(writer: impl Write, message: &Value) -> Result<()> {
    write_bounded(writer, message, FRAME_LIMIT)
}
pub fn read_frame(reader: impl Read) -> Result<Value> {
    read_bounded(reader, FRAME_LIMIT)
}
pub fn write_bounded(mut writer: impl Write, message: &Value, limit: usize) -> Result<()> {
    let data = serde_json::to_vec(message)?;
    check_frame_size(data.len(), limit)?;
    writer.write_all(&(data.len() as u32).to_be_bytes())?;
    writer.write_all(&data)?;
    writer.flush()?;
    Ok(())
}
pub fn read_bounded(mut reader: impl Read, limit: usize) -> Result<Value> {
    let mut size = [0; 4];
    reader.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    check_frame_size(size, limit)?;
    let mut data = vec![0; size];
    reader.read_exact(&mut data)?;
    let value: Value = serde_json::from_slice(&data)?;
    ensure!(value.is_object(), "frame must be an object");
    Ok(value)
}
fn check_frame_size(size: usize, limit: usize) -> Result<()> {
    if size > limit {
        if limit == FRAME_LIMIT {
            bail!("JSON frame exceeds 1 MiB");
        }
        bail!("protocol header exceeds inline limit");
    }
    Ok(())
}
pub fn read_worker(reader: impl Read) -> Result<Value> {
    let value = read_bounded(reader, WORKER_INLINE)?;
    ensure!(
        value["schema"].as_u64() == Some(1),
        "unsupported protocol schema"
    );
    ensure!(value["kind"].is_string(), "missing worker message kind");
    Ok(value)
}
pub fn write_worker(writer: impl Write, value: &Value) -> Result<()> {
    ensure!(
        value["schema"].as_u64() == Some(1),
        "unsupported protocol schema"
    );
    write_bounded(writer, value, WORKER_INLINE)
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn check_hash(digest: &str) -> Result<()> {
    ensure!(
        digest.len() == 64
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid blob hash"
    );
    Ok(())
}
/// Consume exactly one bounded binary frame. No payload-sized allocation.
pub fn copy_blob(mut reader: impl Read, mut writer: impl Write, header: &Value) -> Result<u64> {
    let size = header["byte_size"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("invalid binary frame size"))?;
    ensure!(size <= BINARY_LIMIT, "invalid binary frame size");
    let digest = header["sha256"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing blob hash"))?;
    check_hash(digest)?;
    let mut buffer = vec![0; BUFFER_SIZE];
    let mut remaining = size;
    let mut hasher = Sha256::new();
    while remaining != 0 {
        let count = remaining.min(BUFFER_SIZE as u64) as usize;
        reader.read_exact(&mut buffer[..count])?;
        hasher.update(&buffer[..count]);
        writer.write_all(&buffer[..count])?;
        remaining -= count as u64;
    }
    ensure!(
        format!("{:x}", hasher.finalize()) == digest,
        "blob hash mismatch"
    );
    Ok(size)
}

/// Exact Python request hashing for the admitted integer/string subset only.
/// Floats and integers outside serde_json's exact range are explicitly unsupported.
/// This prototype never admits durable operations; expand using golden fixtures first.
pub fn request_hash(value: &Value) -> Result<String> {
    ensure!(value.is_object(), "request must be an object");
    let mut request = value.clone();
    request.as_object_mut().unwrap().remove("request_id");
    Ok(hash(canonical(&request)?.as_bytes()))
}
fn canonical(value: &Value) -> Result<String> {
    Ok(match value {
        Value::Null => "null".into(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) if v.is_i64() || v.is_u64() => v.to_string(),
        Value::Number(_) => bail!("native request hash numeric form is not qualified"),
        Value::String(v) => {
            let mut text = String::from("\"");
            for c in v.chars() {
                match c {
                    '"' => text.push_str("\\\""),
                    '\\' => text.push_str("\\\\"),
                    '\u{08}' => text.push_str("\\b"),
                    '\u{0c}' => text.push_str("\\f"),
                    '\n' => text.push_str("\\n"),
                    '\r' => text.push_str("\\r"),
                    '\t' => text.push_str("\\t"),
                    c if (' '..='~').contains(&c) => text.push(c),
                    c => {
                        for unit in c.encode_utf16(&mut [0; 2]).iter() {
                            use std::fmt::Write;
                            write!(&mut text, "\\u{unit:04x}")?;
                        }
                    }
                }
            }
            text.push('"');
            text
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical)
                .collect::<Result<Vec<_>>>()?
                .join(",")
        ),
        Value::Object(values) => {
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort();
            let fields = keys
                .into_iter()
                .map(|key| {
                    Ok(format!(
                        "{}:{}",
                        canonical(&Value::String(key.clone()))?,
                        canonical(&values[key])?
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            format!("{{{}}}", fields.join(","))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{self, Cursor};

    struct Fragmented(Cursor<Vec<u8>>);
    impl Read for Fragmented {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let count = buf.len().min(1);
            self.0.read(&mut buf[..count])
        }
    }
    #[test]
    fn fragmented_and_invalid_frames() {
        let value = json!({"schema":1,"kind":"ready"});
        let mut wire = Vec::new();
        write_worker(&mut wire, &value).unwrap();
        assert_eq!(
            read_worker(Fragmented(Cursor::new(wire.clone()))).unwrap(),
            value
        );
        for count in 0..wire.len() {
            assert!(read_worker(&wire[..count]).is_err());
        }
        assert!(read_worker(&((WORKER_INLINE + 1) as u32).to_be_bytes()[..]).is_err());
        assert!(read_frame(&[0, 0, 0, 1, 255][..]).is_err());
        assert!(read_frame(&[0, 0, 0, 2, b'[', b']'][..]).is_err());
        assert!(read_worker(&[0, 0, 0, 2, b'{', b'}'][..]).is_err());
    }
    #[test]
    fn corrupt_binary_and_limits() {
        let header = json!({"byte_size":3,"sha256":hash(b"abc")});
        assert_eq!(copy_blob(&b"abc"[..], io::sink(), &header).unwrap(), 3);
        assert!(copy_blob(&b"abd"[..], io::sink(), &header).is_err());
        for size in [json!(-1), json!(true), json!(BINARY_LIMIT + 1), json!(null)] {
            assert!(
                copy_blob(
                    io::empty(),
                    io::sink(),
                    &json!({"byte_size":size,"sha256":hash(b"")})
                )
                .is_err()
            );
        }
    }
    #[test]
    fn python_hash_fixtures() {
        let fixtures: Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/runtime/request-hashes.json"
        ))
        .unwrap();
        for case in fixtures.as_array().unwrap() {
            if case["native_supported"] == true {
                assert_eq!(
                    request_hash(&case["request"]).unwrap(),
                    case["sha256"].as_str().unwrap(),
                    "{}",
                    case["name"]
                );
            } else {
                assert!(request_hash(&case["request"]).is_err());
            }
        }
    }
}
