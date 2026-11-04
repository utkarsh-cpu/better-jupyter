use std::{cell::Cell, rc::Rc};

use super::*;

/// Tracks one settings group against its scroll viewport.
///
/// Activation of a clipped group scrolls that group into view on the next
/// frame, before the action runs. The scroll container marks itself as
/// clipping so fully covered controls are not presented as on-screen.
#[derive(Clone)]
pub(crate) struct GroupReveal {
    scroll: ScrollHandle,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl GroupReveal {
    pub(super) fn new(scroll: ScrollHandle) -> Self {
        Self {
            scroll,
            bounds: Rc::new(Cell::new(Bounds::default())),
        }
    }

    pub(super) fn track<E>(&self, group: E) -> E
    where
        E: ParentElement + Styled,
    {
        let bounds = self.bounds.clone();
        group.relative().child(
            canvas(move |painted, _, _| bounds.set(painted), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
    }

    fn is_clipped(&self) -> bool {
        let viewport = self.scroll.bounds();
        let group = self.bounds.get();
        if viewport.size.height <= px(1.) || group.size.height <= px(1.) {
            return false;
        }
        group.top() < viewport.top() - px(1.) || group.bottom() > viewport.bottom() + px(1.)
    }

    fn schedule(&self, window: &mut Window) {
        let scroll = self.scroll.clone();
        let bounds = self.bounds.clone();
        window.on_next_frame(move |_, _| {
            let viewport = scroll.bounds();
            let group = bounds.get();
            if viewport.size.height <= px(1.) || group.size.height <= px(1.) {
                return;
            }
            let mut offset = scroll.offset();
            if group.size.height <= viewport.size.height {
                if group.bottom() > viewport.bottom() {
                    offset.y -= group.bottom() - viewport.bottom();
                }
                if group.top() < viewport.top() {
                    offset.y += viewport.top() - group.top();
                }
            } else {
                offset.y -= group.top() - viewport.top();
            }
            offset.y = offset.y.clamp(-scroll.max_offset().y, px(0.));
            scroll.set_offset(offset);
        });
    }

    /// Runs `apply` immediately when the group is already visible.
    /// A clipped group scrolls first; `apply` runs on the next frame.
    pub(super) fn activate(
        &self,
        view: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
        apply: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    ) {
        if self.is_clipped() {
            self.schedule(window);
            cx.on_next_frame(window, apply);
        } else {
            apply(view, window, cx);
        }
    }
}

pub(super) struct WorkbenchPresentation {
    pub comfortable: bool,
}
impl gpui::Global for WorkbenchPresentation {}

/// One minimum pointer target for labeled and icon-only workbench actions.
pub(super) fn action_button(id: impl Into<ElementId>) -> WorkbenchButton {
    let id = id.into();
    WorkbenchButton {
        inner: Button::new(id.clone())
            // Interactive controls must occlude the title bar's native drag
            // hitbox, and must not arm its manual drag handler on other hosts.
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .rounded(px(6.))
            .font_weight(FontWeight::MEDIUM)
            .min_h(rems(1.875))
            .min_w(rems(1.75))
            .flex_none(),
        id,
        disabled: false,
        name: None,
        treatment: ButtonTreatment::Secondary,
        a11y_click: None,
        full_row: false,
    }
}

/// Kit correctly blocks disabled activation, but this GPUI version does not
/// expose an aria-disabled setter. Populate the existing AccessKit node directly.
type AccessibilityClick = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub(super) struct WorkbenchButton {
    inner: Button,
    id: ElementId,
    disabled: bool,
    name: Option<SharedString>,
    treatment: ButtonTreatment,
    a11y_click: Option<AccessibilityClick>,
    full_row: bool,
}
enum ButtonTreatment {
    Secondary,
    Quiet,
    Primary,
    Danger,
    Segment(bool),
}
impl WorkbenchButton {
    pub fn unavailable(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.inner = self.inner.disabled(disabled);
        self
    }
    pub fn hint(mut self, label: impl Into<SharedString>) -> Self {
        let text = label.into();
        // Use GPUI's element-owned tooltip lifecycle, rather than Kit's global
        // managed overlay, whose hover owner can outlive a destination change.
        self.inner
            .interactivity()
            .tooltip(move |_, cx| cx.new(|_| WorkbenchHint(text.clone())).into());
        self
    }
    pub fn debug_selector(mut self, name: &'static str) -> Self {
        self.inner = self.inner.debug_selector(move || name.into());
        self
    }
    pub fn semantics_in_parent(mut self) -> Self {
        self.inner = self.inner.role(None);
        self
    }
    pub fn contextual(mut self, group: impl Into<SharedString>, always: bool) -> Self {
        if !always {
            self.inner = self
                .inner
                .opacity(0.)
                .group_hover(group, |style| style.opacity(1.))
                .focus(|style| style.opacity(1.));
        }
        self
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        let text = label.into();
        if self.name.is_none() {
            self.name = Some(text.clone());
        }
        self.inner = self.inner.label(text);
        self
    }
    #[allow(dead_code)]
    pub fn menu_label(self, label: impl Into<SharedString>) -> Self {
        self.menu_item(label, None)
    }

    pub fn menu_item(mut self, label: impl Into<SharedString>, shortcut: Option<&str>) -> Self {
        let text = label.into();
        self.name = Some(text.clone());
        self.full_row = true;
        let shortcut = shortcut.and_then(super::kbd);
        // gpui-component centers Button contents. A stretching child is what
        // makes the label sit on the leading edge and the shortcut on the trailing
        // edge, the way a desktop menu row is supposed to read.
        self.inner = self
            .inner
            .w_full()
            .flex_1()
            .justify_start()
            .compact()
            .px_2()
            .accessibility_label(text.clone())
            .child(
                h_flex()
                    .flex_1()
                    .w_full()
                    .min_w_0()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().min_w_0().text_left().child(text))
                    .when_some(shortcut, |row, keys| {
                        row.child(div().flex_none().text_xs().opacity(0.72).child(keys))
                    }),
            );
        self
    }
    /// A full-width row: the label truncates on the leading side, quieter
    /// detail text sits on the trailing edge.
    pub fn detail_row(
        mut self,
        label: impl Into<SharedString>,
        detail: impl Into<SharedString>,
    ) -> Self {
        let text = label.into();
        self.name = Some(text.clone());
        self.full_row = true;
        self.inner = self
            .inner
            .w_full()
            .flex_1()
            .justify_start()
            .px_2()
            .accessibility_label(text.clone())
            .child(
                h_flex()
                    .flex_1()
                    .w_full()
                    .min_w_0()
                    .justify_between()
                    .items_center()
                    .gap_4()
                    .child(div().flex_1().min_w_0().text_left().truncate().child(text))
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .opacity(0.78)
                            .child(detail.into()),
                    ),
            );
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        self.name = Some(label.clone());
        self.inner = self.inner.accessibility_label(label);
        self
    }
    pub fn icon(mut self, icon: Glyph) -> Self {
        self.inner = self.inner.icon(icon);
        self
    }
    pub fn outline(mut self) -> Self {
        self.inner = self.inner.outline();
        self
    }
    pub fn small(mut self) -> Self {
        self.inner = self.inner.small();
        self
    }
    pub fn ghost(mut self) -> Self {
        self.treatment = ButtonTreatment::Quiet;
        self.inner = self.inner.ghost();
        self
    }
    pub fn primary(mut self) -> Self {
        self.treatment = ButtonTreatment::Primary;
        self.inner = self.inner.primary();
        self
    }
    pub fn danger(mut self) -> Self {
        self.treatment = ButtonTreatment::Danger;
        self.inner = self.inner.danger();
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.inner = self.inner.selected(selected);
        self
    }
    pub fn segment(mut self, selected: bool) -> Self {
        self.treatment = ButtonTreatment::Segment(selected);
        // Kit's generic selected treatment uses the accent fill. Segments own
        // their raised surface while retaining explicit toggle semantics.
        self.inner = self
            .inner
            .ghost()
            .selected(false)
            .toggled(selected)
            .rounded(px(5.));
        self
    }
    pub fn dropdown_caret(mut self, value: bool) -> Self {
        self.inner = self.inner.dropdown_caret(value);
        self
    }
    pub fn on_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.inner = self.inner.on_click(listener);
        self
    }

