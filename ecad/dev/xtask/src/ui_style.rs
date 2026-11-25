//! ECAD's UI sources (App and Programs) take their look from the `ecad-widgets` tokens and their value
//! controls from `ecad_widgets::control`: no local styling, and no raw egui value control outside the files
//! still listed in [`RAW_CONTROL_FILES`].

use super::*;

/// Styling written in place, which ECAD UI code may no longer use: colours, sizes and fonts come from the tokens of `ecad-widgets`.
/// `Tokens::get` reads CADCraft's theme colours directly; the `from_rgba` entries cover every RGBA
/// colour constructor (`Color32::from_rgba_unmultiplied`, `Rgba::from_rgba_premultiplied`, …), not an
/// image's pixels. A plain ban: no file may style locally.
pub(crate) const LOCAL_STYLE: &[&str] = &[
    "Color32::from_rgb",
    "Color32::from_gray",
    "Color32::from_rgba",
    "Rgba::from_rgba",
    "FontId::new",
    "FontId::proportional",
    "FontId::monospace",
    "CornerRadius",
    "Rounding::",
    "Tokens::get",
    // egui's named colours are local styling too.
    "Color32::WHITE",
    "Color32::GRAY",
    "Color32::RED",
    "Color32::YELLOW",
];

/// Raw egui value controls (design `docs/design/components.md`): panels and dialogs use
/// `ecad_widgets::control` (text entry, number with unit, selector, toggle, value box) instead, so
/// anything the person can change looks editable and a read value never looks like an input.
pub(crate) const RAW_CONTROL: &[&str] = &["TextEdit::", "ComboBox::"];

/// ECAD UI files that still use raw egui value controls ([`RAW_CONTROL`]): panels made before the
/// components, converted to `ecad_widgets::control` in a later pass. No counts: a listed file may keep
/// its controls, any other file may not, and a listed file that has none left fails until it is removed.
pub(crate) const RAW_CONTROL_FILES: &[&str] = &[
    "crates/ecad-app/src/developer/mod.rs",
    "crates/ecad-app/src/library/builder.rs",
    "crates/ecad-app/src/library/details.rs",
    "crates/ecad-app/src/settings/style_catalogues.rs",
    "crates/ecad-app/src/pages/project_panel.rs",
    "crates/ecad-app/src/publish/mod.rs",
    "crates/ecad-app/src/library/search.rs",
    "crates/ecad-app/src/settings/project.rs",
    "crates/ecad-app/src/settings/standards.rs",
    "crates/ecad-app/src/shell/placing.rs",
    "crates/ecad-app/src/inspector/layout_tab.rs",
    "crates/ecad-app/src/inspector/mod.rs",
    "crates/ecad-app/src/settings/text_styles.rs",
];

/// The places in one ECAD UI source file (`rel`, `/` separated) that style locally
/// ([`LOCAL_STYLE`]), as `rel:line: text`; comment lines and `//` comments are skipped.
pub(crate) fn local_style_places(rel: &str, text: &str) -> Vec<String> {
    places_matching(rel, text, LOCAL_STYLE)
}

/// The places in one ECAD UI source file that use a raw egui value control ([`RAW_CONTROL`]),
/// as `rel:line: text`; comment lines and `//` comments are skipped.
pub(crate) fn raw_control_places(rel: &str, text: &str) -> Vec<String> {
    places_matching(rel, text, RAW_CONTROL)
}
