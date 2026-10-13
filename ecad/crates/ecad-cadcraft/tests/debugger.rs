//! The debugger's data (persistence stage 2b): a selected drawing object leads to its ECAD IDs,
//! the model objects, the transactions that touched them with before/after values, and the saved
//! record's offset and bytes. Pending and saved transactions are told apart; snapshots are copied
//! under a short lock and never wait for a busy project; checks run only when asked; history views
//! never change the live project.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use cadcraft_engine::Session;
use common::{H, small_project};
use ecad_cadcraft::debug::{
    self, DebugSnapshot, ObjectId, RecordState, TxCursor, op_view, read_record, resolve, state_at, touches,
    verify_file, verify_projection, verify_replay,
};
use ecad_cadcraft::{BUSY, Mapped, Persistence};
use ecad_file::AttemptStage;
use ecad_model::{Subject, Transaction};
use serde_json::{Value, json};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-debugger-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The panel's side: its copy of the history, extended from each snapshot.
#[derive(Default)]
struct Panel {
    log: Vec<Transaction>,
    snap: Option<DebugSnapshot>,
}

impl Panel {
    fn cursor(&self) -> TxCursor {
        TxCursor { len: self.log.len() as u64, last: self.log.last().map(|t| t.id) }
    }
    /// Refresh like the panel does: `Ok(true)` when a new snapshot was taken.
    fn refresh(&mut self, h: &mut H) -> Result<bool, String> {
        let key = self.snap.as_ref().map(|s| s.key.clone());
        let Some(s) = h.project.debug_snapshot(&mut h.s, key.as_ref(), self.cursor())? else { return Ok(false) };
        self.log.truncate(s.log_from as usize);
        self.log.extend(s.new_transactions.iter().cloned());
        self.snap = Some(s);
        Ok(true)
    }
    fn snap(&self) -> &DebugSnapshot {
        self.snap.as_ref().unwrap()
    }
    fn tx(&self, seq: u64) -> &Transaction {
        &self.log[seq as usize - 1]
    }
}

fn file_bytes(path: &str, offset: u64, len: u64) -> Vec<u8> {
    let all = std::fs::read(path).unwrap();
    all[offset as usize..(offset + len) as usize].to_vec()
}

