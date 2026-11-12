//! Tester's contract tests for brief 023 (upstream sync to CADCraft 0.19.0): what the merge could
//! break at the owner seams. Undo and redo on an ECAD page mixed with plain drawing after upstream's
//! LISP-`U` change (X3), a transparent command inside an owned command (X2), autosave, the new
//! emergency save and crash recovery never writing an ECAD page as a drawing (X5), every editing
//! command upstream added in 0.16 to 0.19 on an ECAD page, and picking after edits (the `rstar`
//! fix, #2539).

use crate::common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use cadcraft_engine::Input;
use cadcraft_engine::doc::{EntityKind, Handle, Space};
use cadcraft_engine::geom::Vec2;
use common::{H, pts, small_project};
use ecad_model::{ConductorId, PageId, TxKind};
use serde_json::{Value, json};

fn active_page(h: &mut H) -> PageId {
    let uid = h.s.docs[h.s.active].uid;
    h.p(|p| p.page_of(uid).unwrap())
}

/// The page's model space as drawn.
fn model(h: &mut H, page: PageId) -> cadcraft_engine::doc::EntityStore {
    h.pv(|p, pg| pg.drawing_of(p, page).unwrap().space(&Space::Model).unwrap().clone())
}

/// The small project plus a plain line from (50, 30) to (80, 30) and its handle.
fn fixture(seed: u64) -> (H, Handle, ConductorId) {
    let (mut h, _, _, w) = small_project(seed);
    let page = h.first_page();
    h.run("line", json!({"points": pts(&[(50.0, 30.0), (80.0, 30.0)])})).unwrap();
    let plain = h.lines_on(page).into_iter().last().unwrap();
    (h, plain, w)
}

/// A 1x1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

struct Probe {
    name: &'static str,
    cmd: &'static str,
    params: fn(&mut H, Handle, ConductorId, &str) -> Value,
}

fn symbol_handle(h: &mut H) -> String {
    let doc = h.doc();
    let item = doc.items.iter().find(|(_, i)| matches!(i.kind, ecad_model::ItemKind::Symbol(_))).map(|(id, _)| *id).expect("a placed symbol");
    let page = h.first_page();
    h.handle_of(page, item).hex()
}

fn wire_handle(h: &mut H, w: ConductorId) -> String {
    let page = h.first_page();
    h.route_handle(page, w).hex()
}

fn component_line(h: &mut H) -> String {
    let c1 = *h.doc().components.keys().next().unwrap();
    let page = h.first_page();
    h.rep_handles(page, c1)[0].clone()
}

/// Every text-like entity on the first page (device tags, reference texts, title block).
fn texts(h: &mut H) -> Vec<String> {
    let page = h.first_page();
    h.pv(|p, pg| pg.entities_of(p, page))
        .iter()
        .filter(|e| matches!(e.kind, EntityKind::Text(_) | EntityKind::MText(_)))
        .map(|e| e.handle.hex())
        .collect()
}

