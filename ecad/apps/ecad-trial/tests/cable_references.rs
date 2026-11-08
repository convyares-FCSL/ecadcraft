//! Brief 010, headless UI by element name (design 010 §3.1, check K): the three Wiring buttons
//! start their first prompts, the two Pages buttons fill the Command field with the current page,
//! and the click-only path of the quick start (Cable Reference, Continue Cable, Go to Reference)
//! works through the buttons.

use cadcraft_engine::Input;
use cadcraft_engine::geom::Vec2;
use ecad_trial::Trial;
use ecad_trial::tools;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn trial() -> Trial {
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    Trial::new(None).unwrap()
}

fn harness(t: Trial) -> Harness<'static, Trial> {
    let mut h = Harness::builder().with_size(eframe::egui::vec2(1600.0, 1000.0)).build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    h.run_steps(5);
    h
}

fn doc(t: &mut Trial) -> ecad_model::Document {
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.document().clone()).unwrap()
}

fn head(t: &mut Trial) -> u64 {
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap()
}

fn prompt(t: &Trial) -> String {
    t.cad.session.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn click(t: &mut Trial, x: f64, y: f64) {
    let at = Vec2::new(x, y);
    t.cad.session.cursor = at;
    let _ = t.cad.session.input(Input::Point(at));
}

#[test]
fn the_reference_buttons_sit_in_wiring_and_start_their_prompts() {
    let tab = tools::tab();
    let wiring = tab.groups.iter().find(|g| g.name == "Wiring").expect("Wiring group");
    let ids: Vec<&str> = wiring.tools.iter().map(|t| t.id.as_str()).collect();
    let at = |id: &str| ids.iter().position(|x| *x == id).unwrap_or_else(|| panic!("{id} in {ids:?}"));
    // Combined trial of briefs 009 and 010 (overseer decision): 009's Core Label and Cable Join come
    // first, after Cable View.
    assert_eq!(at("ecad.cableref"), at("ecad.cablefan") + 1);
    assert_eq!(at("ecad.cablecontinue"), at("ecad.cableref") + 1);
    assert_eq!(at("ecad.cablegoto"), at("ecad.cablecontinue") + 1);
    let pages = tab.groups.iter().find(|g| g.name == "Pages").expect("Pages group");
    let ids: Vec<&str> = pages.tools.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["ecad.newpage", "ecad.openpage", "ecad.renamepage", "ecad.movepage"]);

    let mut h = harness(trial());
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    let firsts = [
        ("Cable Reference", "cableref", "Select a cable to reference:"),
        // No reference waits for its other end: no default is offered.
        ("Continue Cable", "cablecontinue", "Enter cable to continue or [List]:"),
        ("Go to Reference", "cablegoto", "Select a cable reference:"),
    ];
    for (label, id, first) in firsts {
        let before = head(h.state_mut());
        h.get_by_label(label).click();
        h.run_steps(3);
        let t = h.state_mut();
        assert_eq!(t.cad.session.running.as_ref().map(|r| r.id.as_str()), Some(id), "{label}");
        assert_eq!(prompt(t), first, "{label}");
        assert!(t.command.is_empty() && t.command_hint().is_none(), "{label}: the field is cleared");
        assert_eq!(head(t), before, "{label}: starting records nothing");
        t.cad.session.cancel();
    }
}

#[test]
fn the_page_buttons_fill_in_the_current_page() {
    let mut h = harness(trial());
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    let page = doc(h.state_mut()).page_order[0].hex();
    let before = head(h.state_mut());
    h.get_by_label("Rename Page").click();
    h.run_steps(4);
    let t = h.state_mut();
    assert_eq!(t.command, format!(r#"renamepage {{"page":"{page}","title":"<title>"}}"#));
    assert!(t.command_hint().unwrap().starts_with("Rename Page:"));
    assert_eq!(head(t), before, "nothing ran");
    assert!(t.submit_command(&format!(r#"renamepage {{"page":"{page}","title":"Supply"}}"#)).is_none());
    assert_eq!(head(t), before + 1);
    // The note went away and moved the panel: lay out again before clicking by position.
    h.run_steps(3);
    h.get_by_label("Move Page").click();
    h.run_steps(4);
    let t = h.state_mut();
    assert_eq!(t.command, format!(r#"movepage {{"page":"{page}","index":<new position>}}"#));
    let line = t.command.clone();
    assert!(t.submit_command(&line).is_some_and(|e| e.contains("<new position>")), "the placeholder never runs");
}

/// The quick start's click-only path (design 010 §12): reference, continue on a new page, go back.
#[test]
fn the_click_only_path_continues_a_cable_and_jumps_between_its_ends() {
    let mut h = harness(trial());
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    let t = h.state_mut();
    t.cad.session.viewport_px = (1280.0, 800.0);
    for y in [0, 5, 10] {
        assert!(t.submit_command(&format!(r#"wire {{"points":[[0,{y}],[60,{y}]]}}"#)).is_none());
    }
    assert!(t.submit_command(r#"cableslash {"from":[28,-3],"to":[32,18],"tag":"C1"}"#).is_none());
    let _ = t.cad.session.zoom_extents();
    h.get_by_label("Cable Reference").click();
    h.run_steps(3);
    let t = h.state_mut();
    click(t, 30.0, 7.5);
    assert_eq!(prompt(t), "Specify reference point or [Undo]:");
    click(t, 65.0, 5.0);
    assert!(t.cad.session.running.is_none());
    assert!(t.cad.session.log.iter().any(|l| l == "Reference for cable C1 placed on page 1."));
    assert!(t.submit_command(r#"newpage {"title":"Field"}"#).is_none());
    let _ = t.cad.session.zoom_extents();
    h.get_by_label("Continue Cable").click();
    h.run_steps(3);
    let t = h.state_mut();
    assert_eq!(prompt(t), "Enter cable to continue or [List] <C1>:");
    t.cad.session.input(Input::Enter).unwrap();
    click(t, 20.0, 5.0);
    click(t, 40.0, 5.0);
    assert!(t.cad.session.running.is_none());
    let said = t.cad.session.log.join("\n");
    assert!(
        said.contains(
            "Cable C1 continued on page 2 with 3 wires; this end shows /1.2 and the end on page 1 now shows /2.1."
        ),
        "{said}"
    );
    assert!(said.contains("Connect each wire's free end to its terminal with Connect."));
    // Go to Reference with Field's reference point selected (pickfirst): lands on page 1 with
    // the other end's point selected.
    let field_uid = t.cad.session.state().unwrap().uid;
    let p = t.project().unwrap().clone();
    let point = p
        .read(&mut t.cad.session, |p, _| {
            let doc = p.ledger.document();
            let page = p.page_of(field_uid)?;
            let r = ecad_cadcraft::references::references_on_page(doc, page).into_iter().next()?;
            p.pages.get(&page)?.map.item(r.item?)
        })
        .unwrap()
        .expect("Field's reference point is drawn");
    t.cad.session.set_selection(vec![point]);
    h.get_by_label("Go to Reference").click();
    h.run_steps(3);
    let t = h.state_mut();
    assert!(t.cad.session.running.is_none(), "{:?}", t.cad.session.log.last());
    assert_ne!(t.cad.session.state().unwrap().uid, field_uid, "the other page is current");
    assert_eq!(t.cad.session.selection().len(), 1, "the other end's point is selected");
    assert!(t.cad.session.log.iter().any(|l| l == "Page 1 \"1\", at the other end of cable C1 (/1.2)."));
}
