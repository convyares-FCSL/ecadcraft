// ECAD-SEAM(owner): font definitions builder (US2). `install_fonts` installs exactly the set
// `font_definitions` builds, so an embedder can extend that set and install it itself.

use cadcraft_ui_egui::theme::{font_definitions, install_fonts};

/// The fonts in use after `install_fonts` are `font_definitions`' result, for English and for a
/// language with extra fallbacks; and the set is a value an embedder can extend.
#[test]
fn install_fonts_sets_exactly_the_font_definitions() {
    for language in ["en", "ja"] {
        let ctx = egui::Context::default();
        install_fonts(&ctx, language);
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        let installed = ctx.fonts(|f| f.definitions().clone());
        assert!(installed == font_definitions(language), "{language}: installed fonts differ from font_definitions");
    }
    let mut extended = font_definitions("en");
    extended.font_data.insert("embedder-ui".into(), std::sync::Arc::new(egui::FontData::from_static(&[])));
    assert!(extended != font_definitions("en"), "the builder's result is an independent value");
}
