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

// ---- Round 2: owner decision 2026-10-31. On ECAD pages the Block Editor, REFEDIT and LISP are
// refused before anything changes; plain drawings keep upstream behaviour.

/// The message every refusal carries.
const REFUSED: &str = "not available on ECAD pages";

/// What must not move when an attempt is refused.
struct Before {
    doc: ecad_model::Document,
    head: u64,
    drawing: std::sync::Arc<cadcraft_engine::doc::Drawing>,
    log: usize,
}

fn before(h: &mut H) -> Before {
    Before { doc: h.doc(), head: h.head(), drawing: h.s.state().unwrap().doc.clone(), log: h.s.log.len() }
}

/// `what` was refused before anything changed: same document, ledger head and page drawing (the
/// same `Arc`, so not even a rebuilt copy), no editor, no command or routine left running, no
/// engine undo step, the refusal shown, and the project consistent.
fn assert_refused(h: &mut H, b: &Before, what: &str, result: Result<(), String>) {
    let shown = h.s.log.get(b.log..).unwrap_or_default().join(" | ");
    let ctx = format!("{what}: result {result:?}; shown {shown:?}");
    // The commands that only act inside an open editor are disabled before the owner is asked,
    // because neither editor can open on an ECAD page; that refusal is accepted for them only.
    let follow_on = ["bsave", "bsaveas", "bclose", "refset", "refclose"]
        .iter()
        .any(|id| format!("{result:?} {shown}").contains(&format!("command `{id}` is not available right now")));
    assert!(
        result.as_ref().err().is_some_and(|e| e.contains(REFUSED)) || shown.contains(REFUSED) || follow_on,
        "not refused with the message\n  {ctx}"
    );
    assert!(h.doc() == b.doc, "the ECAD document changed\n  {ctx}");
    assert_eq!(h.head(), b.head, "the ledger moved\n  {ctx}");
    let st = h.s.state().unwrap();
    assert!(std::sync::Arc::ptr_eq(&st.doc, &b.drawing), "the page drawing changed\n  {ctx}");
    assert!(st.block_edit.is_none(), "the Block Editor is open\n  {ctx}");
    assert!(h.s.running.is_none(), "a command is left running\n  {ctx}");
    assert!(!h.s.lisp_running(), "a LISP routine is left running\n  {ctx}");
    assert_eq!(engine_steps(h), 0, "an engine undo step on an owned page\n  {ctx}");
    consistent(h).map_err(|e| format!("{e}\n  {ctx}")).unwrap();
}

/// The commands the owner decision covers, with every alias the engine knows.
fn refused_names() -> Vec<String> {
    ecad_cadcraft::install();
    let ids = ["bedit", "bsave", "bsaveas", "bclose", "refedit", "refset", "refclose", "appload", "lisp"];
    let mut out = Vec::new();
    for id in ids {
        let spec = cadcraft_engine::find_command(id).unwrap_or_else(|| panic!("{id} is not a command"));
        out.push(spec.id.to_string());
        out.push(spec.id.to_ascii_uppercase());
        out.extend(spec.aliases.iter().map(|a| a.to_string()));
    }
    out
}

#[test]
fn every_refused_command_and_alias_typed_on_an_ecad_page_changes_nothing() {
    for name in refused_names() {
        let mut h = fixture();
        let b = before(&mut h);
        let r = h.s.cmdline(&name).map_err(|e| e.to_string());
        if h.s.running.is_some() {
            // Not refused at start: cancel so the report shows the real failure below.
            h.s.cancel();
        }
        assert_refused(&mut h, &b, &format!("typed {name:?}"), r);
    }
}

#[test]
fn every_refused_command_started_interactively_on_an_ecad_page_changes_nothing() {
    for name in refused_names() {
        let mut h = fixture();
        let b = before(&mut h);
        let r = h.s.start(&name).map_err(|e| e.to_string());
        if h.s.running.is_some() {
            h.s.cancel();
        }
        assert_refused(&mut h, &b, &format!("start {name:?}"), r);
    }
}

