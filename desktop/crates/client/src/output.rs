//! Safe native output preparation. No HTML engine, SVG or network resources.
use super::*;
use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io::Cursor, sync::Mutex};

pub const VIEW_TEXT_LIMIT: usize = 65_536;
pub const MAX_PAGE_BYTES: usize = 262_144;
pub const IMAGE_CACHE_BUDGET: u64 = 64 * 1024 * 1024;
const IMAGE_BYTE_LIMIT: usize = 32 * 1024 * 1024;
const IMAGE_PIXEL_LIMIT: u64 = 16_000_000;

static CACHE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobPage {
    pub bytes: Vec<u8>,
    pub offset: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CacheRecord {
    hash: String,
    bytes: u64,
    used: u64,
}

fn text(node: &Handle, result: &mut String) {
    if result.len() > 4096 {
        return;
    }
    if let NodeData::Element { name, .. } = &node.data
        && matches!(
            name.local.as_ref(),
            "script" | "style" | "iframe" | "object" | "embed" | "svg"
        )
    {
        return;
    }
    if let NodeData::Text { contents } = &node.data {
        result.push_str(&contents.borrow());
    }
    for child in node.children.borrow().iter() {
        text(child, result);
    }
}
fn rows(node: &Handle, result: &mut Vec<Vec<String>>) {
    if result.len() >= 1000 {
        return;
    }
    if let NodeData::Element { name, .. } = &node.data {
        if matches!(
            name.local.as_ref(),
            "script" | "style" | "iframe" | "object" | "embed" | "svg"
        ) {
            return;
        }
        if name.local.as_ref() == "tr" {
            let mut row = Vec::new();
            for child in node.children.borrow().iter() {
                if let NodeData::Element { name, .. } = &child.data
                    && matches!(name.local.as_ref(), "td" | "th")
                    && row.len() < 100
                {
                    let mut value = String::new();
                    text(child, &mut value);
                    row.push(bounded(&value, 4096));
                }
            }
            if !row.is_empty() {
                result.push(row);
            }
            return;
        }
    }
    for child in node.children.borrow().iter() {
        rows(child, result);
    }
}
pub fn table(html: &str) -> Result<Vec<Vec<String>>> {
    ensure!(
        html.len() <= 8 * 1024 * 1024,
        "HTML table exceeds viewer limit"
    );
    let dom = parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    let mut result = Vec::new();
    rows(&dom.document, &mut result);
    ensure!(
        !result.is_empty(),
        "No supported HTML table; use text fallback"
    );
    Ok(result)
}
pub fn bounded(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n… viewer limit reached; recorded output remains in history",
        &text[..end]
    )
}
pub fn safe_markdown(source: &str) -> String {
    // Literal source fallback for image-bearing Markdown prevents implicit network/file loads.
    // Text, lists, code fences and simple tables still use the native Markdown renderer.
    let source = bounded(source, VIEW_TEXT_LIMIT)
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    if source.contains("![") {
        format!("```text\n{}\n```", source.replace("```", "~~~"))
    } else {
        source
    }
}

/// Align a byte window to UTF-8 character boundaries.
/// The returned end is the next offset a following page should request.
pub fn text_window(bytes: &[u8], offset: u64, total: u64) -> (String, u64, u64) {
    if bytes.is_empty() {
        let at = offset.min(total);
        return (String::new(), at, at);
    }
    let mut start = 0;
    while start < bytes.len() && bytes[start] & 0b1100_0000 == 0b1000_0000 {
        start += 1;
    }
    let mut end = bytes.len();
    if end > start {
        let from = end.saturating_sub(4).max(start);
        let lead = (from..end)
            .rev()
            .find(|index| bytes[*index] & 0b1100_0000 != 0b1000_0000);
        if let Some(lead) = lead {
            let width = match bytes[lead] {
                0x00..=0x7F => 1,
                0xC2..=0xDF => 2,
                0xE0..=0xEF => 3,
                0xF0..=0xF4 => 4,
                _ => 1,
            };
            if lead + width > end {
                end = lead;
            }
        } else {
            end = start;
        }
    }
    let text = String::from_utf8_lossy(&bytes[start..end]).into_owned();
    let aligned = offset + start as u64;
    let next = (offset + end as u64).min(total);
    (text, aligned.min(total), next)
}

pub fn write_page(event: &mut Value, slot: &str, text: &str, offset: u64, end: u64, total: u64) {
    let page = json!({"offset": offset, "end": end, "total": total});
    if slot.is_empty() {
        event["text"] = json!(text);
        event["page"] = page;
        return;
    }
    if event["bundle"][slot].is_object() {
        event["bundle"][slot]["value"] = json!(text);
        event["bundle"][slot]["page"] = page;
    }
}