    /// Handles UI Automation Invoke directly. The toolkit's default Invoke
    /// clicks the control center, which misses when that center is clipped.
    pub fn on_a11y_click(mut self, listener: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.a11y_click = Some(Rc::new(listener));
        self
    }
}
impl Styled for WorkbenchButton {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}
impl RenderOnce for WorkbenchButton {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui_kit::component::button::ButtonCustomVariant;
        let theme = cx.theme();
        let high_contrast = theme.background.l == 0.;
        match self.treatment {
            ButtonTreatment::Secondary => {
                self.inner = self
                    .inner
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(theme.popover)
                            .foreground(theme.foreground)
                            .hover(theme.muted)
                            .active(theme.list_active)
                            .shadow(!high_contrast),
                    )
                    .border_1()
                    .border_color(edge(cx));
            }
            ButtonTreatment::Segment(selected) => {
                self.inner = self
                    .inner
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(if selected {
                                theme.popover
                            } else {
                                theme.transparent
                            })
                            .foreground(if selected {
                                theme.foreground
                            } else {
                                theme.muted_foreground
                            })
                            .hover(theme.popover)
                            .active(theme.list_active)
                            .shadow(selected && !high_contrast),
                    )
                    // Custom variants mix their base fill with transparency.
                    // Segmented selection uses an opaque raised surface.
                    .bg(if selected {
                        theme.popover
                    } else {
                        theme.transparent
                    })
                    .when(selected && !high_contrast, |button| button.shadow_sm())
                    .border_1()
                    .border_color(if selected {
                        if high_contrast { theme.ring } else { edge(cx) }
                    } else {
                        theme.transparent
                    });
            }
            ButtonTreatment::Quiet | ButtonTreatment::Primary | ButtonTreatment::Danger => {}
        }
        if cx
            .try_global::<WorkbenchPresentation>()
            .is_some_and(|p| p.comfortable)
        {
            self.inner = self.inner.min_h(rems(2.75)).min_w(rems(2.75));
        }
        let align_self = self.inner.style().align_self;
        let a11y_click = self.a11y_click.take();
        let a11y_name = self.name.clone();
        let full_row = self.full_row;
        let segment_selected = matches!(self.treatment, ButtonTreatment::Segment(true));
        if self.disabled {
            // Unavailable actions use neutral colors rather than looking selected.
            self.inner = self.inner.custom(
                ButtonCustomVariant::new(cx)
                    .color(theme.muted)
                    .foreground(theme.muted_foreground)
                    .hover(theme.muted)
                    .active(theme.muted)
                    .shadow(false),
            );
            let mut wrapper = div()
                .id((self.id, "disabled-state"))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .flex_none()
                .role(Role::Button)
                .aria_label(self.name.unwrap_or_else(|| "Unavailable action".into()))
                .a11y_synthetic_children(|tree| tree.parent_node().set_disabled())
                .when(full_row, |d| d.w_full());
            wrapper.style().align_self = align_self;
            wrapper.child(self.inner.role(None)).into_any_element()
        } else if let Some(listener) = a11y_click {
            // Invoke is handled here because this button type cannot register
            // an accessibility action on its own node. The visible button keeps
            // pointer and test identity; this node is what UI Automation invokes.
            let mut wrapper = div()
                .id((self.id, "invoke"))
                .flex_none()
                .role(Role::Button)
                .when_some(a11y_name, |node, name| node.aria_label(name))
                .when(
                    matches!(self.treatment, ButtonTreatment::Segment(_)),
                    |node| {
                        node.aria_toggled(if segment_selected {
                            accesskit::Toggled::True
                        } else {
                            accesskit::Toggled::False
                        })
                    },
                )
                .on_a11y_action(accesskit::Action::Click, move |_, window, cx| {
                    listener(window, cx);
                })
                .when(full_row, |d| d.w_full());
            wrapper.style().align_self = align_self;
            wrapper.child(self.inner.role(None)).into_any_element()
        } else {
            self.inner.into_any_element()
        }
    }
}
pub(super) struct WorkbenchHint(pub SharedString);
impl Render for WorkbenchHint {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let high_contrast = cx.theme().background.l == 0.;
        div()
            .px_2p5()
            .py_1p5()
            .max_w(px(360.))
            .rounded(px(6.))
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .border_1()
            .border_color(edge(cx))
            .when(!high_contrast, |d| d.shadow_sm())
            .text_xs()
            .line_height(rems(1.15))
            .child(self.0.clone())
    }
}