#[test]
fn every_refused_command_by_json_on_an_ecad_page_changes_nothing() {
    let cases = [
        ("bedit", json!(null)),
        ("bedit", json!({"name": "TESTBLK", "create": true})),
        ("bedit", json!({"name": "<Current Drawing>"})),
        ("bedit", json!({"name": "TESTBLK", "create": true, "check": true})),
        ("bsave", json!({})),
        ("bsaveas", json!({"name": "OTHER"})),
        ("bclose", json!({"save": true})),
        ("bclose", json!({"save": false})),
        ("refedit", json!(null)),
        ("refset", json!({"add": []})),
        ("refclose", json!({"option": "discard"})),
        ("appload", json!({"path": "missing.lsp"})),
        ("lisp", json!({"code": "(command \"line\" \"0,90\" \"9,90\" \"\")"})),
        ("lisp", json!({"code": "(entdel (ssname (ssget \"X\") 0))"})),
    ];
    for (id, params) in cases {
        let mut h = fixture();
        let b = before(&mut h);
        let r = h.s.execute(id, &params).map(|_| ()).map_err(|e| e.to_string());
        assert_refused(&mut h, &b, &format!("execute {id} {params}"), r);
    }
}

#[test]
fn lisp_typed_at_the_command_line_or_at_a_prompt_on_an_ecad_page_changes_nothing() {
    let lines = [
        "(+ 1 2)",
        "!x",
        "(command \"line\" \"0,90\" \"9,90\" \"\")",
        "(entdel (ssname (ssget \"X\" '((0 . \"LINE\"))) 0))",
        "(defun c:zz () (command \"erase\" \"all\" \"\"))",
    ];
    for line in lines {
        let mut h = fixture();
        let b = before(&mut h);
        let r = h.s.cmdline(line).map_err(|e| e.to_string());
        assert_refused(&mut h, &b, &format!("typed {line:?}"), r);
    }
    // A LISP expression as the answer to a command's point prompt.
    let mut h = fixture();
    let b = before(&mut h);
    let r = h.s.cmdline("line (list 0 90) ").map_err(|e| e.to_string());
    let shown = h.s.log.get(b.log..).unwrap_or_default().join(" | ");
    h.s.cancel();
    assert!(shown.contains(REFUSED), "LISP at a point prompt was not refused: {r:?} {shown:?}");
    assert!(h.doc() == b.doc && h.head() == b.head, "LISP at a prompt changed the project: {shown:?}");
    assert_eq!(engine_steps(&h), 0);
    consistent(&mut h).unwrap();
}

#[test]
fn a_script_with_block_editor_and_lisp_lines_on_an_ecad_page_changes_nothing() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    for (name, text) in [
        ("bedit.scr", "-bedit\nTESTBLK\n"),
        ("lisp.scr", "(command \"line\" \"0,90\" \"9,90\" \"\")\n"),
        ("crlf.scr", "(entdel (ssname (ssget \"X\") 0))\r\nbedit\r\n"),
    ] {
        let path = dir.join(format!("ecad-007-{name}"));
        std::fs::write(&path, text).unwrap();
        let mut h = fixture();
        let b = before(&mut h);
        let r = h.s.execute("script", &json!({"path": path.to_string_lossy()})).map(|_| ()).map_err(|e| e.to_string());
        if h.s.running.is_some() {
            h.s.cancel();
        }
        assert_refused(&mut h, &b, &format!("script {name}"), r);
    }
}

