use super::*;

/// Vertical rhythm of a notebook cell, shared by the notebook and the Settings preview.
///
/// Compact tightens the space inside a cell; the gap between cells stays larger
/// than any gap within one, so successive cells still read as separate units.
/// Pointer targets are owned by `action_button`, never by these values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CellSpacing {
    /// Header row minimum height, in rems.
    pub header_min_h: f32,
    pub header_py: f32,
    pub source_top: f32,
    pub source_bottom: f32,
    pub output_py: f32,
    pub output_gap: f32,
    pub boundary: f32,
    pub boundary_last: f32,
}

pub(crate) fn cell_spacing(comfortable: bool) -> CellSpacing {
    if comfortable {
        CellSpacing {
            header_min_h: 2.75,
            header_py: 2.,
            source_top: 6.,
            source_bottom: 6.,
            output_py: 8.,
            output_gap: 8.,
            boundary: 28.,
            boundary_last: 44.,
        }
    } else {
        CellSpacing {
            header_min_h: 1.75,
            header_py: 0.,
            source_top: 2.,
            source_bottom: 2.,
            output_py: 4.,
            output_gap: 6.,
            boundary: 14.,
            boundary_last: 32.,
        }
    }
}

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
    // Quiet ledger: warm paper canvas, precision cobalt, chrome that recedes.
    let accent: Hsla = rgb(if dark { 0x6CB6FF } else { 0x0B5CAB }).into();
    let on_accent = if dark {
        hsla(215. / 360., 0.50, 0.09, 1.)
    } else {
        hsla(0., 0., 1., 1.)
    };
    let shade = |delta: f32| Hsla {
        l: (accent.l + delta).clamp(0., 1.),
        ..accent
    };
    let theme = Theme::global_mut(cx);
    let colors = &mut theme.colors;
    colors.background = rgb(if dark { 0x101317 } else { 0xF3F2ED }).into();
    colors.foreground = rgb(if dark { 0xECEEF1 } else { 0x161B21 }).into();
    colors.muted_foreground = rgb(if dark { 0xA6B0B9 } else { 0x505A62 }).into();
    colors.muted = rgb(if dark { 0x171B21 } else { 0xE8E6E0 }).into();
    colors.danger = rgb(if dark { 0xFF9A9A } else { 0xB4232C }).into();
    colors.success = rgb(if dark { 0x8ED6A4 } else { 0x1E6F43 }).into();
    colors.warning = rgb(if dark { 0xE8C078 } else { 0x7D5200 }).into();
    colors.info = accent;
    // Sidebars, title, and status share the canvas. Hairlines carry hierarchy.
    colors.sidebar = colors.background;
    colors.sidebar_foreground = colors.foreground;
    colors.border = rgb(if dark { 0x6B7784 } else { 0x7A868F }).into();
    colors.sidebar_border = colors.border;
    colors.title_bar = colors.background;
    colors.title_bar_border = colors.border;
    colors.status_bar = colors.background;
    colors.status_bar_border = colors.border;
    colors.popover = rgb(if dark { 0x1A1F25 } else { 0xFFFEFB }).into();
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
    colors.list_hover = if dark {
        hsla(210. / 360., 0.18, 0.85, 0.07)
    } else {
        hsla(210. / 360., 0.18, 0.15, 0.05)
    };
    colors.sidebar_primary = accent;
    colors.sidebar_primary_foreground = on_accent;
    colors.overlay = hsla(215. / 360., 0.20, 0.04, if dark { 0.56 } else { 0.26 });
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
        // Selected and hovered rows are dark bands. They are never the only
        // selection cue: list rows draw a 2px accent left border, because a
        // neutral fill cannot clear 3:1 against black in both directions
        // (yellow text needs a dark fill, a dark fill cannot read as a band).
        colors.list_active = hsla(0., 0., 0.22, 1.);
        colors.list_active_border = yellow;
        colors.list_hover = hsla(0., 0., 0.18, 1.);
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
    // Pinned Kit 0.7 paints the active line even in unfocused editors and
    // offers no per-Editor override. The caret and source focus rail carry
    // editing focus; suppress the misleading persistent row wash.
    highlight.style.editor_active_line = None;
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
    theme.mono_font_size = px(13.);
    theme.radius = px(6.);
    theme.radius_lg = px(8.);
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
