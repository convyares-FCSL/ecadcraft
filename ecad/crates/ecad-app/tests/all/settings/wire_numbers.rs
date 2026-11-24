//! Project Settings' "Wire numbers" row switches what one wire
//! number counts, one per net or one per wire, as one `wiresettings` line and one transaction in
//! which every wire's shown number follows.

use egui_kittest::kittest::Queryable;

use crate::canvas::mouse_acceptance::{doc, harness_with, head, ok};

#[test]
fn wire_numbers_one_per_wire_from_project_settings_in_one_transaction() {
    let mut h = harness_with(None);
    crate::common::points_at(h.state_mut(), &[(0.0, 0.0), (30.0, 0.0), (60.0, 0.0)]);
    ok(&mut h, r#"wire {"points":[[0,0],[30,0]],"potential":"24V"}"#);
    ok(&mut h, r#"wire {"points":[[30,0],[60,0]],"potential":"24V"}"#);
    let numbers = |d: &ecad_model::Document| {
        let mut v: Vec<_> = d.wires().values().map(|w| (w.net_order, w.number_text().map(str::to_string))).collect();
        v.sort();
        v.into_iter().map(|x| x.1).collect::<Vec<_>>()
    };
    let per_net = numbers(&doc(&mut h));
    assert!(per_net.len() == 2 && per_net[0].is_some() && per_net[0] == per_net[1], "{per_net:?}");

    crate::pages::project_panel::project_menu(&mut h, ecad_app::pages::project_panel::PROJECT_SETTINGS);
    h.get_by_label("ecad.projectsettings.number_by.conductor").scroll_to_me();
    h.run_steps(3);
    assert!(h.query_by_label("ecad.projectsettings.number_by.net").is_some());
    crate::pages::project_panel::click(&mut h, "ecad.projectsettings.number_by.conductor");
    let line = h.state().dialogs.project_props.as_ref().unwrap().line().unwrap().unwrap();
    assert_eq!(line, r#"wiresettings {"number_by":"conductor"}"#);
    let before = head(&mut h);
    crate::pages::project_panel::click(&mut h, "ecad.projectprops.ok");
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let d = doc(&mut h);
    assert_eq!(d.settings().wiring.number_by, ecad_model::connectivity::wiring::NumberBy::Conductor);
    let per_wire = numbers(&d);
    assert_eq!(per_wire[0], per_net[0], "the first wire keeps the number");
    assert!(per_wire[1].is_some() && per_wire[1] != per_wire[0], "{per_wire:?}");
    ok(&mut h, "u");
    assert_eq!(numbers(&doc(&mut h)), per_net, "u takes the setting and the numbers back together");
}