pub fn validate_attachment(cell_id: &str, name: &str) -> Result<()> {
    ensure!(
        !cell_id.is_empty()
            && cell_id.len() <= 64
            && cell_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && !matches!(
                cell_id.to_ascii_uppercase().as_str(),
                "CON" | "PRN" | "AUX" | "NUL"
            ),
        "Invalid attachment cell"
    );
    ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && name.len() <= 180
            && !name.contains(['/', '\\', '\0', ':'])
            && name.trim_end_matches(['.', ' ']) == name,
        "Invalid attachment name"
    );
    Ok(())
}

fn content_hash(hash: &str) -> Result<()> {
    ensure!(
        hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()),
        "Invalid image cache key"
    );
    Ok(())
}

fn cache_manifest(dir: &Path) -> PathBuf {
    dir.join("manifest.json")
}

fn read_manifest(dir: &Path) -> Vec<CacheRecord> {
    let Ok(bytes) = fs::read(cache_manifest(dir)) else {
        return Vec::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn write_manifest(dir: &Path, records: &[CacheRecord]) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(cache_manifest(dir), serde_json::to_vec(records)?)?;
    Ok(())
}

/// Drop the least-recently used records until `budget` bytes remain.
/// Records absent from `visible` are dropped before records that are on screen.
fn choose_evictions(
    records: &[CacheRecord],
    visible: &BTreeSet<String>,
    budget: u64,
) -> Vec<String> {
    let mut ranked: Vec<&CacheRecord> = records.iter().collect();
    ranked.sort_by(|left, right| {
        visible
            .contains(&left.hash)
            .cmp(&visible.contains(&right.hash))
            .then(left.used.cmp(&right.used))
            .then(left.hash.cmp(&right.hash))
    });
    let mut kept = records.iter().map(|record| record.bytes).sum::<u64>();
    let mut dropped = Vec::new();
    for record in ranked {
        if kept <= budget {
            break;
        }
        dropped.push(record.hash.clone());
        kept = kept.saturating_sub(record.bytes);
    }
    dropped
}

pub fn store_cached_image(dir: &Path, hash: &str, bytes: &[u8], budget: u64) -> Result<PathBuf> {
    content_hash(hash)?;
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    fs::create_dir_all(dir)?;
    let path = dir.join(hash);
    if !path.exists() {
        fs::write(&path, bytes)?;
    }
    let mut records = read_manifest(dir);
    records.retain(|record| record.hash != hash && dir.join(&record.hash).is_file());
    let used = records.iter().map(|record| record.used).max().unwrap_or(0) + 1;
    records.push(CacheRecord {
        hash: hash.to_owned(),
        bytes: bytes.len() as u64,
        used,
    });
    let visible = BTreeSet::from([hash.to_owned()]);
    for hash in choose_evictions(&records, &visible, budget) {
        let _ = fs::remove_file(dir.join(&hash));
        records.retain(|record| record.hash != hash);
    }
    write_manifest(dir, &records)?;
    ensure!(path.is_file(), "Image was evicted before it could be shown");
    Ok(path)
}

pub fn touch_cached_image(dir: &Path, hash: &str) {
    if content_hash(hash).is_err() {
        return;
    }
    let _guard = CACHE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let mut records = read_manifest(dir);
    let next = records.iter().map(|record| record.used).max().unwrap_or(0) + 1;
    let Some(record) = records.iter_mut().find(|record| record.hash == hash) else {
        return;
    };
    record.used = next;
    let _ = write_manifest(dir, &records);
}

pub fn prepare_image(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() <= IMAGE_BYTE_LIMIT,
        "Image exceeds 32 MiB viewer limit"
    );
    let reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    ensure!(
        matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
        ),
        "Unsupported image format"
    );
    let (width, height) = reader.into_dimensions()?;
    ensure!(
        u64::from(width) * u64::from(height) <= IMAGE_PIXEL_LIMIT,
        "Image exceeds 16 megapixel viewer limit"
    );
    Ok(())
}

pub fn read_limited(
    mut read: impl FnMut(&str, u64, usize) -> Result<BlobPage>,
    digest: &str,
    limit: usize,
) -> Result<Vec<u8>> {
    let mut all = Vec::new();
    loop {
        if all.len() >= limit {
            break;
        }
        let want = (limit - all.len()).min(FRAME_LIMIT);
        let page = read(digest, all.len() as u64, want)?;
        if page.bytes.is_empty() {
            break;
        }
        let room = limit - all.len();
        let take = page.bytes.len().min(room);
        all.extend_from_slice(&page.bytes[..take]);
        if all.len() as u64 >= page.total || take < page.bytes.len() {
            ensure!(
                page.total <= limit as u64,
                "Image exceeds 32 MiB viewer limit"
            );
            break;
        }
    }
    Ok(all)
}

fn stamp_text(slot: &mut Value, page: &BlobPage) {
    let (text, start, end) = text_window(&page.bytes, page.offset, page.total);
    slot["value"] = json!(text);
    slot["page"] = json!({"offset": start, "end": end, "total": page.total});
}

