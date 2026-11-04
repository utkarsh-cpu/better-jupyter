//! Immutable source locations never resolve an arbitrary filesystem path.
use anyhow::{Result, ensure};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub cell_id: Option<String>,
    pub exec_id: String,
    pub file: String,
    pub revision: String,
    pub line: u64,
}

impl Location {
    pub fn parse(file: &str, revision: &str, line: u64) -> Result<Self> {
        ensure!((1..=100000).contains(&line), "Source line is out of bounds");
        ensure!(
            crate::media::valid_digest(revision),
            "Recorded source revision is unavailable"
        );
        let (name, execution) = file
            .rsplit_once('@')
            .ok_or_else(|| anyhow::anyhow!("Source has no recorded execution"))?;
        let exec_id = uuid::Uuid::parse_str(execution)?.to_string();
        let cell_id = if name == "<unattributed>" {
            None
        } else {
            let id = name
                .strip_prefix("cells/")
                .and_then(|value| value.strip_suffix(".py"))
                .ok_or_else(|| {
                    anyhow::anyhow!("External/library source is not a recorded notebook cell")
                })?;
            ensure!(
                !id.is_empty()
                    && id.len() <= 64
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
                "Invalid source cell identifier"
            );
            Some(id.to_owned())
        };
        Ok(Self {
            cell_id,
            exec_id,
            file: file.to_owned(),
            revision: revision.to_owned(),
            line,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_locations_accept_only_recorded_notebook_filenames() {
        let revision = "a".repeat(64);
        let execution = uuid::Uuid::new_v4();
        assert_eq!(
            Location::parse(&format!("cells/cell.py@{execution}"), &revision, 2)
                .unwrap()
                .cell_id
                .as_deref(),
            Some("cell")
        );
        assert!(
            Location::parse(&format!("<unattributed>@{execution}"), &revision, 1)
                .unwrap()
                .cell_id
                .is_none()
        );
        for file in [
            format!("cells/../../secret.py@{execution}"),
            format!("C:/secret.py@{execution}"),
            "cells/cell.py@not-an-id".into(),
        ] {
            assert!(Location::parse(&file, &revision, 1).is_err());
        }
        assert!(Location::parse(&format!("cells/cell.py@{execution}"), &revision, 0).is_err());
    }
}
