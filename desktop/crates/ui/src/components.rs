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
            .rounded(px(7.))
            .font_weight(FontWeight::MEDIUM)
            .min_h(rems(1.875))
            .min_w(rems(1.75))
            .flex_none(),
        id,
        disabled: false,
        name: None,
        treatment: ButtonTreatment::Secondary,
        a11y_click: None,
    }
}

/// Kit correctly blocks disabled activation, but this GPUI version does not
/// expose an aria-disabled setter. Populate the existing AccessKit node directly.
#[derive(IntoElement)]
pub(super) struct WorkbenchButton {
    inner: Button,
    id: ElementId,
    disabled: bool,
    name: Option<SharedString>,
    treatment: ButtonTreatment,
    a11y_click: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
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
    pub fn menu_label(mut self, label: impl Into<SharedString>) -> Self {
        let text = label.into();
        self.name = Some(text.clone());
        self.inner = self
            .inner
            .accessibility_label(text.clone())
            .child(div().w_full().text_left().child(text));
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
            .rounded(px(6.));
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
                    .border_color(if high_contrast {
                        theme.border
                    } else {
                        divider(cx)
                    });
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
                        if high_contrast {
                            theme.ring
                        } else {
                            divider(cx)
                        }
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
        let a11y_click = self.a11y_click.take();
        let a11y_name = self.name.clone();
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
            div()
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
                .child(self.inner.role(None))
                .into_any_element()
        } else if let Some(listener) = a11y_click {
            // Invoke is handled here because this button type cannot register
            // an accessibility action on its own node. The visible button keeps
            // pointer and test identity; this node is what UI Automation invokes.
            div()
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
                .child(self.inner.role(None))
                .into_any_element()
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
            .py_1()
            .max_w(px(360.))
            .rounded(px(6.))
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .border_1()
            .border_color(divider(cx))
            .when(!high_contrast, |d| d.shadow_sm())
            .text_xs()
            .child(self.0.clone())
    }
}

pub(super) fn divider(cx: &App) -> Hsla {
    let color = cx.theme().border;
    // High contrast keeps explicit boundaries; decorative lines elsewhere are
    // deliberately independent of the focus ring and control-border roles.
    if cx.theme().background.l == 0. {
        color
    } else {
        color.opacity(0.24)
    }
}

pub(super) fn outcome_label(value: &str) -> &str {
    match value {
        "ok" => "Succeeded",
        "error" => "Failed",
        "complete" => "Complete",
        "idle" => "Ready",
        "running" => "Running",
        "verified" => "Active",
        _ => value,
    }
}

pub(super) fn badge(label: impl Into<SharedString>, color: Hsla) -> Div {
    h_flex()
        .flex_none()
        .min_h(rems(1.125))
        .px(px(6.))
        .py(px(1.5))
        .rounded(px(5.))
        .bg(color.opacity(0.11))
        .border_1()
        .border_color(color.opacity(0.22))
        .items_center()
        .text_color(color)
        .text_size(rems(0.75))
        .font_weight(FontWeight::MEDIUM)
        .child(label.into())
}
pub(super) fn dot(color: Hsla) -> Div {
    div().flex_none().size(px(7.)).rounded_full().bg(color)
}
pub(super) fn icon_tile(glyph: Glyph, size: f32, cx: &App) -> Div {
    let primary = cx.theme().primary;
    div()
        .flex_none()
        .size(px(size))
        .rounded(px(size / 4.))
        .flex()
        .items_center()
        .justify_center()
        .bg(primary.opacity(0.10))
        .border_1()
        .border_color(primary.opacity(0.20))
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
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().foreground)
            .child(title.to_string()),
    )
}
pub(super) fn card(cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .border_1()
        .rounded(px(10.))
        .border_color(divider(cx))
        .bg(theme.popover)
        .overflow_hidden()
}

/// Wraps native buttons so keyboard activation and selected state stay intact.
/// Wrapping is intentional at large interface sizes and narrow window widths.
pub(super) fn segmented_track(cx: &App) -> Div {
    h_flex()
        .self_start()
        .min_w_0()
        .flex_wrap()
        .gap(px(2.))
        .p(px(3.))
        .rounded(px(8.))
        .bg(cx.theme().muted)
        .border_1()
        .border_color(divider(cx))
}

pub(super) fn settings_group(title: &str, cx: &App) -> Div {
    section(title, cx)
        .w_full()
        .min_w_0()
        .p_4()
        .gap_3()
        .rounded(px(10.))
        .bg(cx.theme().popover)
        .border_1()
        .border_color(divider(cx))
}

pub(super) fn setting_row(
    label: &str,
    description: &str,
    control: impl IntoElement,
    cx: &App,
) -> Div {
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1p5()
        .pt_2()
        .border_t_1()
        .border_color(divider(cx))
        .child(
            v_flex()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().foreground)
                        .child(label.to_string()),
                )
                .when(!description.is_empty(), |d| {
                    d.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(description.to_string()),
                    )
                }),
        )
        .child(control)
}

pub(super) fn row(cx: &App) -> Div {
    h_flex()
        .flex_wrap()
        .min_w_0()
        .gap_3()
        .px_4()
        .py_2p5()
        .items_center()
        .border_b_1()
        .border_color(divider(cx))
}
pub(super) fn empty_row(text: &str, cx: &App) -> Div {
    div()
        .px_4()
        .py_5()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_string())
}
pub(super) fn record_row(glyph: Glyph, id: &str, status: &str, cx: &App) -> Div {
    let theme = cx.theme();
    row(cx)
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
        .when(!status.is_empty(), |d| {
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
