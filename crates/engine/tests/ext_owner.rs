//! ECAD-SEAM(owner): the embedder extension interface (`cadcraft_engine::ext`).
//!
//! A minimal owner keeps its own snapshot history to show that every commit point reaches it,
//! that it can refuse, that undo/redo/save/close are handed over, and that provenance and
//! clipboard copies are reported. Plain documents in the same session are unaffected.

use std::any::Any;
use std::sync::{Arc, Mutex};

use cadcraft_engine::Session;
use cadcraft_engine::doc::{Drawing, EntityKind, Handle, Space};
use cadcraft_engine::ext::{Commit, DocumentOwner, OwnerRef, Source, Start, Verdict};
use serde_json::{Value, json};

#[derive(Default)]
struct Log {
    commits: Vec<(String, usize, usize)>,
    clipboard: Vec<usize>,
    undos: usize,
    redos: usize,
    saves: Vec<(Option<String>, bool)>,
    closed: Vec<u64>,
    recovered: usize,
    reentry: Vec<String>,
}

struct TestOwner {
    log: Arc<Mutex<Log>>,
    history: Vec<Arc<Drawing>>,
    redo: Vec<Arc<Drawing>>,
    last: Option<Arc<Drawing>>,
    refuse_next: bool,
    panic_next: bool,
    panic_in_query: bool,
    panic_in_close: bool,
    reenter: bool,
    unsaved: bool,
    /// Refuse CIRCLE and LISP before they start; panic when asked (`panic_in_start`).
    refuse_starts: bool,
    panic_in_start: bool,
}

impl TestOwner {
    fn new(log: Arc<Mutex<Log>>) -> Self {
        Self {
            log,
            history: Vec::new(),
            redo: Vec::new(),
            last: None,
            refuse_next: false,
            panic_next: false,
            panic_in_query: false,
            panic_in_close: false,
            reenter: false,
            unsaved: false,
            refuse_starts: false,
            panic_in_start: false,
        }
    }
    fn swap_to(&mut self, s: &mut Session, uid: u64, d: Arc<Drawing>) {
        if let Some(st) = s.docs.iter_mut().find(|d| d.uid == uid) {
            st.doc = d.clone();
        }
        self.last = Some(d);
    }
}

