"""One-off source transformation for the v5 workbench extraction."""
from pathlib import Path

root = Path('desktop/crates/ui/src')
p = root / 'lib.rs'
s = p.read_text(encoding='utf-8')
# Keep a reviewable baseline in this non-git workspace.
backup = Path('artifacts/frontend-v5/baseline')
backup.mkdir(parents=True, exist_ok=True)
(backup / 'ui-lib.rs').write_text(s, encoding='utf-8')

def replace(start, end, value):
    global s
    a, b = s.index(start), s.index(end, s.index(start))
    s = s[:a] + value + s[b:]

replace('    fn header(', '    fn sidebar(', '''    fn header(&self, cx: &mut Context<Self>) -> Div {
        let mut header = h_flex().h(px(44.)).flex_none().px_3().gap_2()
            .items_center().bg(cx.theme().title_bar).border_b_1().border_color(cx.theme().border);
        let mut notebooks = h_flex().max_w(px(if self.compact { 160. } else { 280. }))
            .min_w_0().overflow_hidden().gap_1();
        for (i, tab) in self.tabs.iter().enumerate() {
            notebooks = notebooks.child(Button::new(("tab", i)).ghost().small()
                .label(tab.model.project.file_name().unwrap_or_default().to_string_lossy().to_string())
                .tooltip(tab.model.project.display().to_string()).selected(i == self.active)
                .on_click(cx.listener(move |v, _, _, cx| {
                    if let Some(tab) = v.tabs.get_mut(v.active) {
                        tab.selections.insert(v.surface.clone(), v.record_active);
                    }
                    v.active = i;
                    v.record_active = v.tabs[i].selections.get(&v.surface).copied().unwrap_or(0);
                    cx.notify();
                })));
        }
        if self.tabs.is_empty() { header = header.child("KernelLedger"); }
        else {
            header = header.child(notebooks).child(vsep(cx.theme().border));
            for surface in ["Notebook", "History", "Checkpoints"] {
                header = header.child(Button::new(surface).ghost().small().label(surface)
                    .selected(self.surface == surface)
                    .accessibility_label(format!("{surface}{}", if self.surface == surface { ", current destination" } else { "" }))
                    .on_click(cx.listener(move |v, _, window, cx| {
                        v.select_surface(surface, cx); v.focus.focus(window, cx);
                    })));
            }
        }
        header.child(div().flex_1()).child(Button::new("palette-trigger").ghost().small()
            .label("Commands").tooltip("Search actions · Ctrl+Shift+P")
            .on_click(cx.listener(|v, _, window, cx| v.palette(&Palette, window, cx))))
            .child(Button::new("runtime-menu").ghost().small().label("Runtime")
                .disabled(self.tabs.is_empty())
                .on_click(cx.listener(|v, _, window, cx| {
                    v.return_focus = window.focused(cx);
                    v.runtime_menu = !v.runtime_menu; v.app_menu = false;
                    v.dialog_focus.focus(window, cx); cx.notify();
                })))
            .child(Button::new("application-menu").ghost().small().label("Menu")
                .on_click(cx.listener(|v, _, window, cx| {
                    v.return_focus = window.focused(cx);
                    v.app_menu = !v.app_menu; v.runtime_menu = false;
                    v.dialog_focus.focus(window, cx); cx.notify();
                })))
    }
    fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        h_flex().flex_none().h(px(40.)).px_3().gap_2().items_center()
            .border_b_1().border_color(cx.theme().border)
            .child(Button::new("outline-toggle").ghost().small().label("Outline")
                .selected(self.sidebar_visible).on_click(cx.listener(|v, _, _, cx| {
                    v.sidebar_visible = !v.sidebar_visible; cx.notify();
                })))
            .child(Button::new("run-selected").ghost().small().icon(Glyph::Play).label("Run")
                .on_click(cx.listener(|v, _, window, cx| v.run(&RunCell, window, cx))))
            .child(Button::new("all").ghost().small().label("Run all")
                .on_click(cx.listener(|v, _, _, _| v.batch("all"))))
            .child(Button::new("interrupt").ghost().small().label("Interrupt")
                .disabled(!runtime_action_available("interrupt", &self.tabs[self.active].runtime, &self.surface))
                .tooltip(action_reason("interrupt", &self.tabs[self.active].runtime).unwrap_or("Interrupt current execution"))
                .on_click(cx.listener(|v, _, window, cx| v.run_command("interrupt", window, cx))))
            .child(div().flex_1())
            .child(Button::new("add-code").ghost().small().label("+ Code")
                .on_click(cx.listener(|v, _, _, _| v.mutate(json!({"action":"add","type":"code"})))))
            .child(Button::new("add-markdown").ghost().small().label("+ Markdown")
                .on_click(cx.listener(|v, _, _, _| v.mutate(json!({"action":"add","type":"markdown"})))))
            .child(Button::new("add-raw").ghost().small().label("+ Raw")
                .on_click(cx.listener(|v, _, _, _| v.mutate(json!({"action":"add","type":"raw"})))))
    }
    fn menu_overlay(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut menu = v_flex().w(px(300.)).max_h(px(440.)).p_3().gap_2()
            .bg(cx.theme().popover).border_1().border_color(cx.theme().border).rounded(px(6.));
        if self.runtime_menu {
            let runtime = &self.tabs[self.active].runtime;
            menu = menu.child(div().font_weight(FontWeight::SEMIBOLD).child(worker_label(runtime["worker_state"].as_str())))
                .child(div().text_xs().whitespace_normal().child(runtime["interpreter"].to_string()));
            for (method, label) in [("interrupt", "Interrupt"), ("restart", "Restart empty…"), ("stop", "Stop runtime…")] {
                menu = menu.child(Button::new(method).ghost().label(label)
                    .disabled(action_reason(method, runtime).is_some())
                    .tooltip(action_reason(method, runtime).unwrap_or(label))
                    .on_click(cx.listener(move |v, _, window, cx| {
                        v.runtime_menu = false; v.run_command(method, window, cx);
                    })));
            }
            menu = menu.child(Button::new("all-runtimes").ghost().label("All runtimes")
                .on_click(cx.listener(|v, _, window, cx| {
                    v.runtime_menu = false; v.select_surface("Runtimes", cx); v.focus.focus(window, cx);
                })));
        } else {
            menu = menu.child(Button::new("open").ghost().label("Open notebook…")
                .on_click(cx.listener(|v, _, window, cx| { v.app_menu = false; v.open_dialog(&OpenNotebook, window, cx); })));
            for surface in ["Settings", "Help", "Diagnostics", "Runtimes"] {
                menu = menu.child(Button::new(surface).ghost().label(surface).disabled(self.tabs.is_empty() && surface != "Help")
                    .on_click(cx.listener(move |v, _, window, cx| {
                        v.app_menu = false; v.select_surface(surface, cx); v.focus.focus(window, cx);
                    })));
            }
        }
        menu = menu.child(Button::new("close-menu").ghost().label("Close menu")
            .on_click(cx.listener(|v, _, window, cx| v.close_menu(window, cx))));
        div().id("workspace-menu").track_focus(&self.dialog_focus).absolute().inset_0()
            .flex().justify_end().items_start().pt(px(48.)).pr_3().child(menu)
    }
    fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.app_menu = false; self.runtime_menu = false;
        if let Some(focus) = self.return_focus.take() { focus.focus(window, cx); }
        cx.notify();
    }
''')
s = s.replace('    sidebar_visible: bool,', '    sidebar_visible: bool,\n    app_menu: bool,\n    runtime_menu: bool,')
s = s.replace('            sidebar_visible: false,', '            sidebar_visible: true,\n            app_menu: false,\n            runtime_menu: false,')
# Check actual initialization rather than silently omitting fields.
if 'app_menu: false' not in s:
    s = s.replace('            sidebar_visible: true,', '            sidebar_visible: true,\n            app_menu: false,\n            runtime_menu: false,')