fn probes() -> Vec<Probe> {
    vec![
        Probe {
            name: "XCLIP a symbol",
            cmd: "xclip",
            params: |h, _, _, _| json!({"handles": [symbol_handle(h)], "rectangle": [[95, -5], [101, 30]]}),
        },
        Probe {
            name: "XCLIP a symbol, inverted",
            cmd: "xclip",
            params: |h, _, _, _| json!({"handles": [symbol_handle(h)], "rectangle": [[95, -5], [101, 30]], "invert": true}),
        },
        Probe { name: "XCLIPFRAME 1", cmd: "xclipframe", params: |_, _, _, _| json!({"value": 1}) },
        Probe { name: "CHANGE point on a wire", cmd: "change", params: |h, _, w, _| json!({"handles": [wire_handle(h, w)], "point": [45, 50]}) },
        Probe {
            name: "CHANGE point on a component line",
            cmd: "change",
            params: |h, _, _, _| json!({"handles": [component_line(h)], "point": [5, -15]}),
        },
        Probe { name: "CHANGE point on a symbol", cmd: "change", params: |h, _, _, _| json!({"handles": [symbol_handle(h)], "point": [150, 60]}) },
        Probe { name: "CHANGE elevation of a wire", cmd: "change", params: |h, _, w, _| json!({"handles": [wire_handle(h, w)], "elevation": 5}) },
        Probe { name: "CHANGE point on plain line", cmd: "change", params: |_, plain, _, _| json!({"handles": [plain.hex()], "point": [90, 40]}) },
        Probe {
            name: "IMAGEATTACH on a page",
            cmd: "imageattach",
            params: |_, _, _, _| json!({"path": "dot.png", "data": PNG, "at": [150, 50], "width": 10}),
        },
        Probe { name: "DVIEW twist", cmd: "dview", params: |_, _, _, _| json!({"twist": 30}) },
        Probe { name: "-UNITS insunits", cmd: "-units", params: |_, _, _, _| json!({"insunits": 6, "lunits": 2}) },
        Probe {
            name: "-WBLOCK a wire",
            cmd: "-wblock",
            params: |h, _, w, dir| json!({"path": format!("{dir}/w.dxf"), "handles": [wire_handle(h, w)]}),
        },
        Probe {
            name: "-WBLOCK a symbol",
            cmd: "-wblock",
            params: |h, _, _, dir| json!({"path": format!("{dir}/s.dxf"), "handles": [symbol_handle(h)]}),
        },
        Probe {
            name: "-WBLOCK plain line",
            cmd: "-wblock",
            params: |_, plain, _, dir| json!({"path": format!("{dir}/p.dxf"), "handles": [plain.hex()]}),
        },
        Probe { name: "FIND/replace on the page", cmd: "find", params: |_, _, _, _| json!({"find": "K", "replace": "Z"}) },
        Probe { name: "FIND/replace digits", cmd: "find", params: |_, _, _, _| json!({"find": "1", "replace": "9"}) },
        Probe {
            name: "TEXTEDIT of a page text",
            cmd: "textedit",
            params: |h, _, _, _| json!({"handle": texts(h).first().cloned().unwrap_or_default(), "text": "EDITED"}),
        },
        Probe { name: "EXTEND a wire", cmd: "extend", params: |h, _, w, _| json!({"handle": wire_handle(h, w), "pick": [42, 10]}) },
        Probe { name: "EXPLODE a symbol", cmd: "explode", params: |h, _, _, _| json!({"handles": [symbol_handle(h)]}) },
        Probe {
            name: "properties.set on wire and plain line (multi-select)",
            cmd: "properties.set",
            params: |h, plain, w, _| json!({"handles": [wire_handle(h, w), plain.hex()], "color": 1}),
        },
        Probe {
            name: "properties.set rotation on a symbol",
            cmd: "properties.set",
            params: |h, _, _, _| json!({"handles": [symbol_handle(h)], "rotation": 45}),
        },
        Probe { name: "chprop layer on a symbol", cmd: "chprop", params: |h, _, _, _| json!({"handles": [symbol_handle(h)], "color": 3}) },
        Probe { name: "DRAWORDER a wire", cmd: "draworder", params: |h, _, w, _| json!({"handles": [wire_handle(h, w)], "order": "front"}) },
    ]
}

#[derive(Debug)]
#[allow(dead_code)]
struct Outcome {
    name: &'static str,
    result: Result<String, String>,
    appended: u64,
    problem: Option<String>,
}

/// Run one probe on a fresh project: either refused with nothing changed, or recorded with the
/// project consistent (§17.4), at most one transaction, no engine undo step, and one UNDO taking
/// the project back.
fn run_probe(seed: u64, p: &Probe, dir: &str) -> Outcome {
    let (mut h, plain, w) = fixture(seed);
    let page = h.first_page();
    let doc = h.doc();
    let head = h.head();
    let drawing = model(&mut h, page);
    let params = (p.params)(&mut h, plain, w, dir);
    let r = h.s.execute(p.cmd, &params).map(|v| v.to_string()).map_err(|e| e.to_string());
    if h.s.running.is_some() {
        h.s.cancel();
    }
    let appended = h.head() - head;
    let mut problem = None;
    let consistent = catch_unwind(AssertUnwindSafe(|| h.check()))
        .map_err(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default());
    if let Err(e) = consistent {
        problem = Some(format!("inconsistent: {e}"));
    } else if appended > 1 {
        problem = Some(format!("{appended} transactions"));
    } else if !h.s.state().unwrap().undo.is_empty() {
        problem = Some("engine undo step on an owned page".into());
    } else if r.is_err() || appended == 0 {
        if h.doc() != doc {
            problem = Some("document changed without a transaction".into());
        } else if model(&mut h, page) != drawing {
            problem = Some("drawing changed but nothing was recorded (silent)".into());
        }
    } else {
        h.s.cmdline("u").unwrap();
        if h.doc() != doc {
            problem = Some("UNDO does not bring the document back".into());
        } else if catch_unwind(AssertUnwindSafe(|| h.check())).is_err() {
            problem = Some("inconsistent after UNDO".into());
        }
    }
    Outcome { name: p.name, result: r, appended, problem }
}