impl DocumentOwner for TestOwner {
    fn refuse_start(&self, _s: &Session, _uid: u64, start: Start<'_>) -> Option<String> {
        assert!(!self.panic_in_start, "injected start failure");
        let refused = matches!(start, Start::Command("circle") | Start::Lisp);
        (self.refuse_starts && refused).then(|| "not started by the test owner".into())
    }
    fn commit(&mut self, s: &mut Session, c: &Commit<'_>) -> Verdict {
        if std::mem::take(&mut self.panic_next) {
            panic!("injected owner failure");
        }
        if std::mem::take(&mut self.reenter) {
            // Calls back into the session from inside a callback, while the engine holds this owner.
            let inner = s.execute("line", &json!({"points": [[0, 9], [1, 9]]}));
            let undo = s.execute("u", &Value::Null);
            let mut l = self.log.lock().unwrap();
            l.reentry.push(format!("execute: {:?}", inner.err().map(|e| e.to_string())));
            l.reentry.push(format!("undo: {:?}", undo.err().map(|e| e.to_string())));
            l.reentry.push(format!("unsaved: {}", s.doc_unsaved(0)));
        }
        self.log.lock().unwrap().commits.push((c.command.to_string(), c.provenance.len(), c.clipboard.map_or(0, |x| x.handles.len())));
        if let Some(cb) = c.clipboard {
            self.log.lock().unwrap().clipboard.push(cb.handles.len());
        }
        let st = s.docs.iter().find(|d| d.uid == c.uid).unwrap();
        let now = st.doc.clone();
        let last = self.last.clone().unwrap();
        if Arc::ptr_eq(&now, &last) {
            return Verdict::Accept;
        }
        if std::mem::take(&mut self.refuse_next) {
            self.swap_to(s, c.uid, last);
            return Verdict::Refuse("refused by the test owner".into());
        }
        self.history.push(last);
        self.redo.clear();
        self.last = Some(now);
        self.unsaved = true;
        Verdict::Accept
    }
    fn undo(&mut self, s: &mut Session, uid: u64) -> cadcraft_engine::Result<Option<String>> {
        self.log.lock().unwrap().undos += 1;
        let Some(prev) = self.history.pop() else { return Ok(None) };
        if let Some(cur) = self.last.clone() {
            self.redo.push(cur);
        }
        self.swap_to(s, uid, prev);
        Ok(Some("step".into()))
    }
    fn redo(&mut self, s: &mut Session, uid: u64) -> cadcraft_engine::Result<Option<String>> {
        self.log.lock().unwrap().redos += 1;
        let Some(next) = self.redo.pop() else { return Ok(None) };
        if let Some(cur) = self.last.clone() {
            self.history.push(cur);
        }
        self.swap_to(s, uid, next);
        Ok(Some("step".into()))
    }
    fn save(&mut self, _s: &mut Session, _uid: u64, path: Option<&str>, save_as: bool) -> cadcraft_engine::Result<Value> {
        self.log.lock().unwrap().saves.push((path.map(str::to_string), save_as));
        self.unsaved = false;
        Ok(json!({"saved": true}))
    }
    fn is_unsaved(&self) -> bool {
        assert!(!self.panic_in_query, "injected query failure");
        self.unsaved
    }
    fn close_needs_prompt(&self, _uid: u64) -> bool {
        assert!(!self.panic_in_query, "injected query failure");
        false
    }
    fn closed(&mut self, _s: &mut Session, uid: u64) -> bool {
        assert!(!std::mem::take(&mut self.panic_in_close), "injected close failure");
        self.log.lock().unwrap().closed.push(uid);
        true
    }
    fn drag_extra(&self, _s: &Session, _uid: u64, _drag: &cadcraft_engine::Drag) -> Vec<EntityKind> {
        vec![EntityKind::Point(cadcraft_engine::doc::Point { p: Default::default(), angle: 0.0 })]
    }
    fn recover(&mut self, s: &mut Session) {
        self.log.lock().unwrap().recovered += 1;
        if let (Some(last), Some(uid)) = (self.last.clone(), s.state().ok().map(|st| st.uid)) {
            self.swap_to(s, uid, last);
        }
    }
    fn label(&self) -> String {
        "test project".into()
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn owned_session() -> (Session, Arc<Mutex<TestOwner>>, Arc<Mutex<Log>>, u64) {
    let mut s = Session::new();
    let log = Arc::new(Mutex::new(Log::default()));
    let owner = Arc::new(Mutex::new(TestOwner::new(log.clone())));
    let uid = s.state().unwrap().uid;
    owner.lock().unwrap().last = Some(s.state().unwrap().doc.clone());
    let r: OwnerRef = owner.clone();
    s.ext.bind(uid, r);
    (s, owner, log, uid)
}

fn lines(s: &Session) -> Vec<Handle> {
    s.doc().unwrap().space(&Space::Model).unwrap().iter().filter(|e| matches!(e.kind, EntityKind::Line(_))).map(|e| e.handle).collect()
}

#[test]
fn owned_commits_reach_the_owner_and_skip_engine_undo() {
    let (mut s, _owner, log, _) = owned_session();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    s.cmdline("line 0,5 10,5 ").unwrap();
    assert!(s.running.is_none());
    assert_eq!(lines(&s).len(), 2);
    let commits = log.lock().unwrap().commits.clone();
    assert_eq!(commits.iter().filter(|c| c.0 == "line").count(), 2, "{commits:?}");
    assert!(s.state().unwrap().undo.is_empty(), "no engine undo step for owned documents");
}

#[test]
fn a_refused_commit_restores_and_reports() {
    let (mut s, owner, _, _) = owned_session();
    owner.lock().unwrap().refuse_next = true;
    let r = s.execute("line", &json!({"points": [[0, 0], [10, 0]]}));
    assert!(r.unwrap_err().to_string().contains("refused by the test owner"));
    assert!(lines(&s).is_empty());
    owner.lock().unwrap().refuse_next = true;
    s.cmdline("line 0,5 10,5 ").unwrap();
    assert!(lines(&s).is_empty());
    assert!(s.log.iter().any(|l| l.contains("refused by the test owner")));
}

#[test]
fn undo_and_redo_are_handed_over() {
    let (mut s, _owner, log, _) = owned_session();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    s.execute("u", &Value::Null).unwrap();
    assert!(lines(&s).is_empty());
    s.execute("redo", &Value::Null).unwrap();
    assert_eq!(lines(&s).len(), 1);
    let l = log.lock().unwrap();
    assert_eq!((l.undos, l.redos), (1, 1));
}

#[test]
fn copies_and_pastes_report_provenance() {
    let (mut s, _owner, log, _) = owned_session();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let h = lines(&s)[0];
    let copied = s.execute("copy", &json!({"handles": [h.hex()], "delta": [0, 5], "count": 2})).unwrap();
    assert_eq!(copied["handles"].as_array().unwrap().len(), 2);
    s.execute("copyclip", &json!({"handles": [h.hex()]})).unwrap();
    s.execute("pasteclip", &json!({"at": [0, 20]})).unwrap();
    let commits = log.lock().unwrap().commits.clone();
    let copy = commits.iter().find(|c| c.0 == "copy").unwrap();
    assert_eq!(copy.1, 2, "two provenance records for two copies");
    assert_eq!(commits.iter().find(|c| c.0 == "copyclip").unwrap().2, 1, "clipboard copy reported");
    assert_eq!(commits.iter().find(|c| c.0 == "pasteclip").unwrap().1, 1);
    assert!(commits.iter().filter(|c| c.0 == "line").all(|c| c.1 == 0), "provenance does not leak into later commands");
}

#[test]
fn provenance_names_its_sources() {
    struct Probe(Arc<Mutex<Vec<(Handle, Source, u32)>>>, Option<Arc<Drawing>>);
    impl DocumentOwner for Probe {
        fn commit(&mut self, s: &mut Session, c: &Commit<'_>) -> Verdict {
            self.0.lock().unwrap().extend(c.provenance.iter().map(|p| (p.new, p.source, p.batch)));
            self.1 = s.state().ok().map(|st| st.doc.clone());
            Verdict::Accept
        }
        fn undo(&mut self, _: &mut Session, _: u64) -> cadcraft_engine::Result<Option<String>> {
            Ok(None)
        }
        fn redo(&mut self, _: &mut Session, _: u64) -> cadcraft_engine::Result<Option<String>> {
            Ok(None)
        }
        fn save(&mut self, _: &mut Session, _: u64, _: Option<&str>, _: bool) -> cadcraft_engine::Result<Value> {
            Ok(Value::Null)
        }
        fn is_unsaved(&self) -> bool {
            false
        }
        fn close_needs_prompt(&self, _: u64) -> bool {
            false
        }
        fn closed(&mut self, _: &mut Session, _: u64) -> bool {
            true
        }
        fn recover(&mut self, _: &mut Session) {}
        fn label(&self) -> String {
            String::new()
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }
    let mut s = Session::new();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let uid = s.state().unwrap().uid;
    s.ext.bind(uid, Arc::new(Mutex::new(Probe(seen.clone(), None))));
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let h = lines(&s)[0];
    s.execute("copy", &json!({"handles": [h.hex()], "delta": [0, 5], "count": 2})).unwrap();
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().all(|(_, src, _)| *src == Source::Entity(h)));
    assert_ne!(seen[0].2, seen[1].2, "each placement is its own batch");
}

#[test]
fn save_close_and_unsaved_state_go_through_the_owner() {
    let (mut s, _owner, log, uid) = owned_session();
    // A plain drawing beside the owned one.
    s.new_drawing(true);
    s.execute("line", &json!({"points": [[0, 0], [1, 0]]})).unwrap();
    assert!(s.doc_unsaved(1) && s.close_needs_prompt(1), "plain drawings keep their own rule");
    s.execute("document.switch", &json!({"index": 0})).ok();
    s.active = 0;
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    assert!(s.doc_unsaved(0));
    assert!(!s.close_needs_prompt(0), "the owner decides");
    s.execute("qsave", &json!({"path": "x.ecad"})).unwrap();
    assert!(!s.doc_unsaved(0));
    assert_eq!(log.lock().unwrap().saves, vec![(Some("x.ecad".to_string()), false)]);
    s.execute("close", &json!({"index": 0})).unwrap();
    assert_eq!(log.lock().unwrap().closed, vec![uid]);
    assert!(s.ext.owners().is_empty(), "an owner with nothing open is removed");
}

#[test]
fn a_panicking_owner_is_recovered() {
    let (mut s, owner, log, _) = owned_session();
    owner.lock().unwrap().panic_next = true;
    let r = s.execute("line", &json!({"points": [[0, 0], [10, 0]]}));
    assert!(r.is_err());
    assert_eq!(log.lock().unwrap().recovered, 1);
    assert!(lines(&s).is_empty(), "recover restored the owner's last drawing");
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    assert_eq!(lines(&s).len(), 1, "the next command works");
}

#[test]
fn the_owner_adds_drag_preview_geometry() {
    let (mut s, _owner, _, _) = owned_session();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    s.cmdline("move all  0,0").unwrap();
    let (_, extra) = s.drag(cadcraft_engine::geom::Vec2::new(5.0, 5.0)).expect("MOVE drags");
    assert!(extra.iter().any(|e| matches!(e.kind, EntityKind::Point(_))));
    s.cancel();
}

#[test]
fn a_panicking_state_query_answers_safely() {
    let (s, owner, _, _) = owned_session();
    owner.lock().unwrap().panic_in_query = true;
    assert!(s.doc_unsaved(0), "a failing owner reads as unsaved");
    assert!(s.close_needs_prompt(0), "and closing asks first");
    assert!(s.has_unsaved_work());
}

#[test]
fn unsupported_undo_options_are_reported_on_owned_documents() {
    let (mut s, _owner, log, _) = owned_session();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    for mode in ["mark", "back", "begin", "end", "auto", "control"] {
        let r = s.execute("undo", &json!({"mode": mode, "on": true, "control": "all"}));
        let e = r.expect_err(mode).to_string();
        assert!(e.contains("not supported for this document"), "{mode}: {e}");
    }
    assert_eq!(lines(&s).len(), 1, "nothing was undone");
    for kw in ["mark", "back", "begin", "end", "auto", "control"] {
        let before = s.log.len();
        s.cmdline(&format!("undo {kw} ")).unwrap();
        assert!(s.running.is_none(), "{kw}: the prompt ends at once");
        assert!(s.log[before..].iter().any(|l| l.contains("not supported for this document")), "{kw}: {:?}", &s.log[before..]);
    }
    assert_eq!(lines(&s).len(), 1);
    // A count still goes to the owner.
    s.execute("undo", &json!({"count": 1})).unwrap();
    assert!(lines(&s).is_empty());
    assert_eq!(log.lock().unwrap().undos, 1);
    // A plain drawing keeps every option.
    s.new_drawing(true);
    s.execute("undo", &json!({"mode": "mark"})).unwrap();
}

#[test]
fn a_failing_close_keeps_the_document_open() {
    let (mut s, owner, log, uid) = owned_session();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner).panic_in_close = true;
    assert!(s.execute("close", &json!({"index": 0})).is_err());
    assert_eq!(s.docs.len(), 1, "the document stays open");
    assert_eq!(s.ext.owner_of(uid).map(|_| ()), Some(()), "and stays bound to its owner");
    assert_eq!(log.lock().unwrap().recovered, 1);
    owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner).panic_in_close = true;
    assert!(s.execute("closeall", &Value::Null).is_err());
    assert_eq!(s.docs.len(), 1);
    assert!(s.ext.owner_of(uid).is_some());
    // A busy owner (its lock held) also stops the close.
    {
        let _held = owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(s.execute("close", &json!({"index": 0})).is_err());
    }
    assert_eq!(s.docs.len(), 1);
    s.execute("close", &json!({"index": 0})).unwrap();
    assert!(s.docs.is_empty() && s.ext.owners().is_empty());
}

