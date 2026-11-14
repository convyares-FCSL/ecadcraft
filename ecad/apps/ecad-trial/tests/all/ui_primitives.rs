//! Brief 037: the trial draws its interface text in ECAD's fonts (IBM Plex Sans; Barlow Semi
//! Condensed registered for the sheet) over CADCraft's font set, with ECAD's style; and the hidden
//! `ui.primitives` page in the Developer section shows every primitive. A screenshot of it is
//! written to `$ECAD_TESTER_SHOTS` when that names a folder and a wgpu adapter exists (for the
//! owner's check beside the spec's Tokens board; never committed).

use std::sync::Arc;

use ecad_trial::developer;
use eframe::egui::{self, FontFamily, TextStyle};
use egui_kittest::kittest::Queryable;

use crate::mouse_tester::{Collect, Deltas, harness_with, shot};

#[test]
fn the_trial_uses_ecad_fonts_and_shows_the_primitives_page() {
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    h.run_steps(3);
    let ctx = h.ctx.clone();
    // ECAD's families are registered with the embedded faces, IBM Plex Sans first in Proportional,
    // and the text styles name them.
    let (ui_chain, sheet_chain, proportional) = ctx.fonts(|f| {
        let d = f.definitions();
        let chain = |name: &str| d.families.get(&FontFamily::Name(name.into())).cloned().unwrap_or_default();
        (chain(ecad_ui::fonts::UI), chain(ecad_ui::fonts::SHEET), d.families.get(&FontFamily::Proportional).cloned().unwrap_or_default())
    });
    assert_eq!(ui_chain.first().map(String::as_str), Some("ecad-ibm-plex-sans-400"));
    assert_eq!(sheet_chain.first().map(String::as_str), Some("ecad-barlow-semi-condensed-400"));
    assert_eq!(proportional.first().map(String::as_str), Some("ecad-ibm-plex-sans-400"));
    let body = ctx.global_style().text_styles.get(&TextStyle::Body).cloned();
    assert_eq!(body.map(|f| f.family), Some(FontFamily::Name(ecad_ui::fonts::UI.into())));
    assert_eq!(ctx.global_style().visuals.panel_fill, ecad_ui::color(ecad_ui::SHELL.panel));
    // The page: Developer ▸ ui.primitives.
    // Brief 038: the Developer section is the Text panel's Commands tab (Ctrl+/).
    h.state_mut().toggle_text_panel();
    h.run_steps(3);
    h.get_by_label(developer::PRIMITIVES).click();
    h.run_steps(5);
    for label in ["Primary normal", "Danger disabled", "Wire tool", "Pill warning", "Field focused", "Library", "Ctrl+K", "List row selected"] {
        assert!(h.query_all_by_label(label).next().is_some(), "{label} on the primitives page");
    }
    if let Some(dir) = std::env::var_os("ECAD_TESTER_SHOTS") {
        let path = std::path::PathBuf::from(dir).join("037-primitives.png");
        if shot(&h, &deltas, &path).is_none() {
            eprintln!("no wgpu adapter: screenshot skipped");
        }
    }
    // Folding it away again.
    h.get_by_label(developer::PRIMITIVES).click();
    h.run_steps(3);
    assert!(h.query_all_by_label("Primary normal").next().is_none());
}

/// The showcase alone, at a size where every state is visible, with the trial's fonts (ECAD's
/// over CADCraft's set for English) and style: the owner's screenshot beside the spec's Tokens
/// board. Written only when `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists.
#[test]
fn the_showcase_screenshot_for_the_owner() {
    let Some(dir) = std::env::var_os("ECAD_TESTER_SHOTS") else { return };
    let deltas: Deltas = Arc::default();
    let mut first = true;
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1100.0, 1500.0)).renderer(Collect(deltas.clone())).build_ui_state(
        move |ui, s: &mut ecad_ui::showcase::Showcase| {
            if std::mem::take(&mut first) {
                ecad_ui::fonts::install(ui.ctx(), cadcraft_ui_egui::theme::font_definitions("en"));
            }
            ecad_ui::style::apply(ui.ctx());
            ecad_ui::showcase::show(ui, s);
        },
        ecad_ui::showcase::Showcase::default(),
    );
    h.run_steps(4);
    let path = std::path::PathBuf::from(dir).join("037-showcase.png");
    if shot(&h, &deltas, &path).is_none() {
        eprintln!("no wgpu adapter: screenshot skipped");
    }
}
