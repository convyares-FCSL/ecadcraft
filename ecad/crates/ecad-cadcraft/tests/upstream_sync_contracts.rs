//! Contract tests for brief 007 (upstream sync to CADCraft 0.15.1), written by the independent
//! tester. CADCraft 0.15.1 brought command paths the previous base (0.6.2) did not have: LISP
//! (`entmake`, `entdel`, `(command …)`, its own undo grouping), the Block Editor (BEDIT, BSAVE,
//! BCLOSE, which swap the drawing and write into the host drawing directly) and new clipboard
//! commands (COPYBASE, PASTEBLOCK). On an ECAD page every one of them must either be recorded by
//! the owner or refused, and leave the project consistent (proposal §17.4). The engine must push no
//! undo step of its own for an owned page: the owner keeps the history.
//!
//! The LISP entity-list functions (`entmake`, `entmod`) are not available in this build; their
//! tests pass only because nothing changes, and become meaningful when upstream adds them.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{H, pts};
use serde_json::json;

/// A page holding a component (four converted lines and a terminal), a placed symbol, a wire and
/// a loose line, as in the command census.
fn fixture() -> H {
    let mut h = H::new(7);
    h.cmdline("line 0,0 40,0 40,20 0,20 c");
    let first = h.first_page();
    let lines = h.lines_on(first);
    h.run("convertcomponent", json!({"handles": lines.iter().map(|x| x.hex()).collect::<Vec<_>>()})).unwrap();
    h.run("addterminal", json!({"handle": lines[1].hex(), "at": [40, 10]})).unwrap();
    h.run("place", json!({"at": [100, 0]})).unwrap();
    h.run("wire", json!({"points": pts(&[(40.0, 10.0), (100.0, 10.0)])})).unwrap();
    h.cmdline("line 0,40 10,40 ");
    h
}

/// The §17.4 checks, as a result rather than a panic, so a table can report every row.
fn consistent(h: &mut H) -> Result<(), String> {
    catch_unwind(AssertUnwindSafe(|| h.check())).map_err(|p| {
        p.downcast_ref::<String>()
            .cloned()
            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default()
    })
}

/// Engine undo and redo steps on the active (owned) page: there must be none.
fn engine_steps(h: &H) -> usize {
    h.s.state().map(|st| st.undo.len() + st.redo.len()).unwrap_or(0)
}

/// What a step did to the project, for the report.
#[derive(Debug)]
#[allow(dead_code)]
struct Outcome {
    step: &'static str,
    result: Result<String, String>,
    head_before: u64,
    head_after: u64,
    engine_steps: usize,
    consistent: Result<(), String>,
}

/// Run `steps` (command-line text) one after another on a fresh fixture; after the last one, the
/// project must be consistent and the page must carry no engine undo step.
fn probe(steps: &[&'static str]) -> Vec<Outcome> {
    let mut h = fixture();
    let mut out = Vec::new();
    for step in steps {
        let head_before = h.head();
        let result = h.s.cmdline(step).map(|()| h.s.log.iter().rev().take(3).cloned().collect::<Vec<_>>().join(" | "));
        let result = result.map_err(|e| e.to_string());
        if h.s.running.is_some() {
            h.s.cancel();
        }
        let head_after = h.head();
        let engine_steps = engine_steps(&h);
        // Mid-way through a Block Editor session the page shows the editor's drawing; consistency
        // is only meaningful once the editor is closed.
        let editing = h.s.state().is_ok_and(|st| st.block_edit.is_some());
        let consistent = if editing { Ok(()) } else { consistent(&mut h) };
        out.push(Outcome { step, result, head_before, head_after, engine_steps, consistent });
    }
    out
}

fn assert_probe(name: &'static str, steps: &[&'static str]) {
    let outcomes = probe(steps);
    let report: String = outcomes.iter().map(|o| format!("  {o:?}\n")).collect();
    println!("{name}:\n{report}");
    for o in &outcomes {
        assert!(
            o.consistent.is_ok(),
            "{name}: step {:?} left the project inconsistent: {:?}\n{report}",
            o.step,
            o.consistent
        );
        assert_eq!(
            o.engine_steps, 0,
            "{name}: step {:?} pushed an engine undo step on an owned page\n{report}",
            o.step
        );
        assert!(
            o.head_after <= o.head_before + 1,
            "{name}: step {:?} appended more than one transaction\n{report}",
            o.step
        );
    }
}

#[test]
fn lisp_entmake_on_an_owned_page_is_recorded_or_refused() {
    assert_probe("entmake", &["(entmake '((0 . \"LINE\") (10 0.0 60.0 0.0) (11 20.0 60.0 0.0)))"]);
}

#[test]
fn lisp_entdel_of_a_component_line_is_recorded_or_refused() {
    assert_probe("entdel", &["(entdel (ssname (ssget \"X\" '((0 . \"LINE\"))) 0))"]);
}

#[test]
fn lisp_command_inside_a_routine_is_recorded_or_refused() {
    assert_probe("command", &["(command \"line\" \"0,70\" \"30,70\" \"\")"]);
}

#[test]
fn lisp_entmod_of_a_line_is_recorded_or_refused() {
    assert_probe(
        "entmod",
        &[
            "(setq e (entget (ssname (ssget \"X\" '((0 . \"LINE\"))) 0))) (entmod (subst '(8 . \"MOVED\") (assoc 8 e) e))",
        ],
    );
}

