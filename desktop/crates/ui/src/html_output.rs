//! Opt-in sandboxed HTML+JS cell output. Native tables and PNG remain the default.
use super::*;
use gpui_webview::WebView;
use kernelledger_client::preferences::{HtmlNetworkPolicy, Preferences, parse_html_origins};
use std::borrow::Cow;
use wry::{NewWindowResponse, WebViewBuilder};

pub(super) const HOST_HEIGHT: f32 = 400.;
pub(super) const MAX_LIVE_VIEWS: usize = 4;

pub(super) struct HtmlState {
    pub token: String,
    pub exec_id: String,
    pub height: f32,
    pub manual_height: Option<f32>,
    pub status: String,
    pub failed: bool,
    pub pending: bool,
}

impl HtmlState {
    fn new(exec_id: String, pending: bool) -> Self {
        Self {
            token: uuid::Uuid::new_v4().to_string(),
            exec_id,
            height: HOST_HEIGHT,
            manual_height: None,
            status: "Loading HTML…".into(),
            failed: false,
            pending,
        }
    }
}

pub(super) struct HtmlSignal {
    pub key: String,
    pub token: String,
    pub value: Value,
}

const PAGE_OBSERVER: &str = r#"
(() => {
  const send = value => { if (window.ipc) window.ipc.postMessage(JSON.stringify(value)); };
  let lastHeight = 0;
  const measure = () => {
    if (!document.body) return;
    const style = getComputedStyle(document.body);
    const height = Math.max(160, Math.min(800, Math.ceil(document.body.getBoundingClientRect().height + parseFloat(style.marginTop || 0) + parseFloat(style.marginBottom || 0))));
    if (Math.abs(height - lastHeight) > 2) { lastHeight = height; send({kind:'height', height}); }
  };
  window.addEventListener('error', () => send({kind:'error'}), true);
  window.addEventListener('unhandledrejection', () => send({kind:'error'}));
  window.addEventListener('securitypolicyviolation', () => send({kind:'blocked'}));
  window.addEventListener('DOMContentLoaded', () => {
    new ResizeObserver(measure).observe(document.body);
    measure(); send({kind:'ready'});
  });
})();
"#;

fn navigation_allowed(url: &str, path: &str) -> bool {
    let url = url.split('#').next().unwrap_or(url);
    url == "about:blank"
        || url == format!("kl-output://localhost{path}")
        || url == format!("http://kl-output.localhost{path}")
}

fn content_security_policy(preferences: &Preferences) -> String {
    let network = match preferences.html_network_policy {
        HtmlNetworkPolicy::Offline => String::new(),
        HtmlNetworkPolicy::AllowedDomains => {
            parse_html_origins(&preferences.html_allowed_origins.join(" "))
                .unwrap_or_default()
                .join(" ")
        }
        HtmlNetworkPolicy::Full => "http: https: ws: wss:".into(),
    };
    let connect = if network.is_empty() {
        "'none'"
    } else {
        &network
    };
    // Serve this as an immutable response header, before any recorded markup is parsed.
    // Frames, workers and objects cannot introduce a document with a weaker policy.
    format!(
        "default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval' data: blob: {network}; style-src 'unsafe-inline' {network}; img-src data: blob: {network}; font-src data: blob: {network}; media-src data: blob: {network}; connect-src {connect}; frame-src 'none'; worker-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; sandbox allow-scripts"
    )
}

fn html_response(
    path: &str,
    expected_path: &str,
    html: &str,
    policy: &str,
) -> wry::http::Response<Cow<'static, [u8]>> {
    wry::http::Response::builder()
        .status(if path == expected_path { 200 } else { 404 })
        .header("Content-Type", "text/html; charset=utf-8")
        .header("Content-Security-Policy", policy)
        .header("Referrer-Policy", "no-referrer")
        .header("X-DNS-Prefetch-Control", "off")
        .header("Cache-Control", "no-store")
        .body(Cow::Owned(if path == expected_path {
            html.as_bytes().to_vec()
        } else {
            Vec::new()
        }))
        .expect("generated HTML response headers are valid")
}

