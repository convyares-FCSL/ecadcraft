//! Independent tester's contracts for brief 005 (traditional / fan cable view), written from the
//! brief, design 005 and the owner's answers O1-O6, Q1-Q4 only.
//!
//! - AC1: ten mixed switches (JSON toggle, JSON explicit, typed + click, pickfirst selection, the
//!   trunk clicked) leave the document, the log, the head, the ID allocator, `derive::nets`, the
//!   change count and every tab's unsaved marker as they were.
//! - AC2: switching writes nothing to the `.ecad` file and never marks the project unsaved; save and
//!   reopen restore each cable's own view through `ECAD_SETTINGS_DIR` (child process, so the env var
//!   never races another test and the owner's real settings are never read); a read-only second
//!   window reads the views; a hostile settings file (missing, garbage, truncated, wrong types,
//!   another format, over 1 MiB, CRLF, BOM, a folder, read-only, a folder path that is a file)
//!   never fails open or save and never panics.
//! - AC3: fans of 1, 2, 3 and 20 wires, a capped fan (spread above 0.4·L), a right-to-left
//!   reference, a bent reference, loose ends, hostile coordinates.
//! - AC4: undo / redo after a switch act on the last real edit.
//! - AC6: edits of the fan (move, erase, copy, rotate, scale, mirror, stretch, grips, properties,
//!   offset, explode, cut, mixed selections, new geometry on `ECAD-CABLES`) are refused with the
//!   spec's message and leave the head and the drawing unchanged.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;

use cadcraft_engine::doc::{Entity, EntityKind, Handle};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use common::{H, id};
use ecad_cadcraft::interact::{hover_lines, overlay_marks};
use ecad_cadcraft::views::{self, FAN_REFUSED, Settings};
use ecad_cadcraft::{Persistence, Project, ProjectHandle};
use ecad_model::{CableId, ConductorId, ItemId, PageId, Pt, Transaction};
use serde_json::{Value, json};

const SEED: u64 = 0x5005_7e57;

// ------------------------------------------------------------------------------------- helpers

struct Tmp(PathBuf);