#[test]
fn the_trail_leads_from_a_selected_wire_to_its_saved_record() {
    let d = TempDir::new("trail");
    let (mut h, c1, _c2, w) = small_project(41);
    h.cfg(|p| p.persistence = Persistence::File);
    let first = h.first_page();
    let path = d.path("trail.ecad");
    h.run("saveas", json!({"path": path})).unwrap();

    // Select the wire's route on the page.
    let route = h.route_handle(first, w);
    h.s.set_selection(vec![route]);
    let mut panel = Panel::default();
    assert!(panel.refresh(&mut h).unwrap());
    let snap = panel.snap();
    assert_eq!(snap.label(), format!("live at seq {}", snap.head));
    assert_eq!(snap.log_from, 0, "the first snapshot copies the whole history");
    assert_eq!(panel.log.len() as u64, snap.head);
    assert_eq!(snap.active_page, Some(first));

    // Selected drawing object → ECAD IDs → model objects.
    let sel = &snap.selection[0];
    assert_eq!(sel.handle, route);
    assert_eq!(sel.entity, "Polyline");
    let Some(mapped @ Mapped::Item(item)) = sel.mapped else { panic!("the route is an ECAD item: {sel:?}") };
    let r = resolve(&snap.doc, mapped);
    assert_eq!(r.item, Some(item));
    assert_eq!(r.subject, Some(Subject::Conductor(w)));
    assert_eq!(r.terminals.len(), 2, "both attached terminals");
    let conductor = debug::object_json(&snap.doc, ObjectId::Conductor(w)).unwrap();
    assert_eq!(conductor["ends"].as_array().unwrap().len(), 2);

    // → the transactions that touched them, with before/after values.
    let ids: BTreeSet<ObjectId> = r.ids.iter().copied().collect();
    let touching: Vec<u64> = panel.log.iter().filter(|t| touches(t, &ids)).map(|t| t.seq).collect();
    let wire_seq = panel
        .log
        .iter()
        .find(|t| t.ops.iter().any(|o| matches!(o, ecad_model::Op::CreateConductor { id, .. } if *id == w)))
        .unwrap()
        .seq;
    assert!(touching.contains(&wire_seq), "{touching:?}");
    let attach = panel
        .tx(wire_seq)
        .ops
        .iter()
        .map(op_view)
        .find(|v| v.name == "AttachEnd")
        .expect("the wire transaction attaches its ends");
    assert_eq!(attach.targets[0], ObjectId::Conductor(w));
    assert!(attach.before.as_ref().unwrap()["terminal"].is_null());
    assert!(attach.after.as_ref().unwrap()["terminal"].is_string());

    // → the saved record: its offset, length and bytes, which decode to exactly that transaction.
    let RecordState::Saved(Some(span)) = snap.record_state(wire_seq) else {
        panic!("saved with a span: {:?}", snap.record_state(wire_seq))
    };
    assert_eq!(span.seq, wire_seq);
    let rec = read_record(Path::new(&path), span, Some(panel.tx(wire_seq))).unwrap();
    assert_eq!(rec.check, Ok(()));
    assert!(!rec.truncated);
    assert_eq!(rec.shown, file_bytes(&path, span.offset, span.len));
    assert!(rec.shown.starts_with(b"{\"crc\":\""));

    // Moving the component is pending: in memory only, no record yet.
    let saved_len = std::fs::metadata(&path).unwrap().len();
    let handles = h.rep_handles(first, c1);
    h.run("move", json!({"handles": handles, "delta": [0, 5]})).unwrap();
    assert!(panel.refresh(&mut h).unwrap());
    let snap = panel.snap();
    let move_seq = snap.head;
    assert_eq!(snap.log_from, move_seq - 1, "only the new transaction is copied");
    assert_eq!(snap.new_transactions.len(), 1);
    assert_eq!(snap.record_state(move_seq), RecordState::Pending { attempt: None });
    assert!(!snap.record_state(move_seq).durable());
    assert!(snap.record_state(wire_seq).durable(), "earlier history stays saved");
    assert_eq!(snap.unsaved, 1);
    // The move modified the wire's route: before and after differ.
    let modify = panel.tx(move_seq).ops.iter().map(op_view).find(|v| v.targets[0] == ObjectId::Item(item)).unwrap();
    assert_eq!(modify.name, "ModifyItem");
    assert_ne!(modify.before, modify.after);

    // Save: the pending transaction becomes saved, appended where the file ended.
    h.run("qsave", Value::Null).unwrap();
    assert!(panel.refresh(&mut h).unwrap());
    let RecordState::Saved(Some(span)) = panel.snap().record_state(move_seq) else { panic!("saved") };
    assert_eq!(span.offset, saved_len);
    let rec = read_record(Path::new(&path), span, Some(panel.tx(move_seq))).unwrap();
    assert_eq!(rec.check, Ok(()));
    // A record read against the wrong transaction is reported, not trusted.
    let wrong = read_record(Path::new(&path), span, Some(panel.tx(wire_seq))).unwrap();
    assert!(wrong.check.is_err());

    // Undo is a new, pending transaction naming what it undoes.
    h.run("u", Value::Null).unwrap();
    assert!(panel.refresh(&mut h).unwrap());
    let undo_seq = panel.snap().head;
    assert_eq!(debug::tx_kind(panel.tx(undo_seq), &panel.log), format!("undo of #{move_seq}"));
    assert_eq!(panel.snap().record_state(undo_seq), RecordState::Pending { attempt: None });
    assert_eq!(panel.snap().file.path.as_deref(), Some(path.as_str()));
    assert!(panel.snap().file.lock_file.is_some(), "the writer lock is held");
}

