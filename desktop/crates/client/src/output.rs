//! Safe native output preparation. No HTML engine, SVG or network resources.
use super::*;
use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::io::Cursor;

pub const VIEW_TEXT_LIMIT: usize = 65_536;

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
pub fn resolve_event(
    event: &mut Value,
    mut read: impl FnMut(&str) -> Result<Vec<u8>>,
) -> Result<()> {
    if let Some(hash) = event["blob"].as_str() {
        let bytes = read(hash)?;
        event["text"] = json!(bounded(&String::from_utf8(bytes)?, VIEW_TEXT_LIMIT));
    }
    if let Some(bundle) = event["bundle"].as_object_mut() {
        for (mime, value) in bundle {
            if let Some(hash) = value["blob"].as_str().map(str::to_owned) {
                let bytes = read(&hash)?;
                if matches!(mime.as_str(), "image/png" | "image/jpeg") {
                    ensure!(
                        bytes.len() <= 32 * 1024 * 1024,
                        "Image exceeds 32 MiB viewer limit"
                    );
                    let reader =
                        image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
                    ensure!(
                        matches!(
                            reader.format(),
                            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
                        ),
                        "Unsupported image format"
                    );
                    let (width, height) = reader.into_dimensions()?;
                    ensure!(
                        u64::from(width) * u64::from(height) <= 16_000_000,
                        "Image exceeds 16 megapixel viewer limit"
                    );
                    let path = profile_path().join("image-cache").join(&hash);
                    fs::create_dir_all(path.parent().unwrap())?;
                    if !path.exists() {
                        fs::write(&path, &bytes)?;
                    }
                    value["local_path"] = json!(path);
                } else if value["encoding"] != "binary" {
                    let text = String::from_utf8(bytes)?;
                    value["value"] = if value["encoding"] == "json" {
                        serde_json::from_str(&text)?
                    } else {
                        json!(bounded(&text, 8 * 1024 * 1024))
                    };
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
}
