//! Recently opened notebooks.
//!
//! `recent.json` keeps its original format, a JSON list of paths, so older
//! builds keep reading and writing it. Last-opened times live beside it in
//! `recent-opened.json`; a path without a recorded time has no known time.
//! Filesystem modification times are never used as opening times.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const LIMIT: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub struct RecentNotebook {
    pub path: PathBuf,
    /// RFC 3339 time this build last opened the notebook, when recorded.
    pub opened: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct OpenedTimes {
    schema: u32,
    opened: BTreeMap<String, String>,
}

fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn load(profile: &Path) -> Vec<RecentNotebook> {
    let paths: Vec<PathBuf> = std::fs::read(profile.join("recent.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let times: OpenedTimes = std::fs::read(profile.join("recent-opened.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    paths
        .into_iter()
        .map(|path| RecentNotebook {
            opened: times.opened.get(&key(&path)).cloned(),
            path,
        })
        .collect()
}

/// Move `path` to the front with its new opening time, keeping at most `LIMIT`.
pub fn record(list: &mut Vec<RecentNotebook>, path: PathBuf, opened: String) {
    list.retain(|item| item.path != path);
    list.insert(
        0,
        RecentNotebook {
            path,
            opened: Some(opened),
        },
    );
    list.truncate(LIMIT);
}

pub fn remove(list: &mut Vec<RecentNotebook>, path: &Path) {
    list.retain(|item| item.path != path);
}

pub fn save(profile: &Path, list: &[RecentNotebook]) -> anyhow::Result<()> {
    std::fs::create_dir_all(profile)?;
    let paths: Vec<&PathBuf> = list.iter().map(|item| &item.path).collect();
    let times = OpenedTimes {
        schema: 1,
        opened: list
            .iter()
            .filter_map(|item| Some((key(&item.path), item.opened.clone()?)))
            .collect(),
    };
    for (name, bytes) in [
        ("recent.json", serde_json::to_vec(&paths)?),
        ("recent-opened.json", serde_json::to_vec_pretty(&times)?),
    ] {
        let temp = profile.join(format!("{name}.tmp"));
        std::fs::write(&temp, bytes)?;
        std::fs::rename(temp, profile.join(name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> PathBuf {
        let directory = std::env::temp_dir().join(format!("kl-recent-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        directory
    }

    #[test]
    fn path_only_recents_survive_with_unknown_times_and_keep_their_format() {
        let directory = profile();
        let old = vec![
            PathBuf::from("C:/work/notebook"),
            PathBuf::from("D:/archive/notebook"),
        ];
        std::fs::write(
            directory.join("recent.json"),
            serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        let mut list = load(&directory);
        assert_eq!(list.iter().map(|i| i.path.clone()).collect::<Vec<_>>(), old);
        assert!(list.iter().all(|item| item.opened.is_none()));

        record(
            &mut list,
            old[1].clone(),
            "2026-11-01T10:00:00+05:30".into(),
        );
        save(&directory, &list).unwrap();
        let paths: Vec<PathBuf> =
            serde_json::from_slice(&std::fs::read(directory.join("recent.json")).unwrap()).unwrap();
        assert_eq!(paths, vec![old[1].clone(), old[0].clone()]);
        let reloaded = load(&directory);
        assert_eq!(
            reloaded[0].opened.as_deref(),
            Some("2026-11-01T10:00:00+05:30")
        );
        assert_eq!(reloaded[1].opened, None);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn record_moves_to_front_and_limits_without_duplicates() {
        let mut list = (0..LIMIT)
            .map(|i| RecentNotebook {
                path: PathBuf::from(format!("n{i}")),
                opened: None,
            })
            .collect::<Vec<_>>();
        record(&mut list, PathBuf::from("n5"), "t".into());
        assert_eq!(list.len(), LIMIT);
        assert_eq!(list[0].path, PathBuf::from("n5"));
        assert_eq!(list.iter().filter(|i| i.path == Path::new("n5")).count(), 1);
        record(&mut list, PathBuf::from("new"), "t".into());
        assert_eq!(list.len(), LIMIT);
        assert_eq!(list[0].path, PathBuf::from("new"));
        remove(&mut list, Path::new("new"));
        assert_eq!(list[0].path, PathBuf::from("n5"));
    }

    #[test]
    fn unreadable_time_file_keeps_every_path() {
        let directory = profile();
        std::fs::write(directory.join("recent.json"), br#"["a","b"]"#).unwrap();
        std::fs::write(directory.join("recent-opened.json"), b"{not json").unwrap();
        let list = load(&directory);
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|item| item.opened.is_none()));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
