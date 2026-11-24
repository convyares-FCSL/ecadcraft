//! Headless in the app: the demo's locations in Properties, its
//! location box and the prefix, the Checks fix, the Reports window with Export, generated pages
//! and Print Book. Screenshots are written when `ECAD_TESTER_SHOTS` names a folder and a wgpu
//! adapter exists (with the printed demo); they are never
//! committed.

use std::path::PathBuf;
use std::sync::Arc;

use ecad_electrical::checks::Kind;
use ecad_model::PageId;
use egui_kittest::Harness;
use {ecad_app::pages::project_panel, ecad_app::reports::panel};

use crate::canvas::mouse_acceptance::{Deltas, doc, harness_with, ok, shot};
use crate::checks::ui::demo_file;

type H = Harness<'static, ecad_app::shell::app::App>;

fn out() -> Option<PathBuf> {
    let dir = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from)?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn save(h: &H, deltas: &Deltas, name: &str) {
    if let Some(dir) = out()
        && shot(h, deltas, &dir.join(name)).is_none()
    {
        eprintln!("no wgpu adapter: {name} skipped");
    }
}

fn page(h: &mut H, n: usize) -> PageId {
    doc(h).page_order()[n]
}

fn show(h: &mut H, p: PageId) {
    h.state_mut().show_page(p).unwrap();
    h.run_steps(2);
    ok(h, "zoom e");
    h.run_steps(3);
}

fn view(h: &mut H, x0: f64, y0: f64, x1: f64, y1: f64) {
    let height = (y1 - y0).max((x1 - x0) * 800.0 / 520.0);
    let per_px = height / 800.0;
    if let Ok(st) = h.state_mut().cad.session.state_mut() {
        let centre = cadcraft_engine::geom::Vec2::new((x0 + x1) / 2.0 - 70.0 * per_px, (y0 + y1) / 2.0);
        let _ = st.set_view(cadcraft_engine::View { center: centre, height, twist: 0.0 });
    }
    h.run_steps(3);
}

fn main_symbol(h: &mut H, tag: &str) -> ecad_model::RepresentationId {
    let d = doc(h);
    let c = d.components_tagged(tag)[0];
    ecad_electrical::symbols::reps::goto(&d, c, None).unwrap()
}

