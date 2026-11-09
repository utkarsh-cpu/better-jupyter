//! Opt-in sandboxed HTML+JS cell output. Native tables and PNG remain the default.
use super::*;
use gpui_webview::WebView;
use wry::{NewWindowResponse, WebViewBuilder};

pub(super) const HOST_HEIGHT: f32 = 400.;
pub(super) const MAX_LIVE_VIEWS: usize = 4;

pub(super) fn navigation_allowed(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("about:blank") || lower.starts_with("data:")
}

fn sandboxed_builder() -> WebViewBuilder<'static> {
    WebViewBuilder::new()
        .with_incognito(true)
        .with_autoplay(false)
        .with_hotkeys_zoom(false)
        .with_focused(false)
        .with_visible(false)
        .with_navigation_handler(|url| navigation_allowed(&url))
        .with_download_started_handler(|_, _| false)
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
}

fn document_html(html: &str) -> String {
    let prefix = html
        .trim_start()
        .get(..15)
        .unwrap_or(html.trim_start())
        .to_ascii_lowercase();
    if prefix.starts_with("<!doctype") || prefix.starts_with("<html") {
        html.to_owned()
    } else {
        format!(
            "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta name=\"color-scheme\" content=\"light dark\"></head><body>{html}</body></html>"
        )
    }
}

fn build_error_message(error: &impl std::fmt::Display) -> String {
    let text = error.to_string();
    if text.to_ascii_lowercase().contains("wayland") {
        "Interactive HTML needs X11 or XWayland on Linux.".into()
    } else {
        format!("Interactive HTML could not start: {text}")
    }
}

pub(super) fn host(view: Entity<WebView>, blocked: bool) -> AnyElement {
    let geometry = view.clone();
    div()
        .h(px(HOST_HEIGHT))
        .w_full()
        .min_h(px(HOST_HEIGHT))
        .occlude()
        .child(
            canvas(
                move |bounds, window, cx| {
                    let clip = window.content_mask().bounds;
                    geometry.update(cx, |wv, _| {
                        if blocked || bounds.intersect(&clip).is_empty() {
                            let _ = wv.set_visible(false);
                            return;
                        }
                        let _ = wv.set_visible(true);
                        let _ = wv.set_geometry(bounds, clip);
                    });
                },
                |_, _, _, _| {},
            )
            .size_full(),
        )
        .into_any_element()
}

pub(super) fn prune_tab(tab: &mut Tab) {
    let drop_views: Vec<String> = tab
        .html_views
        .keys()
        .filter(|key| notebook_key_current(key, tab) == Some(false))
        .cloned()
        .collect();
    for key in drop_views {
        tab.html_views.remove(&key);
        tab.html_notes.remove(&key);
    }
    let drop_notes: Vec<String> = tab
        .html_notes
        .keys()
        .filter(|key| notebook_key_current(key, tab) == Some(false))
        .cloned()
        .collect();
    for key in drop_notes {
        tab.html_notes.remove(&key);
    }
}

fn notebook_key_current(key: &str, tab: &Tab) -> Option<bool> {
    let mut matched: Option<(bool, usize)> = None;
    for (id, cell) in &tab.model.cells {
        let prefix = format!("{id}-");
        if let Some(rest) = key.strip_prefix(&prefix)
            && !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_digit())
        {
            let index = rest.parse::<usize>().ok()?;
            let current = index < cell.output.len();
            matched = Some(match matched {
                Some(previous) if id.len() < previous.1 => previous,
                _ => (current, id.len()),
            });
        }
    }
    matched.map(|(current, _)| current)
}

impl Workspace {
    pub(super) fn html_views_blocked(&self) -> bool {
        self.app_menu
            || self.export_menu
            || self.runtime_menu
            || self.notebook_menu
            || self.cell_menu.is_some()
            || self.insert_menu.is_some()
            || self.palette
            || self.confirmation.is_some()
    }

