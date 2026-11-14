//! Brief 048b and 048c in the trial at 1920 × 1080: the symbol builder in the Library (its tools,
//! Details and Missing list, Save as version 1), the "Before saving" review in the drawer with a
//! clash (Update selected pre-selected), Make symbol from lines on a page with its offers, the
//! wheel's Edit and Make symbol slots, Update and Compare from Properties, and the owner's
//! screenshots (written only when `ECAD_TESTER_SHOTS` names a folder).

use cadcraft_engine::doc::Handle;
use ecad_cadcraft::actions::{self, ObjectKind, Slot};
use ecad_cadcraft::symbols;
use ecad_model::{ComponentId, Pt, RepresentationId, Subject};
use ecad_trial::Trial;
use ecad_trial::library::builder::{self, Tool};
use ecad_trial::library::draft::Picked;
use ecad_trial::library::{browser, details, review};
use ecad_trial::shell::Workspace;
use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

use crate::project_panel::{doc, head, trial};

type H = Harness<'static, Trial>;

fn shell(deltas: Option<crate::mouse_tester::Deltas>) -> H {
    let mut b = Harness::builder().with_size(vec2(1920.0, 1080.0)).with_step_dt(1.0 / 60.0);
    if let Some(d) = deltas {
        b = b.renderer(crate::mouse_tester::Collect(d));
    }
    let mut h = b.build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        trial(),
    );
    h.run_steps(5);
    h
}

fn run(h: &mut H, cmd: &str, params: Value) -> Value {
    let v = h.state_mut().cad.run(cmd, params).unwrap_or_else(|e| panic!("{cmd}: {e}"));
    h.run_steps(2);
    v
}

/// The screen point of builder millimetre point (x, y).
fn mm(h: &H, x: f64, y: f64) -> Pos2 {
    let main = h.state().shell.slots.expect("laid out").main;
    h.state().library.builder.as_ref().expect("the builder").to_screen(main, Pt::new(x, y))
}

