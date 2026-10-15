//! The debugger panel in the trial, without a window: closed by default, opened from the banner,
//! rebuilt only when something changes, never waiting for a busy project, and following the
//! quick-start guide's worked example (place, wire, save, move, undo, save).

use std::path::PathBuf;

use cadcraft_engine::Session;
use ecad_cadcraft::BUSY;
use ecad_cadcraft::debug::{ObjectId, RecordState};
use ecad_trial::Trial;
use ecad_trial::debugger::{Action, CheckKind, View, saved_summary};
use serde_json::Value;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-trial-dbg-{name}-{}", ecad_store::disk::unique()));
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

fn trial() -> Trial {
    ecad_cadcraft::install();
    Trial::new(None).unwrap()
}

fn ok(t: &mut Trial, line: &str) {
    let e = t.submit_command(line);
    assert!(e.is_none(), "{line}: {e:?}");
}

fn frame(t: &mut Trial, ctx: &eframe::egui::Context) {
    let mut out = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
        t.logic(ui.ctx());
        t.ui(ui);
    });
    out.textures_delta.clear();
}

/// Every view of the panel lays out without failing.
fn every_view(t: &mut Trial, ctx: &eframe::egui::Context) {
    for v in View::ALL {
        t.debugger.view = v;
        frame(t, ctx);
    }
    t.debugger.view = View::Selection;
}

/// Select the objects the handle map gives for `ids` (as clicking them would).
fn select_ecad(t: &mut Trial, pick: impl Fn(&ecad_model::Document) -> Vec<ecad_model::ItemId>) {
    let p = t.project().unwrap().clone();
    let handles = p
        .read(&mut t.cad.session, |p, _| {
            let doc = p.ledger.document();
            let page = doc.page_order[0];
            pick(doc).into_iter().filter_map(|i| p.pages[&page].map.item(i)).collect::<Vec<_>>()
        })
        .unwrap();
    assert!(!handles.is_empty());
    t.cad.session.set_selection(handles);
}