pub(super) fn info_tip(
    id: impl Into<ElementId>,
    tip: impl Into<SharedString>,
    cx: &App,
) -> Stateful<Div> {
    let tip = tip.into();
    let muted = cx.theme().muted_foreground;
    div()
        .id(id.into())
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(18.))
        .rounded_full()
        .cursor_default()
        .text_color(muted)
        .hover(move |s| s.bg(muted.opacity(0.12)))
        .child(Icon::new(Glyph::Info).with_size(px(13.)))
        .tooltip(move |_, cx| cx.new(|_| WorkbenchHint(tip.clone())).into())
}

pub(super) fn divider(cx: &App) -> Hsla {
    let color = cx.theme().border;
    // High contrast keeps explicit boundaries; decorative lines elsewhere are
    // deliberately independent of the focus ring and control-border roles.
    if cx.theme().background.l == 0. {
        color
    } else {
        color.opacity(0.18)
    }
}

/// Boundary of an interactive control (button, input, segmented track).
///
/// Decorative rules may fade to `divider`. A control edge is what tells a user
/// where the control begins, so it uses the border token at full strength:
/// 3.3:1 in light and 3.8:1 in dark, over background, muted, and popover alike.
pub(super) fn edge(cx: &App) -> Hsla {
    cx.theme().border
}