fn sandboxed_builder(
    html: &str,
    preferences: &Preferences,
    path: String,
    signals: std::sync::mpsc::SyncSender<HtmlSignal>,
    key: String,
    token: String,
) -> WebViewBuilder<'static> {
    let document = document_html(html);
    let policy = content_security_policy(preferences);
    let navigation_path = path.clone();
    WebViewBuilder::new()
        .with_incognito(true)
        .with_autoplay(false)
        .with_hotkeys_zoom(false)
        .with_focused(false)
        .with_visible(false)
        .with_initialization_script(PAGE_OBSERVER)
        .with_ipc_handler(move |request| {
            if request.body().len() <= 1024
                && let Ok(value) = serde_json::from_str::<Value>(request.body())
            {
                let _ = signals.try_send(HtmlSignal { key: key.clone(), token: token.clone(), value });
            }
        })
        // CSP connect-src does not cover WebRTC's direct peer/STUN traffic.
        // Output networking is limited to HTTP(S) and WebSockets in every mode.
        .with_initialization_script("for (const name of ['RTCPeerConnection', 'webkitRTCPeerConnection']) { Object.defineProperty(window, name, { value: undefined, configurable: false, writable: false }); }")
        .with_custom_protocol("kl-output".into(), move |_, request| {
            html_response(request.uri().path(), &path, &document, &policy)
        })
        .with_navigation_handler(move |url| navigation_allowed(&url, &navigation_path))
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

pub(super) fn host(view: Entity<WebView>, blocked: bool, height: f32) -> AnyElement {
    let geometry = view.clone();
    div()
        .h(px(height))
        .w_full()
        .min_h(px(height))
        .occlude()
        .relative()
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
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
        // Let the library update native composition, display scale and hitboxes.
        .child(view)
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
        tab.html_states.remove(&key);
    }
    let drop_notes: Vec<String> = tab
        .html_notes
        .keys()
        .filter(|key| notebook_key_current(key, tab) == Some(false))
        .cloned()
        .collect();
    for key in drop_notes {
        tab.html_notes.remove(&key);
        tab.html_states.remove(&key);
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
            let current = index < cell.output.len()
                && tab.html_states.get(key).is_none_or(|state| {
                    state.exec_id.is_empty()
                        || cell.exec_id.as_deref() == Some(state.exec_id.as_str())
                });
            matched = Some(match matched {
                Some(previous) if id.len() < previous.1 => previous,
                _ => (current, id.len()),
            });
        }
    }
    matched.map(|(current, _)| current)
}