#[test]
fn an_unchanged_project_is_not_copied_again() {
    let (mut h, ..) = small_project(42);
    let mut panel = Panel::default();
    assert!(panel.refresh(&mut h).unwrap());
    assert!(!panel.refresh(&mut h).unwrap(), "nothing changed: no new snapshot");
    // A selection change is a new snapshot, with no history to copy.
    let first = h.first_page();
    let some = h.lines_on(first)[0];
    h.s.set_selection(vec![some]);
    assert!(panel.refresh(&mut h).unwrap());
    assert!(panel.snap().new_transactions.is_empty());
    assert_eq!(panel.snap().selection.len(), 1);
    // A copy that does not end in this project's history (another project's transaction) is
    // replaced, not extended.
    let mut stale = Panel { log: panel.log[..2].to_vec(), snap: None };
    stale.log[1].id = ecad_model::TxId(7);
    assert!(stale.refresh(&mut h).unwrap());
    assert_eq!(stale.snap().log_from, 0);
    assert_eq!(stale.log, panel.log);
}

#[test]
fn a_busy_project_is_not_waited_for() {
    let (mut h, ..) = small_project(43);
    let other = h.project.clone();
    let r = h.cfg(|_held| {
        let mut elsewhere = Session::empty();
        (
            other.debug_snapshot(&mut elsewhere, None, TxCursor::default()).err(),
            other.debug_projection_inputs(&mut elsewhere).err(),
        )
    });
    assert_eq!(r, (Some(BUSY.to_string()), Some(BUSY.to_string())));
}

#[test]
fn history_and_checks_work_on_copies_and_leave_the_live_project_alone() {
    let d = TempDir::new("checks");
    let (mut h, c1, ..) = small_project(44);
    h.cfg(|p| p.persistence = Persistence::File);
    let path = d.path("checks.ecad");
    h.run("saveas", json!({"path": path})).unwrap();
    let first = h.first_page();
    let handles = h.rep_handles(first, c1);
    h.run("move", json!({"handles": handles, "delta": [5, 0]})).unwrap();
    let mut panel = Panel::default();
    panel.refresh(&mut h).unwrap();
    let live = h.doc();
    let head = h.head();
    let events = h.p(|p| p.events.events().count());

    // History at every seq is rebuilt from the panel's copy; the live project is unchanged.
    for n in 0..=head {
        assert_eq!(state_at(&panel.log, n).unwrap(), h.p(|p| p.ledger.state_at(n).unwrap()));
    }
    assert!(state_at(&panel.log, head + 1).is_err());
    assert_eq!(h.doc(), live);
    assert_eq!(h.head(), head);
    assert!(!panel.refresh(&mut h).unwrap(), "nothing about the project changed");

    // The checks, on request.
    let snap = panel.snap();
    let replay = verify_replay(&panel.log, &snap.doc);
    assert!(replay.ok, "{:?}", replay.lines);
    assert_eq!(replay.at_seq, head);
    let inputs = h.project.debug_projection_inputs(&mut h.s).unwrap();
    let projection = verify_projection(&inputs);
    assert!(projection.ok, "{:?}", projection.lines);
    let before = std::fs::read(&path).unwrap();
    let file = verify_file(Path::new(&path), &panel.log, &snap.file, snap.persisted);
    assert!(file.ok, "{:?}", file.lines);
    assert!(file.lines.iter().any(|l| l.contains("1 pending in memory")), "{:?}", file.lines);
    assert_eq!(std::fs::read(&path).unwrap(), before, "checking reads only");
    // None of it touched the project or recorded anything.
    assert_eq!(h.doc(), live);
    assert_eq!(h.p(|p| p.events.events().count()), events);
    assert!(h.p(|p| p.unsaved()));

    // A replay check against a document that is not the history's result fails.
    let mut other = snap.doc.clone();
    other.pages.clear();
    assert!(!verify_replay(&panel.log, &other).ok);
    // A file changed behind the project's back fails the file check.
    let mut changed = before.clone();
    changed.extend_from_slice(b"{\"crc\":\"00000000\",\"rec\":{}}\n");
    std::fs::write(&path, &changed).unwrap();
    let file = verify_file(Path::new(&path), &panel.log, &snap.file, snap.persisted);
    assert!(!file.ok, "{:?}", file.lines);
}