#[test]
fn the_demo_locations_reports_generated_pages_and_print_book() {
    ecad_engine::export::printbook::install_io();
    let dir = std::env::temp_dir().join(format!("ecad-024-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = demo_file(&dir);
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    h.state_mut().close_project();
    h.run_steps(2);
    h.state_mut().open_project(&path).unwrap();
    h.run_steps(3);

    // 1. -K01's coil: brief 052d, it took the project's +CP1 when it was placed (a proposal, now
    // stored on the device); nothing is inherited, so no proposal is offered for it any more.
    let k1 = main_symbol(&mut h, "-K01");
    h.state_mut().goto_symbol(k1).unwrap();
    h.run_steps(4);
    let rows = h.state().props.lock().unwrap().view.sections.clone();
    let loc = rows.iter().flat_map(|s| s.rows.iter()).find(|r| r.key == "location").unwrap().clone();
    assert_eq!((loc.value.as_str(), loc.inherited), ("+CP1", false), "{loc:?}");
    assert_eq!(loc.proposal, None, "{loc:?}");
    save(&h, &deltas, "024-props-location.png");
    // Take every device's proposal (its go-to symbol's context), as the user would, so the rest
    // of the demo has its locations stored.
    let d = doc(&mut h);
    let mut proposed: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for c in d.devices().keys().copied().filter(|c| ecad_electrical::locations::is_device(&d, *c)) {
        let Some(r) = ecad_electrical::symbols::reps::goto(&d, c, None) else { continue };
        let ctx = ecad_electrical::locations::rep_context(&d, &d.representations()[&r], &mut ecad_electrical::locations::BoxCache::default());
        if let Some(v) = ctx.value {
            proposed.entry(v).or_default().push(c.hex());
        }
    }
    assert!(proposed.contains_key("+CP1"), "{proposed:?}");
    for (v, cs) in proposed {
        ok(&mut h, &format!(r#"setlocation {{"devices":{},"value":"{v}"}}"#, serde_json::json!(cs)));
    }

    // 2. Page 3: the box +JB1 round -X02, a polyline box added, and -K01's NC contact prefixed.
    let p3 = page(&mut h, 2);
    show(&mut h, p3);
    ok(&mut h, r#"locationbox {"location":"+JB2","points":[[110,60],[150,60],[160,75],[150,92],[110,92]]}"#);
    let d = doc(&mut h);
    let m1 = d.components_tagged("-M01")[0];
    // Brief 052: the new box proposes +JB2 for -M01; its stored location does not change.
    let m1_rep = ecad_electrical::symbols::reps::goto(&d, m1, None).unwrap();
    let m1_ctx = ecad_electrical::locations::rep_context(&d, &d.representations()[&m1_rep], &mut ecad_electrical::locations::BoxCache::default());
    assert_eq!(m1_ctx.value.as_deref(), Some("+JB2"));
    assert_ne!(ecad_electrical::locations::resolve(&d, m1).value.as_deref(), Some("+JB2"), "a box gives nothing");
    let texts: Vec<String> = h
        .state()
        .cad
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            cadcraft_engine::doc::EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect();
    // Brief 052 (design 052 §5.8): -K01's stored function =PUMP differs from the page's (none),
    // so it shows before the location; -M01 keeps its stored +FIELD inside the new +JB2 box.
    assert!(texts.contains(&"=PUMP+CP1-K01".to_string()), "{texts:?}");
    assert!(texts.contains(&"+FIELD-M01".to_string()), "{texts:?}");
    assert!(texts.contains(&"+JB1".to_string()) && texts.contains(&"+JB2".to_string()), "box labels");
    view(&mut h, 30.0, 45.0, 175.0, 160.0);
    save(&h, &deltas, "024-location-box.png");
    save(&h, &deltas, "024-prefix.png");

    // 3. Brief 052: a stored location none of its symbols is in: Checks warns; Use it sets the
    // box's in one step.
    let x2 = doc(&mut h).components_tagged("-X02")[0];
    ok(&mut h, &format!(r#"setprop {{"component":"{}","key":"location","value":"+CP9"}}"#, x2.hex()));
    h.state_mut().panel.tab = project_panel::Tab::Checks;
    h.run_steps(4);
    let rows = h.state().panel.checks.rows().to_vec();
    assert!(rows.iter().any(|r| r.check.kind == Kind::LocationUnused), "{rows:?}");
    save(&h, &deltas, "024-check-own-location.png");
    let before = doc(&mut h);
    h.state_mut().panel_action(project_panel::Action::UseContext(x2, ecad_electrical::locations::designations::Aspect::Location));
    h.run_steps(3);
    assert_ne!(doc(&mut h), before, "one step");
    let d = doc(&mut h);
    assert_eq!(ecad_electrical::locations::resolve(&d, x2).source.word(), "device");

    // 4. Reports: the BOM has devices only, -S01 flagged; wires; cables.
    h.state_mut().panel_action(project_panel::Action::Reports);
    h.run_steps(4);
    let r = h.state().dialogs.reports.as_ref().unwrap();
    let bom = r.table().clone();
    assert_eq!(bom.headers[0], "Location");
    let qty: usize = bom.rows.iter().map(|row| row[1].text.parse::<usize>().unwrap()).sum();
    // The demo's eleven devices, no wire; brief 028: strips -X01 and -X02 count their terminals.
    assert_eq!(qty, 15, "the demo's devices and terminals, no wire");
    assert!(bom.rows.iter().any(|row| row[2].text == "-S01" && row[4].flag.is_some()));
    save(&h, &deltas, "024-reports-bom.png");
    // Export CSV, Excel-safe.
    let csv = dir.join("demo-bom.csv");
    h.state_mut().export_report(&csv.to_string_lossy());
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(text.contains("'-S01") && text.contains("'+CP1"), "{text}");
    h.state_mut().dialogs.reports.as_mut().unwrap().tab = panel::Tab::Wires;
    h.run_steps(4);
    let wires = h.state().dialogs.reports.as_ref().unwrap().table().clone();
    // -W01 continues on page 3 (F4, F5): each core is one wire from -K01 to -X02, listed with its
    // cable.
    let cored = wires.rows.iter().filter(|row| row[9].text.starts_with("-W01 core")).count();
    assert_eq!(cored, 3, "the three cores with their cable");
    save(&h, &deltas, "024-reports-wires.png");
    h.state_mut().dialogs.reports.as_mut().unwrap().tab = panel::Tab::Cables;
    h.run_steps(4);
    let cables = h.state().dialogs.reports.as_ref().unwrap().table().clone();
    let row: Vec<String> = cables.rows[0].iter().map(|c| c.text.clone()).collect();
    // Brief 028: Cores is the offer read from the type. -W01 continues on page 3, so its ends read
    // +CP1 (-K01) and +JB1 (-X02); 3 used and 1 open (GNYE).
    assert_eq!(row[..7], ["-W01", "4G1.5", "4", "+CP1", "+JB1", "3", "1 (GNYE)"], "{row:?}");
    h.state_mut().dialogs.reports = None;

    // 5. Generated pages: a cover and contents, the BOM page.
    for k in [ecad_model::GenKind::Cover, ecad_model::GenKind::Contents] {
        h.state_mut().panel_action(project_panel::Action::NewGenerated(k));
        h.run_steps(3);
    }
    let n = doc(&mut h).page_order().len();
    let cover = page(&mut h, n - 2);
    let contents = page(&mut h, n - 1);
    show(&mut h, cover);
    save(&h, &deltas, "024-cover.png");
    show(&mut h, contents);
    save(&h, &deltas, "024-contents.png");
    let bom_page = page(&mut h, 4);
    show(&mut h, bom_page);
    save(&h, &deltas, "024-bom-page.png");
    let e = h.state_mut().cad.cmdline(r#"place {"symbol":"coil","at":[100,100]}"#).unwrap_or_default();
    assert!(e.contains("is generated (BOM by location)"), "{e}");

    // 6. Print Book: one PDF of every page.
    h.state_mut().panel_action(project_panel::Action::PrintBook(None));
    h.run_steps(4);
    let pdf = dir.join("demo.pdf");
    h.state_mut().dialogs.print_book.as_mut().unwrap().path = pdf.to_string_lossy().into_owned();
    h.run_steps(3);
    assert_eq!(h.state().dialogs.print_book.as_ref().unwrap().sheets, 7, "five pages, a cover and contents");
    save(&h, &deltas, "024-printbook.png");
    let v = h.state_mut().print_from_dialog().unwrap();
    assert_eq!(v["sheets"], 7, "{v}");
    assert!(std::fs::metadata(&pdf).unwrap().len() > 1000);
    if let Some(shots) = out() {
        let _ = std::fs::copy(&pdf, shots.join("024-demo.pdf"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}