#[test]
fn delegated_undo_bumps_the_revision() {
    let (mut s, _owner, _, _) = owned_session();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let rev = s.state().unwrap().revision;
    s.execute("u", &Value::Null).unwrap();
    let after_undo = s.state().unwrap().revision;
    assert!(after_undo > rev, "undo changed the drawing: {rev} -> {after_undo}");
    s.execute("redo", &Value::Null).unwrap();
    assert!(s.state().unwrap().revision > after_undo);
    // Nothing to undo: no change, no new revision.
    s.execute("u", &Value::Null).unwrap();
    s.execute("u", &Value::Null).ok();
    let r = s.state().unwrap().revision;
    s.execute("u", &Value::Null).ok();
    assert_eq!(s.state().unwrap().revision, r);
}

#[test]
fn a_reentrant_owner_call_is_refused_not_deadlocked() {
    let (mut s, owner, log, _) = owned_session();
    owner.lock().unwrap().reenter = true;
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let r = log.lock().unwrap().reentry.clone();
    assert_eq!(r.len(), 3, "{r:?}");
    assert!(r[0].contains("busy (re-entrant call)"), "the inner command is refused: {r:?}");
    assert!(r[1].contains("busy (re-entrant call)"), "the inner undo is refused: {r:?}");
    assert_eq!(r[2], "unsaved: true", "a state query from inside answers conservatively");
    assert_eq!(lines(&s).len(), 1, "only the outer command's line is kept");
    s.execute("line", &json!({"points": [[0, 5], [10, 5]]})).unwrap();
    assert_eq!(lines(&s).len(), 2, "the owner is usable afterwards");
}

