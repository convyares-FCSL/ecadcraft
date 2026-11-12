//! Brief 031 Q1 headless in the trial (design 031 §4.1): Project settings ▸ Text. Set the tag
//! height and OK: one `textstyle` transaction, the page redraws, `u` takes it back. Screenshots
//! are written when `ECAD_TESTER_SHOTS` names a folder and a wgpu adapter exists; they are for the
//! owner's approval, never committed.

use std::path::PathBuf;
use std::sync::Arc;

use ecad_trial::project_panel;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::mouse_tester::{Deltas, doc, harness_with, head, ok, shot};

type H = Harness<'static, ecad_trial::Trial>;

fn save(h: &H, deltas: &Deltas, name: &str) {
    let Some(dir) = std::env::var_os("ECAD_TESTER_SHOTS").map(PathBuf::from) else { return };
    if std::fs::create_dir_all(&dir).is_ok() && shot(h, deltas, &dir.join(name)).is_none() {
        eprintln!("no wgpu adapter: {name} skipped");
    }
}

fn click(h: &mut H, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

#[test]
fn the_text_section_writes_one_textstyle_transaction() {
    let deltas: Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    h.state_mut().cad.session.viewport_px = (1280.0, 800.0);
    ok(&mut h, r#"place {"symbol":"coil","at":[100,100]}"#);
    click(&mut h, project_panel::PROJECT_SETTINGS);
    {
        let d = h.state_mut().project_props.as_mut().unwrap();
        let t = d.text.as_mut().expect("the Text section");
        let tag = t.rows.iter_mut().find(|r| r.kind == "tag").unwrap();
        assert_eq!((tag.height.as_str(), tag.bold, tag.show.as_deref()), ("2.5", true, Some("on")));
        tag.height = "3".into();
    }
    h.run_steps(2);
    save(&h, &deltas, "031-text-section.png");
    let before = head(&mut h);
    click(&mut h, "ecad.projectprops.ok");
    assert_eq!(head(&mut h), before + 1, "one transaction");
    assert!(h.state().project_props.is_none(), "OK closes the popout");
    let d = doc(&mut h);
    assert_eq!(d.properties.get("text.tag.height").map(String::as_str), Some("3"));
    let page = d.page_order[0];
    let tags: Vec<f64> = ecad_cadcraft::devices::texts_on_page(&d, page)
        .into_iter()
        .filter(|t| t.of == ecad_cadcraft::devices::TextOf::Tag)
        .map(|t| t.height)
        .collect();
    assert_eq!(tags, vec![3.0]);
    ok(&mut h, "u");
    assert!(!doc(&mut h).properties.contains_key("text.tag.height"));

    // An out-of-range height keeps OK disabled: nothing is written.
    click(&mut h, project_panel::PROJECT_SETTINGS);
    h.state_mut().project_props.as_mut().unwrap().text.as_mut().unwrap().rows[0].height = "11".into();
    h.run_steps(2);
    assert!(h.state().project_props.as_ref().unwrap().line().is_err());
    let before = head(&mut h);
    click(&mut h, "ecad.projectprops.cancel");
    assert_eq!(head(&mut h), before);
}