impl Tmp {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-005-tester-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        // A read-only file would stop the removal on Windows.
        if let Ok(rd) = std::fs::read_dir(&self.0) {
            for e in rd.flatten() {
                if let Ok(m) = e.metadata() {
                    let mut perm = m.permissions();
                    #[allow(clippy::permissions_set_readonly_false)]
                    perm.set_readonly(false);
                    let _ = std::fs::set_permissions(e.path(), perm);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn new_lines(h: &mut H, page: PageId, line: &str) -> Vec<Handle> {
    let before = h.lines_on(page);
    h.cmdline(line);
    h.lines_on(page).into_iter().filter(|x| !before.contains(x)).collect()
}

/// A drawn component (closed rectangle converted) with terminals at x, y = 0, 5, 10.
fn component(h: &mut H, page: PageId, line: &str, x: f64) {
    let lines = new_lines(h, page, line);
    assert_eq!(lines.len(), 4);
    h.run("convertcomponent", json!({"handles": lines.iter().map(|x| x.hex()).collect::<Vec<_>>()})).unwrap();
    for y in [0.0, 5.0, 10.0] {
        h.run("addterminal", json!({"handle": lines[0].hex(), "at": [x, y]})).unwrap();
    }
}

struct G {
    h: H,
    page: PageId,
    c1: CableId,
    slash: ItemId,
    wires: Vec<ConductorId>,
}

/// Design §9 fixture G on `h`.
fn fixture_on(mut h: H) -> G {
    h.s.viewport_px = (1280.0, 800.0);
    let page = h.first_page();
    component(&mut h, page, "line -10,-5 0,-5 0,15 -10,15 c", 0.0);
    component(&mut h, page, "line 60,-5 70,-5 70,15 60,15 c", 60.0);
    let mut wires = Vec::new();
    for y in [0, 5, 10] {
        let r = h.run("wire", json!({"points": [[0, y], [60, y]]})).unwrap();
        wires.push(id(&r, "conductor"));
    }
    let r = h.run("cableslash", json!({"from": [28, -3], "to": [32, 18], "tag": "C1"})).unwrap();
    h.run("zoom", json!({"mode": "window", "p1": [-20, -30], "p2": [80, 50]})).unwrap();
    G { h, page, c1: id(&r, "cable"), slash: id(&r, "item"), wires }
}

fn fixture() -> G {
    fixture_on(H::new(SEED))
}

fn views_of(h: &mut H) -> Value {
    h.run("cableview", json!({})).unwrap()["views"].clone()
}

fn view_of(h: &mut H, k: CableId) -> String {
    views_of(h)[k.hex()].as_str().unwrap_or("absent").to_string()
}

fn set(h: &mut H, k: CableId, v: &str) -> Value {
    h.run("cableview", json!({"cable": k.hex(), "view": v})).unwrap()
}

fn entities(h: &mut H, page: PageId) -> Vec<Entity> {
    h.pv(|p, pg| pg.entities_of(p, page)).iter().map(|e| (**e).clone()).collect()
}

fn path_of(e: &Entity) -> Vec<Pt> {
    match &e.kind {
        EntityKind::Line(l) => vec![Pt::new(l.a.x, l.a.y), Pt::new(l.b.x, l.b.y)],
        EntityKind::LwPolyline(p) => p.vertices.iter().map(|v| Pt::new(v.p.x, v.p.y)).collect(),
        other => panic!("unexpected entity on the cable layer: {other:?}"),
    }
}

/// Paths on the reserved layer, each normalised so its smaller end comes first.
fn fan_paths(h: &mut H, page: PageId) -> Vec<Vec<Pt>> {
    let mut out: Vec<Vec<Pt>> = entities(h, page)
        .iter()
        .filter(|e| e.common.layer == views::LAYER)
        .map(|e| {
            let p = path_of(e);
            let (a, b) = (p[0], p[p.len() - 1]);
            if (b.x, b.y) < (a.x, a.y) { p.into_iter().rev().collect() } else { p }
        })
        .collect();
    out.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    out
}

fn norm(v: &[(f64, f64)]) -> Vec<Pt> {
    let p: Vec<Pt> = v.iter().map(|(x, y)| Pt::new(*x, *y)).collect();
    let (a, b) = (p[0], p[p.len() - 1]);
    if (b.x, b.y) < (a.x, a.y) { p.into_iter().rev().collect() } else { p }
}

fn assert_paths(got: &[Vec<Pt>], want: &[Vec<(f64, f64)>]) {
    assert_eq!(got.len(), want.len(), "fan paths: {got:?}");
    for w in want {
        let w = norm(w);
        assert!(
            got.iter().any(|g| g.len() == w.len() && g.iter().zip(&w).all(|(p, q)| p.near(*q, 1e-9))),
            "{w:?} not in {got:?}"
        );
    }
}

fn fan_handles(h: &mut H, page: PageId) -> Vec<Handle> {
    h.p(|p| p.pages[&page].map.fan_handles().collect())
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn click(h: &mut H, x: f64, y: f64) {
    let at = Vec2::new(x, y);
    h.s.cursor = at;
    let _ = h.s.input(Input::Point(at));
    h.check();
}

/// Everything AC1 says a switch must not change.
#[derive(Debug, PartialEq)]
struct Snap {
    doc: String,
    log: Vec<Transaction>,
    head: u64,
    ids: String,
    nets: String,
    change_count: u64,
    unsaved: bool,
    tabs: Vec<bool>,
}

fn snap(h: &mut H) -> Snap {
    let doc = h.doc();
    Snap {
        doc: serde_json::to_string(&doc).unwrap(),
        log: h.p(|p| p.ledger.log().to_vec()),
        head: h.head(),
        ids: h.p(|p| format!("{:?}", p.ids)),
        nets: format!("{:?}", ecad_model::derive::nets(&doc)),
        change_count: h.p(|p| p.unsaved_transactions()),
        unsaved: h.p(|p| p.unsaved()),
        tabs: (0..h.s.docs.len()).map(|i| h.s.doc_unsaved(i)).collect(),
    }
}

/// Non-cable overlay marks (terminal rings, attached dots, loose-end squares), rounded.
fn marks(h: &mut H, page: PageId) -> Vec<(String, i64, i64)> {
    let uid = h.p(|p| p.pages[&page].uid).unwrap();
    let mut v: Vec<(String, i64, i64)> = h
        .p(|p| overlay_marks(p, uid))
        .into_iter()
        .filter(|m| (m.color.0, m.color.1, m.color.2) != (0xff, 0x00, 0xff))
        .map(|m| (format!("{:?}", m.shape), (m.at.x * 1e6).round() as i64, (m.at.y * 1e6).round() as i64))
        .collect();
    v.sort();
    v
}

fn magenta_marks(h: &mut H, page: PageId) -> usize {
    let uid = h.p(|p| p.pages[&page].uid).unwrap();
    h.p(|p| overlay_marks(p, uid)).iter().filter(|m| (m.color.0, m.color.1, m.color.2) == (0xff, 0x00, 0xff)).count()
}

// --------------------------------------------------------------------------------------- AC1

/// AC1 / design check B: ten switches by five different paths change nothing in the project.
#[test]
fn ac1_ten_mixed_switches_leave_document_log_head_ids_and_nets_unchanged() {
    let mut g = fixture();
    let before = snap(&mut g.h);
    for n in 0..10 {
        let was_fan = view_of(&mut g.h, g.c1) == "fan";
        match n % 5 {
            // JSON toggle.
            0 => {
                let r = g.h.run("cableview", json!({"cable": g.c1.hex()})).unwrap();
                assert_eq!(r["view"], if was_fan { "traditional" } else { "fan" }, "{r}");
            }
            // JSON explicit.
            1 => {
                let r = set(&mut g.h, g.c1, if was_fan { "traditional" } else { "fan" });
                assert!(r.get("changed").is_none(), "{r}");
            }
            // Typed at the command line, then a click on the slash or on the trunk.
            2 | 4 => {
                g.h.cmdline("cableview");
                assert_eq!(prompt(&g.h), "Select a cable to switch its view:");
                if was_fan {
                    click(&mut g.h, 40.0, 5.0)
                } else {
                    click(&mut g.h, 30.0, 7.5)
                }
                let _ = g.h.s.input(Input::Enter);
                assert!(g.h.s.running.is_none());
            }
            // Pickfirst: the slash (or a fan line) selected, then the command.
            _ => {
                let h = if was_fan {
                    fan_handles(&mut g.h, g.page)[0]
                } else {
                    g.h.p(|p| p.pages[&g.page].map.item(g.slash)).unwrap()
                };
                g.h.s.set_selection(vec![h]);
                g.h.s.start("cableview").unwrap();
                assert!(g.h.s.running.is_none(), "a selection switches at once and ends the command");
                assert!(g.h.s.selection().is_empty(), "the selection is cleared");
            }
        }
        assert_eq!(view_of(&mut g.h, g.c1) == "fan", !was_fan, "switch {n} took effect");
        // Reading every view is not an edit either.
        let _ = views_of(&mut g.h);
        assert_eq!(snap(&mut g.h), before, "switch {n}");
        g.h.check();
    }
    assert_eq!(view_of(&mut g.h, g.c1), "traditional", "an even number of switches");
    // Same drawing as before any switch (same handles for the routes and the slash).
    assert!(fan_handles(&mut g.h, g.page).is_empty());
    assert!(g.h.p(|p| p.pages[&g.page].map.item(g.slash)).is_some());
}

/// Design §3.4: the JSON form's results and errors, none of which writes anything.
#[test]
fn ac1_json_form_results_and_errors_table() {
    let mut g = fixture();
    let c1 = g.c1.hex();
    let before = snap(&mut g.h);
    let cases: Vec<(Value, Result<Value, &str>)> = vec![
        (json!({}), Ok(json!({"views": {c1.clone(): "traditional"}}))),
        (json!({"cable": c1, "view": "traditional"}), Ok(json!({"changed": false}))),
        (json!({"cable": "00000000000000000000000000000001"}), Err("no such cable")),
        (json!({"cable": "not hex"}), Err("`cable` (hex id) is required")),
        (json!({"cable": 12}), Err("`cable` (hex id) is required")),
        (json!({"cable": c1, "view": "Fan!"}), Err("`view` must be \"traditional\" or \"fan\"")),
        (json!({"cable": c1, "view": 1}), Err("`view` must be \"traditional\" or \"fan\"")),
        (json!({"view": "fan"}), Err("`view` needs `cable`")),
        (json!({"cable": c1, "view": "fan"}), Ok(json!({"view": "fan"}))),
        (json!({"cable": c1, "view": "fan"}), Ok(json!({"changed": false, "view": "fan"}))),
        (json!({"cable": c1}), Ok(json!({"view": "traditional"}))),
    ];
    for (params, want) in cases {
        let got = g.h.run("cableview", params.clone());
        match (&got, &want) {
            (Ok(v), Ok(w)) => {
                for (k, x) in w.as_object().unwrap() {
                    assert_eq!(&v[k], x, "{params}: {v}");
                }
            }
            (Err(e), Err(w)) => assert!(e.contains(w), "{params}: {e}"),
            _ => panic!("{params}: got {got:?}, want {want:?}"),
        }
        assert_eq!(snap(&mut g.h), before, "{params}");
    }
}

// --------------------------------------------------------------------------------------- AC2

/// AC2: switching never writes the `.ecad` file; a save in fan view writes the same project as one
/// in traditional view.
#[test]
fn ac2_switching_never_touches_the_ecad_file_and_saves_are_view_independent() {
    let dir = Tmp::new("bytes");
    let settings = Tmp::new("bytes-settings");
    let mut g = fixture();
    g.h.cfg(|p| {
        p.persistence = Persistence::File;
        p.settings = Settings::at(&settings.0);
    });
    let path = dir.path("g.ecad");
    g.h.run("saveas", json!({"path": dir.path("g")})).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    for _ in 0..10 {
        g.h.run("cableview", json!({"cable": g.c1.hex()})).unwrap();
        assert!(!g.h.p(|p| p.unsaved()), "a switch is not an unsaved change");
    }
    set(&mut g.h, g.c1, "fan");
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "the .ecad file is untouched");
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), modified);
    // Save while in fan view, then Save As copies in each view: the same project every time.
    g.h.run("qsave", Value::Null).unwrap();
    g.h.run("saveas", json!({"path": dir.path("fan")})).unwrap();
    set(&mut g.h, g.c1, "traditional");
    g.h.run("saveas", json!({"path": dir.path("trad")})).unwrap();
    let fan = std::fs::read(dir.path("fan.ecad")).unwrap();
    let trad = std::fs::read(dir.path("trad.ecad")).unwrap();
    assert!(!String::from_utf8_lossy(&fan).contains(views::LAYER), "no cable layer in the file");
    assert!(!String::from_utf8_lossy(&fan).contains("ecad-cable-views"), "no view in the file");
    let doc = g.h.doc();
    let log = g.h.p(|p| p.ledger.log().to_vec());
    while !g.h.s.docs.is_empty() {
        g.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    for (name, b) in [("fan", &fan), ("trad", &trad), ("g", &std::fs::read(&path).unwrap())] {
        let mut s = Session::empty();
        let (handle, _) = ProjectHandle::open_file_in(&mut s, &dir.path(&format!("{name}.ecad")), 3, Settings::none())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut h = H { s, project: handle, trail: Vec::new() };
        assert_eq!(h.doc(), doc, "{name}: the document");
        assert_eq!(h.p(|p| p.ledger.log().to_vec()), log, "{name}: the log");
        assert!(!b.is_empty());
        h.check();
    }
}

/// AC2 through the real lookup (`ECAD_SETTINGS_DIR`, read by `Project::create_with(File)` and
/// `ProjectHandle::open_file`): run in a child process with its own environment. `APPDATA`, `HOME`
/// and `XDG_CONFIG_HOME` point at an empty folder that must stay empty.
#[test]
fn ac2_views_survive_reopen_through_ecad_settings_dir() {
    let settings = Tmp::new("env-settings");
    let decoy = Tmp::new("env-decoy");
    let work = Tmp::new("env-work");
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_reopen_through_env", "--ignored", "--nocapture", "--test-threads=1"])
        .env("ECAD_SETTINGS_DIR", &settings.0)
        .env("APPDATA", &decoy.0)
        .env("HOME", &decoy.0)
        .env("XDG_CONFIG_HOME", &decoy.0)
        .env("ECAD_TESTER_WORK", &work.0)
        .output()
        .unwrap();
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success() && text.contains("1 passed"), "child failed:\n{text}");
    assert!(settings.0.join(views::FILE_NAME).is_file(), "the file is in ECAD_SETTINGS_DIR");
    assert_eq!(std::fs::read_dir(&decoy.0).unwrap().count(), 0, "nothing written to APPDATA/HOME");
    let left: Vec<_> = std::fs::read_dir(&settings.0).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(left.len(), 1, "no temporary files left: {left:?}");
}

#[test]
#[ignore = "run by ac2_views_survive_reopen_through_ecad_settings_dir in a child process"]
fn child_reopen_through_env() {
    let Some(work) = std::env::var_os("ECAD_TESTER_WORK").map(PathBuf::from) else {
        panic!("run only as a child of ac2_views_survive_reopen_through_ecad_settings_dir");
    };
    let settings = PathBuf::from(std::env::var_os("ECAD_SETTINGS_DIR").unwrap());
    ecad_cadcraft::install();
    let mut s = Session::empty();
    let handle = Project::create_with(&mut s, "Env", SEED, Persistence::File).unwrap();
    let mut g = fixture_on(H { s, project: handle, trail: Vec::new() });
    // C2: three more wires, slashed.
    for y in [30, 35, 40] {
        g.h.run("wire", json!({"points": [[0, y], [60, y]]})).unwrap();
    }
    let c2: CableId =
        id(&g.h.run("cableslash", json!({"from": [30, 25], "to": [30, 45], "tag": "C2"})).unwrap(), "cable");
    let path = work.join("two.ecad").display().to_string();
    g.h.run("saveas", json!({"path": path})).unwrap();
    assert_eq!(set(&mut g.h, g.c1, "fan")["remembered"], true);
    assert_eq!(set(&mut g.h, c2, "fan")["remembered"], true);
    assert_eq!(set(&mut g.h, c2, "traditional")["remembered"], true);
    assert!(!g.h.p(|p| p.unsaved()));
    let project = g.h.p(|p| p.id);
    let doc = g.h.doc();
    let log = g.h.p(|p| p.ledger.log().to_vec());

    // A second, read-only window of the same file reads the views too (design §7 "Read").
    let mut s2 = Session::empty();
    let (h2, summary) = ProjectHandle::open_file(&mut s2, &path, 7).unwrap();
    assert!(summary.contains("read-only"), "{summary}");
    let mut h2 = H { s: s2, project: h2, trail: Vec::new() };
    let v2 = views_of(&mut h2);
    assert_eq!((v2[g.c1.hex()].as_str(), v2[c2.hex()].as_str()), (Some("fan"), Some("traditional")), "{v2}");
    // Views are not edits: switching works on the read-only project, and the writer's own entry is
    // kept (one file, one project ID: the last write wins, as for a second window).
    let r = set(&mut h2, c2, "fan");
    assert_eq!(r["view"], "fan", "{r}");
    set(&mut h2, c2, "traditional");
    h2.check();
    drop(h2);

    while !g.h.s.docs.is_empty() {
        g.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let text = std::fs::read_to_string(settings.join(views::FILE_NAME)).unwrap();
    assert!(!text.contains('\r'), "LF only");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["format"], "ecad-cable-views/1");
    assert_eq!(v["projects"].as_object().unwrap().len(), 1, "{text}");
    assert_eq!(v["projects"][project.hex()]["fan"], json!([g.c1.hex()]), "{text}");

    let mut s = Session::empty();
    let (handle, summary) = ProjectHandle::open_file(&mut s, &path, 8).unwrap();
    assert!(!summary.contains("could not be read"), "{summary}");
    let mut h = H { s, project: handle, trail: Vec::new() };
    assert_eq!(h.doc(), doc, "conductors and cables unchanged");
    assert_eq!(h.p(|p| p.ledger.log().to_vec()), log);
    let v = views_of(&mut h);
    assert_eq!(v[g.c1.hex()], "fan");
    assert_eq!(v[c2.hex()], "traditional");
    assert_eq!(fan_paths(&mut h, g.page).len(), 7, "C1 drawn as its fan at once");
    assert!(!h.p(|p| p.unsaved()));
    h.check();
}

/// Design §7: with no settings folder at all the switch still works and says once that views will
/// not be remembered (child process: `ECAD_SETTINGS_DIR`, `APPDATA`, `HOME`, `XDG_CONFIG_HOME` unset).
#[test]
fn ac2_no_settings_folder_switches_and_says_so_once() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_no_settings_folder", "--ignored", "--nocapture", "--test-threads=1"])
        .env_remove("ECAD_SETTINGS_DIR")
        .env_remove("APPDATA")
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env("ECAD_TESTER_CHILD", "1")
        .output()
        .unwrap();
    let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success() && text.contains("1 passed"), "child failed:\n{text}");
}