pub fn resolve_event(
    event: &mut Value,
    mut read: impl FnMut(&str, u64, usize) -> Result<BlobPage>,
    cache_dir: &Path,
) -> Result<()> {
    if let Some(hash) = event["blob"].as_str().map(str::to_owned) {
        let page = read(&hash, 0, VIEW_TEXT_LIMIT)?;
        let (text, start, end) = text_window(&page.bytes, page.offset, page.total);
        event["text"] = json!(text);
        event["page"] = json!({"offset": start, "end": end, "total": page.total});
    }
    if let Some(bundle) = event["bundle"].as_object_mut() {
        for (mime, value) in bundle {
            if let Some(hash) = value["blob"].as_str().map(str::to_owned) {
                if matches!(mime.as_str(), "image/png" | "image/jpeg") {
                    let bytes = read_limited(&mut read, &hash, IMAGE_BYTE_LIMIT)?;
                    prepare_image(&bytes)?;
                    let path = store_cached_image(cache_dir, &hash, &bytes, IMAGE_CACHE_BUDGET)?;
                    value["local_path"] = json!(path);
                } else if value["encoding"] != "binary" {
                    let page = read(&hash, 0, VIEW_TEXT_LIMIT)?;
                    if value["encoding"] == "json"
                        && page.offset == 0
                        && page.bytes.len() as u64 >= page.total
                    {
                        let text = String::from_utf8(page.bytes.clone())?;
                        value["value"] = serde_json::from_str(&text)?;
                    } else {
                        stamp_text(value, &page);
                    }
                }
            }
            if mime == "text/html"
                && let Some(html) = value["value"].as_str()
                && let Ok(rows) = table(html)
            {
                value["rows"] = json!(rows);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table_discards_active_content() {
        let rows = table("<table><tr><th>Name</th><td onclick='evil()'>Safe<script>evil()</script><img src='https://evil'></td></tr></table>").unwrap();
        assert_eq!(rows, vec![vec!["Name", "Safe"]]);
    }
    #[test]
    fn truncation_is_unicode_safe() {
        assert!(bounded("ééé", 3).starts_with("é\n"));
    }
    #[test]
    fn text_window_keeps_character_boundaries() {
        let bytes = "éé".as_bytes();
        let (text, start, end) = text_window(&bytes[1..], 1, bytes.len() as u64);
        assert_eq!(text, "é");
        assert_eq!((start, end), (2, 4));
    }
    #[test]
    fn text_blob_reads_one_page_of_a_large_stream() {
        let mut calls = Vec::new();
        let mut event = json!({"kind":"stdout","blob":"abc"});
        let total = 100 * 1024 * 1024;
        resolve_event(
            &mut event,
            |digest, offset, length| {
                calls.push((digest.to_string(), offset, length));
                Ok(BlobPage {
                    bytes: vec![b'a'; length],
                    offset,
                    total,
                })
            },
            Path::new("unused-cache"),
        )
        .unwrap();
        assert_eq!(calls, vec![("abc".into(), 0, VIEW_TEXT_LIMIT)]);
        assert_eq!(event["text"].as_str().unwrap().len(), VIEW_TEXT_LIMIT);
        assert_eq!(event["page"]["total"].as_u64().unwrap(), total);
        assert_eq!(
            event["page"]["end"].as_u64().unwrap(),
            VIEW_TEXT_LIMIT as u64
        );
    }
    #[test]
    fn image_cache_drops_offscreen_records_before_visible_ones() {
        let records = vec![
            CacheRecord {
                hash: "a".repeat(64),
                bytes: 40,
                used: 1,
            },
            CacheRecord {
                hash: "b".repeat(64),
                bytes: 40,
                used: 2,
            },
        ];
        let visible = BTreeSet::from(["b".repeat(64)]);
        assert_eq!(
            choose_evictions(&records, &visible, 50),
            vec!["a".repeat(64)]
        );
        assert!(choose_evictions(&records, &visible, 80).is_empty());
    }
    #[test]
    fn stored_images_stay_inside_the_cache_budget() {
        let dir = std::env::temp_dir().join(format!("kernelledger-cache-{}", Uuid::new_v4()));
        let _ = fs::remove_dir_all(&dir);
        let first = "a".repeat(64);
        let second = "b".repeat(64);
        let third = "c".repeat(64);
        store_cached_image(&dir, &first, &[1; 30], 70).unwrap();
        store_cached_image(&dir, &second, &[2; 30], 70).unwrap();
        touch_cached_image(&dir, &second);
        store_cached_image(&dir, &third, &[3; 30], 70).unwrap();
        assert!(!dir.join(&first).exists());
        assert!(dir.join(&second).exists());
        assert!(dir.join(&third).exists());
        let manifest = read_manifest(&dir);
        let bytes: u64 = manifest.iter().map(|record| record.bytes).sum();
        assert!(bytes <= 70);
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn attachment_names_reject_path_separators() {
        assert!(validate_attachment("cell", "notes.txt").is_ok());
        assert!(validate_attachment("cell", "../notes.txt").is_err());
        assert!(validate_attachment("../cell", "notes.txt").is_err());
    }
}
