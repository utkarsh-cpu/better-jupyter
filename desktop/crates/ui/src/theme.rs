use super::*;

pub(super) fn apply_theme(dark: bool, high_contrast: bool, window: &mut Window, cx: &mut App) {
    Theme::change(
        if dark || high_contrast {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        Some(window),
        cx,
    );
    let accent: Hsla = rgb(if dark { 0x8AB5D6 } else { 0x315E83 }).into();
    let on_accent = if dark {
        hsla(232. / 360., 0.45, 0.10, 1.)
    } else {
        hsla(0., 0., 1., 1.)
    };
    let shade = |delta: f32| Hsla {
        l: (accent.l + delta).clamp(0., 1.),
        ..accent
    };
    let theme = Theme::global_mut(cx);
    let colors = &mut theme.colors;
    colors.background = rgb(if dark { 0x15181B } else { 0xFAFAF8 }).into();
    colors.foreground = rgb(if dark { 0xE6E9EB } else { 0x20262A }).into();
    colors.muted_foreground = rgb(if dark { 0xA8B0B7 } else { 0x59636B }).into();
    colors.muted = rgb(if dark { 0x1C2024 } else { 0xF1F2EF }).into();
    colors.sidebar = colors.muted;
    colors.sidebar_foreground = colors.foreground;
    colors.border = rgb(if dark { 0x68757F } else { 0x849097 }).into();
    colors.sidebar_border = colors.border;
    colors.title_bar = colors.muted;
    colors.title_bar_border = colors.border;
    colors.status_bar = colors.muted;
    colors.status_bar_border = colors.border;
    colors.popover = rgb(if dark { 0x23282D } else { 0xFFFFFF }).into();
    colors.popover_foreground = colors.foreground;
    colors.primary = accent;
    colors.primary_hover = shade(-0.05);
    colors.primary_active = shade(-0.10);
    colors.primary_foreground = on_accent;
    colors.button_primary = accent;
    colors.button_primary_hover = shade(-0.05);
    colors.button_primary_active = shade(-0.10);
    colors.button_primary_foreground = on_accent;
    colors.ring = accent;
    colors.link = accent;
    colors.list_active = accent.opacity(0.12);
    colors.list_active_border = accent;
    colors.sidebar_primary = accent;
    colors.sidebar_primary_foreground = on_accent;
    colors.overlay = hsla(0., 0., 0., if dark { 0.5 } else { 0.22 });
    if high_contrast {
        let black = hsla(0., 0., 0., 1.);
        let white = hsla(0., 0., 1., 1.);
        let yellow = hsla(1. / 6., 1., 0.5, 1.);
        colors.background = black;
        colors.foreground = white;
        colors.muted = black;
        colors.muted_foreground = white;
        colors.popover = black;
        colors.popover_foreground = white;
        colors.sidebar = black;
        colors.sidebar_foreground = white;
        colors.title_bar = black;
        colors.title_bar_border = white;
        colors.status_bar = black;
        colors.status_bar_border = white;
        colors.border = white;
        colors.sidebar_border = white;
        colors.primary = yellow;
        colors.primary_foreground = black;
        colors.button_primary = yellow;
        colors.button_primary_foreground = black;
        colors.ring = yellow;
        colors.link = yellow;
        colors.list_active = hsla(0., 0., 0.2, 1.);
        colors.list_active_border = yellow;
        colors.danger = hsla(0., 1., 0.75, 1.);
        colors.warning = yellow;
        colors.info = hsla(0.5, 1., 0.75, 1.);
    }
    theme.font_family = "Segoe UI".into();
    theme.mono_font_family = "Consolas".into();
    theme.font_size = px(14.);
    theme.mono_font_size = px(14.);
    theme.radius = px(4.);
    theme.radius_lg = px(6.);
    theme.tokens = ThemeTokens::from(&theme.colors);
    window.refresh();
}