s = s.replace('        if self.confirmation.is_some() {\n', '''        if self.app_menu || self.runtime_menu {
            if key == "escape" { self.close_menu(window, cx); }
            else if key == "tab" {
                if m.shift { window.focus_prev(cx); } else { window.focus_next(cx); }
                if !self.dialog_focus.contains_focused(window, cx) { self.dialog_focus.focus(window, cx); }
            } else { return; }
            cx.stop_propagation(); return;
        }
        if self.confirmation.is_some() {
''', 1)
replace('            root = root\n                .child(self.toolbar(cx))', '        if self.palette {\n', '''            root = root
                .when(self.surface == "Notebook", |d| d.child(self.toolbar(cx)))
                .child(self.banners(window, cx))
                .child(h_flex().flex_1().min_h_0().relative().child(
                    h_resizable("notebook-layout-v5")
                        .when(self.surface == "Notebook" && !self.compact && self.sidebar_visible, |d| {
                            d.child(resizable_panel().size(px(220.)).size_range(px(180.)..px(300.)).child(self.sidebar(cx)))
                        })
                        .child(resizable_panel().child(center)),
                ).when(self.surface == "Notebook" && self.compact && self.sidebar_visible, |d| {
                    d.child(div().absolute().left_0().top_0().bottom_0().w(px(220.)).border_r_1()
                        .border_color(cx.theme().border).child(self.sidebar(cx)))
                }))
                .child(self.status_bar(cx));
        }
        if self.app_menu || self.runtime_menu { root = root.child(self.menu_overlay(cx)); }
''')
# Compact mode starts with its drawer closed; manual collapse survives later resizing.
s = s.replace('sidebar_visible: true,', 'sidebar_visible: window.viewport_size().width >= px(1024.),', 1)
s = s.replace('.max_w(px(980.))\n            .rounded(radius)\n            .border_1()', '.border_l_2()')
s = s.replace('primary.opacity(0.7)\n            } else {\n                border', 'primary\n            } else {\n                background')
s = s.replace('            .when(selected, |d| d.shadow_md())\n', '')
s = s.replace('            theme.radius_lg,\n        );\n        let mono', '            theme.radius,\n        );\n        let mono', 1)
s = s.replace('let (border, muted, muted_fg, primary, background, radius)', 'let (border, muted, muted_fg, primary, background, _radius)')
s = s.replace('.bg(muted.opacity(0.45))', '.bg(background)')
s = s.replace('badge("Stale output", warning)', 'badge("Edited since run", warning)')
s = s.replace('            .justify_center()\n            .px_6()', '            .flex_col()\n            .px_4()')
s = s.replace('            .child(card)\n            .into_any_element()', '''            .child(card)
            .child(h_flex().gap_1().pt_2().children([("code", "+ Code"), ("markdown", "+ Markdown"), ("raw", "+ Raw")].into_iter().map(|(kind, label)| {
                Button::new((SharedString::from(format!("insert-{kind}")), index)).ghost().small().label(label)
                    .accessibility_label(format!("Insert {kind} after cell {}", index + 1))
                    .on_click(cx.listener(move |v, _, _, _| v.mutate(json!({"action":"add","type":kind,"index":index + 1}))))
            })))
            .into_any_element()''', 1)