#[test]
fn failed_and_unconfirmed_saves_are_not_shown_as_saved() {
    let d = TempDir::new("states");
    let mut h = H::new(45);
    h.cfg(|p| p.persistence = Persistence::File);
    let mut panel = Panel::default();
    panel.refresh(&mut h).unwrap();
    assert_eq!(panel.snap().record_state(1), RecordState::Pending { attempt: None });
    assert!(panel.snap().file.path.is_none());

    // A Save As whose folder flush fails: written, but not confirmed durable.
    h.cfg(|p| p.file.faults.fail_dir_sync = true);
    let path = d.path("states.ecad");
    assert!(h.run("saveas", json!({"path": path})).is_err());
    panel.refresh(&mut h).unwrap();
    let RecordState::Unconfirmed(span) = panel.snap().record_state(1) else {
        panic!("unconfirmed: {:?}", panel.snap().record_state(1))
    };
    assert!(panel.snap().file.unconfirmed);
    assert_eq!(read_record(Path::new(&path), span, Some(panel.tx(1))).unwrap().check, Ok(()));
    h.run("qsave", Value::Null).unwrap();
    panel.refresh(&mut h).unwrap();
    assert!(panel.snap().record_state(1).durable());

    // A save whose flush fails: the transaction stays pending, marked with the failed attempt.
    h.run("newpage", json!({"title": "2"})).unwrap();
    h.cfg(|p| p.file.disk_mut().unwrap().file_mut().medium_mut().faults.fail_next_flush = true);
    assert!(h.run("qsave", Value::Null).is_err());
    panel.refresh(&mut h).unwrap();
    let seq = panel.snap().head;
    assert_eq!(panel.snap().record_state(seq), RecordState::Pending { attempt: Some(AttemptStage::FlushFailed) });
    assert!(panel.snap().file.attempt.is_some());
    h.run("qsave", Value::Null).unwrap();
    panel.refresh(&mut h).unwrap();
    assert!(panel.snap().record_state(seq).durable());
    assert!(panel.snap().file.attempt.is_none());
}

#[test]
fn a_project_without_persistence_saves_nothing() {
    let mut h = H::new(46);
    h.cfg(|p| p.persistence = Persistence::Unavailable);
    let mut panel = Panel::default();
    panel.refresh(&mut h).unwrap();
    assert_eq!(panel.snap().record_state(1), RecordState::NotSaved);
}

#[test]
fn exported_snapshots_carry_the_persistence_facts() {
    let d = TempDir::new("export");
    let (mut h, ..) = small_project(47);
    h.cfg(|p| p.persistence = Persistence::File);
    let path = d.path("export.ecad");
    h.run("saveas", json!({"path": path})).unwrap();
    let snap = h.project.snapshot(&mut h.s, None).unwrap();
    assert_eq!(snap["format"], "ecad-inspect/2");
    let f = &snap["persistence"];
    assert_eq!(f["path"], path);
    assert_eq!(f["records"].as_array().unwrap().len() as u64, h.head());
    assert_eq!(f["header"]["container"], "ecad-project");
    let historical = h.project.snapshot(&mut h.s, Some(1)).unwrap();
    assert!(historical["persistence"].is_null());
}

#[test]
fn record_bytes_are_shown_as_text_with_control_bytes_escaped() {
    assert_eq!(debug::printable(b"{\"crc\":\"00\"}\n"), "{\"crc\":\"00\"}\\n");
    assert_eq!(debug::printable("é\t".as_bytes()), "é\\t");
    assert_eq!(debug::printable(b"a\xffb"), "a\\xffb");
}

#[test]
fn relations_lead_from_a_definition_to_its_instances_and_from_a_terminal_to_its_wires() {
    let (mut h, c1, c2, w) = small_project(48);
    let doc = h.doc();
    let def = doc.components[&c2].definition.unwrap();
    let users = debug::used_by(&doc, ObjectId::Definition(def));
    assert!(users.contains(&ObjectId::Component(c2)));
    assert!(users.iter().any(|u| matches!(u, ObjectId::Item(_))), "the symbol instance");
    let t = doc.conductors[&w].ends[0].unwrap();
    assert_eq!(debug::used_by(&doc, ObjectId::Terminal(t)), vec![ObjectId::Conductor(w)]);
    let of_c1 = debug::used_by(&doc, ObjectId::Component(c1));
    assert!(of_c1.iter().any(|u| matches!(u, ObjectId::Terminal(_))));
    assert!(of_c1.iter().any(|u| matches!(u, ObjectId::Representation(_))));
}
