//! Explicit verified output export. Browsing a media node reads no blob bytes.
use crate::output::BlobPage;
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{io::Write, path::Path};

pub const BYTE_LIMIT: u64 = 32 * 1024 * 1024;
const CHUNK: usize = 65536;
static TRANSFERS: AtomicUsize = AtomicUsize::new(0);

pub struct Transfer;
impl Drop for Transfer {
    fn drop(&mut self) {
        TRANSFERS.fetch_sub(1, Ordering::AcqRel);
    }
}
pub fn reserve_transfer() -> Result<Transfer> {
    TRANSFERS
        .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < 2).then_some(n + 1)
        })
        .map_err(|_| {
            anyhow::anyhow!("Two output exports are already active; wait for one to finish")
        })?;
    Ok(Transfer)
}

pub fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn extension(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "audio/wav" => "wav",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        _ => return None,
    })
}

pub fn save(
    path: &Path,
    digest: &str,
    mime: &str,
    mut read: impl FnMut(&str, u64, usize) -> Result<BlobPage>,
) -> Result<u64> {
    ensure!(extension(mime).is_some(), "Unsupported media output type");
    ensure!(valid_digest(digest), "Invalid output digest");
    let expected = extension(mime).unwrap();
    ensure!(
        path.extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case(expected)
                || (mime == "image/jpeg" && s.eq_ignore_ascii_case("jpeg"))),
        "Use a .{expected} filename for this output"
    );
    ensure!(
        !path.exists(),
        "Choose a new filename; existing files are never overwritten"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut offset = 0;
    let mut total = None;
    let mut hasher = Sha256::new();
    loop {
        let page = read(digest, offset, CHUNK)?;
        ensure!(
            page.offset == offset
                && page.bytes.len() <= CHUNK
                && offset + page.bytes.len() as u64 <= page.total,
            "Invalid output page"
        );
        ensure!(page.total <= BYTE_LIMIT, "Media output exceeds 32 MiB");
        ensure!(
            total.is_none_or(|value| value == page.total),
            "Output size changed during export"
        );
        total = Some(page.total);
        ensure!(
            !page.bytes.is_empty() || offset == page.total,
            "Empty output page before completion"
        );
        hasher.update(&page.bytes);
        temporary.write_all(&page.bytes)?;
        offset += page.bytes.len() as u64;
        if offset == page.total {
            break;
        }
    }
    ensure!(
        format!("{:x}", hasher.finalize()) == digest,
        "Output integrity check failed"
    );
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(path)?;
    Ok(offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash;
    #[test]
    fn active_exports_are_bounded_and_slots_are_released() {
        let first = reserve_transfer().unwrap();
        let second = reserve_transfer().unwrap();
        assert!(reserve_transfer().is_err());
        drop(first);
        let third = reserve_transfer().unwrap();
        drop((second, third));
    }
    #[test]
    fn export_verifies_every_page_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("sound.wav");
        let bytes = vec![42; CHUNK * 2 + 7];
        let digest = hash(&bytes);
        let mut offsets = Vec::new();
        let size = save(&target, &digest, "audio/wav", |_, offset, length| {
            offsets.push(offset);
            Ok(BlobPage {
                offset,
                total: bytes.len() as u64,
                bytes: bytes[offset as usize..(offset as usize + length).min(bytes.len())].to_vec(),
            })
        })
        .unwrap();
        assert_eq!(size, bytes.len() as u64);
        assert_eq!(offsets, vec![0, CHUNK as u64, 2 * CHUNK as u64]);
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
        assert!(
            save(&target, &digest, "audio/wav", |_, _, _| panic!(
                "Existing output must not be read"
            ))
            .is_err()
        );
    }
    #[test]
    fn corrupt_oversized_or_inconsistent_outputs_publish_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("movie.mp4");
        let digest = hash(b"expected");
        assert!(
            save(&target, &digest, "video/mp4", |_, offset, _| Ok(BlobPage {
                offset,
                total: 3,
                bytes: b"bad".to_vec()
            }))
            .is_err()
        );
        assert!(!target.exists());
        assert!(
            save(&target, &digest, "video/mp4", |_, offset, _| Ok(BlobPage {
                offset,
                total: BYTE_LIMIT + 1,
                bytes: vec![]
            }))
            .is_err()
        );
        assert!(!target.exists());
        assert!(
            save(&target, &digest, "text/html", |_, _, _| panic!(
                "Unsupported content must not be read"
            ))
            .is_err()
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