fn click_at(h: &mut H, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

fn click_mm(h: &mut H, x: f64, y: f64) {
    let at = mm(h, x, y);
    click_at(h, at);
}

fn key(h: &mut H, k: Key) {
    h.key_press_modifiers(Modifiers::NONE, k);
    h.run_steps(2);
}

fn type_text(h: &mut H, text: &str) {
    h.event(Event::Text(text.to_string()));
    h.run_steps(1);
}

fn tool(h: &mut H, t: Tool) {
    h.state_mut().library.builder.as_mut().expect("the builder").set_tool(t);
    h.run_steps(1);
}

/// Fill a Details field by its element name.
fn fill(h: &mut H, name: &str, text: &str) {
    h.get_by_label(name).click();
    h.run_steps(2);
    type_text(h, text);
    h.run_steps(1);
}

/// Draw a 10 × 20 mm box with two points (BN, BU) on its ends and a tag slot, by the tools.
fn draw_float(h: &mut H) {
    let main = h.state().shell.slots.unwrap().main;
    h.event(Event::PointerMoved(main.center()));
    h.run_steps(1);
    tool(h, Tool::Rectangle);
    click_mm(h, -5.0, -10.0);
    click_mm(h, 5.0, 10.0);
    tool(h, Tool::Line);
    click_mm(h, 0.0, 10.0);
    click_mm(h, 0.0, 15.0);
    key(h, Key::Escape);
    tool(h, Tool::Line);
    click_mm(h, 0.0, -10.0);
    click_mm(h, 0.0, -15.0);
    key(h, Key::Escape);
    tool(h, Tool::Point);
    click_mm(h, 0.0, 15.0);
    type_text(h, "BN");
    key(h, Key::Enter);
    click_mm(h, 0.0, -15.0);
    type_text(h, "BU");
    key(h, Key::Enter);
    tool(h, Tool::Slot);
    click_mm(h, 7.5, 2.5);
}

#[test]
fn new_symbol_opens_an_empty_builder_whose_missing_list_names_what_it_needs() {
    let mut h = shell(None);
    h.state_mut().new_symbol();
    h.run_steps(4);
    assert_eq!(h.state().shell_state().workspace, Workspace::Library);
    assert!(h.query_by_label(builder::EMPTY_HINT).is_some(), "the empty state's hint");
    for t in builder::STRIP {
        assert!(h.query_by_label(t.name).is_some(), "{}", t.name);
    }
    let slots = h.state().shell.slots.unwrap();
    assert_eq!(slots.right.width(), 380.0, "the builder's right panel is 380 px");
    assert!(slots.tool_strip.is_some(), "the builder has a tool strip");
    assert!(!slots.drawer_open, "the drawer is closed while drawing");
    for item in ["name", "point", "prefix", "slot"] {
        assert!(h.query_by_label(&format!("library.details.missing.{item}")).is_some(), "Missing: {item}");
    }
    assert!(h.get_by_label(details::SAVE).accesskit_node().is_disabled(), "Save waits for the Missing list");
    // A point off the 5 mm grid is refused at the cursor, in the rules' words.
    let main = h.state().shell.slots.unwrap().main;
    h.event(Event::PointerMoved(main.center()));
    tool(&mut h, Tool::Point);
    click_mm(&mut h, 2.5, 0.0);
    let b = h.state().library.builder.as_ref().unwrap();
    assert_eq!(b.at_cursor.as_ref().map(|(w, _)| w.as_str()), Some("A point goes on the 5 mm grid."));
    assert!(b.draft.shape.points.is_empty());
}

#[test]
fn a_drawn_symbol_saves_as_version_1_and_the_browser_selects_it() {
    let mut h = shell(None);
    h.state_mut().new_symbol();
    h.run_steps(4);
    draw_float(&mut h);
    {
        let d = &h.state().library.builder.as_ref().unwrap().draft;
        assert_eq!(d.shape.graphics.len(), 3, "a box and two leads");
        assert_eq!(d.shape.points.iter().map(|p| (p.key.as_str(), p.label.as_deref())).collect::<Vec<_>>(), [("1", Some("BN")), ("2", Some("BU"))]);
        assert_eq!(d.shape.slots.first().map(|s| s.text.as_str()), Some("tag"));
    }
    fill(&mut h, details::NAME, "Float switch");
    fill(&mut h, details::KEY, "float1");
    fill(&mut h, details::PREFIX, "B");
    h.run_steps(2);
    let before = head(&mut h);
    h.get_by_label(details::SAVE).click();
    h.run_steps(4);
    assert_eq!(head(&mut h), before + 1, "Save is one transaction");
    let d = doc(&mut h);
    let v = symbols::project_by_key(&d, "float1").expect("saved");
    assert_eq!(v.seq, 1);
    assert!(h.state().library.builder.is_none(), "back to the browser");
    assert_eq!(h.state().library.browser.selected, Some(browser::Tile::Project(v.lineage)));
    assert!(h.query_by_label(&browser::Tile::Project(v.lineage).name()).is_some(), "its tile");
    assert!(h.query_by_label(browser::PLACED_NEXT).is_some(), "Placed next");
    // It places from Ctrl+K's search too.
    let (rows, _, _) = ecad_trial::search::rows(Some(&d), "float");
    assert!(rows.iter().any(|r| r.pick == ecad_trial::search::Pick::Project("float1".into())));
}

/// Three float switches, the first wired on point 2.
fn three_copies(h: &mut H) -> (Vec<RepresentationId>, Vec<ComponentId>) {
    run(
        h,
        "symbolnew",
        json!({
            "name": "Float switch", "key": "float1",
            "graphics": [{"kind": "polyline", "vertices": [[-2.5, -7.5], [2.5, -7.5], [2.5, 7.5], [-2.5, 7.5]], "closed": true}],
            "points": [{"key": "1", "label": "BN", "at": [0, 10]}, {"key": "2", "label": "BU", "at": [0, -10]}],
            "slots": [{"text": "tag", "at": [5, 0]}],
            "defaults": {"prefix": "B", "description": "Float switch", "join": "device"},
        }),
    );
    let mut comps = Vec::new();
    for x in [100.0, 150.0, 200.0] {
        let v = run(h, "place", json!({"symbol": "float1", "at": [x, 100.0]}));
        comps.push(ComponentId::from(u128::from_str_radix(v["component"].as_str().unwrap(), 16).unwrap()));
    }
    run(h, "wire", json!({"points": [[100.0, 90.0], [100.0, 60.0]]}));
    let d = doc(h);
    let reps = comps.iter().map(|c| d.representations_of(Subject::Component(*c)).next().unwrap().0).collect();
    (reps, comps)
}

#[test]
fn edit_symbol_reviews_every_copy_with_update_selected_when_one_clashes() {
    let mut h = shell(None);
    let (reps, _) = three_copies(&mut h);
    h.state_mut().edit_placed(reps[1]).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label(builder::BACK).is_some(), "Back to the page");
    // Delete point 2 (wired on the first copy).
    h.state_mut().library.builder.as_mut().unwrap().draft.delete(Picked::Point(1));
    h.run_steps(2);
    h.get_by_label(details::SAVE).click();
    h.run_steps(4);
    let r = h.state().library.builder.as_ref().and_then(|b| b.review.clone()).expect("the review");
    assert_eq!(r.choice, review::Choice::Selected, "a clash pre-selects Update selected");
    assert!(r.header().ends_with("3 placed copies on 1 page · 1 would clash"), "{}", r.header());
    assert!(h.state().shell.slots.unwrap().drawer_open, "the review opens the drawer");
    assert!(h.query_by_label(review::HEADER).is_some());
    let clash = format!("{}.row.{}.check", review::BODY, reps[0].hex());
    assert!(h.get_by_label(&clash).accesskit_node().is_disabled(), "a clashing row cannot be ticked");
    let before = head(&mut h);
    h.get_by_label(review::SAVE).click();
    h.run_steps(4);
    assert_eq!(head(&mut h), before + 1, "one transaction");
    let d = doc(&mut h);
    assert!(ecad_cadcraft::symbol_cmds::update_available(&d, reps[0]).is_some(), "the clashing copy keeps v1");
    assert!(ecad_cadcraft::symbol_cmds::update_available(&d, reps[1]).is_none(), "updated");
    assert!(h.state().library.builder.is_none());
}

