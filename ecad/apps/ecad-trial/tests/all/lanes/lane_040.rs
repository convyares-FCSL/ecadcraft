//! Brief 040 (U4a): the canvas in the trial. The paper sheet and CADCraft's view switches at
//! 1920 × 1080, `ui.sheet` switching to the dark sheet and remembered, the painter reading the
//! project only when its key moves, the related placements of a selected device, nothing painted
//! while the page is hidden, and the first-open zoom.

use cadcraft_engine::doc::Handle;
use ecad_model::{ComponentId, PageId, RepresentationId, Subject};
use ecad_trial::Trial;
use ecad_trial::canvas::{self, Sheet};
use ecad_trial::shell::state::{ShellState, Workspace};
use eframe::egui::vec2;
use egui_kittest::Harness;
use serde_json::{Value, json};

use crate::project_panel::{click, trial};

fn harness() -> Harness<'static, Trial> {
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        trial(),
    );
    h.run_steps(5);
    h
}

fn exec(h: &mut Harness<'static, Trial>, cmd: &str, params: Value) -> Value {
    let v = h.state_mut().cad.session.execute(cmd, &params).unwrap_or_else(|e| panic!("{cmd} {params}: {e}"));
    h.run_steps(2);
    v
}

fn hex_id<T: From<u128>>(v: &Value, key: &str) -> T {
    let s = v[key].as_str().unwrap_or_else(|| panic!("{key} in {v}"));
    let hex = s.rsplit('-').next().unwrap_or(s);
    T::from(u128::from_str_radix(hex, 16).unwrap())
}

fn reads(h: &Harness<'static, Trial>) -> u64 {
    canvas::with_state(&h.ctx, |s| s.reads)
}

/// Whether any text painted in the last frame contains `needle`.
fn painted_text(h: &Harness<'static, Trial>, needle: &str) -> bool {
    fn walk(s: &eframe::egui::Shape, needle: &str) -> bool {
        match s {
            eframe::egui::Shape::Text(t) => t.galley.text().contains(needle),
            eframe::egui::Shape::Vec(v) => v.iter().any(|s| walk(s, needle)),
            _ => false,
        }
    }
    h.output().shapes.iter().any(|c| walk(&c.shape, needle))
}

#[test]
fn the_page_is_paper_without_viewcube_ucs_icon_label_or_grid_and_dark_is_one_command_away() {
    let mut h = harness();
    let paper = ecad_ui::color(ecad_ui::tokens::sheet::PAPER.sheet);
    {
        let t = h.state();
        assert_eq!(t.cad.options.model_background, Some(paper), "#F6F4EF");
        assert_eq!(t.cad.options.selection_color, Some(ecad_ui::color(ecad_ui::tokens::sheet::PAPER.selection)));
        assert!(!t.cad.ui.show_viewcube && !t.cad.ui.show_ucs_icon, "no ViewCube, no UCS icon");
        assert!(!t.cad.session.settings.gridmode, "CADCraft's grid off: ECAD paints the dots");
    }
    assert!(!painted_text(&h, "2D Wireframe"), "no viewport label");
    // `ui.sheet dark` in the Text panel's Commands tab: the dark set, no restart, remembered.
    h.state_mut().shell.state.text_panel = true;
    h.run_steps(2);
    h.state_mut().command = "ui.sheet dark".into();
    h.run_steps(1);
    click(&mut h, "Run");
    h.run_steps(2);
    assert_eq!(h.state().shell_state().sheet, Sheet::Dark);
    assert_eq!(h.state().cad.options.model_background, Some(ecad_ui::color(ecad_ui::tokens::sheet::DARK.sheet)));
    let stored = h.state().shell_state().to_stored();
    assert_eq!(ShellState::from_stored(Some(&stored)).0.sheet, Sheet::Dark, "kept with the shell state");
    assert_eq!(ShellState::from_stored(Some("{}")).0.sheet, Sheet::Paper, "paper by default");
    // The developer switch brings CADCraft's own view furniture back.
    h.state_mut().set_cad_chrome(true);
    h.run_steps(3);
    assert!(h.state().cad.ui.show_viewcube && h.state().cad.ui.show_ucs_icon);
}

