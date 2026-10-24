//! Read-only ledger access for the native helper. Never creates or writes a database.
use crate::store::invalid;
use anyhow::Result;
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde_json::{Map, Value};
use std::{path::Path, time::Duration};

pub type Row = Map<String, Value>;

pub struct Ledger {
    conn: Connection,
}

/// `Path.as_uri()` equivalent for SQLite URI filenames.
fn file_uri(path: &Path) -> String {
    let mut text = crate::display_path(path).replace('\\', "/");
    let unc = text.starts_with("//");
    if !unc && !text.starts_with('/') {
        text.insert(0, '/');
    }
    let mut uri = String::from("file:");
    if !unc {
        uri.push_str("//");
    }
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"/:-._~".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

fn writable(dir: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
            return false;
        };
        // SAFETY: path is a valid NUL-terminated C string for the call duration.
        unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
    }
    #[cfg(not(unix))]
    {
        std::fs::metadata(dir).is_ok_and(|m| !m.permissions().readonly())
    }
}

impl Ledger {
    /// `None` when no history exists. Immutable mode is used only for a
    /// non-writable state directory without a running supervisor, as in Python.
    pub fn open(state: &Path) -> Result<Option<(Self, bool)>> {
        let path = state.join("ledger.sqlite");
        if !path.exists() {
            return Ok(None);
        }
        let immutable = !writable(state) && !state.join("supervisor.json").exists();
        let absolute = path.canonicalize()?;
        let uri = file_uri(&absolute)
            + if immutable {
                "?immutable=1"
            } else {
                "?mode=ro"
            };
        let conn = Connection::open_with_flags(
            uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if !(1..=2).contains(&version) {
            return Err(invalid("unsupported ledger schema"));
        }
        Ok(Some((Self { conn }, immutable)))
    }

    /// Short read transaction so a count and its page observe one snapshot.
    pub fn snapshot<T>(&self, body: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        self.conn.execute_batch("BEGIN DEFERRED")?;
        let result = body(self);
        let _ = self.conn.execute_batch("COMMIT");
        result
    }

    pub fn rows(&self, sql: &str, args: &[rusqlite::types::Value]) -> Result<Vec<Row>> {
        let mut statement = self.conn.prepare(sql)?;
        let names: Vec<String> = statement
            .column_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut rows = statement.query(rusqlite::params_from_iter(args.iter()))?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            let mut item = Map::new();
            for (i, name) in names.iter().enumerate() {
                let value = match row.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(v) => Value::from(v),
                    ValueRef::Real(v) => serde_json::Number::from_f64(v)
                        .map(Value::Number)
                        .ok_or_else(|| {
                            invalid("Out of range float values are not JSON compliant")
                        })?,
                    ValueRef::Text(v) => Value::String(std::str::from_utf8(v)?.to_owned()),
                    ValueRef::Blob(_) => {
                        return Err(invalid("Object of type bytes is not JSON serializable"));
                    }
                };
                item.insert(name.clone(), value);
            }
            result.push(item);
        }
        Ok(result)
    }

    pub fn one(&self, sql: &str, args: &[rusqlite::types::Value]) -> Result<Row> {
        self.rows(sql, args)?.into_iter().next().ok_or_else(|| {
            crate::store::coded("not_found", "record not found", serde_json::json!({}))
        })
    }

    pub fn count(&self, sql: &str, args: &[rusqlite::types::Value]) -> Result<u64> {
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM ({sql}) AS listed"),
            rusqlite::params_from_iter(args.iter()),
            |r| r.get(0),
        )?;
        Ok(total as u64)
    }

    /// Port of `Ledger.events`, optionally filtered and paged in SQL.
    pub fn events(
        &self,
        exec_id: &str,
        after_seq: i64,
        window: Option<(u64, u64)>,
    ) -> Result<Vec<Value>> {
        let mut sql = String::from(
            "SELECT kind,payload_json,seq,timestamp FROM events WHERE exec_id=? AND seq>? ORDER BY seq",
        );
        let mut args = vec![exec_id.to_owned().into(), after_seq.into()];
        if let Some((size, offset)) = window {
            sql.push_str(" LIMIT ? OFFSET ?");
            args.push((size as i64).into());
            args.push((offset as i64).into());
        }
        self.rows(&sql, &args)?
            .into_iter()
            .map(|row| {
                let mut event = Map::new();
                event.insert("kind".into(), row["kind"].clone());
                let payload: Value =
                    serde_json::from_str(row["payload_json"].as_str().unwrap_or(""))
                        .map_err(|e| invalid(e.to_string()))?;
                let Value::Object(payload) = payload else {
                    return Err(invalid("event payload must be an object"));
                };
                event.extend(payload);
                event.insert("seq".into(), row["seq"].clone());
                event.insert("timestamp".into(), row["timestamp"].clone());
                Ok(Value::Object(event))
            })
            .collect()
    }
}

/// Python truthiness for JSON values.
pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// Port of `kernelledger.ledger.fold`.
pub fn fold(events: Vec<Value>) -> Vec<Value> {
    let mut visible: Vec<Value> = Vec::new();
    let mut pending_clear = false;
    for event in events {
        let kind = event["kind"].as_str().unwrap_or("").to_owned();
        if kind == "clear" {
            if truthy(event.get("wait")) {
                pending_clear = true;
            } else {
                visible.clear();
                pending_clear = false;
            }
        } else if matches!(
            kind.as_str(),
            "stdout" | "stderr" | "display" | "result" | "error" | "truncated" | "display_update"
        ) {
            if pending_clear {
                visible.clear();
                pending_clear = false;
            }
            if kind == "display_update" {
                let display_id = event.get("display_id").filter(|v| !v.is_null()).cloned();
                if let Some(id) = display_id
                    && let Some(slot) = visible
                        .iter_mut()
                        .rev()
                        .find(|item| item.get("display_id") == Some(&id))
                {
                    let mut replaced = event.clone();
                    replaced["kind"] = Value::from("display");
                    *slot = replaced;
                }
            } else {
                visible.push(event);
            }
        }
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fold_matches_python_semantics() {
        let events = vec![
            json!({"kind":"stdout","text":"a"}),
            json!({"kind":"display","display_id":"d","v":1}),
            json!({"kind":"clear","wait":true}),
            json!({"kind":"status"}),
            json!({"kind":"stdout","text":"b"}),
            json!({"kind":"display","display_id":"d","v":2}),
            json!({"kind":"display_update","display_id":"d","v":3}),
            json!({"kind":"display_update","display_id":null,"v":4}),
        ];
        let folded = fold(events);
        assert_eq!(folded.len(), 2);
        assert_eq!(folded[0]["text"], "b");
        assert_eq!(folded[1], json!({"kind":"display","display_id":"d","v":3}));
        assert!(fold(vec![json!({"kind":"stdout"}), json!({"kind":"clear"})]).is_empty());
    }

    #[test]
    fn uri_escaping() {
        let uri = file_uri(Path::new("/tmp/a b/c#?%.sqlite"));
        assert_eq!(uri, "file:///tmp/a%20b/c%23%3F%25.sqlite");
    }
}