#[test]
fn the_device_wheel_offers_edit_symbol_and_a_drawn_device_make_symbol() {
    let w = actions::wheel(ObjectKind::Device);
    assert_eq!(w[Slot::TopLeft.index()].map(|a| a.id), Some("symbol.edit"));
    assert_eq!(actions::wheel(ObjectKind::Mixed)[Slot::Right.index()].map(|a| a.id), Some("symbol.make"));
    assert_eq!(actions::wheel(ObjectKind::Mixed)[Slot::TopLeft.index()].map(|a| a.id), Some("edit.mirror"), "Mirror keeps the selection wheel");
    let mut t = ecad_cadcraft::interact::kind::Target::new(ObjectKind::Device, [0.0, 0.0], "-A01");
    t.drawn = true;
    assert_eq!(actions::wheel_for(&t)[Slot::TopLeft.index()].map(|a| a.id), Some("symbol.make"));
}

fn lines(h: &mut H) -> Vec<Handle> {
    let mut out = Vec::new();
    for (a, b) in
        [((0.0, -10.0), (0.0, -5.0)), ((-5.0, -5.0), (5.0, -5.0)), ((5.0, -5.0), (5.0, 5.0)), ((5.0, 5.0), (-5.0, 5.0)), ((-5.0, 5.0), (-5.0, -5.0))]
    {
        let v = run(h, "line", json!({"points": [[a.0 + 300.0, a.1 + 100.0], [b.0 + 300.0, b.1 + 100.0]]}));
        out.push(Handle::parse_hex(v["handles"][0].as_str().unwrap_or_else(|| panic!("a handle: {v}"))).unwrap());
    }
    out
}