#[test]
#[ignore = "run by ac2_no_settings_folder_switches_and_says_so_once in a child process"]
fn child_no_settings_folder() {
    assert!(std::env::var_os("ECAD_TESTER_CHILD").is_some(), "run only as a child");
    assert!(std::env::var_os("APPDATA").is_none());
    ecad_cadcraft::install();
    let mut s = Session::empty();
    let handle = Project::create_with(&mut s, "Nowhere", SEED, Persistence::File).unwrap();
    let mut g = fixture_on(H { s, project: handle, trail: Vec::new() });
    assert!(g.h.p(|p| p.settings.dir().is_none()));
    // Interactive: the notice is echoed once, the switch line says it was not remembered or not.
    g.h.cmdline("cableview");
    click(&mut g.h, 30.0, 7.5);
    click(&mut g.h, 40.0, 5.0);
    click(&mut g.h, 30.0, 7.5);
    let _ = g.h.s.input(Input::Enter);
    let said = g.h.s.log.iter().filter(|l| l.as_str() == views::NOT_REMEMBERED).count();
    assert_eq!(said, 1, "said once: {:?}", g.h.s.log);
    assert_eq!(view_of(&mut g.h, g.c1), "fan");
    let r = set(&mut g.h, g.c1, "traditional");
    assert_eq!(r["remembered"], false, "{r}");
    assert!(r.get("notice").is_none(), "only once: {r}");
}

/// What a hostile settings file is, and what opening must then show.
struct Hostile {
    name: &'static str,
    /// Writes the bad settings in `dir` (the settings folder), given the project and C1 hex.
    make: fn(&std::path::Path, &str, &str),
    /// C1's view after opening.
    view: &'static str,
    /// Whether the open summary carries the "could not be read" line.
    message: bool,
}

fn file_in(dir: &std::path::Path) -> PathBuf {
    dir.join(views::FILE_NAME)
}

fn valid(project: &str, c1: &str) -> String {
    format!("{{\"format\":\"ecad-cable-views/1\",\"projects\":{{\"{project}\":{{\"fan\":[\"{c1}\"]}}}}}}\n")
}