/// Quiet outline for panels and records. Selection and focus use `edge` or the
/// accent bar; resting cards should not compete with the content they hold.
pub(super) fn surface_edge(cx: &App) -> Hsla {
    let color = cx.theme().border;
    if cx.theme().background.l == 0. {
        color
    } else {
        color.opacity(0.22)
    }
}

pub(super) fn outcome_label(value: &str) -> &str {
    match value {
        "ok" => "Succeeded",
        "error" => "Failed",
        "complete" => "Complete",
        "partial" => "Partial",
        "interrupted" => "Interrupted",
        "idle" => "Ready",
        "running" => "Running",
        "verified" => "Active",
        _ => value,
    }
}

/// Success is the expected resting state. Surface only outcomes that ask
/// for attention so lists and cell headers stay quiet.
pub(super) fn notable_status(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "ok"
            | "success"
            | "succeeded"
            | "complete"
            | "completed"
            | "unknown"
            | "idle"
            | "ready"
    )
}

pub(super) fn badge(label: impl Into<SharedString>, color: Hsla) -> Div {
    h_flex()
        .flex_none()
        .min_h(px(18.))
        .px(px(6.))
        .py(px(1.))
        .rounded(px(4.))
        .bg(color.opacity(0.12))
        .items_center()
        .text_color(color)
        .text_size(rems(0.6875))
        .line_height(rems(1.))
        .font_weight(FontWeight::MEDIUM)
        .child(label.into())
}
pub(super) fn dot(color: Hsla) -> Div {
    div().flex_none().size(px(7.)).rounded_full().bg(color)
}
pub(super) fn status_rule(cx: &App) -> Div {
    div()
        .flex_none()
        .w(px(1.))
        .h(px(12.))
        .rounded_full()
        .bg(divider(cx))
}
pub(super) fn page_heading(
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    cx: &App,
) -> Div {
    let description = description.into();
    v_flex()
        .min_w_0()
        .flex_1()
        .gap_0p5()
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .when(!description.is_empty(), |d| {
            d.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .max_w(rems(42.))
                    .child(description),
            )
        })
}
pub(super) fn icon_tile(glyph: Glyph, size: f32, cx: &App) -> Div {
    let primary = cx.theme().primary;
    div()
        .flex_none()
        .size(px(size))
        .rounded(px((size / 4.).max(8.)))
        .flex()
        .items_center()
        .justify_center()
        .bg(primary.opacity(0.10))
        .border_1()
        .border_color(primary.opacity(0.16))
        .text_color(primary)
        .child(Icon::new(glyph).with_size(px(size / 2.)))
}
pub(super) fn banner(color: Hsla, glyph: Glyph) -> Div {
    h_flex()
        .w_full()
        .gap_3()
        .px_4()
        .py_2()
        .items_center()
        .bg(color.opacity(0.08))
        .border_b_1()
        .border_color(color.opacity(0.24))
        .child(
            div()
                .flex_none()
                .text_color(color)
                .child(Icon::new(glyph).small()),
        )
}
pub(super) fn section(title: &str, cx: &App) -> Div {
    v_flex().gap_2().child(
        div()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().muted_foreground)
            .child(title.to_string()),
    )
}
pub(super) fn card(cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .border_1()
        .rounded(theme.radius_lg)
        .border_color(surface_edge(cx))
        .bg(theme.popover)
        .overflow_hidden()
}