#[test]
fn make_symbol_from_lines_offers_points_and_replaces_the_drawing_on_save() {
    let mut h = shell(None);
    let hs = lines(&mut h);
    h.state_mut().cad.session.set_selection(hs.clone());
    h.state_mut().make_symbol(None).unwrap();
    h.run_steps(3);
    {
        let d = &h.state().library.builder.as_ref().unwrap().draft;
        assert_eq!(d.shape.graphics.len(), 5);
        assert_eq!(d.shape.offers.len(), 1, "the lead's free end is offered: {:?}", d.shape.offers);
        assert!(d.shape.points.is_empty(), "nothing applied until accepted");
    }
    assert!(h.query_by_label(details::REPLACE).is_some(), "the replace choice");
    h.get_by_label(details::ACCEPT_ALL).click();
    h.run_steps(2);
    let main = h.state().shell.slots.unwrap().main;
    h.event(Event::PointerMoved(main.center()));
    tool(&mut h, Tool::Slot);
    click_mm(&mut h, 7.5, 0.0);
    fill(&mut h, details::NAME, "Box");
    fill(&mut h, details::KEY, "box1");
    fill(&mut h, details::PREFIX, "A");
    h.run_steps(2);
    h.get_by_label(details::SAVE).click();
    h.run_steps(4);
    let d = doc(&mut h);
    let v = symbols::project_by_key(&d, "box1").expect("made");
    assert_eq!(d.items_using(v.id).len(), 1, "a device of it where the lines were");
    assert!(h.state().library.builder.is_none());
}

#[test]
fn make_symbol_on_a_drawn_device_promotes_it_keeping_its_ids_and_wires() {
    let mut h = shell(None);
    let mut hs = Vec::new();
    for (a, b) in [((0.0, 0.0), (10.0, 0.0)), ((10.0, 0.0), (10.0, 10.0))] {
        let v = run(&mut h, "line", json!({"points": [[a.0 + 100.0, a.1 + 100.0], [b.0 + 100.0, b.1 + 100.0]]}));
        hs.push(v["handles"][0].as_str().unwrap().to_string());
    }
    let v = run(&mut h, "convertcomponent", json!({"handles": hs}));
    let c = ComponentId::from(u128::from_str_radix(v["component"].as_str().unwrap(), 16).unwrap());
    for (at, label) in [([100, 100], "A1"), ([110, 110], "A2")] {
        run(&mut h, "addterminal", json!({"handle": hs[0], "at": at, "label": label}));
    }
    let w = run(&mut h, "wire", json!({"points": [[100, 100], [100, 80]]}));
    let mut t = ecad_cadcraft::interact::kind::Target::new(ObjectKind::Device, [100.0, 100.0], "component");
    t.component = Some(c);
    t.drawn = true;
    h.state_mut().make_symbol(Some(&t)).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().library.builder.as_ref().unwrap().draft.shape.points.len(), 2, "its points come with it");
    let main = h.state().shell.slots.unwrap().main;
    h.event(Event::PointerMoved(main.center()));
    tool(&mut h, Tool::Slot);
    click_mm(&mut h, -10.0, 5.0);
    fill(&mut h, details::NAME, "Mine");
    fill(&mut h, details::KEY, "mine");
    fill(&mut h, details::PREFIX, "A");
    h.run_steps(2);
    h.get_by_label(details::SAVE).click();
    h.run_steps(4);
    assert!(h.state().library.builder.is_none(), "saved: {:?}", h.state().library.builder.as_ref().and_then(|b| b.refusal.clone()));
    let d = doc(&mut h);
    let (_, rep) = d.representations_of(Subject::Component(c)).next().expect("the same device");
    assert_eq!(rep.form, ecad_model::Form::Symbol);
    let w = ecad_model::ConductorId::from(u128::from_str_radix(w["conductor"].as_str().unwrap(), 16).unwrap());
    assert!(d.terminals_of(c).any(|(term, _)| d.ends_at(term).iter().any(|(x, _)| *x == w)), "the wire stays attached");
    let v = symbols::project_by_key(&d, "mine").unwrap();
    let slot = v.def.texts.first().expect("the tag slot");
    assert!(slot.at.x.abs() < 50.0 && slot.at.y.abs() < 50.0, "the slot is in the symbol's frame: {:?}", slot.at);
}