    pub(super) fn conceal_html_views(&self, cx: &mut App) {
        for tab in &self.tabs {
            for view in tab.html_views.values() {
                view.update(cx, |wv, _| {
                    let _ = wv.set_visible(false);
                });
            }
        }
    }

    fn evict_hidden_html_views(&mut self, cx: &mut App) {
        if self.html_views_blocked() || self.tabs.is_empty() {
            return;
        }
        let hidden: Vec<String> = self.tabs[self.active]
            .html_views
            .iter()
            .filter(|(_, view)| !view.read(cx).visible())
            .map(|(key, _)| key.clone())
            .collect();
        let tab = &mut self.tabs[self.active];
        for key in hidden {
            tab.html_views.remove(&key);
        }
    }

    pub(super) fn html_view(&self, key: &str) -> Option<Entity<WebView>> {
        self.tabs
            .get(self.active)
            .and_then(|tab| tab.html_views.get(key).cloned())
    }

    pub(super) fn html_note(&self, key: &str) -> Option<String> {
        self.tabs
            .get(self.active)
            .and_then(|tab| tab.html_notes.get(key).cloned())
    }

    fn html_slot_open(&self, key: &str) -> bool {
        self.tabs.get(self.active).is_some_and(|tab| {
            tab.html_views.contains_key(key)
                || tab
                    .html_notes
                    .get(key)
                    .is_some_and(|note| note.contains("automated tests"))
        })
    }

    fn html_slot_count(&self) -> usize {
        self.tabs.get(self.active).map_or(0, |tab| {
            tab.html_views.len()
                + tab
                    .html_notes
                    .values()
                    .filter(|note| note.contains("automated tests"))
                    .count()
        })
    }

    pub(super) fn start_html_output(
        &mut self,
        key: String,
        html: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.is_empty() || !self.preferences.allow_html_output {
            return;
        }
        if self.html_slot_open(&key) {
            return;
        }
        self.evict_hidden_html_views(cx);
        if self.html_slot_count() >= MAX_LIVE_VIEWS {
            self.tabs[self.active].html_notes.insert(
                key,
                "Too many interactive outputs are open. Stop rendering one to continue.".into(),
            );
            return;
        }
        if cfg!(test) {
            self.tabs[self.active].html_notes.insert(
                key,
                "Interactive HTML is not created in automated tests.".into(),
            );
            return;
        }
        match WebView::build(sandboxed_builder(), window, cx) {
            Ok(view) => {
                let _ = view.set_visible(false);
                if let Err(error) = view.load_html(&document_html(&html)) {
                    self.tabs[self.active]
                        .html_notes
                        .insert(key, build_error_message(&error));
                    return;
                }
                self.tabs[self.active].html_notes.remove(&key);
                self.tabs[self.active].html_views.insert(key, cx.new(|_| view));
            }
            Err(error) => {
                self.tabs[self.active]
                    .html_notes
                    .insert(key, build_error_message(&error));
            }
        }
    }

    pub(super) fn stop_html_output(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if let Some(view) = tab.html_views.remove(key) {
                view.update(cx, |wv, _| {
                    let _ = wv.set_visible(false);
                });
            }
            tab.html_notes.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{document_html, navigation_allowed};

    #[test]
    fn navigation_allows_blank_and_data_only() {
        assert!(navigation_allowed("about:blank"));
        assert!(navigation_allowed("data:text/html,hi"));
        assert!(!navigation_allowed("https://cdn.plot.ly/plotly.js"));
        assert!(!navigation_allowed("http://example.com"));
        assert!(!navigation_allowed("file:///tmp/page.html"));
        assert!(!navigation_allowed("ftp://example.com/a"));
        assert!(!navigation_allowed("javascript:alert(1)"));
    }

    #[test]
    fn document_html_wraps_fragments_and_keeps_full_pages() {
        let fragment = document_html("<div>hi</div>");
        assert!(fragment.contains("<!DOCTYPE html>"));
        assert!(fragment.contains("<div>hi</div>"));
        let page = "<!DOCTYPE html><html><body>ok</body></html>";
        assert_eq!(document_html(page), page);
    }
}