s = s.replace('let mut body = v_flex().w_full().min_w_0().max_w(px(920.)).gap_6()', 'let mut body = v_flex().w_full().min_w_0().gap_4()')
# Remove page emblems, keep useful title and description.
a=s.index('                .child(\n                    div()\n                        .flex_none()\n                        .size(px(36.))',s.index('    fn panel('))
b=s.index('                .child(\n                    v_flex()',a)
s=s[:a]+s[b:]
s=s.replace('let (glyph, description) = SURFACES','let (_glyph, description) = SURFACES')
s=s.replace('format!("{description} | recorded data")','description.to_string()')
s=s.replace('.child(title.to_uppercase())','.child(title.to_string())')
s=s.replace('        .rounded(theme.radius_lg)\n        .border_1()\n        .border_color(theme.border)\n        .bg(theme.background)', '        .border_t_1()\n        .border_color(theme.border)\n        .bg(theme.background)')
s=s.replace('.px_8()\n                    .py_6()', '.px_4()\n                    .py_4()')
# A checkpoint is a global runtime action, independent of the destination.
replace('fn runtime_action_available(', 'fn history_matches(', '''fn runtime_action_available(method: &str, runtime: &Value, _surface: &str) -> bool {
    action_reason(method, runtime).is_none()
}
fn action_reason(method: &str, runtime: &Value) -> Option<&'static str> {
    let state = runtime["worker_state"].as_str();
    match method {
        "checkpoint" | "training-checkpoint" if state != Some("idle") => Some("Run a cell and wait until the runtime is ready"),
        "training-checkpoint" if !runtime["capabilities"].as_array().is_some_and(|v| v.iter().any(|c| c == "portable_training")) => Some("This interpreter does not support portable training checkpoints"),
        "interrupt" | "interrupt_kill" if !matches!(state, Some("running" | "waiting_input" | "interrupting")) => Some("No execution is running"),
        "stop" | "restart" if !matches!(state, Some("idle" | "running" | "waiting_input" | "interrupting")) => Some("The runtime is not running"),
        _ => None,
    }
}
''')
# Creation lives on the checkpoint page, including its empty state.
at='        if self.surface == "Checkpoints" {\n'
s=s.replace(at,at+'''            body = body.child(Button::new("create-checkpoint").outline().label("Create checkpoint")
                .disabled(action_reason("checkpoint", &self.tabs[self.active].runtime).is_some())
                .tooltip(action_reason("checkpoint", &self.tabs[self.active].runtime).unwrap_or("Save supported values from the live namespace"))
                .on_click(cx.listener(|v, _, window, cx| v.run_command("checkpoint", window, cx))));
''',1)
# Palette destinations and experimental actions stay discoverable.
s=s.replace('const COMMANDS: [(&str, &str, &str, Glyph); 14]', 'const COMMANDS: [(&str, &str, &str, Glyph); 18]')
s=s.replace('const COMMANDS: [(&str, &str, &str, Glyph); 18] = [','''const COMMANDS: [(&str, &str, &str, Glyph); 18] = [
    ("settings", "Settings", "Appearance and interpreter", Glyph::Settings),
    ("diagnostics", "Diagnostics", "Recovery and technical details", Glyph::Stethoscope),
    ("runtimes", "All runtimes", "Manage notebook runtimes", Glyph::Cpu),
    ("checkpoints", "Checkpoints", "Inspect saved state", Glyph::Flag),''')