#[test]
fn editing_a_built_in_from_the_browser_saves_a_copy_that_remembers_it() {
    let mut h = shell(None);
    h.state_mut().edit_starter("coil").unwrap();
    h.run_steps(3);
    let main = h.state().shell.slots.unwrap().main;
    h.event(Event::PointerMoved(main.center()));
    tool(&mut h, Tool::Slot);
    click_mm(&mut h, 10.0, 0.0);
    fill(&mut h, details::KEY, "coil2");
    h.run_steps(2);
    h.get_by_label(details::SAVE).click();
    h.run_steps(4);
    let d = doc(&mut h);
    let v = symbols::project_by_key(&d, "coil2").expect("saved");
    let from = v.def.derived_from.clone().expect("derived_from (FP4)");
    assert_eq!(from.definition, symbols::by_key("coil").unwrap().id);
}

/// Tester 048b/c: Ctrl+Z and Ctrl+Y in the builder step the draft only, never the project.
#[test]
fn ctrl_z_in_the_builder_undoes_the_draft_step_not_the_project() {
    let mut h = shell(None);
    h.state_mut().new_symbol();
    h.run_steps(4);
    let main = h.state().shell.slots.unwrap().main;
    h.event(Event::PointerMoved(main.center()));
    h.run_steps(1);
    tool(&mut h, Tool::Line);
    click_mm(&mut h, 0.0, 0.0);
    click_mm(&mut h, 10.0, 0.0);
    key(&mut h, Key::Escape);
    assert_eq!(h.state().library.builder.as_ref().unwrap().draft.shape.graphics.len(), 1);
    let before = head(&mut h);
    h.event(Event::PointerMoved(main.center()));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert!(h.state().library.builder.as_ref().unwrap().draft.shape.graphics.is_empty(), "Ctrl+Z undid the line");
    assert_eq!(head(&mut h), before, "the project is not undone");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Y);
    h.run_steps(3);
    assert_eq!(h.state().library.builder.as_ref().unwrap().draft.shape.graphics.len(), 1, "Ctrl+Y redid it");
    assert_eq!(head(&mut h), before);
}

/// Tester 048b/c: Make symbol on a selection holding a wire refuses naming the wire, as
/// `makesymbol` does, and opens no builder.
#[test]
fn make_symbol_on_a_wire_refuses_naming_it() {
    let mut h = shell(None);
    let mut hs = lines(&mut h);
    let w = run(&mut h, "wire", json!({"points": [[400.0, 100.0], [450.0, 100.0]]}));
    run(&mut h, "wirenumber", json!({"conductor": w["conductor"], "value": "24V-05"}));
    let d = doc(&mut h);
    let c = ecad_model::ConductorId::from(u128::from_str_radix(w["conductor"].as_str().unwrap(), 16).unwrap());
    let page = d.page_order[0];
    let route = d.representations_of(Subject::Conductor(c)).find(|(_, r)| r.page == page).and_then(|(_, r)| r.items.first().copied()).unwrap();
    let handle = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| p.pages.get(&page).and_then(|v| v.map.item(route))).unwrap().expect("its handle")
    };
    hs.push(handle);
    h.state_mut().cad.session.set_selection(hs);
    let e = h.state_mut().make_symbol(None).unwrap_err();
    assert_eq!(e, "Wire 24V-05 is not plain geometry; Make symbol takes lines, arcs and text.");
    assert!(h.state().library.builder.is_none());
}

