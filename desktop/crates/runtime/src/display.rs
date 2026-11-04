//! Read-only display replay. Limits match kernelledger.display.
use crate::ledger::Ledger;
use anyhow::Result;
use serde_json::{Map, Value, json};

fn bounded_json(raw: &str) -> Option<Value> {
    let bytes = raw.as_bytes();
    let (mut at, mut depth, mut quoted, mut escaped) = (0, 0i32, false, false);
    while at < bytes.len() {
        let c = bytes[at];
        if quoted {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                quoted = false;
            }
        } else if c == b'"' {
            quoted = true;
        } else if matches!(c, b'[' | b'{') {
            depth += 1;
            if depth > 64 {
                return None;
            }
        } else if matches!(c, b']' | b'}') {
            depth -= 1;
        } else if c == b'-' || c.is_ascii_digit() {
            let start = at;
            while at < bytes.len()
                && matches!(bytes[at], b'0'..=b'9' | b'-' | b'+' | b'e' | b'E' | b'.')
            {
                at += 1;
            }
            let number = &raw[start..at];
            if !number.contains(['.', 'e', 'E'])
                && number.parse::<i64>().is_err()
                && number.parse::<u64>().is_err()
            {
                return None;
            }
            continue;
        }
        at += 1;
    }
    serde_json::from_str(raw).ok()
}

pub fn updates(ledger: &Ledger, request: &Value) -> Result<Value> {
    let mut after = request["after_rowid"].as_i64().unwrap();
    let through = request["through_rowid"].as_i64().unwrap();
    let mut sql = String::from(
        "SELECT ev.rowid AS event_rowid,ev.kind='display_update' AS is_update,\
         CASE WHEN length(CAST(ev.exec_id AS BLOB))<=128 THEN ev.exec_id END AS exec_id,\
         CASE WHEN typeof(ev.seq)='integer' AND ev.seq>0 THEN ev.seq END AS seq,\
         CASE WHEN length(CAST(ev.timestamp AS BLOB))<=128 THEN ev.timestamp END AS timestamp,\
         CASE WHEN length(CAST(x.session_id AS BLOB))<=128 THEN x.session_id END AS session_id,\
         CASE WHEN typeof(x.ordinal)='integer' AND x.ordinal>0 THEN x.ordinal END AS ordinal,\
         CASE WHEN ev.kind='display_update' THEN length(CAST(ev.payload_json AS BLOB)) ELSE 0 END AS payload_bytes,\
         CASE WHEN ev.kind='display_update' AND length(CAST(ev.payload_json AS BLOB))<=65536 \
         THEN ev.payload_json END AS payload_json \
         FROM events ev JOIN executions x ON x.exec_id=ev.exec_id \
         WHERE ev.rowid>? AND ev.rowid<=?",
    );
    let mut args = vec![after.into(), through.into()];
    if let Some(session) = request["session_id"].as_str().filter(|s| !s.is_empty()) {
        sql.push_str(" AND x.session_id=?");
        args.push(session.to_owned().into());
    }
    sql.push_str(" ORDER BY ev.rowid LIMIT 128");
    let rows = ledger.rows(&sql, &args)?;
    let mut items = Vec::new();
    let mut used = 0;
    let mut complete = true;
    for row in &rows {
        if row["is_update"] == 1 {
            let cost = row["payload_bytes"].as_u64().unwrap_or(0).min(65536) + 4096;
            if items.len() >= 16 || used + cost > 262144 {
                complete = false;
                break;
            }
            let mut item = Map::new();
            for key in [
                "event_rowid",
                "exec_id",
                "session_id",
                "ordinal",
                "seq",
                "timestamp",
            ] {
                item.insert(key.into(), row[key].clone());
            }
            if ["exec_id", "session_id", "timestamp"]
                .iter()
                .any(|key| !row[*key].as_str().is_some_and(|s| !s.is_empty()))
                || ["ordinal", "seq"]
                    .iter()
                    .any(|key| row[*key].as_i64().is_none())
            {
                item.insert("omitted".into(), json!("invalid_metadata"));
            } else if let Some(payload) = row["payload_json"].as_str() {
                match bounded_json(payload) {
                    Some(Value::Object(mut event)) => {
                        if event
                            .get("display_id")
                            .and_then(Value::as_str)
                            .is_some_and(|id| {
                                !id.is_empty()
                                    && id.len() <= 256
                                    && id.chars().all(|c| c as u32 >= 32)
                            })
                        {
                            event.insert("kind".into(), json!("display_update"));
                            item.insert("event".into(), Value::Object(event));
                        } else {
                            item.insert("omitted".into(), json!("invalid_display_id"));
                        }
                    }
                    _ => {
                        item.insert("omitted".into(), json!("invalid_payload"));
                    }
                }
            } else {
                item.insert("omitted".into(), json!("oversized_payload"));
            }
            items.push(Value::Object(item));
            used += cost;
        }
        after = row["event_rowid"].as_i64().unwrap();
    }
    if complete && rows.len() < 128 {
        after = through;
    }
    Ok(json!({"items":items,"after_rowid":after,"through_rowid":through,"more":after < through}))
}
