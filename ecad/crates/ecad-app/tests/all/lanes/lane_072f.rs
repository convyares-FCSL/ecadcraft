//! Lane 072 F, F1b (design 072 §6.2): a selected continuation arrow shows its own section in
//! Properties, and renaming or captioning it (`setprop {aids}`, one `ModifyAid`) refreshes that
//! section: an aid has no `ObjectRef`, so an aid op must not leave the panel stale.

use ecad_model::Subject;

use crate::mouse_tester::{doc, harness_with, ok};

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