/// AC2, design §7 and check G: hostile settings files never fail open or save and never panic.
#[test]
fn ac2_hostile_settings_files_never_fail_open_or_save() {
    let cases: Vec<Hostile> = vec![
        Hostile { name: "missing", make: |_, _, _| {}, view: "traditional", message: false },
        Hostile {
            name: "garbage",
            make: |d, _, _| std::fs::write(file_in(d), [0xff, 0x00, 0xfe, 0x7b, 0x80, 0x0a]).unwrap(),
            view: "traditional",
            message: true,
        },
        Hostile {
            name: "empty",
            make: |d, _, _| std::fs::write(file_in(d), b"").unwrap(),
            view: "traditional",
            message: true,
        },
        Hostile {
            name: "truncated",
            make: |d, p, c| {
                let v = valid(p, c);
                std::fs::write(file_in(d), &v.as_bytes()[..v.len() / 2]).unwrap()
            },
            view: "traditional",
            message: true,
        },
        Hostile {
            name: "top-level array",
            make: |d, _, _| std::fs::write(file_in(d), b"[1,2,3]").unwrap(),
            view: "traditional",
            message: true,
        },
        Hostile {
            name: "other format",
            make: |d, p, c| std::fs::write(file_in(d), valid(p, c).replace("views/1", "views/2")).unwrap(),
            view: "traditional",
            message: true,
        },
        Hostile {
            name: "projects not an object",
            make: |d, _, _| std::fs::write(file_in(d), br#"{"format":"ecad-cable-views/1","projects":5}"#).unwrap(),
            view: "traditional",
            message: true,
        },
        Hostile {
            name: "fan not a list",
            make: |d, p, _| {
                let t = format!(r#"{{"format":"ecad-cable-views/1","projects":{{"{p}":{{"fan":"x"}}}}}}"#);
                std::fs::write(file_in(d), t).unwrap()
            },
            view: "traditional",
            message: false,
        },
        Hostile {
            name: "mixed list entries",
            make: |d, p, c| {
                let t = format!(
                    r#"{{"format":"ecad-cable-views/1","projects":{{"{p}":{{"fan":[1,null,"zz","{c}",{{}},"00000000000000000000000000000009"]}}}}}}"#
                );
                std::fs::write(file_in(d), t).unwrap()
            },
            view: "fan",
            message: false,
        },
        Hostile {
            name: "over 1 MiB",
            make: |d, p, c| {
                let mut t = valid(p, c).into_bytes();
                t.extend(std::iter::repeat_n(b' ', 1024 * 1024 + 10));
                std::fs::write(file_in(d), t).unwrap()
            },
            view: "traditional",
            message: true,
        },
        Hostile {
            name: "CRLF (edited on Windows)",
            make: |d, p, c| {
                std::fs::write(file_in(d), valid(p, c).replace(',', ",\r\n").replace('\n', "\r\n")).unwrap()
            },
            view: "fan",
            message: false,
        },
        Hostile {
            name: "a folder named cable-views.json",
            make: |d, _, _| std::fs::create_dir_all(file_in(d)).unwrap(),
            view: "traditional",
            message: true,
        },
        Hostile {
            name: "read-only file",
            make: |d, p, c| {
                std::fs::write(file_in(d), valid(p, c)).unwrap();
                let mut perm = std::fs::metadata(file_in(d)).unwrap().permissions();
                perm.set_readonly(true);
                std::fs::set_permissions(file_in(d), perm).unwrap();
            },
            view: "fan",
            message: false,
        },
    ];
    // One saved project, reopened once per case.
    let work = Tmp::new("hostile");
    let mut g = fixture();
    g.h.cfg(|p| p.persistence = Persistence::File);
    let path = work.path("h.ecad");
    g.h.run("saveas", json!({"path": path})).unwrap();
    let (project, c1) = (g.h.p(|p| p.id).hex(), g.c1.hex());
    let doc = g.h.doc();
    while !g.h.s.docs.is_empty() {
        g.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut observed = Vec::new();
    for case in &cases {
        let settings = Tmp::new("hostile-settings");
        (case.make)(&settings.0, &project, &c1);
        let mut s = Session::empty();
        let (handle, summary) = ProjectHandle::open_file_in(&mut s, &path, 5, Settings::at(&settings.0))
            .unwrap_or_else(|e| panic!("{}: open failed: {e}", case.name));
        assert_eq!(
            summary.matches(views::UNREADABLE).count(),
            usize::from(case.message),
            "{}: open summary {summary:?}",
            case.name
        );
        let mut h = H { s, project: handle, trail: Vec::new() };
        assert_eq!(h.doc(), doc, "{}", case.name);
        assert_eq!(view_of(&mut h, g.c1), case.view, "{}", case.name);
        h.check();
        // Switch: never fails, may not be remembered.
        let head = h.head();
        let r = h.run("cableview", json!({"cable": c1})).unwrap_or_else(|e| panic!("{}: switch: {e}", case.name));
        assert_ne!(r["view"].as_str(), Some(case.view), "{}: the switch happened: {r}", case.name);
        assert_eq!(h.head(), head);
        if r["remembered"] == false {
            assert!(r["reason"].as_str().is_some_and(|x| !x.is_empty()), "{}: a reason: {r}", case.name);
        }
        observed.push(format!("{}: remembered={} reason={}", case.name, r["remembered"], r["reason"]));
        // Save never fails because of the settings (nothing to save is fine).
        h.run("newpage", json!({"title": "after"})).unwrap();
        h.run("qsave", Value::Null).unwrap_or_else(|e| panic!("{}: save failed: {e}", case.name));
        assert!(!h.p(|p| p.unsaved()));
        h.check();
        h.run("u", Value::Null).unwrap();
        h.run("qsave", Value::Null).unwrap();
        // A remembered write leaves a file that reads back.
        if r["remembered"] == true {
            let l = Settings::at(&settings.0).load(h.p(|p| p.id));
            assert!(!l.unreadable, "{}: rewritten readable", case.name);
            assert_eq!(l.fan.contains(&g.c1), r["view"] == "fan", "{}", case.name);
        }
        while !h.s.docs.is_empty() {
            h.s.execute("close", &json!({"index": 0})).unwrap();
        }
    }
    eprintln!("hostile settings outcomes:\n{}", observed.join("\n"));
}

/// Design §7: a UTF-8 byte order mark (Notepad's "UTF-8 with BOM") and a folder path that is a
/// file: never a failure; what happens is printed for the report.
#[test]
fn ac2_bom_and_folder_that_is_a_file() {
    let work = Tmp::new("bom");
    let mut g = fixture();
    g.h.cfg(|p| p.persistence = Persistence::File);
    let path = work.path("b.ecad");
    g.h.run("saveas", json!({"path": path})).unwrap();
    let (project, c1) = (g.h.p(|p| p.id).hex(), g.c1.hex());
    while !g.h.s.docs.is_empty() {
        g.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let settings = Tmp::new("bom-settings");
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend(valid(&project, &c1).into_bytes());
    std::fs::write(file_in(&settings.0), bytes).unwrap();
    let mut s = Session::empty();
    let (handle, summary) = ProjectHandle::open_file_in(&mut s, &path, 5, Settings::at(&settings.0)).unwrap();
    let mut h = H { s, project: handle, trail: Vec::new() };
    eprintln!("BOM: view {} summary {summary:?}", view_of(&mut h, g.c1));
    h.check();
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    // The settings folder is a file.
    let blocker = work.0.join("not-a-folder");
    std::fs::write(&blocker, b"x").unwrap();
    let mut s = Session::empty();
    let (handle, summary) = ProjectHandle::open_file_in(&mut s, &path, 5, Settings::at(&blocker)).unwrap();
    let mut h = H { s, project: handle, trail: Vec::new() };
    assert_eq!(view_of(&mut h, g.c1), "traditional");
    let r = set(&mut h, g.c1, "fan");
    assert_eq!((r["view"].as_str(), r["remembered"].as_bool()), (Some("fan"), Some(false)), "{r}");
    eprintln!("folder is a file: summary {summary:?}, switch {r}");
    // Interactive echo: the reason is added to the switch line.
    h.cmdline("cableview");
    click(&mut h, 40.0, 5.0);
    let _ = h.s.input(Input::Enter);
    let line = h.s.log.iter().rev().find(|l| l.starts_with("Cable C1 now shown")).cloned().unwrap_or_default();
    assert!(line.starts_with("Cable C1 now shown in traditional view (not remembered: "), "{line}");
    assert_eq!(std::fs::read(&blocker).unwrap(), b"x", "the blocking file is untouched");
    h.run("qsave", Value::Null).unwrap();
}

/// Design §7 "Written": a write changes only this project's entry; IDs not at the head are dropped
/// at the next write for that project.
#[test]
fn ac2_writes_keep_other_projects_and_drop_unknown_cables() {
    let settings = Tmp::new("others");
    let other = "0123456789abcdef0123456789abcdef";
    let stale = "fedcba9876543210fedcba9876543210";
    let mut g = fixture();
    let project = g.h.p(|p| p.id).hex();
    std::fs::write(
        file_in(&settings.0),
        format!(
            r#"{{"format":"ecad-cable-views/1","projects":{{"{other}":{{"fan":["{stale}"]}},"{project}":{{"fan":["{stale}"]}}}}}}"#
        ),
    )
    .unwrap();
    g.h.cfg(|p| p.settings = Settings::at(&settings.0));
    assert_eq!(set(&mut g.h, g.c1, "fan")["remembered"], true);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(file_in(&settings.0)).unwrap()).unwrap();
    assert_eq!(v["projects"][other], json!({"fan": [stale]}), "another project's entry is kept");
    assert_eq!(v["projects"][&project]["fan"], json!([g.c1.hex()]), "unknown cable dropped");
    // Back to traditional: this project's entry goes, the other stays.
    set(&mut g.h, g.c1, "traditional");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(file_in(&settings.0)).unwrap()).unwrap();
    assert!(v["projects"].get(&project).is_none(), "no entry without fan cables (review M2): {v}");
    assert_eq!(v["projects"][other], json!({"fan": [stale]}));
}

// --------------------------------------------------------------------------------------- AC3

/// `n` parallel wires (x 0..60, 1 apart from `y0`), slashed into one cable.
fn parallel_cable(h: &mut H, y0: f64, n: usize, gap: f64) -> CableId {
    for i in 0..n {
        let y = y0 + i as f64 * gap;
        h.run("wire", json!({"points": [[0.0, y], [60.0, y]]})).unwrap();
    }
    let r = h
        .run("cableslash", json!({"from": [20.0, y0 - gap / 2.0], "to": [20.0, y0 + (n as f64 - 0.5) * gap]}))
        .unwrap();
    assert_eq!(r["conductors"].as_array().unwrap().len(), n);
    id(&r, "cable")
}

/// AC3: 1, 2 and 20 wires; a one-wire cable is not offered fan (owner Q4) and a cable reduced to one
/// wire while in fan draws its single trunk.
#[test]
fn ac3_fans_of_one_two_and_twenty_wires() {
    let mut h = H::new(SEED + 1);
    let page = h.first_page();
    let one = parallel_cable(&mut h, 0.0, 1, 1.0);
    let head = h.head();
    let e = h.run("cableview", json!({"cable": one.hex(), "view": "fan"})).unwrap_err();
    assert!(e.contains(views::ONE_WIRE), "{e}");
    assert_eq!(h.head(), head);
    assert_eq!(view_of(&mut h, one), "traditional");
    // Toggling it also refuses; interactive click too, prompt repeats.
    assert!(h.run("cableview", json!({"cable": one.hex()})).is_err());
    h.cmdline("cableview");
    click(&mut h, 20.0, 0.0);
    assert!(h.s.log.last().is_some_and(|l| l.contains(views::ONE_WIRE)), "{:?}", h.s.log.last());
    assert_eq!(prompt(&h), "Select a cable to switch its view:");
    let _ = h.s.input(Input::Enter);
    assert_eq!(view_of(&mut h, one), "traditional");

    let two = parallel_cable(&mut h, 10.0, 2, 4.0);
    set(&mut h, two, "fan");
    // k = 2: reference is the first (y = 10); spread 4 → d = 4. Trunk (4,10)-(56,10).
    assert_paths(
        &fan_paths(&mut h, page),
        &[
            vec![(4.0, 10.0), (56.0, 10.0)],
            vec![(0.0, 10.0), (4.0, 10.0)],
            vec![(56.0, 10.0), (60.0, 10.0)],
            vec![(0.0, 14.0), (4.0, 10.0)],
            vec![(56.0, 10.0), (60.0, 14.0)],
        ],
    );
    let twenty = parallel_cable(&mut h, 30.0, 20, 1.0);
    set(&mut h, twenty, "fan");
    h.check();
    let paths = fan_paths(&mut h, page);
    // 5 for `two`, 1 trunk + 2 × 20 for `twenty` (the reference's own pieces included).
    assert_eq!(paths.len(), 5 + 41, "{}", paths.len());
    // Reference = member 9 (y = 39), s = 10 → F₀ = (10,39), F₁ = (50,39).
    assert!(
        paths
            .iter()
            .any(|p| p.len() == 2 && p[0].near(Pt::new(10.0, 39.0), 1e-9) && p[1].near(Pt::new(50.0, 39.0), 1e-9))
    );
    // A fan cable left with one wire: the whole route as the trunk, no panic.
    let members: Vec<ConductorId> = h.doc().cables[&two].members.iter().map(|m| m.conductor).collect();
    h.run("ecaddelete", json!({"conductor": members[1].hex()})).unwrap();
    h.check();
    assert_eq!(view_of(&mut h, two), "fan");
    let paths = fan_paths(&mut h, page);
    assert!(
        paths
            .iter()
            .any(|p| p.len() == 2 && p[0].near(Pt::new(0.0, 10.0), 1e-9) && p[1].near(Pt::new(60.0, 10.0), 1e-9)),
        "{paths:?}"
    );
    h.run("u", Value::Null).unwrap();
    h.check();
    // An emptied cable in fan view (owner Q4 of 004: it stays): nothing drawn, no panic.
    h.run("ecaddelete", json!({"conductor": members[0].hex()})).unwrap();
    h.run("ecaddelete", json!({"conductor": members[1].hex()})).unwrap();
    h.check();
    let _ = views_of(&mut h);
    set(&mut h, two, "traditional");
    h.check();
}

/// AC3 / design §5 rule 3: the fan is capped at 0.4·L when the ends are spread wider.
#[test]
fn ac3_fan_lengths_are_capped_at_forty_percent() {
    let mut h = H::new(SEED + 2);
    let page = h.first_page();
    let k = parallel_cable(&mut h, 0.0, 3, 50.0);
    set(&mut h, k, "fan");
    // Reference y = 50, L = 60, s = 50 → d = 24.
    assert_paths(
        &fan_paths(&mut h, page),
        &[
            vec![(24.0, 50.0), (36.0, 50.0)],
            vec![(0.0, 0.0), (24.0, 50.0)],
            vec![(0.0, 50.0), (24.0, 50.0)],
            vec![(0.0, 100.0), (24.0, 50.0)],
            vec![(36.0, 50.0), (60.0, 0.0)],
            vec![(36.0, 50.0), (60.0, 50.0)],
            vec![(36.0, 50.0), (60.0, 100.0)],
        ],
    );
}

/// AC3 / design check H: a right-to-left reference fans to the correct sides; a bent reference's
/// trunk follows its bends; loose-end squares stay at the fan line ends.
#[test]
fn ac3_right_to_left_and_bent_references_and_loose_ends() {
    let mut h = H::new(SEED + 3);
    let page = h.first_page();
    for pts in [json!([[0, 0], [60, 0]]), json!([[60, 5], [0, 5]]), json!([[0, 10], [60, 10]])] {
        h.run("wire", json!({ "points": pts })).unwrap();
    }
    let k: CableId = id(&h.run("cableslash", json!({"from": [28, -3], "to": [32, 18]})).unwrap(), "cable");
    let before = marks(&mut h, page);
    assert!(before.iter().filter(|m| m.0 == "Square").count() >= 6, "loose-end squares: {before:?}");
    set(&mut h, k, "fan");
    assert_paths(
        &fan_paths(&mut h, page),
        &[
            vec![(5.0, 5.0), (55.0, 5.0)],
            vec![(0.0, 0.0), (5.0, 5.0)],
            vec![(0.0, 5.0), (5.0, 5.0)],
            vec![(0.0, 10.0), (5.0, 5.0)],
            vec![(55.0, 5.0), (60.0, 0.0)],
            vec![(55.0, 5.0), (60.0, 5.0)],
            vec![(55.0, 5.0), (60.0, 10.0)],
        ],
    );
    assert_eq!(marks(&mut h, page), before, "loose-end squares stay where they were");
    assert_eq!(magenta_marks(&mut h, page), 0, "no crossing dots in fan view");
    h.check();

    // A bent reference (the middle wire): (0,25)-(30,25)-(30,40)-(60,40); L = 75.
    let mut h = H::new(SEED + 4);
    let page = h.first_page();
    for pts in [json!([[0, 20], [60, 20]]), json!([[0, 25], [30, 25], [30, 40], [60, 40]]), json!([[0, 30], [60, 30]])]
    {
        h.run("wire", json!({ "points": pts })).unwrap();
    }
    let k: CableId = id(&h.run("cableslash", json!({"from": [10, 17], "to": [10, 33]})).unwrap(), "cable");
    set(&mut h, k, "fan");
    h.check();
    // s₀ = 5 → F₀ = (5,25); s₁ = max(|(60,20)-(60,40)|, |(60,30)-(60,40)|) = 20 → F₁ at 55 = (40,40).
    let paths = fan_paths(&mut h, page);
    let trunk = norm(&[(5.0, 25.0), (30.0, 25.0), (30.0, 40.0), (40.0, 40.0)]);
    assert!(
        paths.iter().any(|p| p.len() == 4 && p.iter().zip(&trunk).all(|(a, b)| a.near(*b, 1e-9))),
        "the trunk follows the bends: {paths:?}"
    );
    for (a, b) in [
        ((0.0, 20.0), (5.0, 25.0)),
        ((0.0, 30.0), (5.0, 25.0)),
        ((40.0, 40.0), (60.0, 20.0)),
        ((40.0, 40.0), (60.0, 30.0)),
    ] {
        let w = norm(&[a, b]);
        assert!(
            paths.iter().any(|p| p.len() == 2 && p[0].near(w[0], 1e-9) && p[1].near(w[1], 1e-9)),
            "{w:?} in {paths:?}"
        );
    }
}

/// AC3 / design check I: hostile coordinates never panic; a fan that cannot be drawn stays
/// traditional and says so on hover.
#[test]
fn ac3_hostile_coordinates_never_panic() {
    let mut h = H::new(SEED + 5);
    let page = h.first_page();
    let mut made = Vec::new();
    for pts in [
        json!([[1e12, 1e12], [1e12 + 60.0, 1e12]]),
        json!([[1e12, 1e12 + 5.0], [1e12 + 60.0, 1e12 + 5.0]]),
        json!([[-1e12, -1e12], [1e12, 1e12]]),
        json!([[0, 0], [1e-9, 0]]),
        json!([[0, 0], [0, 0]]),
        json!([[0, 0], [1e300, 0]]),
    ] {
        made.push((pts.clone(), h.run("wire", json!({ "points": pts }))));
    }
    eprintln!("hostile wires: {:?}", made.iter().map(|(p, r)| format!("{p} -> {}", r.is_ok())).collect::<Vec<_>>());
    if let Ok(r) = h.run("cableslash", json!({"from": [1e12 + 20.0, 1e12 - 2.0], "to": [1e12 + 20.0, 1e12 + 7.0]})) {
        let k: CableId = id(&r, "cable");
        let _ = h.run("cableview", json!({"cable": k.hex(), "view": "fan"}));
        h.check();
        let _ = fan_paths(&mut h, page);
    }
    // A cable whose middle (reference) member is a tiny wire: L at or under the tolerance.
    let mut h = H::new(SEED + 6);
    let page = h.first_page();
    h.run("wire", json!({"points": [[0, 0], [60, 0]]})).unwrap();
    let tiny = h.run("wire", json!({"points": [[20, 2], [20.000000001, 2]]}));
    h.run("wire", json!({"points": [[0, 4], [60, 4]]})).unwrap();
    eprintln!("tiny wire accepted: {}", tiny.is_ok());
    if let Ok(r) = h.run("cableslash", json!({"from": [20, -1], "to": [20, 5]})) {
        let k: CableId = id(&r, "cable");
        let n = h.doc().cables[&k].members.len();
        let r = h.run("cableview", json!({"cable": k.hex(), "view": "fan"}));
        h.check();
        eprintln!("tiny-reference cable of {n}: {r:?}, fan paths {:?}", fan_paths(&mut h, page));
    }
}

/// Design §5 "L = 0, or any value not finite": a reference route whose length overflows to infinity
/// draws traditional and hovers `View: fan (cannot be drawn on this page)`.
#[test]
fn ac3_a_fan_that_cannot_be_drawn_stays_traditional_and_says_so() {
    let mut h = H::new(SEED + 7);
    let page = h.first_page();
    h.run("wire", json!({"points": [[0, 0], [60, 0]]})).unwrap();
    // Two finite segments whose total length overflows to infinity.
    let huge = h.run("wire", json!({"points": [[0.0, 5.0], [1.7e308, 5.0], [0.0, 6.0]]}));
    h.run("wire", json!({"points": [[0, 10], [60, 10]]})).unwrap();
    let Ok(huge) = huge else {
        eprintln!("a wire of overflowing length is refused: {huge:?}; the fallback is not reachable from outside");
        return;
    };
    let r = h.run("cableslash", json!({"from": [20, -3], "to": [20, 13]})).unwrap();
    let k: CableId = id(&r, "cable");
    if h.doc().cables[&k].members.len() != 3 {
        eprintln!("the slash does not take the overflowing wire: {r}; the fallback is not reachable from outside");
        return;
    }
    assert_eq!(h.doc().cables[&k].members[1].conductor, id::<ConductorId>(&huge, "conductor"), "the reference");
    let r = h.run("cableview", json!({"cable": k.hex(), "view": "fan"})).unwrap();
    assert_eq!(r["view"], "fan", "{r}");
    h.check();
    assert!(fan_paths(&mut h, page).is_empty(), "drawn traditional on this page");
    let item = ecad_cadcraft::slash::slashes_of(&h.doc(), page, k)[0].0;
    let handle = h.handle_of(page, item);
    let uid = h.p(|p| p.pages[&page].uid).unwrap();
    let lines = h.p(|p| hover_lines(p, uid, handle)).unwrap_or_default();
    assert!(lines.iter().any(|l| l == "View: fan (cannot be drawn on this page)"), "{lines:?}");
}

/// Design §5 degenerate rows, on a copy of fixture G's document whose reference route (W5) is
/// replaced: zero length, a NaN point, an infinite point, a huge but finite route. `fan_on_page`
/// never panics, and gives up (traditional) exactly when L is 0 or a value is not finite.
#[test]
fn ac3_degenerate_reference_routes_fall_back_without_panic() {
    let mut g = fixture();
    let doc = g.h.doc();
    let (item, _) = ecad_model::derive::route_on_page(&doc, g.wires[1], g.page).unwrap();
    let cases: Vec<(&str, ecad_model::Primitive, bool)> = vec![
        ("zero length", ecad_model::Primitive::Line { a: Pt::new(0.0, 5.0), b: Pt::new(0.0, 5.0) }, true),
        ("NaN", ecad_model::Primitive::Line { a: Pt::new(0.0, 5.0), b: Pt::new(f64::NAN, 5.0) }, true),
        ("infinite", ecad_model::Primitive::Line { a: Pt::new(0.0, 5.0), b: Pt::new(f64::INFINITY, 5.0) }, true),
        (
            "overflowing sum",
            ecad_model::Primitive::Polyline {
                vertices: vec![Pt::new(0.0, 5.0), Pt::new(1.7e308, 5.0), Pt::new(-1.7e308, 5.0), Pt::new(60.0, 5.0)],
                closed: false,
            },
            true,
        ),
        ("huge finite", ecad_model::Primitive::Line { a: Pt::new(-1e15, 5.0), b: Pt::new(1e15, 5.0) }, false),
    ];
    for (name, prim, fails) in cases {
        let mut d = doc.clone();
        d.items.get_mut(&item).unwrap().kind = ecad_model::ItemKind::Primitive(prim);
        let got = std::panic::catch_unwind(|| views::fan_on_page(&d, g.page, g.c1, 1e-6));
        let got = got.unwrap_or_else(|_| panic!("{name}: fan_on_page panicked"));
        match (&got, fails) {
            (views::FanOnPage::Failed, true) => {}
            (views::FanOnPage::Drawn(f), false) => {
                assert!(f.paths().all(|p| p.iter().all(|q| q.x.is_finite() && q.y.is_finite())), "{name}: {f:?}");
            }
            _ => panic!("{name}: {got:?}"),
        }
        // The projection with the views never panics either.
        let set = BTreeSet::from([g.c1]);
        let r = ecad_cadcraft::project_page_with(
            &d,
            g.page,
            &cadcraft_engine::doc::Drawing::new_metric(),
            &Default::default(),
            &set,
            1e-6,
        );
        if let Ok((_, map)) = r {
            assert_eq!(map.fan_handles().next().is_some(), !fails, "{name}");
        }
    }
}

// --------------------------------------------------------------------------------------- AC4

/// AC4 / design check D, and the stricter case: a real edit made in fan view, then a switch, then
/// `u` undoes the edit, not the switch.
#[test]
fn ac4_undo_and_redo_act_on_the_last_real_edit() {
    let mut g = fixture();
    let after_slash = g.h.doc();
    set(&mut g.h, g.c1, "fan");
    g.h.cmdline("u");
    assert!(!g.h.doc().cables.contains_key(&g.c1), "u took the cableslash");
    g.h.cmdline("redo");
    assert_eq!(g.h.doc(), after_slash);
    assert_eq!(view_of(&mut g.h, g.c1), "fan", "still fan after redo");
    assert_eq!(fan_paths(&mut g.h, g.page).len(), 7);
    // A real edit in fan view (owner O3: moving equipment is allowed): a new wire.
    g.h.run("wire", json!({"points": [[0, 30], [60, 30]]})).unwrap();
    let with_wire = g.h.doc();
    set(&mut g.h, g.c1, "traditional");
    set(&mut g.h, g.c1, "fan");
    g.h.cmdline("u");
    assert_eq!(g.h.doc(), after_slash, "u undid the wire, not a switch");
    assert_eq!(view_of(&mut g.h, g.c1), "fan");
    g.h.cmdline("redo");
    assert_eq!(g.h.doc(), with_wire);
    // The engine's own undo stack never holds a switch (checked by `check`), and the debugger's
    // projection check agrees with the drawing in fan view.
    g.h.check();
}

/// Switching while another command is in progress: the cable slash's first point is down, then a
/// programmatic switch arrives (control channel / MCP). Nothing panics, nothing is recorded, and the
/// slash can still finish or be cancelled.
#[test]
fn switching_during_an_interactive_slash() {
    let mut g = fixture();
    for y in [30, 35, 40] {
        g.h.run("wire", json!({"points": [[0, y], [60, y]]})).unwrap();
    }
    let head = g.h.head();
    g.h.s.start("cableslash").unwrap();
    let at = Vec2::new(30.0, 25.0);
    g.h.s.cursor = at;
    g.h.s.input(Input::Point(at)).unwrap();
    let r = g.h.s.execute("cableview", &json!({"cable": g.c1.hex(), "view": "fan"}));
    eprintln!("switch while slashing: {r:?}, running after: {:?}", g.h.s.running.as_ref().map(|r| r.id.clone()));
    assert_eq!(g.h.head(), head, "the switch recorded nothing");
    if g.h.s.running.is_some() {
        let at = Vec2::new(30.0, 45.0);
        g.h.s.cursor = at;
        let _ = g.h.s.input(Input::Point(at));
    }
    let _ = g.h.s.input(Input::Cancel);
    g.h.check();
    assert!(g.h.head() <= head + 1);
    // Starting Cable View while a slash is running.
    g.h.s.start("cableslash").unwrap();
    g.h.s.start("cableview").unwrap();
    let _ = g.h.s.input(Input::Cancel);
    g.h.check();
}

// --------------------------------------------------------------------------------------- AC6

/// AC6 / design check E: every way of editing the fan is refused with the spec's message, and
/// leaves the head and the drawing as they were.
#[test]
fn ac6_edits_of_the_fan_are_refused_and_change_nothing() {
    let mut g = fixture();
    set(&mut g.h, g.c1, "fan");
    let fans = fan_handles(&mut g.h, g.page);
    assert_eq!(fans.len(), 7);
    // The trunk is the fan line through (30,5).
    let trunk = *fans
        .iter()
        .find(|h| {
            let es = entities(&mut g.h, g.page);
            es.iter().any(|e| {
                e.handle == **h
                    && path_of(e).iter().any(|p| p.near(Pt::new(5.0, 5.0), 1e-9))
                    && path_of(e).iter().any(|p| p.near(Pt::new(55.0, 5.0), 1e-9))
            })
        })
        .expect("trunk");
    let fan_line = *fans.iter().find(|h| **h != trunk).unwrap();
    let before_plain = g.h.lines_on(g.page);
    g.h.s.cmdline("line 0,40 60,40").unwrap();
    g.h.s.input(Input::Enter).unwrap();
    g.h.check();
    let plain = *g.h.lines_on(g.page).iter().find(|x| !before_plain.contains(x)).expect("the plain line");
    let head = g.h.head();
    let before = entities(&mut g.h, g.page);
    let t = trunk.hex();
    let f = fan_line.hex();
    let cases: Vec<(&str, Value)> = vec![
        ("move", json!({"handles": [t], "delta": [0, 10]})),
        ("move", json!({"handles": [plain.hex(), f], "delta": [0, 10]})),
        ("erase", json!({"handles": [f]})),
        ("erase", json!({"handles": [plain.hex(), t]})),
        ("copy", json!({"handles": [t], "delta": [0, 30]})),
        ("rotate", json!({"handles": [t], "base": [30, 5], "angle": 15})),
        ("scale", json!({"handles": [f], "base": [0, 0], "factor": 2})),
        ("mirror", json!({"handles": [t], "p1": [0, 20], "p2": [60, 20], "erase": true})),
        ("stretch", json!({"window": [[25, 3], [35, 7]], "delta": [0, 10]})),
        // A window round the fan-out point F0 (5,5): the trunk's and three fan lines' ends.
        ("stretch", json!({"window": [[3, 3], [7, 7]], "delta": [0, 10]})),
        ("grip.stretch", json!({"handle": t, "index": 0, "to": [5, 20]})),
        ("grip.move", json!({"handles": [t], "index": 0, "to": [5, 20], "mode": "move"})),
        ("properties.set", json!({"handles": [t], "color": 1})),
        ("properties.set", json!({"handles": [f], "layer": "0"})),
        ("offset", json!({"handle": t, "distance": 2, "side": [30, 20]})),
        ("explode", json!({"handles": [t]})),
        ("cutclip", json!({"handles": [f]})),
    ];
    let mut outcomes = Vec::new();
    for (cmd, params) in cases {
        let r = g.h.run(cmd, params.clone());
        outcomes.push(format!("{cmd} {params}: {r:?}"));
        match &r {
            Err(e) => assert!(
                e.contains(FAN_REFUSED) || e.contains(views::LAYER_RESERVED),
                "{cmd} {params}: refused for another reason: {e}"
            ),
            Ok(v) => {
                // A command that found nothing to change (e.g. explode of a plain line) may succeed,
                // but only if the drawing is unchanged.
                assert_eq!(entities(&mut g.h, g.page), before, "{cmd} {params} succeeded and changed the drawing: {v}");
            }
        }
        assert_eq!(g.h.head(), head, "{cmd} {params}: head moved");
        assert_eq!(entities(&mut g.h, g.page), before, "{cmd} {params}: the drawing is restored");
        assert_eq!(view_of(&mut g.h, g.c1), "fan");
    }
    eprintln!("refusals:\n{}", outcomes.join("\n"));
    // Selecting fan lines is allowed (design §6.1).
    g.h.s.set_selection(fans.clone());
    assert_eq!(g.h.s.selection().len(), fans.len());
    g.h.s.set_selection(Vec::new());
    // New geometry on the reserved layer, typed like a user.
    let r = g.h.s.execute("setvar", &json!({"name": "clayer", "value": views::LAYER}));
    if r.is_ok() {
        let _ = g.h.s.cmdline("line 0,50 60,50");
        let _ = g.h.s.input(Input::Enter);
        assert!(g.h.s.running.is_none());
        let msg = g.h.s.log.iter().rev().take(5).cloned().collect::<Vec<_>>();
        assert_eq!(g.h.head(), head, "a line on ECAD-CABLES is not recorded: {msg:?}");
        assert!(msg.iter().any(|l| l.contains(views::LAYER_RESERVED)), "{msg:?}");
        g.h.s.execute("setvar", &json!({"name": "clayer", "value": "0"})).unwrap();
    } else {
        eprintln!("setvar clayer ECAD-CABLES refused: {r:?}");
    }
    g.h.check();
    // ECAD picks on a fan line: Disconnect, Add Terminal, Convert to Component.
    for cmd in ["disconnect", "connect"] {
        g.h.s.start(cmd).unwrap();
        let p0 = prompt(&g.h);
        click(&mut g.h, 2.5, 2.5);
        assert_eq!(g.h.s.log.last().map(String::as_str), Some(FAN_REFUSED), "{cmd}");
        assert_eq!(prompt(&g.h), p0, "{cmd}: the prompt repeats");
        let _ = g.h.s.input(Input::Cancel);
        assert_eq!(g.h.head(), head, "{cmd}");
    }
    // Hover a fan line: the cable with `View: fan` after `id …`.
    let uid = g.h.p(|p| p.pages[&g.page].uid).unwrap();
    let lines = g.h.p(|p| hover_lines(p, uid, trunk)).unwrap_or_default();
    assert!(lines.first().is_some_and(|l| l == "Cable C1"), "{lines:?}");
    let at = lines.iter().position(|l| l == "View: fan").expect("View: fan");
    assert!(lines.get(at - 1).is_some_and(|l| l.starts_with("id ")), "after the id line: {lines:?}");
    // Programmatic calls ignore the view (O4).
    g.h.run("disconnect", json!({"conductor": g.wires[1].hex(), "end": "B"})).unwrap();
    assert_eq!(g.h.head(), head + 1);
    g.h.check();
}

/// Design §2 rule 3: edit a route in traditional view, switch to fan, and the fan follows.
#[test]
fn the_fan_follows_route_edits_made_in_traditional_view() {
    let mut g = fixture();
    let r = g.h.route_handle(g.page, g.wires[1]);
    let set_ = BTreeSet::from([r]);
    // Moving a whole attached wire is refused by 008's rules; disconnect and move the B end instead.
    g.h.run("disconnect", json!({"conductor": g.wires[1].hex(), "end": "B"})).unwrap();
    let res = g.h.run("grip.stretch", json!({"handle": r.hex(), "index": 1, "to": [60, 6]}));
    eprintln!("stretch end: {res:?} ({set_:?})");
    set(&mut g.h, g.c1, "fan");
    let ends = g.h.route_ends(g.page, g.wires[1]);
    let paths = fan_paths(&mut g.h, g.page);
    assert!(paths.iter().any(|p| p.iter().any(|q| q.near(ends.1, 1e-9))), "fan ends at {:?}: {paths:?}", ends.1);
    g.h.check();
}

// ------------------------------------------------------------------- round 2 (review M1-M5)

fn ids(range: std::ops::RangeInclusive<u128>) -> BTreeSet<CableId> {
    range.map(CableId).collect()
}

fn stamp(path: &std::path::Path) -> (Vec<u8>, std::time::SystemTime) {
    (std::fs::read(path).unwrap(), std::fs::metadata(path).unwrap().modified().unwrap())
}

/// M1: which settings files a write replaces and which it leaves alone (same bytes, same mtime).
/// Every case goes through the command: the switch happens; a left-alone file says "not
/// remembered" with the reason.
#[test]
fn round2_m1_only_missing_or_non_json_files_are_rewritten() {
    let mut g = fixture();
    let p = g.h.p(|pr| pr.id).hex();
    let c1 = g.c1.hex();
    let newer = valid(&p, &c1).replace("views/1", "views/2");
    let mut big = valid(&p, &c1).trim_end().trim_end_matches('}').to_string();
    big.push_str("},\"pad\":\"");
    big.push_str(&"x".repeat(views::MAX_FILE_BYTES as usize));
    big.push_str("\"}");
    let mut bom = vec![0xef, 0xbb, 0xbf];
    bom.extend(valid(&p, &c1).into_bytes());
    // (name, bytes, rewritten)
    let cases: Vec<(&str, Vec<u8>, bool)> = vec![
        ("garbage", vec![0xde, 0xad, 0x00], true),
        ("truncated JSON", valid(&p, &c1).as_bytes()[..30].to_vec(), true),
        ("empty", Vec::new(), true),
        ("UTF-8 BOM", bom, true),
        ("newer format", newer.into_bytes(), false),
        ("top-level array", b"[1,2]".to_vec(), false),
        ("projects not an object", br#"{"format":"ecad-cable-views/1","projects":[]}"#.to_vec(), false),
        ("over 1 MiB", big.into_bytes(), false),
    ];
    let mut out = Vec::new();
    for (name, bytes, rewritten) in cases {
        let dir = Tmp::new("m1");
        let file = file_in(&dir.0);
        std::fs::write(&file, &bytes).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let before = stamp(&file);
        g.h.cfg(|pr| pr.settings = Settings::at(&dir.0));
        let head = g.h.head();
        let r = g.h.run("cableview", json!({"cable": c1})).unwrap();
        assert_eq!(g.h.head(), head);
        g.h.check();
        out.push(format!("{name}: {r}"));
        if rewritten {
            assert_eq!(r["remembered"], true, "{name}: {r}");
            let l = Settings::at(&dir.0).load(g.h.p(|pr| pr.id));
            assert!(!l.unreadable, "{name}: the rewritten file reads back");
        } else {
            assert_eq!(r["remembered"], false, "{name}: {r}");
            assert!(r["reason"].as_str().is_some_and(|x| x.contains("left as it is")), "{name}: {r}");
            assert_eq!(stamp(&file), before, "{name}: same bytes and mtime");
        }
    }
    eprintln!("M1 outcomes:\n{}", out.join("\n"));
}

/// M1 on Windows: a file held open by another program with no sharing is left alone; the switch
/// still happens and is "not remembered"; an open at that moment never fails.
#[cfg(windows)]
#[test]
fn round2_m1_a_locked_file_is_left_alone() {
    use std::os::windows::fs::OpenOptionsExt;
    let mut g = fixture();
    let dir = Tmp::new("m1-locked");
    let file = file_in(&dir.0);
    let p = g.h.p(|pr| pr.id).hex();
    std::fs::write(&file, valid(&p, &g.c1.hex())).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let before = stamp(&file);
    let work = Tmp::new("m1-locked-work");
    g.h.cfg(|pr| pr.persistence = Persistence::File);
    g.h.run("saveas", json!({"path": work.path("l")})).unwrap();
    let lock = std::fs::OpenOptions::new().read(true).share_mode(0).open(&file).unwrap();
    g.h.cfg(|pr| pr.settings = Settings::at(&dir.0));
    let head = g.h.head();
    let r = g.h.run("cableview", json!({"cable": g.c1.hex(), "view": "fan"})).unwrap();
    assert_eq!((r["view"].as_str(), r["remembered"].as_bool()), (Some("fan"), Some(false)), "{r}");
    assert!(r["reason"].as_str().is_some_and(|x| x.contains("left as it is")), "{r}");
    assert_eq!(g.h.head(), head);
    // Interactive echo while locked.
    g.h.cmdline("cableview");
    click(&mut g.h, 40.0, 5.0);
    let _ = g.h.s.input(Input::Enter);
    let line = g.h.s.log.iter().rev().find(|l| l.starts_with("Cable C1 now shown")).cloned().unwrap_or_default();
    assert!(line.contains("(not remembered: "), "{line}");
    // A read-only second window opened while the file is locked: never fails, says so.
    let mut s = Session::empty();
    let (_h2, summary) = ProjectHandle::open_file_in(&mut s, &work.path("l.ecad"), 4, Settings::at(&dir.0)).unwrap();
    assert!(summary.contains(views::UNREADABLE), "{summary}");
    drop(lock);
    assert_eq!(stamp(&file), before, "same bytes and mtime after the lock is gone");
    eprintln!("locked: {r}\n{line}");
}

/// M2: a project's entry goes when its last fan cable goes back to traditional.
#[test]
fn round2_m2_no_entry_without_fan_cables() {
    let mut g = fixture();
    let dir = Tmp::new("m2-entry");
    g.h.cfg(|pr| pr.settings = Settings::at(&dir.0));
    set(&mut g.h, g.c1, "fan");
    set(&mut g.h, g.c1, "traditional");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(file_in(&dir.0)).unwrap()).unwrap();
    assert_eq!(v["projects"], json!({}), "{v}");
}

/// M2: the per-project limits and the round trip. The largest set the writer accepts reads back
/// exactly; one more is refused and leaves the file alone; the declared cap and cap+1 are refused
/// without a write; the file is never one the reader refuses.
#[test]
fn round2_m2_caps_and_round_trip() {
    let dir = Tmp::new("m2-caps");
    let st = Settings::at(&dir.0);
    let file = file_in(&dir.0);
    let project = ecad_model::ProjectId(0x00a1);
    // Largest n that fits (binary search on the writer itself).
    let (mut lo, mut hi) = (1u128, views::MAX_CABLES_PER_PROJECT as u128);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let _ = std::fs::remove_file(&file);
        if st.store(project, &ids(1..=mid)).is_ok() { lo = mid } else { hi = mid - 1 }
    }
    let max = lo;
    let _ = std::fs::remove_file(&file);
    st.store(project, &ids(1..=max)).unwrap();
    let bytes = std::fs::read(&file).unwrap();
    assert!(bytes.len() as u64 <= views::MAX_FILE_BYTES);
    let l = st.load(project);
    assert!(!l.unreadable);
    assert_eq!(l.fan, ids(1..=max), "round trip at the largest accepted set");
    std::thread::sleep(std::time::Duration::from_millis(20));
    let before = stamp(&file);
    let e = st.store(project, &ids(1..=max + 1)).unwrap_err();
    assert_eq!(stamp(&file), before, "max+1 left the file alone: {e}");
    eprintln!("store of {} ids: {e}", max + 1);
    for n in [views::MAX_CABLES_PER_PROJECT as u128, views::MAX_CABLES_PER_PROJECT as u128 + 1] {
        let e = st.store(project, &ids(1..=n)).unwrap_err();
        assert_eq!(stamp(&file), before, "{n}: {e}");
        eprintln!("store of {n} ids: {e}");
    }
    eprintln!("largest per-project set that is remembered: {max} cables (file {} bytes)", bytes.len());
}

/// M2: many projects written in turn with pseudo-random fan sets: every file written reads back,
/// holds the writing project's set exactly and stays within 1 MiB.
#[test]
fn round2_m2_many_projects_always_readable() {
    let dir = Tmp::new("m2-many");
    let st = Settings::at(&dir.0);
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut last: std::collections::BTreeMap<u128, BTreeSet<CableId>> = Default::default();
    for round in 0..600 {
        let project = u128::from(next() % 400) + 1;
        let n = match next() % 10 {
            0 => 0,
            1 => 5_000 + next() % 4_000,
            _ => next() % 40,
        };
        let base = u128::from(next());
        let fan: BTreeSet<CableId> = (0..n).map(|i| CableId(base + u128::from(i))).collect();
        st.store(ecad_model::ProjectId(project), &fan).unwrap_or_else(|e| panic!("round {round}: {e}"));
        let bytes = std::fs::read(file_in(&dir.0)).unwrap();
        assert!(bytes.len() as u64 <= views::MAX_FILE_BYTES, "round {round}: {}", bytes.len());
        let l = st.load(ecad_model::ProjectId(project));
        assert!(!l.unreadable, "round {round}");
        assert_eq!(l.fan, fan, "round {round}");
        last.insert(project, fan);
    }
    let v: Value = serde_json::from_slice(&std::fs::read(file_in(&dir.0)).unwrap()).unwrap();
    let kept = v["projects"].as_object().unwrap().len();
    for (p, fan) in &last {
        let got = st.load(ecad_model::ProjectId(*p)).fan;
        assert!(got == *fan || got.is_empty(), "project {p:x}: neither its last set nor dropped");
    }
    eprintln!("projects kept in the file at the end: {kept} of {}", last.len());
}

/// M3: the two-writer limitation in the quick start. Two different projects written one after the
/// other: both kept. The same project in a read-only second window: what happens to window 2's
/// earlier switch when window 1 switches later (printed for the report).
#[test]
fn round2_m3_two_windows() {
    let settings = Tmp::new("m3-settings");
    let work = Tmp::new("m3-work");
    let mut g = fixture();
    for y in [30, 35, 40] {
        g.h.run("wire", json!({"points": [[0, y], [60, y]]})).unwrap();
    }
    let c2: CableId =
        id(&g.h.run("cableslash", json!({"from": [30, 25], "to": [30, 45], "tag": "C2"})).unwrap(), "cable");
    g.h.cfg(|pr| {
        pr.persistence = Persistence::File;
        pr.settings = Settings::at(&settings.0);
    });
    g.h.run("saveas", json!({"path": work.path("m3")})).unwrap();
    let mut s2 = Session::empty();
    let (h2, summary) =
        ProjectHandle::open_file_in(&mut s2, &work.path("m3.ecad"), 6, Settings::at(&settings.0)).unwrap();
    assert!(summary.contains("read-only"), "{summary}");
    let mut w2 = H { s: s2, project: h2, trail: Vec::new() };
    // Window 2 switches C2; afterwards (not at the same moment) window 1 switches C1.
    set(&mut w2, c2, "fan");
    set(&mut g.h, g.c1, "fan");
    let project = g.h.p(|pr| pr.id);
    let file = Settings::at(&settings.0).load(project).fan;
    assert!(file.contains(&g.c1));
    eprintln!(
        "same project, one switch in each window, one after the other: C2 kept in the file: {}",
        file.contains(&c2)
    );
    // Two different projects one after the other: both kept.
    let mut other = fixture_on(H::new(SEED + 9));
    other.h.cfg(|pr| pr.settings = Settings::at(&settings.0));
    set(&mut other.h, other.c1, "fan");
    set(&mut g.h, g.c1, "traditional");
    set(&mut g.h, g.c1, "fan");
    assert!(Settings::at(&settings.0).load(other.h.p(|pr| pr.id)).fan.contains(&other.c1), "another project kept");
}

/// M5: a selection of four cables (one of one wire) with one switch failing: each cable reports
/// its outcome, the others switch, nothing is recorded. Then every switch failing.
#[test]
fn round2_m5_selection_with_a_failure_carries_on() {
    let mut g = fixture();
    let mut cables = vec![g.c1];
    for (y0, n) in [(30, 3), (50, 3), (70, 1)] {
        for i in 0..n {
            g.h.run("wire", json!({"points": [[0, y0 + 5 * i], [60, y0 + 5 * i]]})).unwrap();
        }
        let r = g.h.run("cableslash", json!({"from": [30, y0 - 3], "to": [30, y0 + 5 * n - 2]})).unwrap();
        cables.push(id(&r, "cable"));
    }
    let slash_handles = |h: &mut H| -> Vec<Handle> {
        let doc = h.doc();
        let page = h.first_page();
        let items: Vec<ItemId> = cables
            .iter()
            .flat_map(|k| ecad_cadcraft::slash::slashes_of(&doc, page, *k).into_iter().map(|x| x.0))
            .collect();
        items.into_iter().filter_map(|i| h.p(|p| p.pages[&page].map.item(i))).collect()
    };
    let before = snap(&mut g.h);
    let log0 = g.h.s.log.len();
    g.h.cfg(|p| p.faults.fail_view_switches = 1);
    let sel = slash_handles(&mut g.h);
    assert_eq!(sel.len(), 4);
    g.h.s.set_selection(sel);
    g.h.s.start("cableview").unwrap();
    assert!(g.h.s.running.is_none());
    let log: Vec<String> = g.h.s.log[log0..].to_vec();
    eprintln!("selection echo, one failing: {log:#?}");
    let vs = views_of(&mut g.h);
    assert_eq!(cables.iter().filter(|k| vs[k.hex()] == "fan").count(), 2, "{vs}");
    assert_eq!(vs[cables[3].hex()], "traditional", "the one-wire cable");
    assert_eq!(log.iter().filter(|l| l.ends_with("now shown in fan view.")).count(), 2, "{log:?}");
    assert_eq!(log.iter().filter(|l| l.contains("was not switched: injected view switch failure")).count(), 1);
    assert_eq!(log.iter().filter(|l| l.contains(views::ONE_WIRE)).count(), 1, "{log:?}");
    assert_eq!(snap(&mut g.h), before, "nothing recorded");
    g.h.check();
    // Every switch failing: the slashes still drawn plus the fan lines of the two fan cables.
    let log0 = g.h.s.log.len();
    g.h.cfg(|p| p.faults.fail_view_switches = 10);
    let mut sel = slash_handles(&mut g.h);
    sel.extend(fan_handles(&mut g.h, g.page));
    g.h.s.set_selection(sel);
    let vs0 = views_of(&mut g.h);
    g.h.s.start("cableview").unwrap();
    let log: Vec<String> = g.h.s.log[log0..].to_vec();
    eprintln!("selection echo, all failing: {log:#?}");
    assert_eq!(views_of(&mut g.h), vs0, "{log:?}");
    assert_eq!(log.iter().filter(|l| l.contains("was not switched")).count(), 3, "{log:?}");
    assert_eq!(snap(&mut g.h), before);
    g.h.cfg(|p| p.faults.fail_view_switches = 0);
    g.h.check();
}

/// Round-1 observation 5: a fan cable's slash undone, another cable switched, then redo.
#[test]
fn round2_observation5_view_after_undo_switch_redo() {
    let mut h = H::new(SEED + 13);
    for y in [0, 5, 10, 30, 35, 40] {
        h.run("wire", json!({"points": [[0, y], [60, y]]})).unwrap();
    }
    let k2: CableId = id(&h.run("cableslash", json!({"from": [30, 25], "to": [30, 45]})).unwrap(), "cable");
    let k1: CableId = id(&h.run("cableslash", json!({"from": [28, -3], "to": [32, 18]})).unwrap(), "cable");
    set(&mut h, k1, "fan");
    h.run("u", Value::Null).unwrap();
    assert!(!h.doc().cables.contains_key(&k1));
    set(&mut h, k2, "fan");
    h.run("redo", Value::Null).unwrap();
    assert!(h.doc().cables.contains_key(&k1));
    eprintln!("observation 5: after undo, another switch and redo, the first cable is {}", view_of(&mut h, k1));
    h.check();
}
