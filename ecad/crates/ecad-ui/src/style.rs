//! egui's style from the tokens (design §3.2): visuals, spacing and text styles for **both** egui
//! themes from the Graphite shell, so the shell has one appearance whatever the operating system
//! prefers. Run [`apply`] once at start, and again when the host re-applies its own style (the
//! trial does it when CADCraft's theme or language changes, until U2's `style` switch keeps
//! CADCraft from touching the style at all).

use ecad_tokens::{line, radius, shadow, space, ui_type};
use egui::{CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Theme, Visuals, style::WidgetVisuals};

use crate::{SHELL, color, fonts};

/// A radius token as egui's corner radius (rounded to whole px, at most 255).
pub fn corner(px: f32) -> CornerRadius {
    CornerRadius::same(px.round().clamp(0.0, 255.0) as u8)
}

/// The one shadow style (popovers and the sheet).
pub fn shadow() -> Shadow {
    let i8_px = |v: f32| v.round().clamp(-128.0, 127.0) as i8;
    Shadow {
        offset: [i8_px(shadow::OFFSET_X), i8_px(shadow::OFFSET_Y)],
        blur: shadow::BLUR.round().clamp(0.0, 255.0) as u8,
        spread: 0,
        color: color(shadow::COLOR),
    }
}

fn widget(fill: ecad_tokens::Color, stroke: Stroke, text: ecad_tokens::Color) -> WidgetVisuals {
    WidgetVisuals {
        bg_fill: color(fill),
        weak_bg_fill: color(fill),
        bg_stroke: stroke,
        corner_radius: corner(radius::BUTTON),
        fg_stroke: Stroke::new(line::HAIRLINE_PX, color(text)),
        expansion: 0.0,
    }
}

/// The shell's visuals (the same for egui's dark and light themes).
pub fn visuals() -> Visuals {
    let s = SHELL;
    let hair = |c| Stroke::new(line::HAIRLINE_PX, color(c));
    let mut v = Visuals::dark();
    v.override_text_color = None;
    v.widgets.noninteractive = widget(s.panel, hair(s.line), s.text_2);
    v.widgets.inactive = widget(s.raised, Stroke::NONE, s.text);
    v.widgets.hovered = widget(s.hover, hair(s.line_strong), s.text);
    v.widgets.active = widget(s.active, hair(s.line_strong), s.text);
    v.widgets.open = widget(s.active, hair(s.line_strong), s.text);
    v.selection.bg_fill = color(s.accent_soft);
    v.selection.stroke = hair(s.accent_text);
    v.hyperlink_color = color(s.accent_text);
    v.faint_bg_color = color(s.raised);
    v.extreme_bg_color = color(s.bg);
    v.text_edit_bg_color = Some(color(s.raised));
    v.code_bg_color = color(s.raised);
    v.warn_fg_color = color(s.warning);
    v.error_fg_color = color(s.error);
    v.window_corner_radius = corner(radius::POPOVER);
    v.menu_corner_radius = corner(radius::POPOVER);
    v.window_shadow = shadow();
    v.popup_shadow = shadow();
    v.window_fill = color(s.panel);
    v.window_stroke = hair(s.line_strong);
    v.panel_fill = color(s.panel);
    v.text_cursor.stroke = Stroke::new(line::FOCUS_RING_PX, color(s.accent_text));
    v
}

/// The text styles from the UI type ramp, in ECAD's families when they are installed
/// ([`fonts::font`]); `Monospace` keeps the host's family at the Small size.
pub fn text_styles(ctx: &egui::Context) -> [(TextStyle, FontId); 5] {
    [
        (TextStyle::Small, fonts::font(ctx, ui_type::SMALL)),
        (TextStyle::Body, fonts::font(ctx, ui_type::BODY)),
        (TextStyle::Button, fonts::font(ctx, ui_type::BODY)),
        (TextStyle::Heading, fonts::font(ctx, ui_type::TITLE)),
        (TextStyle::Monospace, FontId::new(ui_type::SMALL.size, FontFamily::Monospace)),
    ]
}

/// Set ECAD's visuals, spacing and text styles on both egui themes. Spacing keeps the host's
/// layout (no layout changes in U1): item spacing and button padding take the nearest spacing
/// steps; control heights are the primitives' own ([`ecad_tokens::size::CONTROL`]).
///
/// # Semantics
/// Text styles name ECAD's families only once they are installed in `ctx` (before that, the same
/// sizes in `Proportional`), so call it again in the pass after [`fonts::install`].
pub fn apply(ctx: &egui::Context) {
    let v = visuals();
    ctx.set_visuals_of(Theme::Dark, v.clone());
    ctx.set_visuals_of(Theme::Light, v);
    let styles = text_styles(ctx);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(space::S, space::XS);
        s.spacing.button_padding = egui::vec2(space::S, space::XS);
        for (style, font) in &styles {
            s.text_styles.insert(style.clone(), font.clone());
        }
    });
}