/// The coil `-K01` and two contacts on page 1, a third contact on page 2; back on page 1.
fn device_on_two_pages(h: &mut Harness<'static, Trial>) -> (ComponentId, PageId, PageId, Vec<Handle>) {
    let k: ComponentId = hex_id(&exec(h, "place", json!({"symbol": "coil", "at": [60, 200]})), "component");
    exec(h, "place", json!({"symbol": "no", "at": [160, 200], "device": k.hex()}));
    exec(h, "place", json!({"symbol": "nc", "at": [220, 200], "device": k.hex()}));
    let p1 = h.state().panel.active.expect("a page");
    let p2: PageId = hex_id(&exec(h, "newpage", json!({"title": "P2"})), "page");
    exec(h, "place", json!({"symbol": "no", "at": [160, 200], "device": k.hex()}));
    h.state_mut().show_page(p1).unwrap();
    h.run_steps(3);
    let handle = h.state().project().cloned().unwrap();
    let coil = handle
        .read(&mut h.state_mut().cad.session, |p, _| {
            let doc = p.ledger.document();
            let (rid, _) = doc.representations_of(Subject::Component(k)).find(|(_, r)| r.page == p1).unwrap();
            ecad_cadcraft::device_list::symbol_target(p, rid).unwrap().2
        })
        .unwrap();
    (k, p1, p2, coil)
}

#[test]
fn selecting_a_placement_outlines_the_others_here_and_lists_the_rest() {
    let mut h = harness();
    let (k, p1, p2, coil) = device_on_two_pages(&mut h);
    assert!(!coil.is_empty());
    h.state_mut().cad.session.set_selection(coil.clone());
    h.run_steps(3);
    let related = canvas::with_state(&h.ctx, |s| s.page.related.clone());
    assert_eq!(related.devices, vec![k]);
    assert_eq!(related.here.len(), 2, "both contacts on page 1 get an outline: {related:?}");
    assert_eq!(related.elsewhere.len(), 1, "the contact on page 2 is listed");
    assert_eq!(related.elsewhere[0].0, p2);
    // The function itself, as 042's inspector calls it.
    let handle = h.state().project().cloned().unwrap();
    let again = handle.read(&mut h.state_mut().cad.session, |p, _| canvas::related(p, p1, &coil)).unwrap();
    assert_eq!(again, related);
    let _: Option<RepresentationId> = again.elsewhere.first().map(|e| e.1);
}

#[test]
fn the_painter_reads_the_project_only_when_its_key_moves() {
    let mut h = harness();
    let (_, _, _, coil) = device_on_two_pages(&mut h);
    h.run_steps(3);
    let before = reads(&h);
    h.run_steps(300);
    assert_eq!(reads(&h), before, "300 frames, same change count and selection: no read");
    h.state_mut().cad.session.set_selection(coil);
    h.run_steps(3);
    assert_eq!(reads(&h), before + 1, "a new selection: one read");
    h.run_steps(50);
    assert_eq!(reads(&h), before + 1);
}

#[test]
fn nothing_is_painted_while_lists_library_or_publish_hide_the_page() {
    let mut h = harness();
    for w in [Workspace::Library, Workspace::Publish] {
        h.state_mut().shell.state.workspace = w;
        h.run_steps(3);
        assert!(h.state().shell.page_rect.is_none());
        let before = reads(&h);
        exec(&mut h, "place", json!({"symbol": "coil", "at": [60, 120]}));
        h.run_steps(3);
        assert_eq!(reads(&h), before, "{w:?}: the hidden page is not read");
    }
}

#[test]
fn a_page_first_opens_where_a_tag_is_13_px_tall() {
    let mut h = harness();
    exec(&mut h, "newpage", json!({"title": "Fresh"}));
    h.run_steps(5);
    let (Some(rect), Some(xf)) = (h.state().cad.canvas.rect, h.state().cad.canvas.xf) else { panic!("drawn") };
    assert!(rect.height() > 0.0);
    let tag_px = 2.5 * xf.scale;
    assert!((tag_px - canvas::TAG_PX).abs() < 0.5, "a 2.5 mm tag is {tag_px} px");
}
