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
    let accent: Hsla = rgb(if dark { 0x58A6FF } else { 0x006ED6 }).into();
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
    colors.danger = rgb(if dark { 0xFF9A9A } else { 0xB42332 }).into();
    colors.success = rgb(if dark { 0x8ED6A4 } else { 0x247044 }).into();
    colors.warning = rgb(if dark { 0xE8C078 } else { 0x825500 }).into();
    colors.info = accent;
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
        colors.primary_hover = yellow;
        colors.primary_active = white;
        colors.button_primary_hover = yellow;
        colors.button_primary_active = white;
        colors.ring = yellow;
        colors.link = yellow;
        colors.list_active = hsla(0., 0., 0.2, 1.);
        colors.list_active_border = yellow;
        colors.danger = hsla(0., 1., 0.75, 1.);
        colors.success = rgb(0x98FFAA).into();
        colors.warning = yellow;
        colors.info = hsla(0.5, 1., 0.75, 1.);
    }
    colors.danger_foreground = if dark || high_contrast {
        hsla(0., 0., 0., 1.)
    } else {
        hsla(0., 0., 1., 1.)
    };
    colors.danger_hover = Hsla {
        l: (colors.danger.l + if dark || high_contrast { 0.05 } else { -0.05 }).clamp(0., 1.),
        ..colors.danger
    };
    colors.danger_active = colors.danger_hover;
    colors.button_danger = colors.danger;
    colors.button_danger_hover = colors.danger_hover;
    colors.button_danger_active = colors.danger_active;
    colors.button_danger_foreground = colors.danger_foreground;
    let background = colors.background;
    let foreground = colors.foreground;
    let muted = colors.muted;
    let muted_foreground = colors.muted_foreground;
    let mut highlight = (*theme.highlight_theme).clone();
    highlight.style.editor_background = Some(background);
    highlight.style.editor_foreground = Some(foreground);
    highlight.style.editor_gutter_background = Some(background);
    highlight.style.editor_active_line = Some(muted);
    highlight.style.editor_line_number = Some(muted_foreground);
    highlight.style.editor_active_line_number = Some(foreground);
    // Audit every Kit syntax style against the source canvas and active line.
    // Keep usable syntax colors; use normal foreground when a supplied style is too faint.
    let mut syntax =
        serde_json::to_value(&highlight.style.syntax).expect("Syntax styles serialize");
    for style in syntax
        .as_object_mut()
        .expect("Syntax styles are an object")
        .values_mut()
    {
        if let Some(color) = style
            .get("color")
            .and_then(|c| serde_json::from_value::<Hsla>(c.clone()).ok())
            && (contrast_ratio(color, background) < 4.5 || contrast_ratio(color, muted) < 4.5)
        {
            style["color"] = serde_json::to_value(foreground).expect("Theme color serializes");
        }
    }
    highlight.style.syntax =
        serde_json::from_value(syntax).expect("Audited syntax styles deserialize");
    theme.highlight_theme = std::sync::Arc::new(highlight);
    theme.font_family = "Segoe UI".into();
    theme.mono_font_family = "Consolas".into();
    theme.font_size = px(14.);
    theme.mono_font_size = px(14.);
    theme.radius = px(6.);
    theme.radius_lg = px(10.);
    theme.shadow = !high_contrast;
    theme.tokens = ThemeTokens::from(&theme.colors);
    Theme::sync_base(cx);
    window.refresh();
}

fn contrast_ratio(a: Hsla, b: Hsla) -> f32 {
    let luminance = |color: Hsla| {
        let color = color.to_rgb();
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