/// Every editing command upstream added or changed in 0.16 to 0.19 (`change`, `xclip`,
/// `xclipframe`, `imageattach`, `dview`, `-units`, `-wblock`, the Find and Replace and TEXTEDIT
/// windows' commands, EXTEND, EXPLODE, the multi-select Properties palette's `properties.set`) on
/// an ECAD page is refused with nothing changed, or recorded so that the drawing is the
/// document's projection and UNDO takes it back. Never a silent change of the drawing alone.
#[test]
fn upstream_editing_commands_on_an_ecad_page_are_recorded_or_refused() {
    let dir = std::env::temp_dir().join(format!("ecad-t023-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.display().to_string().replace('\\', "/");
    let outcomes: Vec<Outcome> = probes().iter().enumerate().map(|(i, p)| run_probe(2300 + i as u64, p, &d)).collect();
    let _ = std::fs::remove_dir_all(&dir);
    let report: String = outcomes.iter().map(|o| format!("  {o:?}\n")).collect();
    println!("{report}");
    let bad: Vec<&Outcome> = outcomes.iter().filter(|o| o.problem.is_some()).collect();
    assert!(bad.is_empty(), "{} command(s) left an ECAD page wrong:\n{}", bad.len(), bad.iter().map(|o| format!("  {o:?}\n")).collect::<String>());
}

/// Undo and redo on an ECAD page, mixed with plain drawing, typed `U`, `UNDO n`, `REDO`, `MREDO n`:
/// each step returns exactly the document after the matching earlier edit, and the engine keeps
/// no undo history of its own (X3 after the LISP-`U` merge in `undo()`/`redo()`).
#[test]
fn undo_and_redo_on_an_ecad_page_mixed_with_plain_drawing_walk_the_ledger() {
    let (mut h, _, w) = fixture(2350);
    let page = h.first_page();
    let mut trail = vec![h.doc()];
    let edits: [(&str, Value); 5] = [
        ("line", json!({"points": pts(&[(0.0, 60.0), (30.0, 60.0)])})),
        ("place", json!({"at": [160, 0]})),
        ("circle", json!({"center": [120, 60], "radius": 5})),
        ("wire", json!({"points": pts(&[(0.0, 80.0), (30.0, 80.0)])})),
        ("move", json!({"handles": [], "from": [0, 0], "to": [0, 0]})),
    ];
    for (cmd, params) in edits.iter().take(4) {
        h.run(cmd, params.clone()).unwrap();
        trail.push(h.doc());
    }
    // Move the plain line drawn first, by handle.
    let first = h
        .lines_on(page)
        .into_iter()
        .find(|l| h.s.doc().unwrap().entity(*l).is_some_and(|e| matches!(&e.kind, EntityKind::Line(x) if x.a.y == 60.0 && x.b.y == 60.0)));
    h.run("move", json!({"handles": [first.unwrap().hex()], "from": [0, 0], "to": [0, 5]})).unwrap();
    trail.push(h.doc());
    let _ = &edits[4];
    let n = trail.len() - 1;
    // U one at a time down to the start, REDO back up.
    for i in (0..n).rev() {
        h.cmdline("u");
        assert_eq!(h.doc(), trail[i], "U to step {i}");
        assert!(h.s.state().unwrap().undo.is_empty());
    }
    for (i, t) in trail.iter().enumerate().skip(1) {
        h.cmdline("redo");
        assert_eq!(&h.doc(), t, "REDO to step {i}");
    }
    // UNDO 3, then MREDO 2, then UNDO 1.
    h.s.cmdline("undo 3").unwrap();
    h.check();
    assert_eq!(h.doc(), trail[n - 3], "UNDO 3");
    h.s.cmdline("mredo 2").unwrap();
    h.check();
    assert_eq!(h.doc(), trail[n - 1], "MREDO 2");
    h.cmdline("undo 1");
    assert_eq!(h.doc(), trail[n - 2], "UNDO 1");
    // A new edit after undo drops the redo branch.
    h.run("line", json!({"points": pts(&[(200.0, 0.0), (210.0, 0.0)])})).unwrap();
    let after = h.doc();
    h.cmdline("redo");
    assert_eq!(h.doc(), after, "nothing to redo after a new edit");
    h.cmdline("u");
    assert_eq!(h.doc(), trail[n - 2]);
    assert!(h.doc().conductors.contains_key(&w));
    h.check();
}

/// The same with a plain drawing open beside the project: undo there uses the engine's own
/// stack (upstream behaviour), and does not reach into the ECAD page, nor the other way round.
#[test]
fn undo_in_a_plain_drawing_and_on_an_ecad_page_stay_apart() {
    let (mut h, _, _) = fixture(2351);
    let page = h.first_page();
    let doc = h.doc();
    h.s.execute("new", &json!({})).unwrap();
    let plain_uid = h.s.docs[h.s.active].uid;
    assert!(h.p(|p| p.page_of(plain_uid)).is_none(), "the new drawing is not a page");
    h.s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    h.s.execute("line", &json!({"points": [[0, 5], [10, 5]]})).unwrap();
    assert_eq!(h.s.state().unwrap().undo.len(), 2, "a plain drawing keeps upstream's undo");
    h.s.cmdline("u").unwrap();
    assert_eq!(h.s.state().unwrap().undo.len(), 1);
    assert_eq!(h.doc(), doc, "undo in the plain drawing leaves the project alone");
    // Back on the page: undo takes back the page's last edit (the plain line), not the drawing's.
    h.activate(page);
    assert_eq!(active_page(&mut h), page);
    let head = h.head();
    h.cmdline("u");
    assert_eq!(h.head(), head + 1);
    assert!(matches!(h.p(|p| p.ledger.log().last().map(|t| t.kind)), Some(TxKind::Undo { .. })));
    let other = h.s.docs.iter().find(|d| d.uid == plain_uid).unwrap();
    assert_eq!(other.undo.len(), 1, "the page's undo does not touch the plain drawing's stack");
    h.cmdline("redo");
    assert_eq!(h.doc(), doc);
}

/// A transparent `'zoom` or `'pan` typed while Wire or Place prompts belongs to that command: the
/// command goes on, commits one transaction, the engine pushes no undo step, and the view change
/// records nothing (X2 after the merged `finish`).
#[test]
fn transparent_zoom_and_pan_inside_wire_and_place_leave_one_transaction() {
    for (tool, transparent) in [("wire", "'zoom"), ("wire", "'pan"), ("place", "'zoom"), ("wire", "'redraw")] {
        let (mut h, _, _) = fixture(2360);
        let head = h.head();
        let conductors = h.doc().conductors.len();
        let components = h.doc().components.len();
        h.s.start(tool).unwrap();
        if tool == "wire" {
            h.s.cmdline("0,100").unwrap();
        }
        let r = h.s.cmdline(transparent);
        assert!(r.is_ok(), "{tool} {transparent}: {r:?}");
        // Answer the transparent command's prompt: extents for ZOOM, a displacement for PAN.
        match transparent {
            "'zoom" => h.s.cmdline("e").unwrap(),
            "'pan" => {
                let _ = h.s.cmdline("0,0");
                let _ = h.s.cmdline("10,10");
            }
            _ => {}
        }
        assert_eq!(h.head(), head, "{tool} {transparent}: nothing recorded mid-command");
        assert!(h.s.running.is_some(), "{tool} {transparent}: {tool} still running; log {:?}", h.s.log.iter().rev().take(4).collect::<Vec<_>>());
        match tool {
            "wire" => {
                h.s.cmdline("40,100").unwrap();
                h.s.input(Input::Enter).unwrap();
            }
            _ => h.s.cmdline("160,60").unwrap(),
        }
        if h.s.running.is_some() {
            h.s.cancel();
        }
        assert_eq!(h.head(), head + 1, "{tool} {transparent}: one transaction");
        let d = h.doc();
        match tool {
            "wire" => assert_eq!(d.conductors.len(), conductors + 1, "{tool} {transparent}"),
            _ => assert_eq!(d.components.len(), components + 1, "{tool} {transparent}"),
        }
        assert!(h.s.state().unwrap().undo.is_empty());
        h.check();
        h.cmdline("u");
        assert_eq!(h.doc().conductors.len(), conductors);
        assert_eq!(h.doc().components.len(), components);
    }
}

/// Autosave (X5) after 0.18's changes: the planner, `save_now` (GPU error) and `emergency_save`
/// (a UI panic) never plan a drawing write of an ECAD page, while a plain drawing open beside it
/// is written. Without file formats in this build each write fails, so a planned write shows up in
/// `failures()`: the plain drawing is there, no page is.
#[test]
fn autosave_save_now_and_emergency_save_never_write_an_ecad_page() {
    use cadcraft_engine::autosave::{self, Autosaver, Job};
    let (mut h, _, _) = fixture(2370);
    // Unsaved edits on two pages.
    h.run("newpage", json!({"title": "2"})).unwrap();
    let p2 = h.doc().page_order[1];
    h.activate(p2);
    h.run("line", json!({"points": pts(&[(0.0, 0.0), (10.0, 0.0)])})).unwrap();
    let page_uids: Vec<u64> = h.s.docs.iter().map(|d| d.uid).collect();
    assert!(!page_uids.is_empty());
    assert!(h.s.docs.iter().all(|d| d.is_dirty()), "the pages have unsaved changes");
    h.s.execute("new", &json!({})).unwrap();
    h.s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let plain = h.s.docs[h.s.active].uid;

    // The planner.
    let mut a = Autosaver::new("9000-a");
    assert!(a.plan(&h.s, 0.0, 1, "x").is_empty());
    let jobs = a.plan(&h.s, 1000.0, 1, "x");
    let writes: Vec<u64> = jobs.iter().filter_map(|j| if let Job::Write { uid, .. } = j { Some(*uid) } else { None }).collect();
    assert_eq!(writes, vec![plain], "only the plain drawing is autosaved");

    // save_now and emergency_save, into a folder of our own.
    let dir = std::env::temp_dir().join(format!("ecad-t023-autosave-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let old = (autosave::savefilepath(), autosave::savetime());
    autosave::set_savefilepath(&dir.display().to_string());
    for (what, f) in [("save_now", 0), ("emergency_save", 1)] {
        let mut a = Autosaver::new(format!("{}-a", 9100 + f));
        let _ = if f == 0 { a.save_now(&h.s) } else { a.emergency_save(&h.s) };
        if f == 0 {
            // Let the background writes finish and be taken in.
            for _ in 0..200 {
                if !a.writing() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            a.reap();
        }
        let planned: Vec<u64> = a.failures().map(|(u, _)| u).collect();
        assert!(planned.contains(&plain) || a.files().iter().any(|_| true), "{what}: the plain drawing was planned");
        for u in &page_uids {
            assert!(!planned.contains(u), "{what}: page document {u} was planned as a drawing write");
        }
        assert!(a.files().len() <= 1, "{what}: {:?}", a.files());
        let left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        for n in &left {
            if let Some((_, _, uid)) = autosave::parse_file_name(n) {
                assert!(!page_uids.contains(&uid), "{what}: autosave file {n} of a page");
            }
        }
    }
    // Drawing Recovery at the next launch offers no page of the project.
    let rec = autosave::recoverable(&dir, "someone-else");
    for r in &rec {
        assert!(!r.drawing.contains("Demo"), "recovery offers a page: {r:?}");
    }
    autosave::set_savefilepath(&old.0);
    autosave::set_savetime(old.1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Picking on an ECAD page after a run of edits, undos and redos (the spatial index is patched
/// after each, #2539): the wire, the symbol and the plain line are found where they are drawn, and
/// the connection point is offered as an endpoint snap.
#[test]
fn picking_and_snapping_on_an_ecad_page_after_edits_find_what_is_drawn() {
    let (mut h, plain, w) = fixture(2380);
    let page = h.first_page();
    let ap = 1.0;
    for round in 0..12 {
        // Edits that remove and insert entities in the page drawing.
        h.run("line", json!({"points": pts(&[(10.0 * f64::from(round), 70.0), (10.0 * f64::from(round) + 5.0, 75.0)])})).unwrap();
        h.run("wire", json!({"points": pts(&[(10.0 * f64::from(round), 90.0), (10.0 * f64::from(round) + 5.0, 90.0)])})).unwrap();
        if round % 3 == 0 {
            h.cmdline("u");
        }
        if round % 4 == 0 {
            h.cmdline("redo");
        }
        let space = Space::Model;
        let wire = h.route_handle(page, w);
        let (a, b) = h.route_ends(page, w);
        let mid = Vec2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let got = h.s.pick_object(&space, mid, ap).unwrap();
        assert_eq!(got, Some(wire), "round {round}: the wire is picked at its middle");
        let got = h.s.pick_object(&space, Vec2::new(65.0, 30.0), ap).unwrap();
        assert_eq!(got, Some(plain), "round {round}: the plain line");
        assert!(h.s.pick_candidates(mid).contains(&wire), "round {round}: hover candidates hold the wire");
        // Endpoint snap at the component's connection point (40, 10).
        let d = h.s.doc().unwrap().clone();
        let hit = cadcraft_engine::snap::osnap(&d, &space, Vec2::new(40.3, 10.2), ap, 1, None, false);
        let hit = hit.map(|s| s.point);
        assert!(hit.is_some_and(|p| (p.x - 40.0).abs() < 1e-9 && (p.y - 10.0).abs() < 1e-9), "round {round}: endpoint snap {hit:?}");
        // Every pick result is a mapped handle of the page.
        for x in [mid, Vec2::new(65.0, 30.0), Vec2::new(20.0, 0.0), Vec2::new(100.0, 5.0)] {
            if let Some(found) = h.s.pick_object(&space, x, ap).unwrap() {
                assert!(h.mapped(page, found).is_some(), "round {round}: picked {found:?} at {x:?} is not on the page");
            }
        }
    }
    h.check();
}

/// OOPS edge cases the worker named: OOPS twice brings nothing back the second time and records
/// nothing; the Delete key (`erase.selection`) counts as an erase; a plain erase followed by an
/// erase of a wire leaves OOPS refused (the stash now holds the wire), never the plain line back
/// in the wire's place; and after any of them the project stays consistent.
#[test]
fn oops_twice_after_the_delete_key_and_after_a_later_erase_of_a_wire() {
    // OOPS twice.
    let (mut h, plain, _) = fixture(2390);
    let page = h.first_page();
    h.run("erase", json!({"handles": [plain.hex()]})).unwrap();
    h.run("oops", json!({})).unwrap();
    let head = h.head();
    let doc = h.doc();
    let r = h.run("oops", json!({}));
    assert_eq!(h.head(), head, "a second OOPS records nothing: {r:?}");
    assert_eq!(h.doc(), doc);
    assert!(h.mapped(page, plain).is_some());
    h.check();

    // The Delete key, then OOPS.
    let (mut h, plain, _) = fixture(2391);
    let page = h.first_page();
    h.s.set_selection(vec![plain]);
    h.s.execute("erase.selection", &json!({})).unwrap();
    h.check();
    assert!(h.mapped(page, plain).is_none(), "Delete removed the line");
    let head = h.head();
    h.run("oops", json!({})).unwrap();
    assert_eq!(h.head(), head + 1, "OOPS after the Delete key brings the line back");
    assert!(h.mapped(page, plain).is_some());

    // A plain erase, then a wire erased: OOPS must not bring the wire back as a loose line.
    let (mut h, plain, w) = fixture(2392);
    let page = h.first_page();
    h.run("erase", json!({"handles": [plain.hex()]})).unwrap();
    let wire = h.route_handle(page, w);
    h.s.set_selection(vec![wire]);
    h.s.execute("erase.selection", &json!({})).unwrap();
    h.check();
    let head = h.head();
    let doc = h.doc();
    let r = h.run("oops", json!({}));
    assert!(r.as_ref().is_err_and(|e| e.contains("use UNDO")), "OOPS after a wire's erase: {r:?}");
    assert_eq!((h.head(), h.doc()), (head, doc));
    h.check();
}

/// With file formats installed (as an app that adopts upstream's DWG worker or `cadcraft-io` would
/// have them; ECAD's apps install none today, so -WBLOCK and autosave writes stop at "file formats
/// are not available"): -WBLOCK and WBLOCK of a wire, a symbol and plain drawing on an ECAD page
/// are recorded or refused like any edit, and `emergency_save` writes the plain drawing beside the
/// project but no file for any page. Run in a child process, because the hooks are process-wide.
#[test]
fn with_file_formats_wblock_and_emergency_save_respect_ecad_pages() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .env("ECAD_TEST_CHILD", "1")
        .args(["--exact", "tester_023_contracts::child_with_file_formats", "--ignored", "--nocapture", "--test-threads=1"])
        .output()
        .unwrap();
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success() && text.contains("1 passed"), "child failed:\n{text}");
}

#[test]
#[ignore = "run by with_file_formats_wblock_and_emergency_save_respect_ecad_pages in a child process"]
fn child_with_file_formats() {
    // Only as a child of its test, which sets `ECAD_TEST_CHILD`; run directly, it does nothing.
    if std::env::var_os("ECAD_TEST_CHILD").is_none() {
        return;
    }
    use cadcraft_engine::autosave::{self, Autosaver};
    use cadcraft_engine::cmd::file::{IoHooks, set_io};
    set_io(IoHooks { read: |_, _| Ok(cadcraft_engine::doc::Drawing::new_metric()), write: |_, _| Ok(b"0\nEOF\n".to_vec()), plot: None });
    let dir = std::env::temp_dir().join(format!("ecad-t023-io-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.display().to_string().replace('\\', "/");
    let probes = [
        Probe {
            name: "-WBLOCK a wire",
            cmd: "-wblock",
            params: |h, _, w, dir| json!({"path": format!("{dir}/w.dxf"), "handles": [wire_handle(h, w)]}),
        },
        Probe {
            name: "-WBLOCK a symbol",
            cmd: "-wblock",
            params: |h, _, _, dir| json!({"path": format!("{dir}/s.dxf"), "handles": [symbol_handle(h)]}),
        },
        Probe {
            name: "-WBLOCK plain line",
            cmd: "-wblock",
            params: |_, plain, _, dir| json!({"path": format!("{dir}/p.dxf"), "handles": [plain.hex()]}),
        },
        Probe {
            name: "WBLOCK a wire",
            cmd: "wblock",
            params: |h, _, w, dir| json!({"path": format!("{dir}/w2.dxf"), "handles": [wire_handle(h, w)]}),
        },
        Probe { name: "WBLOCK whole page", cmd: "wblock", params: |_, _, _, dir| json!({"path": format!("{dir}/all.dxf")}) },
    ];
    let outcomes: Vec<Outcome> = probes.iter().enumerate().map(|(i, p)| run_probe(2400 + i as u64, p, &d)).collect();
    let report: String = outcomes.iter().map(|o| format!("  {o:?}\n")).collect();
    println!("{report}");
    let bad: Vec<&Outcome> = outcomes.iter().filter(|o| o.problem.is_some()).collect();
    assert!(bad.is_empty(), "with file formats, {} command(s) left an ECAD page wrong:\n{report}", bad.len());

    // A plain -WBLOCK then OOPS brings the line back (the record is plain); a wire's is refused.
    let (mut h, plain, _) = fixture(2410);
    let page = h.first_page();
    let r = h.run("-wblock", json!({"path": format!("{d}/o.dxf"), "handles": [plain.hex()]}));
    println!("-wblock plain: {r:?}");
    if r.is_ok() && h.mapped(page, plain).is_none() {
        h.run("oops", json!({})).unwrap();
        assert!(h.mapped(page, plain).is_some(), "OOPS after -WBLOCK brings the plain line back");
    }

    // Emergency save writes real files now: none of them is a page.
    let (mut h, _, _) = fixture(2420);
    h.run("line", json!({"points": pts(&[(0.0, 50.0), (10.0, 50.0)])})).unwrap();
    let page_uids: Vec<u64> = h.s.docs.iter().map(|x| x.uid).collect();
    h.s.execute("new", &json!({})).unwrap();
    h.s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let plain_uid = h.s.docs[h.s.active].uid;
    let auto = dir.join("autosave");
    std::fs::create_dir_all(&auto).unwrap();
    autosave::set_savefilepath(&auto.display().to_string());
    let mut a = Autosaver::new("9001-a");
    let n = a.emergency_save(&h.s);
    let mut a2 = Autosaver::new("9002-b");
    let m = a2.save_now(&h.s);
    for _ in 0..300 {
        if !a2.writing() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    a2.reap();
    let names: Vec<String> = std::fs::read_dir(&auto).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    println!("emergency_save {n}, save_now {m}, files {names:?}");
    assert_eq!((n, m), (1, 1), "only the plain drawing is written");
    let uids: Vec<u64> = names.iter().filter_map(|x| autosave::parse_file_name(x)).map(|(_, _, u)| u).collect();
    assert!(uids.contains(&plain_uid), "the plain drawing's file is there");
    assert!(uids.iter().all(|u| !page_uids.contains(u)), "a page was autosaved as a drawing: {names:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