#[test]
fn the_owner_refuses_commands_and_lisp_before_they_start() {
    let (mut s, owner, log, uid) = owned_session();
    owner.lock().unwrap().refuse_starts = true;
    let before = s.state().unwrap().doc.clone();
    // JSON form, typed at the command line, and LISP: refused with the owner's message, the drawing unchanged.
    let r = s.execute("circle", &json!({"center": [0, 0], "radius": 5}));
    assert!(r.unwrap_err().to_string().contains("not started by the test owner"));
    assert!(s.cmdline("circle 0,0 5").unwrap_err().to_string().contains("not started by the test owner"));
    assert!(s.running.is_none());
    let r = s.lisp_eval("(command \"line\" \"0,0\" \"10,0\" \"\")", false, true, Vec::new());
    assert!(r.unwrap_err().to_string().contains("not started by the test owner"));
    assert!(!s.lisp_running());
    let st = s.state().unwrap();
    assert!(Arc::ptr_eq(&before, &st.doc), "the drawing is untouched");
    assert!(st.undo.is_empty() && st.redo.is_empty(), "no engine undo step");
    assert!(log.lock().unwrap().commits.is_empty(), "nothing reached the commit gate");
    // A command the owner allows still runs.
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    assert_eq!(lines(&s).len(), 1);
    // A plain drawing in the same session is not asked: LISP runs there as upstream.
    s.execute("new", &json!({})).unwrap();
    assert_ne!(s.state().unwrap().uid, uid);
    s.lisp_eval("(command \"circle\" \"0,0\" \"5\")", false, true, Vec::new()).unwrap();
    assert_eq!(s.doc().unwrap().space(&Space::Model).unwrap().iter().count(), 1);
    assert_eq!(s.state().unwrap().undo.len(), 1);
}

#[test]
fn a_panicking_start_check_refuses_and_recovers() {
    let (mut s, owner, log, _) = owned_session();
    owner.lock().unwrap().panic_in_start = true;
    let r = s.execute("line", &json!({"points": [[0, 0], [10, 0]]}));
    assert!(r.is_err());
    assert!(lines(&s).is_empty());
    assert_eq!(log.lock().unwrap().recovered, 1);
}
