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
    assert!(tab.groups.iter().all(|g| g.name != "Pages"), "the Project panel has the page actions");

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
fn the_page_menu_renames_in_a_name_box_and_adds_a_page() {
    let mut h = harness(trial());
    h.run_steps(3);
    let before = head(h.state_mut());
    // The row's right-click menu: Rename… asks the new title in a name box, prefilled with the
    // current one (owner, 2026-11-08): OK renames, one transaction.
    h.get_by_label("Page 1: 1").click_secondary();
    h.run_steps(3);
    h.get_by_label("Rename…").click();
    h.run_steps(4);
    let t = h.state_mut();
    assert_eq!(t.name_box.as_ref().map(|b| b.text.as_str()), Some("1"), "prefilled with the title");
    assert_eq!(head(t), before, "nothing ran yet");
    t.name_box.as_mut().unwrap().text = "Supply".into();
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let t = h.state_mut();
    assert!(t.name_box.is_none());
    assert_eq!(head(t), before + 1);
    assert_eq!(doc(t).pages[&doc(t).page_order[0]].title, "Supply");
    // New page from the same menu: the name box with the next number; OK adds it.
    h.get_by_label("Page 1: Supply").click_secondary();
    h.run_steps(3);
    h.get_by_label("New page…").click();
    h.run_steps(3);
    assert_eq!(h.state().name_box.as_ref().map(|b| b.text.as_str()), Some("2"));
    h.get_by_label("OK").click();
    h.run_steps(3);
    let t = h.state_mut();
    assert_eq!(head(t), before + 2);
    assert_eq!(doc(t).page_order.len(), 2);
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
            "Cable C1 continued on page 2 with 3 wires; this end shows 1-B-6 and the end on page 1 now shows 2-A-6."
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
    // One page window (brief 016): the same document now shows the other page.
    let p = t.project().unwrap().clone();
    let shown = p.read(&mut t.cad.session, |p, _| p.active_page()).unwrap();
    assert_eq!(shown, Some(doc(t).page_order[0]), "the other page is current");
    assert_eq!(t.cad.session.selection().len(), 1, "the other end's point is selected");
    assert!(t.cad.session.log.iter().any(|l| l == "Page 1 \"1\", at the other end of cable C1 (1-B-6)."));
}

/// The reference texts the active page draws now (the drawing as it is, not a fresh derivation),
/// and that page.
fn drawn_ref_texts(t: &mut Trial) -> (Option<ecad_model::PageId>, Vec<String>) {
    let p = t.project().unwrap().clone();
    p.read(&mut t.cad.session, |p, pg| {
        let Some(page) = p.active_page() else { return (None, Vec::new()) };
        let Some(d) = pg.drawing_of(p, page) else { return (Some(page), Vec::new()) };
        let texts = p.pages[&page]
            .map
            .ref_text_handles()
            .filter_map(|hd| {
                d.model.iter().filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER).find(|e| e.handle == hd)
            })
            .filter_map(|e| match &e.kind {
                cadcraft_engine::doc::EntityKind::Text(x) => Some(x.value.clone()),
                _ => None,
            })
            .collect();
        (Some(page), texts)
    })
    .unwrap()
}

/// Owner's two-page trial (brief 014 fix round) with one page window (brief 016): page 2's cable
/// end is erased (page 1's marker becomes `?`), the owner clicks page 1 in the Project panel and
/// types `u`: page 2, where the erase was, comes to the front with its end back (AC8), and page 1
/// shows `2-A-6` when it is clicked again. Through undo and redo, the page shown always draws what
/// the document says.
#[test]
fn undo_typed_on_page_1_brings_page_2_back_after_an_erase_there() {
    let mut h = harness(trial());
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    let t = h.state_mut();
    t.cad.session.viewport_px = (1280.0, 800.0);
    for y in [0, 5, 10] {
        assert!(t.submit_command(&format!(r#"wire {{"points":[[0,{y}],[60,{y}]]}}"#)).is_none());
    }
    assert!(t.submit_command(r#"cableslash {"from":[28,-3],"to":[32,18],"tag":"C1"}"#).is_none());
    let cable = doc(t).cables.keys().next().copied().unwrap();
    assert!(t.submit_command(&format!(r#"cableref {{"cable":"{}","at":[65,5]}}"#, cable.hex())).is_none());
    assert!(t.submit_command(r#"newpage {"title":"Field"}"#).is_none());
    assert!(
        t.submit_command(&format!(r#"cablecontinue {{"cable":"{}","at":[20,5],"toward":[40,5]}}"#, cable.hex()))
            .is_none()
    );
    let (page1, page2) = (doc(t).page_order[0], doc(t).page_order[1]);
    h.run_steps(3);
    // Erase page 2's end: every drawn object on page 2 except terminal markers.
    let t = h.state_mut();
    let p = t.project().unwrap().clone();
    let erase: Vec<String> = p
        .read(&mut t.cad.session, |p, pg| {
            let map = &p.pages[&page2].map;
            pg.drawing_of(p, page2)
                .unwrap()
                .model
                .iter()
                .filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER)
                .map(|e| e.handle)
                .filter(|hd| !matches!(map.get(*hd), Some(ecad_cadcraft::Mapped::Marker(..))))
                .map(|hd| hd.hex())
                .collect()
        })
        .unwrap();
    let before = head(t);
    assert!(t.submit_command(&format!(r#"erase {{"handles":{}}}"#, serde_json::to_string(&erase).unwrap())).is_none());
    assert_eq!(head(t), before + 1, "{:?}", t.cad.session.log.last());
    h.run_steps(3);
    // Click page 1 in the Project panel.
    h.get_by_label("Page 1: 1").click();
    h.run_steps(3);
    let t = h.state_mut();
    let (shown, texts) = drawn_ref_texts(t);
    assert_eq!(shown, Some(page1), "the row click made page 1 active");
    assert!(texts.iter().any(|x| x == "?"), "{texts:?}");
    // Typed `u`: page 2 comes to the front with its end back.
    assert!(t.submit_command("u").is_none());
    h.run_steps(3);
    let t = h.state_mut();
    assert_eq!(head(t), before + 2);
    let (shown, texts) = drawn_ref_texts(t);
    assert_eq!(shown, Some(page2), "the undo brought page 2 to the front");
    assert!(texts.iter().any(|x| x == "1-B-6"), "{texts:?}");
    assert!(t.cad.session.log.iter().any(|l| l == "Page 2 \"Field\" brought to the front."));
    h.get_by_label("Page 1: 1").click();
    h.run_steps(3);
    let (_, texts) = drawn_ref_texts(h.state_mut());
    assert!(texts.iter().any(|x| x == "2-A-6"), "page 1 shows {texts:?}");
    // Pressing `u` again and again, then redoing: the page shown always draws what the document
    // says, with no frame in which the change count moved and the text did not.
    let derived = |t: &mut Trial, page: ecad_model::PageId| {
        let d = doc(t);
        let mut v: Vec<String> =
            ecad_cadcraft::references::markers_on_page(&d, page).into_iter().map(|m| m.text).collect();
        v.sort();
        v
    };
    for cmd in ["u", "u", "u", "u", "redo", "redo", "redo", "redo", "u"] {
        let t = h.state_mut();
        let _ = t.submit_command(cmd);
        let (shown, mut drawn) = drawn_ref_texts(t);
        drawn.sort();
        let shown = shown.expect("a page is shown");
        assert_eq!(drawn, derived(t, shown), "right after {cmd}: {:?}", t.cad.session.log.last());
        h.run_steps(2);
    }
}
