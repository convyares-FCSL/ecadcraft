//! Brief 052c (design 052 §9.4, §10.15): Settings ▸ Project info ▸ Functions and locations shows
//! the project's list with the counts `designations` gives, and its edits are immediate commands.

use ecad_app::App;
use ecad_app::settings::Section;
use ecad_cadcraft::designations::{self, Aspect};
use eframe::egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::project_panel::{doc, rev3_app};

type H = Harness<'static, App>;

fn window() -> H {
    let mut h = Harness::builder().with_size(vec2(1920.0, 1080.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, t: &mut App| {
            let r = ui.ctx().content_rect();
            ui.scope_builder(eframe::egui::UiBuilder::new().max_rect(r), |ui| {
                ui.set_clip_rect(r);
                t.logic(&ui.ctx().clone());
                t.ui(ui);
            });
        },
        rev3_app(),
    );
    h.run_steps(5);
    h
}

fn ok(h: &mut H, line: &str) {
    let e = h.state_mut().submit_command(line);
    assert!(e.is_none(), "{line}: {e:?}");
    h.run_steps(2);
}

#[test]
fn the_settings_card_lists_entries_and_acts_at_once() {
    let mut h = window();
    ok(&mut h, r#"place {"symbol": "coil", "at": [60, 150]}"#);
    let c = doc(&mut h).devices.iter().find(|(_, x)| x.tag.is_some()).map(|(id, _)| *id).unwrap();
    ok(&mut h, &format!(r#"setlocation {{"component": "{}", "value": "+CP1"}}"#, c.hex()));
    ok(&mut h, r#"designationadd {"aspect": "location", "code": "+SPARE"}"#);
    ok(&mut h, r#"projectprops {"location": "+CP1"}"#);
    h.state_mut().open_settings(Section::Project);
    h.run_steps(4);
    assert!(h.query_by_label(ecad_app::settings::designations::CARD).is_some(), "the card is shown");
    for name in ["ecad.designation.l.+CP1.code", "ecad.designation.l.+CP1.count", "ecad.designation.l.+CP1.default", "ecad.designation.l.+SPARE.code"]
    {
        assert!(h.query_by_label(name).is_some(), "{name}");
    }
    assert!(h.query_by_label("ecad.designation.l.+SPARE.default").is_none(), "only the project's default is marked");
    // A commit elsewhere that touches the list is read again: a new entry appears.
    ok(&mut h, r#"designationadd {"aspect": "function", "code": "=F1"}"#);
    h.run_steps(2);
    assert!(h.query_by_label("ecad.designation.f.=F1.code").is_some(), "the new entry is listed");
    // Remove unused: one transaction for both unused entries, at once (no Save).
    h.get_by_label("ecad.designation.l.remove_unused").click();
    h.run_steps(4);
    let d = doc(&mut h);
    assert!(designations::find(&d, Aspect::Location, "+SPARE").is_none(), "removed at once");
    assert!(designations::find(&d, Aspect::Location, "+CP1").is_some(), "a used entry stays");
    assert!(designations::find(&d, Aspect::Function, "=F1").is_some(), "the location list's button removes locations only");
    assert!(h.query_by_label("ecad.designation.l.+SPARE.code").is_none());
}
