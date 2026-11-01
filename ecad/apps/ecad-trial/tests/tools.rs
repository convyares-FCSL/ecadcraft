//! The ECAD group in CADCraft's Tool Sets panel: it lists only implemented commands, a click is
//! handled once through the ordinary command path (validation, transaction, undo), the six
//! interactive tools start their prompts (brief 008), Convert uses the selection or asks for one,
//! and templates fill the Command field without running their placeholders.

use cadcraft_engine::doc::{EntityKind, Handle};
use ecad_trial::Trial;
use ecad_trial::tools::{self, Kind, TOOLS};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn trial() -> Trial {
    ecad_cadcraft::install();
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

/// A rectangle of four lines on the page; returns their handles.
fn rectangle(t: &mut Trial) -> Vec<Handle> {
    assert!(t.submit_command("line 0,0 40,0 40,20 0,20 c").is_none());
    t.cad
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .filter(|e| matches!(e.kind, EntityKind::Line(_)))
        .map(|e| e.handle)
        .collect()
}

#[test]
fn the_group_lists_only_implemented_commands_with_namespaced_ids() {
    let specs = ecad_cadcraft::commands::specs();
    let ids: Vec<&str> = specs.iter().map(|s| s.id).collect();
    let mut seen = std::collections::BTreeSet::new();
    for t in TOOLS {
        assert!(t.id.starts_with("ecad."), "{} is namespaced", t.id);
        assert!(seen.insert(t.id), "{} is unique", t.id);
        assert!(!t.label.is_empty() && !t.tooltip.is_empty());
        let line = match t.kind {
            Kind::Start(command) => {
                let spec = cadcraft_engine::find_command(command).unwrap();
                assert!(spec.interactive.is_some(), "{} starts an interactive form", t.id);
                command.to_string()
            }
            Kind::Run(line) => line.to_string(),
            Kind::Template => tools::template(t.id, &tools::Picked::default()).unwrap().0,
        };
        let command = line.split_whitespace().next().unwrap();
        assert!(ids.contains(&command), "{} runs the existing command {command}", t.id);
    }
    let tab = tools::tab();
    assert_eq!(tab.name, "ECAD");
    assert_eq!(tab.groups.iter().map(|g| g.tools.len()).sum::<usize>(), TOOLS.len(), "every tool is in a group");
    assert!(tab.groups.iter().all(|g| !g.tools.is_empty()));
}

#[test]
fn convert_in_tool_sets_uses_the_selection_and_undoes_like_the_typed_command() {
    let mut t = trial();
    let lines = rectangle(&mut t);
    assert_eq!(lines.len(), 4);
    t.cad.session.set_selection(lines.clone());
    let before = head(&mut t);
    let mut h = harness(t);
    assert!(h.query_by_label("Convert to Component").is_none(), "Drafting is shown first, unchanged");
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    assert_eq!(h.state().cad.ui.toolset_tab, "ECAD", "ECAD is a Tool Sets tab beside Drafting and Modeling");
    h.get_by_label("Convert to Component").click();
    h.run_steps(4);
    let t = h.state_mut();
    assert!(t.cad.take_tool_requests().is_empty(), "the click was consumed");
    assert_eq!(head(t), before + 1, "one transaction, recorded once");
    let d = doc(t);
    assert_eq!(d.components.len(), 1);
    let rep = d.representations.values().next().unwrap();
    assert_eq!(rep.items.len(), 4, "made from the four selected lines");
    // Undo is the project's own: the component goes away.
    assert!(t.submit_command("u").is_none());
    assert!(doc(t).components.is_empty());
}

#[test]
fn convert_with_nothing_selected_asks_to_select_objects() {
    let mut t = trial();
    let before = head(&mut t);
    t.tool("ecad.convertcomponent").unwrap();
    assert_eq!(t.cad.session.current_prompt().unwrap().display(), "Select objects to convert to a component:");
    assert_eq!(head(&mut t), before, "nothing recorded yet");
    t.cad.session.cancel();
    assert!(t.cad.session.running.is_none());
    assert_eq!(head(&mut t), before, "Esc records nothing");
}

/// Design 008 check A: each interactive button starts its form at its first prompt, with nothing
/// left in the Command field and no template note, even when a template was waiting there.
#[test]
fn the_interactive_buttons_start_their_prompts() {
    let mut h = harness(trial());
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    let firsts = [
        ("Convert to Component", "convertcomponent", "Select objects to convert to a component:"),
        // No component on the page: no number is promised (fix round 2, N3).
        ("Add Terminal", "addterminal", "Specify point for a terminal on a component or [Name]:"),
        ("Place Symbol", "place", "Specify insertion point (rotation 0, not mirrored) or [Rotate/Mirror]:"),
        ("Wire", "wire", "Specify first point of wire:"),
        ("Connect", "connect", "Select wire near the end to connect:"),
        ("Disconnect", "disconnect", "Select wire near the end to disconnect:"),
    ];
    for (label, id, first) in firsts {
        h.state_mut().tool("ecad.newpage").unwrap();
        assert!(h.state().command_hint().is_some(), "a template waits in the field");
        // Its note moves the panel below it: lay out once before clicking by position.
        h.run_steps(2);
        let before = head(h.state_mut());
        h.get_by_label(label).click();
        h.run_steps(3);
        let t = h.state_mut();
        assert_eq!(t.cad.session.running.as_ref().map(|r| r.id.as_str()), Some(id), "{label}");
        assert_eq!(t.cad.session.current_prompt().unwrap().display(), first, "{label}");
        assert!(t.command.is_empty() && t.command_hint().is_none(), "{label}: the field is cleared");
        assert_eq!(head(t), before, "{label}: starting records nothing");
        t.cad.session.cancel();
    }
}

#[test]
fn a_template_fills_the_command_field_and_its_placeholders_never_run() {
    let mut h = harness(trial());
    h.get_by_label(tools::TAB).click();
    h.run_steps(3);
    let before = head(h.state_mut());
    let selection = vec![Handle(0x7777)];
    h.state_mut().cad.session.set_selection(selection.clone());
    h.get_by_label("New Page").click();
    h.run_steps(4);
    let t = h.state_mut();
    assert_eq!(t.command, r#"newpage {"title":"<title>"}"#);
    assert!(t.command_hint().unwrap().starts_with("New Page:"));
    assert_eq!(head(t), before, "nothing ran");
    assert_eq!(t.cad.session.selection(), selection, "the selection is left as it was");
    // Pressing Enter on the unedited template is refused.
    let line = t.command.clone();
    let e = t.submit_command(&line).unwrap();
    assert!(e.contains("Replace the placeholders first") && e.contains("<title>"), "{e}");
    assert_eq!(head(t), before);
    // Edited, it runs through the ordinary path and the note goes away.
    assert!(t.submit_command(r#"newpage {"title":"2"}"#).is_none());
    assert_eq!(head(t), before + 1);
    assert!(t.command_hint().is_none());
}

#[test]
fn templates_take_ids_from_the_selection() {
    let mut t = trial();
    assert!(t.submit_command(r#"wire {"points":[[0,0],[40,0]]}"#).is_none());
    let d = doc(&mut t);
    let (cid, _) = d.conductors.iter().next().unwrap();
    let page = d.page_order[0];
    let (item, _) = ecad_model::derive::route_on_page(&d, *cid, page).unwrap();
    let p = t.project().unwrap().clone();
    let h = p.read(&mut t.cad.session, |p, _| p.pages[&page].map.item(item).unwrap()).unwrap();
    t.cad.session.set_selection(vec![h]);
    t.tool("ecad.setprop").unwrap();
    assert_eq!(t.command, format!(r#"setprop {{"conductor":"{}","key":"tag","value":"<value>"}}"#, cid.hex()));
    t.tool("ecad.delete").unwrap();
    assert_eq!(t.command, format!(r#"ecaddelete {{"conductor":"{}"}}"#, cid.hex()));
    t.tool("ecad.cableadd").unwrap();
    assert!(t.command.contains(&cid.hex()));
    // With the value filled in, the template runs as the typed command would.
    let line = t.command.replace("<cable tag>", "-W1");
    assert!(t.submit_command(&line).is_none(), "{line}");
    assert_eq!(doc(&mut t).cables.len(), 1);
}

#[test]
fn a_read_only_project_is_treated_as_the_typed_command_treats_it() {
    let dir = std::env::temp_dir().join(format!("ecad-trial-tools-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("shared.ecad").display().to_string();
    let mut first = trial();
    first.save_as(&path).unwrap();
    // A second window: the same file opens read-only.
    let mut second = Trial::with_file(None, &path).unwrap();
    assert!(second.status().unwrap().file.read_only.is_some());
    second.tool("ecad.inspect").unwrap();
    let lines = rectangle(&mut second);
    second.cad.session.set_selection(lines);
    second.tool("ecad.convertcomponent").unwrap();
    assert_eq!(doc(&mut second).components.len(), 1, "edits stay in memory, as for typed commands");
    assert!(second.cad.run("qsave", json!(null)).is_err(), "and still cannot be saved over the other session's file");
    drop(second);
    drop(first);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn placeholders_are_found_and_ordinary_text_is_not() {
    assert_eq!(tools::placeholders(r#"place {"at":[<x>,<y>]}"#), vec!["<x>", "<y>"]);
    assert_eq!(tools::placeholders(r#"setprop {"value":"<cable tag>"}"#), vec!["<cable tag>"]);
    assert!(tools::placeholders(r#"setprop {"value":"a < b > c"}"#).is_empty());
    assert!(tools::placeholders("wire {\"points\":[[0,0],[1,1]]}").is_empty());
}

#[test]
fn a_run_button_leaves_a_waiting_template_and_its_check_alone() {
    let mut t = trial();
    t.tool("ecad.newpage").unwrap();
    let template = t.command.clone();
    let before = head(&mut t);
    t.tool("ecad.inspect").unwrap();
    assert_eq!(t.command, template);
    assert!(t.command_hint().is_some());
    let e = t.submit_command(&template).unwrap();
    assert!(e.contains("<title>"), "{e}");
    assert_eq!(head(&mut t), before, "the quoted placeholder was not run as a page title");
}

#[test]
fn a_command_waiting_at_a_prompt_is_cancelled_first() {
    let mut t = trial();
    let lines = rectangle(&mut t);
    t.cad.session.set_selection(lines);
    // LINE waits for its first point; the button's line must not become that point.
    assert!(t.submit_command("line").is_none());
    assert!(t.cad.session.running.is_some());
    let before = head(&mut t);
    t.tool("ecad.convertcomponent").unwrap();
    assert!(t.cad.session.running.is_none(), "LINE was cancelled");
    assert_eq!(head(&mut t), before + 1, "and the component was made");
    assert_eq!(doc(&mut t).components.len(), 1);
    // A template is not handed to a running command either.
    assert!(t.submit_command("line").is_none());
    t.tool("ecad.setprop").unwrap();
    assert!(t.cad.session.running.is_none());
    assert!(t.command.starts_with("setprop "));
    // And an interactive tool replaces the waiting command.
    assert!(t.submit_command("line").is_none());
    t.tool("ecad.wire").unwrap();
    assert_eq!(t.cad.session.running.as_ref().unwrap().id, "wire");
}

#[test]
fn a_huge_selection_is_followed_only_so_far() {
    let mut t = trial();
    let handles: Vec<Handle> = (1..=(tools::MAX_PICKED as u64 + 50)).map(Handle).collect();
    t.cad.session.set_selection(handles);
    t.tool("ecad.setprop").unwrap();
    assert!(t.command.contains("<component id>"), "nothing it names is ECAD: placeholders stay");
    let p = t.project().unwrap().clone();
    let uid = t.cad.session.state().unwrap().uid;
    let many: Vec<Handle> = (1..=(tools::MAX_PICKED as u64 + 50)).map(Handle).collect();
    let picked = p.read(&mut t.cad.session, |p, _| tools::Picked::from_selection(p, uid, many)).unwrap();
    assert!(picked.truncated);
}
