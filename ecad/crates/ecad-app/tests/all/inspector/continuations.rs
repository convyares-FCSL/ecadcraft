//! Lane 072 F, F1b (design 072 §6.2): a selected continuation arrow shows its own section in
//! Properties, and renaming or captioning it (`setprop {aids}`, one `ModifyAid`) refreshes that
//! section: an aid has no `ObjectRef`, so an aid op must not leave the panel stale.

use ecad_model::Subject;

use crate::canvas::mouse_acceptance::{doc, harness_with, ok};

fn row(h: &mut egui_kittest::Harness<'static, ecad_app::App>, key: &str) -> Option<String> {
    let props = h.state().props.lock().unwrap();
    let s = props.view.sections.iter().find(|s| s.kind == "continuation")?;
    s.rows.iter().find(|r| r.key == key).map(|r| r.value.clone())
}

#[test]
fn renaming_or_captioning_a_shown_arrow_refreshes_its_section() {
    let mut h = harness_with(None);
    ok(&mut h, r#"place {"symbol":"continuation","at":[20,20],"name":"24V"}"#);
    let d = doc(&mut h);
    let (aid, _) = d.aids.iter().next().expect("the arrow is an aid");
    let aid = *aid;
    let rep = d.representations_of(Subject::Aid(aid)).map(|(id, _)| id).next().expect("its symbol");
    h.state_mut().goto_symbols(&[rep]).unwrap();
    h.run_steps(4);
    assert_eq!(row(&mut h, "name").as_deref(), Some("24V"), "the section is shown");

    ok(&mut h, &format!(r#"setprop {{"aids":["{}"],"key":"name","value":"0V"}}"#, aid.hex()));
    h.run_steps(2);
    assert_eq!(row(&mut h, "name").as_deref(), Some("0V"), "a rename shows at once");

    ok(&mut h, &format!(r#"setprop {{"aids":["{}"],"key":"caption","value":"TO PSU"}}"#, aid.hex()));
    h.run_steps(2);
    assert_eq!(row(&mut h, "caption").as_deref(), Some("TO PSU"), "a caption shows at once");
}

fn box_row(h: &mut egui_kittest::Harness<'static, ecad_app::App>, key: &str) -> Option<String> {
    let props = h.state().props.lock().unwrap();
    let s = props.view.sections.iter().find(|s| s.kind == "box")?;
    s.rows.iter().find(|r| r.key == key).map(|r| r.value.clone())
}

/// F1c: a selected location box shows its own section, and a new location (`setprop {aids}`, one
/// `ModifyAid`) refreshes it at once.
#[test]
fn a_new_location_on_a_shown_box_refreshes_its_section() {
    let mut h = harness_with(None);
    ok(&mut h, r#"locationbox {"location":"+CP1","corners":[[10,10],[80,60]]}"#);
    let d = doc(&mut h);
    let (aid, _) = d.aids.iter().find(|(_, a)| a.as_box().is_some()).expect("the box is an aid");
    let aid = *aid;
    assert!(d.devices.is_empty(), "no device");
    let rep = d.representations_of(Subject::Aid(aid)).map(|(id, _)| id).next().expect("its outline");
    h.state_mut().goto_symbols(&[rep]).unwrap();
    h.run_steps(4);
    assert_eq!(box_row(&mut h, "location").as_deref(), Some("+CP1"), "the section is shown");

    ok(&mut h, &format!(r#"setprop {{"aids":["{}"],"key":"location","value":"+CP2"}}"#, aid.hex()));
    h.run_steps(2);
    assert_eq!(box_row(&mut h, "location").as_deref(), Some("+CP2"), "a new location shows at once");
}

/// F1c-2: a shown box's Panel temperature (°C, editable) and Style rows show the project's until
/// set, and a `setprop {aids}` edit of either refreshes the section at once.
#[test]
fn a_shown_boxs_panel_temperature_and_style_rows_refresh() {
    let mut h = harness_with(None);
    ok(&mut h, r#"locationbox {"location":"+CP1","corners":[[10,10],[80,60]]}"#);
    let d = doc(&mut h);
    let (aid, _) = d.aids.iter().find(|(_, a)| a.as_box().is_some()).expect("the box is an aid");
    let aid = *aid;
    let rep = d.representations_of(Subject::Aid(aid)).map(|(id, _)| id).next().expect("its outline");
    h.state_mut().goto_symbols(&[rep]).unwrap();
    h.run_steps(4);
    let editable = |h: &mut egui_kittest::Harness<'static, ecad_app::App>, key: &str| {
        let props = h.state().props.lock().unwrap();
        let s = props.view.sections.iter().find(|s| s.kind == "box").expect("the box section");
        s.rows.iter().find(|r| r.key == key).map(|r| (r.editable, r.inherited, r.label.clone()))
    };
    let (rw, inherited, label) = editable(&mut h, "panel_temperature").expect("a Panel temperature row");
    assert!(rw && inherited && label.contains("\u{b0}C"), "editable, the project's, in °C: {label}");
    let (rw, inherited, _) = editable(&mut h, "style").expect("a Style row");
    assert!(rw && inherited, "editable, the project's");

    ok(&mut h, &format!(r#"setprop {{"aids":["{}"],"key":"panel_temperature","value":"45"}}"#, aid.hex()));
    h.run_steps(2);
    assert_eq!(box_row(&mut h, "panel_temperature").as_deref(), Some("45"), "a new temperature shows at once");
    assert_eq!(editable(&mut h, "panel_temperature").map(|r| r.1), Some(false), "its own");

    ok(&mut h, &format!(r#"setprop {{"aids":["{}"],"key":"style","value":"box"}}"#, aid.hex()));
    h.run_steps(2);
    assert_eq!(box_row(&mut h, "style").as_deref(), Some("Location box"), "its own style by name");
    assert_eq!(editable(&mut h, "style").map(|r| r.1), Some(false), "its own");
}