type Attempt = (&'static str, fn(&mut cadcraft_ui_egui::CadApp) -> Result<(), String>);

#[test]
fn the_ui_entry_points_on_an_ecad_page_change_nothing() {
    // Menus, Tool Sets, the context menu and the Block Editor dialog reach BEDIT through
    // `CadApp::run`/`start`/`cmdline`.
    let attempts: [Attempt; 7] = [
        ("run bedit", |a| a.run("bedit", json!({"name": "TESTBLK", "create": true})).map(|_| ())),
        ("run bedit current", |a| a.run("bedit", json!({"name": "<Current Drawing>"})).map(|_| ())),
        ("run bclose", |a| a.run("bclose", json!({"save": true})).map(|_| ())),
        ("run refedit", |a| a.run("refedit", json!(null)).map(|_| ())),
        ("cmdline be", |a| a.cmdline("be").map_or(Ok(()), Err)),
        ("cmdline lisp", |a| a.cmdline("(+ 1 2)").map_or(Ok(()), Err)),
        ("start bedit", |a| {
            a.start("bedit");
            Ok(())
        }),
    ];
    for (what, f) in attempts {
        let mut h = fixture();
        let b = before(&mut h);
        let s = std::mem::replace(&mut h.s, cadcraft_engine::Session::empty());
        let mut app = cadcraft_ui_egui::CadApp::new(
            s,
            cadcraft_ui_egui::Services { pick_open: None, pick_save: None, pick_lisp: None, no_file_system: false },
        );
        let r = f(&mut app);
        let dialog = app.ui.dialog.take();
        h.s = std::mem::replace(&mut app.session, cadcraft_engine::Session::empty());
        if h.s.running.is_some() {
            h.s.cancel();
        }
        println!("{what}: dialog {dialog:?}");
        if dialog.is_some() {
            // The dialog opened instead of the command (it only lists blocks; confirming it is
            // `run("bedit", …)`, tested above): nothing may have changed.
            assert!(h.doc() == b.doc && h.head() == b.head && engine_steps(&h) == 0, "{what} changed the project");
            assert!(h.s.state().unwrap().block_edit.is_none(), "{what} opened the Block Editor");
            consistent(&mut h).unwrap();
        } else {
            assert_refused(&mut h, &b, what, r);
        }
    }
}

#[test]
fn a_routine_from_a_plain_drawing_cannot_continue_on_an_ecad_page() {
    let mut h = fixture();
    let ecad = h.s.active;
    h.s.execute("new", &json!({})).unwrap();
    h.s.execute("line", &json!({"points": [[0, 0], [1, 1]]})).unwrap();
    let r = h.s.lisp_eval("(getpoint) (entdel (ssname (ssget \"X\") 0))", false, false, Vec::new());
    assert!(r.is_ok(), "the routine starts on a plain drawing: {r:?}");
    h.s.active = ecad;
    let b = before(&mut h);
    let _ = h.s.cmdline("5,5");
    let shown = h.s.log.get(b.log..).unwrap_or_default().join(" | ");
    assert!(h.doc() == b.doc && h.head() == b.head, "the routine changed the ECAD page: {shown:?}");
    assert!(std::sync::Arc::ptr_eq(&h.s.state().unwrap().doc, &b.drawing), "the page drawing changed: {shown:?}");
    assert_eq!(engine_steps(&h), 0);
    consistent(&mut h).unwrap();
}

#[test]
fn refused_block_editor_attempts_leave_undo_to_the_earlier_real_edit() {
    let mut h = fixture();
    let doc_before_edit = h.doc();
    h.cmdline("line 0,80 10,80 ");
    let head_after_edit = h.head();
    assert!(h.doc() != doc_before_edit, "the real edit was recorded");
    let b = before(&mut h);
    for (id, params) in [
        ("bedit", json!({"name": "TESTBLK", "create": true})),
        ("bclose", json!({"save": false})),
        ("bedit", json!({"name": "<Current Drawing>"})),
        ("bclose", json!({"save": true})),
        ("bsave", json!({})),
    ] {
        let r = h.s.execute(id, &params).map(|_| ()).map_err(|e| e.to_string());
        assert_refused(&mut h, &b, &format!("{id} {params}"), r);
    }
    let undo = h.s.execute("undo", &json!({}));
    let report = format!("undo: {undo:?}");
    assert!(undo.is_ok(), "{report}");
    assert!(h.doc() == doc_before_edit, "UNDO did not undo exactly the LINE\n  {report}");
    assert_eq!(h.head(), head_after_edit + 1, "UNDO is one transaction\n  {report}");
    assert_eq!(engine_steps(&h), 0);
    consistent(&mut h).map_err(|e| format!("{e}\n  {report}")).unwrap();
}

#[test]
fn plain_drawings_keep_upstream_block_editor_refedit_and_lisp() {
    // An ECAD project is open (its owner registered), but the active drawing is a plain one.
    let mut h = fixture();
    h.s.execute("new", &json!({})).unwrap();
    let s = &mut h.s;
    s.execute("bedit", &json!({"name": "B", "create": true})).unwrap();
    assert!(s.state().unwrap().block_edit.is_some(), "BEDIT opens on a plain drawing");
    s.execute("line", &json!({"points": [[0, 0], [5, 5]]})).unwrap();
    s.execute("bsave", &json!({})).unwrap();
    s.execute("bclose", &json!({"save": true})).unwrap();
    assert!(s.doc().unwrap().block("B").is_some_and(|b| b.entities.len() == 1), "BSAVE defined the block");
    assert!(!s.state().unwrap().undo.is_empty(), "the Block Editor keeps its engine undo step on a plain drawing");
    let ins = s.execute("insert", &json!({"name": "B", "at": [10, 10]})).unwrap();
    let handle = ins["handle"].as_str().or_else(|| ins["handles"][0].as_str()).map(str::to_string);
    let handle = handle.unwrap_or_else(|| panic!("insert returned {ins}"));
    s.execute("refedit", &json!({"handle": handle})).unwrap();
    s.execute("refclose", &json!({"option": "discard"})).unwrap();
    let undo = s.state().unwrap().undo.len();
    s.cmdline("(command \"line\" \"0,20\" \"9,20\" \"\")").unwrap();
    let v = s.lisp_eval("(+ 1 2)", false, true, Vec::new()).unwrap();
    assert!(format!("{v:?}").contains('3'), "LISP evaluates on a plain drawing: {v:?}");
    assert!(s.state().unwrap().undo.len() > undo, "LISP records engine undo on a plain drawing");
}

#[test]
fn every_command_started_interactively_and_answered_with_enter_leaves_the_project_consistent() {
    // The census runs each command's JSON form; this starts each one as typed, accepts the
    // defaults with Enter twice and cancels, looking for any other path around the owner or an
    // engine undo step left on an ECAD page.
    const SKIP: &[&str] =
        &["close", "closeall", "new", "qnew", "open", "quit", "exit", "newsheetset", "recover", "recoverall", "delay"];
    ecad_cadcraft::install();
    let ours: Vec<&str> = ecad_cadcraft::commands::specs().iter().map(|c| c.id).collect();
    let mut broken = Vec::new();
    for spec in cadcraft_engine::command_specs().iter().filter(|c| !SKIP.contains(&c.id) && !ours.contains(&c.id)) {
        let mut h = fixture();
        let page = h.first_page();
        let all: Vec<_> = h.pv(|p, pg| pg.entities_of(p, page)).iter().map(|e| e.handle).collect();
        h.s.set_selection(all);
        let _ = h.s.start(spec.id);
        for _ in 0..2 {
            if h.s.running.is_some() {
                let _ = h.s.cmdline("");
            }
        }
        if h.s.running.is_some() {
            h.s.cancel();
        }
        let editing = h.s.state().is_ok_and(|st| st.block_edit.is_some());
        if let Err(e) = consistent(&mut h) {
            broken.push(format!("{}: {}", spec.id, e.lines().next().unwrap_or_default()));
        } else if editing || h.s.lisp_running() {
            broken.push(format!("{}: left the Block Editor or a LISP routine open", spec.id));
        }
    }
    assert!(broken.is_empty(), "{} commands broke the project:\n  {}", broken.len(), broken.join("\n  "));
}
