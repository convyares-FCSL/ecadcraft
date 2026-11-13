//! The egui style from the tokens: both themes alike, the Graphite colours, the shadow.

use ecad_tokens::GRAPHITE;
use ecad_ui::color;
use egui::Theme;

#[test]
fn both_egui_themes_get_the_graphite_shell() {
    let ctx = egui::Context::default();
    ecad_ui::style::apply(&ctx);
    for theme in [Theme::Dark, Theme::Light] {
        let v = ctx.style_of(theme).visuals.clone();
        assert_eq!(v.panel_fill, color(GRAPHITE.panel), "{theme:?}");
        assert_eq!(v.widgets.inactive.bg_fill, color(GRAPHITE.raised));
        assert_eq!(v.widgets.hovered.bg_fill, color(GRAPHITE.hover));
        assert_eq!(v.selection.bg_fill, color(GRAPHITE.accent_soft));
        assert_eq!(v.text_edit_bg_color, Some(color(GRAPHITE.raised)));
        assert_eq!(v.window_shadow.offset, [0, 8]);
        assert_eq!(v.window_shadow.blur, 24);
        assert_eq!(v.text_cursor.stroke.width, ecad_tokens::line::FOCUS_RING_PX);
    }
    assert!(ctx.style_of(Theme::Dark).visuals == ctx.style_of(Theme::Light).visuals, "one appearance");
}