impl Workspace {
    pub(super) fn reset_html_outputs(&mut self, cx: &mut App) {
        self.conceal_html_views(cx);
        for tab in &mut self.tabs {
            tab.html_views.clear();
            tab.html_notes.clear();
            tab.html_states.clear();
            tab.list
                .splice(0..tab.model.order.len(), tab.model.order.len());
        }
    }

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
            tab.html_states.remove(&key);
        }
    }

    pub(super) fn request_html_output(
        &mut self,
        key: String,
        html: String,
        digest: Option<String>,
        exec_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.preferences.allow_html_output || self.tabs.is_empty() || self.html_slot_open(&key)
        {
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
        let state = HtmlState::new(exec_id, digest.is_some());
        let token = state.token.clone();
        self.tabs[self.active]
            .html_states
            .insert(key.clone(), state);
        if let Some(digest) = digest {
            self.tabs[self.active]
                .html_notes
                .insert(key.clone(), "Loading recorded HTML…".into());
            if self.tabs[self.active]
                .service
                .commands
                .send(Command::LoadHtml {
                    key: key.clone(),
                    token,
                    digest,
                })
                .is_err()
            {
                self.tabs[self.active].html_states.remove(&key);
                self.tabs[self.active].html_notes.insert(
                    key,
                    "HTML could not load: notebook service is disconnected.".into(),
                );
            }
        } else {
            self.start_html_output(key, html, window, cx);
        }
    }

    pub(super) fn html_pending(&self, key: &str) -> bool {
        self.tabs
            .get(self.active)
            .and_then(|t| t.html_states.get(key))
            .is_some_and(|s| s.pending)
    }

    pub(super) fn html_height(&self, key: &str) -> f32 {
        self.tabs
            .get(self.active)
            .and_then(|t| t.html_states.get(key))
            .map_or(HOST_HEIGHT, |s| s.manual_height.unwrap_or(s.height))
    }

    pub(super) fn resize_html_output(
        &mut self,
        key: &str,
        delta: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        let height = self.html_height(key);
        if let Some(tab) = self.tabs.get_mut(self.active)
            && let Some(state) = tab.html_states.get_mut(key)
        {
            state.manual_height = delta.map(|delta| (height + delta).clamp(160., 800.));
            tab.list
                .splice(0..tab.model.order.len(), tab.model.order.len());
            cx.notify();
        }
    }

    pub(super) fn html_view(&self, key: &str) -> Option<Entity<WebView>> {
        self.tabs
            .get(self.active)
            .and_then(|tab| tab.html_views.get(key).cloned())
    }

    pub(super) fn html_note(&self, key: &str) -> Option<String> {
        self.tabs.get(self.active).and_then(|tab| {
            tab.html_notes.get(key).cloned().or_else(|| {
                tab.html_states
                    .get(key)
                    .filter(|state| !state.status.is_empty())
                    .map(|state| state.status.clone())
            })
        })
    }

    fn html_slot_open(&self, key: &str) -> bool {
        self.tabs.get(self.active).is_some_and(|tab| {
            tab.html_views.contains_key(key)
                || tab.html_states.get(key).is_some_and(|s| s.pending)
                || tab
                    .html_notes
                    .get(key)
                    .is_some_and(|note| note.contains("automated tests"))
        })
    }

    fn html_slot_count(&self) -> usize {
        self.tabs.get(self.active).map_or(0, |tab| {
            tab.html_views.len()
                + tab.html_states.values().filter(|s| s.pending).count()
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
        let state = self.tabs[self.active]
            .html_states
            .entry(key.clone())
            .or_insert_with(|| HtmlState::new(String::new(), false));
        let token = state.token.clone();
        let path = format!("/{token}/index.html");
        let url = format!("kl-output://localhost{path}");
        match WebView::build(
            sandboxed_builder(
                &html,
                &self.preferences,
                path,
                self.html_signals.clone(),
                key.clone(),
                token,
            ),
            window,
            cx,
        ) {
            Ok(view) => {
                let _ = view.set_visible(false);
                if let Err(error) = view.raw().load_url(&url) {
                    self.tabs[self.active]
                        .html_notes
                        .insert(key, build_error_message(&error));
                    return;
                }
                self.tabs[self.active].html_notes.remove(&key);
                self.tabs[self.active]
                    .html_views
                    .insert(key, cx.new(|_| view));
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
            tab.html_states.remove(key);
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
    use super::{content_security_policy, document_html, html_response, navigation_allowed};
    use kernelledger_client::preferences::{HtmlNetworkPolicy, Preferences};

    #[test]
    fn navigation_cannot_escape_the_policy_document() {
        let path = "/unique/index.html";
        for url in [
            "about:blank",
            "kl-output://localhost/unique/index.html",
            "http://kl-output.localhost/unique/index.html#anchor",
        ] {
            assert!(navigation_allowed(url, path), "{url}");
        }
        for url in [
            "data:text/html,hi",
            "about:blank.evil",
            "https://cdn.plot.ly/plotly.js",
            "http://example.com",
            "file:///tmp/page.html",
            "ftp://example.com/a",
            "javascript:alert(1)",
            "kl-output://localhost/other/index.html",
            "http://kl-output.localhost.evil/unique/index.html",
        ] {
            assert!(!navigation_allowed(url, path), "{url}");
        }
    }

    #[test]
    fn response_enforces_network_policy_before_untrusted_markup() {
        let mut prefs = Preferences::default();
        let attack = "<script src='https://evil.example/a.js'></script><meta http-equiv='Content-Security-Policy' content='default-src *'>";
        let response = html_response("/test", "/test", attack, &content_security_policy(&prefs));
        let csp = response.headers()["Content-Security-Policy"]
            .to_str()
            .unwrap();
        assert!(csp.contains("connect-src 'none'"));
        assert!(csp.contains("default-src 'none'"));
        assert!(csp.contains("frame-src 'none'; worker-src 'none'; object-src 'none'"));
        assert!(csp.contains("form-action 'none'; sandbox allow-scripts"));
        assert!(!csp.contains("https:"));
        assert_eq!(response.body().as_ref(), attack.as_bytes());
        assert_eq!(html_response("/other", "/test", attack, csp).status(), 404);

        prefs.html_network_policy = HtmlNetworkPolicy::AllowedDomains;
        prefs.html_allowed_origins = vec![
            "https://cdn.plot.ly".into(),
            "http://127.0.0.1:18763".into(),
        ];
        let csp = content_security_policy(&prefs);
        assert!(csp.contains("connect-src https://cdn.plot.ly http://127.0.0.1:18763;"));
        assert!(!csp.contains("evil.example"));
        prefs
            .html_allowed_origins
            .push("https://good.example; default-src *".into());
        assert!(content_security_policy(&prefs).contains("connect-src 'none'"));

        prefs.html_network_policy = HtmlNetworkPolicy::Full;
        let csp = content_security_policy(&prefs);
        assert!(csp.contains("connect-src http: https: ws: wss:;"));
        assert!(csp.contains("frame-src 'none'"));
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