#[test]
fn update_from_properties_and_compare_show_the_copy_in_the_drawer() {
    let mut h = shell(None);
    let (reps, _) = three_copies(&mut h);
    // A v2 that moves nothing wired: copy 3 keeps v1.
    let d = doc(&mut h);
    let v1 = symbols::project_by_key(&d, "float1").unwrap().id;
    run(
        &mut h,
        "symboledit",
        json!({
            "definition": v1.hex(), "name": "Float switch", "key": "float1",
            "graphics": [{"kind": "polyline", "vertices": [[-5, -7.5], [5, -7.5], [5, 7.5], [-5, 7.5]], "closed": true}],
            "points": [{"key": "1", "label": "BN", "at": [0, 10]}, {"key": "2", "label": "BU", "at": [0, -10]}],
            "slots": [{"text": "tag", "at": [7.5, 0]}],
            "defaults": {"prefix": "B", "description": "Float switch", "join": "device"},
            "update": "selected", "copies": [reps[0].hex()],
        }),
    );
    h.state_mut().compare_symbol(reps[2]);
    h.run_steps(3);
    let c = h.state().library.compare.clone().expect("Compare");
    assert_eq!(c.rows.len(), 1);
    assert!(h.query_by_label(review::HEADER).is_some(), "Compare in the drawer");
    h.get_by_label(review::UPDATE).click();
    h.run_steps(3);
    assert!(ecad_cadcraft::symbol_cmds::update_available(&doc(&mut h), reps[2]).is_none(), "updated from Compare");
    assert!(h.state().library.compare.is_none());
}

/// The builder (empty and with a symbol), the review with one clash, Make symbol's offers and the
/// browser at 1920 × 1080, for the owner's side-by-side check with the spec's screens g, i, j and
/// k. Nothing is asserted about the pictures.
#[test]
fn screenshots_for_the_owner() {
    let Some(out) = crate::project_panel::shots() else { return };
    let deltas: crate::mouse_tester::Deltas = std::sync::Arc::default();
    let mut h = shell(Some(deltas.clone()));
    let shot = |h: &H, name: &str| {
        let _ = crate::mouse_tester::shot(h, &deltas, &out.join(name));
    };
    h.state_mut().new_symbol();
    h.run_steps(4);
    shot(&h, "048-builder-empty.png");
    draw_float(&mut h);
    fill(&mut h, details::NAME, "Float switch");
    tool(&mut h, Tool::Line);
    click_mm(&mut h, -5.0, 5.0);
    let at = mm(&h, 2.5, 7.5);
    h.event(Event::PointerMoved(at));
    h.run_steps(3);
    shot(&h, "048-builder-drawing.png");
    key(&mut h, Key::Escape);
    fill(&mut h, details::KEY, "float1");
    fill(&mut h, details::PREFIX, "B");
    h.run_steps(3);
    shot(&h, "048-builder-ready.png");
    h.get_by_label(details::SAVE).click();
    h.run_steps(4);
    shot(&h, "048-browser.png");
    // Three copies, one wired on point 2; edit and delete point 2: the review.
    for x in [100.0, 150.0, 200.0] {
        run(&mut h, "place", json!({"symbol": "float1", "at": [x, 100.0]}));
    }
    run(&mut h, "wire", json!({"points": [[100.0, 85.0], [100.0, 60.0]]}));
    let d = doc(&mut h);
    let v = symbols::project_by_key(&d, "float1").unwrap();
    let rep = d.items_using(v.id).first().and_then(|i| d.representation_of_item(*i)).map(|(r, _)| r).unwrap();
    h.state_mut().edit_placed(rep).unwrap();
    h.run_steps(3);
    h.state_mut().library.builder.as_mut().unwrap().draft.delete(Picked::Point(1));
    h.run_steps(2);
    h.get_by_label(details::SAVE).click();
    h.run_steps(4);
    shot(&h, "048-review.png");
    h.get_by_label(review::BACK).click();
    h.state_mut().close_builder(true);
    h.run_steps(3);
    // Make symbol: the lines arrive on the grid with their offers.
    h.state_mut().set_workspace(Workspace::Draw);
    h.run_steps(2);
    let mut hs = lines(&mut h);
    if let Ok(v) = h.state_mut().cad.run("text", json!({"at": [307.5, 97.5], "text": "-LS1", "height": 2.5})) {
        hs.extend(v["handles"].as_array().into_iter().flatten().chain(v.get("handle")).filter_map(|x| x.as_str()).filter_map(Handle::parse_hex));
    }
    h.state_mut().cad.session.set_selection(hs);
    h.state_mut().make_symbol(None).unwrap();
    h.run_steps(4);
    shot(&h, "048-make.png");
}