/// One grouped surface for stacked settings or inspector sections.
///
/// Unlike `card`, this must not clip: settings groups measure themselves with
/// an absolutely positioned tracker, and focus rings sit on the control edge.
pub(super) fn sheet(cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .border_1()
        .rounded(theme.radius_lg)
        .border_color(surface_edge(cx))
        .bg(theme.popover)
        .gap_0()
}

/// Wraps native buttons so keyboard activation and selected state stay intact.
/// Wrapping is intentional at large interface sizes and narrow window widths.
pub(super) fn segmented_track(cx: &App) -> Div {
    h_flex()
        .self_start()
        .min_w_0()
        .flex_wrap()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(6.))
        .bg(cx.theme().muted)
        .when(cx.theme().background.l == 0., |d| {
            d.border_1().border_color(edge(cx))
        })
}

pub(super) fn settings_group(title: &str, cx: &App) -> Div {
    settings_group_with(title, "", cx)
}

pub(super) fn settings_group_with(title: &str, description: &str, cx: &App) -> Div {
    v_flex()
        .w_full()
        .min_w_0()
        .px_4()
        .py_3()
        .gap_2()
        .border_b_1()
        .border_color(divider(cx))
        .child(
            v_flex()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .when(!description.is_empty(), |d| {
                    d.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .whitespace_normal()
                            .child(description.to_string()),
                    )
                }),
        )
}

pub(super) fn setting_row(
    label: &str,
    description: &str,
    control: impl IntoElement,
    cx: &App,
) -> Div {
    // A fixed label column keeps controls aligned; narrow pages wrap the control below.
    h_flex()
        .w_full()
        .min_w_0()
        .items_start()
        .flex_wrap()
        .gap_x_4()
        .gap_y_1p5()
        .pt_2()
        .border_t_1()
        .border_color(divider(cx))
        .child(
            h_flex()
                .w(rems(11.))
                .flex_none()
                .min_h(rems(1.875))
                .gap_1p5()
                .items_center()
                .child(
                    div()
                        .min_w_0()
                        .whitespace_normal()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().foreground)
                        .child(label.to_string()),
                )
                .when(!description.is_empty(), |d| {
                    d.child(info_tip(
                        SharedString::from(format!("setting-tip-{label}")),
                        description.to_string(),
                        cx,
                    ))
                }),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w(rems(14.))
                .max_w(rems(36.))
                .child(control),
        )
}

pub(super) fn row(cx: &App) -> Div {
    h_flex()
        .flex_wrap()
        .min_w_0()
        .gap_3()
        .px_3()
        .py_2()
        .items_center()
        .border_b_1()
        .border_color(divider(cx))
}
pub(super) fn empty_row(text: &str, cx: &App) -> Div {
    div()
        .px_1()
        .py_6()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .whitespace_normal()
        .child(text.to_string())
}
pub(super) fn record_row(glyph: Glyph, id: &str, status: &str, selected: bool, cx: &App) -> Div {
    let theme = cx.theme();
    row(cx)
        .when(selected, |d| d.bg(theme.list_active))
        .hover(move |s| s.bg(theme.list_hover))
        .child(Icon::new(glyph).small().text_color(theme.muted_foreground))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .whitespace_normal()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(id.to_string()),
        )
        .when(notable_status(status), |d| {
            d.child(badge(outcome_label(status).to_string(), tone(status, cx)))
        })
}

pub(super) struct CopyNotice {
    pub revision: u64,
    pub visible: bool,
}
impl gpui::Global for CopyNotice {}
pub(super) fn copy_acknowledged(text: String, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    let revision = cx.try_global::<CopyNotice>().map_or(1, |n| n.revision + 1);
    cx.set_global(CopyNotice {
        revision,
        visible: true,
    });
    cx.refresh_windows();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_secs(2)).await;
        cx.update(|cx| {
            if cx
                .try_global::<CopyNotice>()
                .is_some_and(|n| n.revision == revision)
            {
                cx.set_global(CopyNotice {
                    revision,
                    visible: false,
                });
                cx.refresh_windows();
            }
        });
    })
    .detach();
}