s=s.replace('"Training checkpoint",', '"Training checkpoint (Experimental)",')
s=s.replace('            "help" => self.surface = "Help".into(),', '''            "settings" => self.select_surface("Settings", cx),
            "diagnostics" => self.select_surface("Diagnostics", cx),
            "runtimes" => self.select_surface("Runtimes", cx),
            "checkpoints" => self.select_surface("Checkpoints", cx),
            "help" => self.select_surface("Help", cx),''')
# Neutral semantic palette shared by all Kit components.
replace('    let accent = if dark {', '    let on_accent =', '''    let accent: Hsla = rgb(if dark { 0x8AB5D6 } else { 0x315E83 }).into();
''')
s=s.replace('    colors.primary = accent;', '''    colors.background = rgb(if dark { 0x15181B } else { 0xFAFAF8 }).into();
    colors.foreground = rgb(if dark { 0xE6E9EB } else { 0x20262A }).into();
    colors.muted_foreground = rgb(if dark { 0xA8B0B7 } else { 0x59636B }).into();
    colors.muted = rgb(if dark { 0x1C2024 } else { 0xF1F2EF }).into();
    colors.sidebar = colors.muted;
    colors.sidebar_foreground = colors.foreground;
    colors.title_bar = colors.muted;
    colors.title_bar_border = colors.border;
    colors.popover = rgb(if dark { 0x23282D } else { 0xFFFFFF }).into();
    colors.popover_foreground = colors.foreground;
    colors.primary = accent;''',1)
s=s.replace('    theme.tokens = ThemeTokens::from(&theme.colors);', '''    theme.font_family = "Segoe UI".into();
    theme.mono_font_family = "Consolas".into();
    theme.radius = px(4.);
    theme.radius_lg = px(6.);
    theme.tokens = ThemeTokens::from(&theme.colors);''')
# Extract coherent modules; private implementation stays accessible to the parent.
def extract(name,start,end,methods=False):
    global s
    a,b=s.index(start),s.index(end,s.index(start))
    part=s[a:b]
    s=s[:a]+s[b:]
    if methods:
        part=part.replace('    fn ', '    pub(super) fn ')
        part='impl Workspace {\n'+part+'}\n'
    else:
        part=part.replace('\nfn ', '\npub(super) fn ')
        if part.startswith('fn '): part='pub(super) '+part
    (root/(name+'.rs')).write_text('use super::*;\n\n'+part,encoding='utf-8')
    s='mod '+name+';\n'+('use '+name+'::*;\n' if not methods else '')+s

extract('theme','fn apply_theme(', 'fn kind_label(')
extract('components','fn badge(', 'fn safe_link(')
extract('shell','    fn header(', '    fn sidebar(',True)
extract('notebook','    fn editor(', '    fn panel(',True)
extract('surfaces','    fn panel(', '    fn choose_interpreter(',True)
extract('accessibility','    fn palette_overlay(', '\n}\nimpl Render for Workspace',True)
p.write_text(s,encoding='utf-8')