/// One traced step of a JSON-form sequence.
fn trace(h: &mut H, cmd: &str, params: serde_json::Value) -> String {
    let head = h.head();
    let r = h.s.execute(cmd, &params).map_err(|e| e.to_string());
    let editing = h.s.state().is_ok_and(|st| st.block_edit.is_some());
    let labels: Vec<String> = h.p(|p| {
        p.ledger.log().iter().skip(head as usize).map(|t| format!("{} ({} ops)", t.label, t.ops.len())).collect()
    });
    let items = h.doc().items.len();
    let entities = h.s.doc().map(|d| d.model.len()).unwrap_or(0);
    format!(
        "{cmd}: {r:?}; head {head} -> {} {labels:?}; items {items}; drawing entities {entities}; editor open {editing}; engine steps {}",
        h.head(),
        engine_steps(h)
    )
}

#[test]
fn block_editor_on_the_current_drawing_is_recorded_or_refused() {
    let mut h = fixture();
    let mut log = Vec::new();
    log.push(trace(&mut h, "bedit", json!({"name": "<Current Drawing>"})));
    if h.s.state().is_ok_and(|st| st.block_edit.is_some()) {
        let all: Vec<_> = h.s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
        log.push(trace(&mut h, "erase", json!({"handles": all})));
        log.push(trace(&mut h, "bclose", json!({"save": true})));
    }
    let report = log.join("\n  ");
    println!("bedit <Current Drawing>:\n  {report}");
    assert!(h.s.state().is_ok_and(|st| st.block_edit.is_none()), "the Block Editor is still open\n  {report}");
    assert_eq!(engine_steps(&h), 0, "an engine undo step on an owned page\n  {report}");
    consistent(&mut h).map_err(|e| format!("{e}\n  {report}")).unwrap();
}

#[test]
fn block_editor_opening_on_an_owned_page_does_not_delete_its_items() {
    let mut h = fixture();
    let items = h.doc().items.len();
    let step = trace(&mut h, "bedit", json!({"name": "TESTBLK", "create": true}));
    println!("{step}");
    assert_eq!(h.doc().items.len(), items, "opening the Block Editor changed the ECAD document\n  {step}");
    let _ = h.s.execute("bclose", &json!({"save": false}));
}

#[test]
fn block_editor_opened_and_discarded_leaves_the_project_as_it_was() {
    // Open the Block Editor on a new block and close it without saving: nothing happened.
    let mut h = fixture();
    let before = h.doc();
    let a = trace(&mut h, "bedit", json!({"name": "TESTBLK", "create": true}));
    let b = trace(&mut h, "bclose", json!({"save": false}));
    let report = format!("{a}\n  {b}");
    println!("{report}");
    assert!(h.doc() == before, "the ECAD document changed\n  {report}");
    consistent(&mut h).map_err(|e| format!("{e}\n  {report}")).unwrap();
}

#[test]
fn block_editor_new_block_saved_into_the_page_is_recorded_or_refused() {
    let mut h = fixture();
    let mut log = Vec::new();
    log.push(trace(&mut h, "bedit", json!({"name": "TESTBLK", "create": true})));
    if h.s.state().is_ok_and(|st| st.block_edit.is_some()) {
        log.push(trace(&mut h, "line", json!({"points": [[0, 0], [5, 5]]})));
        log.push(trace(&mut h, "bsave", json!({})));
        log.push(trace(&mut h, "bclose", json!({"save": true})));
    }
    let report = log.join("\n  ");
    println!("bedit new block:\n  {report}");
    assert!(h.s.state().is_ok_and(|st| st.block_edit.is_none()), "the Block Editor is still open\n  {report}");
    assert_eq!(engine_steps(&h), 0, "an engine undo step on an owned page\n  {report}");
    consistent(&mut h).map_err(|e| format!("{e}\n  {report}")).unwrap();
}

#[test]
fn copybase_and_pasteblock_on_an_owned_page_are_recorded_or_refused() {
    let mut h = fixture();
    let page = h.first_page();
    let all: Vec<_> = h.pv(|p, pg| pg.entities_of(p, page)).iter().map(|e| e.handle.hex()).collect();
    let a = trace(&mut h, "copybase", json!({"handles": all, "base": [0, 0]}));
    let b = trace(&mut h, "pasteblock", json!({"at": [200, 200]}));
    println!("{a}\n{b}");
    assert_eq!(engine_steps(&h), 0, "PASTEBLOCK pushed an engine undo step on an owned page");
    consistent(&mut h).unwrap();
}

#[test]
fn a_plain_drawing_keeps_upstream_lisp_undo_grouping() {
    // Documents without an owner behave exactly as upstream: a routine that runs two commands is
    // one engine undo step.
    ecad_cadcraft::install();
    let mut s = cadcraft_engine::Session::empty();
    s.execute("new", &json!({})).unwrap();
    let before = s.state().map(|st| st.undo.len()).unwrap();
    let r = s.lisp_eval(
        "(command \"line\" \"0,0\" \"10,0\" \"\") (command \"line\" \"0,5\" \"10,5\" \"\")",
        false,
        true,
        Vec::new(),
    );
    let after = s.state().map(|st| st.undo.len()).unwrap();
    println!("{r:?} log: {:?}", s.log);
    assert_eq!(s.doc().unwrap().model.len(), 2);
    assert_eq!(after, before + 1, "a LISP routine on a plain drawing is one undo step");
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 0, "undo removes both objects of the routine");
}

#[test]
fn undo_after_a_discarded_block_editor_session_restores_the_items() {
    let mut h = fixture();
    let before = h.doc();
    let _ = h.s.execute("bedit", &json!({"name": "TESTBLK", "create": true}));
    let _ = h.s.execute("bclose", &json!({"save": false}));
    let undo = h.s.execute("undo", &json!({}));
    let report = format!("undo: {undo:?}; items {} (were {})", h.doc().items.len(), before.items.len());
    println!("{report}");
    assert!(h.doc() == before, "UNDO did not bring the document back\n  {report}");
    consistent(&mut h).map_err(|e| format!("{e}\n  {report}")).unwrap();
}
