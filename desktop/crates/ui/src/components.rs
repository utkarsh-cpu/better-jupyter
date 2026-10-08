use super::*;

pub(super) fn badge(label: impl Into<SharedString>, color: Hsla) -> Div {
    h_flex()
        .flex_none()
        .h(px(18.))
        .px(px(6.))
        .items_center()
        .rounded(px(4.))
        .bg(color.opacity(0.12))
        .text_color(color)
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .child(label.into())
}
pub(super) fn dot(color: Hsla) -> Div {
    div().flex_none().size(px(7.)).rounded_full().bg(color)
}
pub(super) fn vsep(color: Hsla) -> Div {
    div().flex_none().w_px().h(px(18.)).mx_1().bg(color)
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
        .bg(primary.opacity(0.1))
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
        .border_color(color.opacity(0.25))
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
        .border_t_1()
        .border_color(theme.border)
        .bg(theme.background)
        .overflow_hidden()
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
        .border_color(cx.theme().border)
}
pub(super) fn empty_row(text: &str, cx: &App) -> Div {
    div()
        .px_4()
        .py_4()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_string())
}
pub(super) fn record_row(glyph: Glyph, id: &str, status: &str, cx: &App) -> Div {
    let theme = cx.theme();
    row(cx)
        .child(Icon::new(glyph).small().text_color(theme.muted_foreground))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .whitespace_normal()
                .text_sm()
                .child(id.to_string()),
        )
        .child(badge(status.to_string(), tone(status, cx)))
}
pub(super) fn setting_label(title: &str, description: &str, cx: &App) -> Div {
    v_flex()
        .flex_1()
        .min_w_0()
        .whitespace_normal()
        .gap_0p5()
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(description.to_string()),
        )
}