#[test]
fn the_debugger_is_closed_until_opened_and_rebuilds_only_on_change() {
    let mut t = trial();
    let ctx = eframe::egui::Context::default();
    frame(&mut t, &ctx);
    assert!(!t.debugger.open, "never shown by default");
    assert_eq!(t.debugger.builds, 0, "a closed panel copies nothing");
    t.toggle_debugger();
    assert!(t.debugger.open);
    assert_eq!(t.debugger.builds, 1);
    for _ in 0..5 {
        frame(&mut t, &ctx);
    }
    assert_eq!(t.debugger.builds, 1, "nothing changed: no new snapshot");
    ok(&mut t, r#"place {"at":[100,0],"rotation":90}"#);
    frame(&mut t, &ctx);
    assert_eq!(t.debugger.builds, 2);
    t.debugger_action(Action::Refresh);
    assert_eq!(t.debugger.builds, 3, "Refresh always takes a new snapshot");
    every_view(&mut t, &ctx);
    t.debugger_action(Action::Close);
    assert!(!t.debugger.open);
}

#[test]
fn a_busy_project_keeps_the_previous_snapshot_marked_stale() {
    let mut t = trial();
    t.toggle_debugger();
    let seq = t.debugger.snapshot().unwrap().head;
    let p = t.project().unwrap().clone();
    let other = p.clone();
    let mut dbg = std::mem::take(&mut t.debugger);
    p.configure(&mut t.cad.session, |_held| dbg.refresh(&other, &mut Session::empty(), true)).unwrap();
    assert_eq!(dbg.stale(), Some(BUSY));
    assert_eq!(dbg.snapshot().unwrap().head, seq, "the previous snapshot is still shown");
    let ctx = eframe::egui::Context::default();
    t.debugger = dbg;
    frame(&mut t, &ctx);
    assert_eq!(t.debugger.stale(), None, "the next refresh succeeds");
}

/// The quick-start guide's worked example (section 8): what to inspect when placing, wiring,
/// saving, moving and undoing.
#[test]
fn the_quick_start_debugger_example_runs_headless() {
    let d = TempDir::new("example");
    let mut t = trial();
    let ctx = eframe::egui::Context::default();
    t.toggle_debugger();
    let snap = t.debugger.snapshot().unwrap();
    assert_eq!(snap.label(), "live at seq 1");
    assert_eq!(
        saved_summary(snap),
        "Saved through transaction 0; 1 pending (in memory only). The banner counts 0: a new project's own setup \
is not counted as a change until the project has a file."
    );
    assert_eq!(snap.record_state(1), RecordState::Pending { attempt: None }, "a new project is in memory only");

    // 1. Place a coil, select it: Selection → item → representation → component; one pending
    //    transaction created it.
    ok(&mut t, r#"place {"at":[100,0],"rotation":90}"#);
    let symbol = |d: &ecad_model::Document| {
        d.items.iter().filter(|(_, i)| matches!(i.kind, ecad_model::ItemKind::Symbol(_))).map(|(id, _)| *id).collect()
    };
    select_ecad(&mut t, symbol);
    frame(&mut t, &ctx);
    let snap = t.debugger.snapshot().unwrap();
    assert_eq!(snap.label(), "live at seq 2");
    assert_eq!(snap.selection.len(), 1);
    assert_eq!(snap.selection[0].entity, "Block Reference");
    t.debugger.follow_selection(0);
    let focus = t.debugger.focus().to_vec();
    assert!(matches!(focus[0], ObjectId::Item(_)));
    assert!(focus.iter().any(|i| matches!(i, ObjectId::Component(_))));
    assert_eq!(focus.iter().filter(|i| matches!(i, ObjectId::Terminal(_))).count(), 2, "the coil's terminals");
    assert_eq!(t.debugger.touching(), vec![2]);
    t.debugger.select_tx(Some(2));
    frame(&mut t, &ctx);
    let place = &t.debugger.log()[1];
    assert_eq!(place.label, "Place symbol");
    let names: Vec<&str> = place.ops.iter().map(ecad_model::Op::name).collect();
    assert!(names.contains(&"CreateComponent") && names.contains(&"LinkRepresentation"), "{names:?}");
    assert_eq!(t.debugger.snapshot().unwrap().record_state(2), RecordState::Pending { attempt: None });
    t.debugger.read_record();
    assert!(t.debugger.record().unwrap().as_ref().unwrap_err().contains("no record in the file"));

    // 2. Wire to the coil's lower terminal: the wire's trail shows the attachment, before (none)
    //    and after (the terminal).
    ok(&mut t, r#"wire {"points":[[60,0],[90,0]]}"#);
    let route = |d: &ecad_model::Document| {
        let (c, _) = d.conductors.iter().next().unwrap();
        vec![ecad_model::derive::route_on_page(d, *c, d.page_order[0]).unwrap().0]
    };
    select_ecad(&mut t, route);
    frame(&mut t, &ctx);
    t.debugger.follow_selection(0);
    assert!(t.debugger.focus().iter().any(|i| matches!(i, ObjectId::Conductor(_))));
    let touching = t.debugger.touching();
    assert!(touching.contains(&3), "{touching:?}");
    t.debugger.select_tx(Some(3));
    let attach = t.debugger.log()[2]
        .ops
        .iter()
        .map(ecad_cadcraft::debug::op_view)
        .find(|v| v.name == "AttachEnd" && !v.after.as_ref().unwrap()["terminal"].is_null())
        .unwrap();
    assert!(attach.before.unwrap()["terminal"].is_null());
    frame(&mut t, &ctx);

    // 3. Save As: everything is saved; the wire's record sits at an offset in the file and its
    //    bytes decode to exactly that transaction.
    let path = d.path("debug-demo.ecad");
    t.save_as(&path).unwrap();
    frame(&mut t, &ctx);
    let snap = t.debugger.snapshot().unwrap();
    assert_eq!((snap.head, snap.persisted), (3, 3));
    assert_eq!(saved_summary(snap), "Saved through transaction 3; 0 pending (in memory only).");
    let RecordState::Saved(Some(span)) = snap.record_state(3) else { panic!("saved: {:?}", snap.record_state(3)) };
    assert_eq!(snap.file.path.as_deref(), Some(path.as_str()));
    t.debugger.select_tx(Some(3));
    t.debugger.read_record();
    let rec = t.debugger.record().unwrap().as_ref().unwrap();
    assert_eq!(rec.check, Ok(()));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(rec.shown, bytes[span.offset as usize..(span.offset + span.len) as usize]);
    every_view(&mut t, &ctx);

    // 4. Move the coil: a pending transaction that modifies the wire's route too.
    select_ecad(&mut t, symbol);
    ok(&mut t, "move 100,0 100,-10");
    frame(&mut t, &ctx);
    let snap = t.debugger.snapshot().unwrap();
    assert_eq!((snap.head, snap.persisted), (4, 3));
    assert_eq!(snap.record_state(4), RecordState::Pending { attempt: None });
    select_ecad(&mut t, route);
    frame(&mut t, &ctx);
    t.debugger.follow_selection(0);
    // The coil's terminal (created by #2) is attached to the wire, so its trail starts there.
    assert_eq!(t.debugger.touching(), vec![2, 3, 4], "terminal placed, wired, then moved with the coil");

    // 5. Undo: a new pending transaction that undoes the move.
    ok(&mut t, "u");
    frame(&mut t, &ctx);
    let snap = t.debugger.snapshot().unwrap();
    assert_eq!(snap.head, 5);
    assert_eq!(ecad_cadcraft::debug::tx_kind(&t.debugger.log()[4], t.debugger.log()), "undo of #4");
    assert_eq!(snap.record_state(5), RecordState::Pending { attempt: None });
    assert_eq!(t.debugger.touching(), vec![2, 3, 4, 5]);
    // History at seq 2 (before the wire) is rebuilt from the copy; the live project is unchanged.
    t.debugger.show_at(2);
    let h = t.debugger.historical().unwrap();
    assert!(h.doc.as_ref().unwrap().conductors.is_empty());
    let p = t.project().unwrap().clone();
    assert_eq!(p.read(&mut t.cad.session, |p, _| p.ledger.head()).unwrap(), 5);
    every_view(&mut t, &ctx);

    // 6. Save: the move and its undo become saved, appended after the wire's record.
    t.save().unwrap();
    frame(&mut t, &ctx);
    let snap = t.debugger.snapshot().unwrap();
    assert_eq!(snap.persisted, 5);
    let RecordState::Saved(Some(s4)) = snap.record_state(4) else { panic!("saved") };
    assert!(s4.offset >= span.offset + span.len);
    // 7. The checks, on request only.
    assert!(t.debugger.checks().is_empty(), "no check ran by itself");
    for k in [CheckKind::Replay, CheckKind::Projection, CheckKind::File] {
        t.debugger_action(Action::Check(k));
    }
    let ran = t.debugger.checks();
    assert_eq!(ran.len(), 3);
    assert!(ran.iter().all(|r| r.check.ok && r.check.at_seq == 5), "{ran:?}");
    t.debugger.view = View::Checks;
    frame(&mut t, &ctx);
    let _ = t.cad.run("qsave", Value::Null);
}

/// A Save As that wrote the file but could not confirm durability: the heading counts those
/// transactions as written and unconfirmed, not as pending in memory, and not as saved.
#[test]
fn an_unconfirmed_save_as_is_neither_saved_nor_memory_only() {
    let d = TempDir::new("unconfirmed");
    let mut t = trial();
    ok(&mut t, r#"place {"at":[100,0],"rotation":90}"#);
    let p = t.project().unwrap().clone();
    p.configure(&mut t.cad.session, |p| p.file.faults.fail_dir_sync = true).unwrap();
    assert!(t.save_as(&d.path("unconfirmed.ecad")).is_err());
    t.toggle_debugger();
    let snap = t.debugger.snapshot().unwrap();
    assert_eq!(snap.record_state(2), RecordState::Unconfirmed(snap.span(2).unwrap()));
    assert_eq!(
        saved_summary(snap),
        "Saved through transaction 0; 2 written but not confirmed durable (Save confirms them); 0 pending (in \
memory only)."
    );
    let ctx = eframe::egui::Context::default();
    t.debugger.view = View::History;
    frame(&mut t, &ctx);
    t.save().unwrap();
    frame(&mut t, &ctx);
    assert_eq!(
        saved_summary(t.debugger.snapshot().unwrap()),
        "Saved through transaction 2; 0 pending (in memory only)."
    );
}
