//! Shared harness for the integration experiment: a CADCraft session with one ECAD project, and
//! the checks of proposal §17.4 run after every step.

#![allow(dead_code)]

use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle, Space};
use ecad_cadcraft::{Mapped, PagesRef, Project, ProjectHandle};
use ecad_model::derive::mismatches;
use ecad_model::{DeviceId, Document, ItemKind, PageId, Pt, TerminalId, WireId};
use serde_json::{Value, json};
use std::path::PathBuf;

pub struct H {
    pub s: Session,
    /// The project, reachable only through the handle's lock boundary.
    pub project: ProjectHandle,
    /// The document after each effective edit, for undo/redo comparisons (S14).
    pub trail: Vec<Document>,
}

pub fn hex(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}

pub fn id<T: From<u128>>(v: &Value, key: &str) -> T {
    T::from(u128::from_str_radix(v[key].as_str().unwrap(), 16).unwrap())
}

impl H {
    pub fn new(seed: u64) -> H {
        ecad_cadcraft::install();
        // Never the developer's own cable-views.json (brief 005 review M4).
        ecad_cadcraft::views::isolate_for_tests();
        let mut s = Session::empty();
        let project = Project::create(&mut s, "Demo", seed).unwrap();
        let mut h = H { s, project, trail: Vec::new() };
        let d = h.doc();
        h.trail.push(d);
        h.check();
        h
    }

    /// Take over a project made by [`ecad_cadcraft::test_support::ProjectBuilder`], with its trail started
    /// at the built document and the harness checks run once.
    pub fn from_built(b: ecad_cadcraft::test_support::Built) -> H {
        let mut h = H { s: b.s, project: b.project, trail: Vec::new() };
        let d = h.doc();
        h.trail.push(d);
        h.check();
        h
    }

    /// Read the project. The session is borrowed for the call, so `f` cannot call it.
    pub fn p<R>(&mut self, f: impl FnOnce(&Project) -> R) -> R {
        self.project.read(&mut self.s, |p, _| f(p)).expect("project not busy")
    }

    /// Read the project together with its page drawings.
    pub fn pv<R>(&mut self, f: impl FnOnce(&Project, &PagesRef<'_>) -> R) -> R {
        self.project.read(&mut self.s, f).expect("project not busy")
    }

    /// Set up faults or the test medium.
    pub fn cfg<R>(&mut self, f: impl FnOnce(&mut Project) -> R) -> R {
        self.project.configure(&mut self.s, f).expect("project not busy")
    }

    pub fn head(&mut self) -> u64 {
        self.p(|p| p.ledger.head())
    }

    pub fn doc(&mut self) -> Document {
        self.p(|p| p.ledger.document().clone())
    }

    /// Run a command's JSON form; record the trail when it appended an edit.
    pub fn run(&mut self, cmd: &str, params: Value) -> Result<Value, String> {
        let before = self.head();
        let r = self.s.execute(cmd, &params).map_err(|e| e.to_string());
        self.after(before);
        r
    }

    /// Type at the command line.
    pub fn cmdline(&mut self, text: &str) {
        let before = self.head();
        self.s.cmdline(text).unwrap();
        self.after(before);
    }

    fn after(&mut self, before: u64) {
        let head = self.head();
        // One command, one transaction (008 §2). The one exception is brief 052d's move
        // follow-up (design 052 §5.4, owner answer Q3): a move, then `Set location …` (or
        // function) as its own transaction, so `u` undoes the location first. The state between
        // the two goes on the trail too, so undo is checked step by step.
        let follow_up = head == before + 2
            && self.p(|p| p.ledger.log().last().is_some_and(|t| t.label.starts_with("Set location") || t.label.starts_with("Set function")));
        assert!(head <= before + 1 || follow_up, "one command appends at most one transaction");
        if self.s.running.is_some() {
            // Mid-command the drawing holds uncommitted work; it is checked at the commit.
            return;
        }
        if follow_up {
            let mut mid = self.doc();
            let ops = self.p(|p| p.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default());
            for op in ops.iter().rev() {
                op.inverse().apply(&mut mid).unwrap();
            }
            mid.rebuild_index();
            self.trail.push(mid);
        }
        if head > before {
            let d = self.doc();
            let last_kind = self.p(|p| p.ledger.log().last().map(|t| t.kind));
            match last_kind {
                Some(ecad_model::TxKind::Undo { .. }) => {
                    self.trail.pop();
                }
                _ => self.trail.push(d),
            }
        }
        self.check();
    }

    pub fn page_index(&mut self, page: PageId) -> Option<usize> {
        self.pv(|p, pg| pg.index_of(p.pages.get(&page)?.uid?))
    }

    /// Make `page` the active page, as a click on its row in the desktop app's Project panel does
    /// (brief 016: one page window, the page projected into it).
    pub fn activate(&mut self, page: PageId) {
        self.project.open_page(&mut self.s, page).expect("page shown");
    }

    /// [`Harness::activate`] (kept for the tests written when each page had its own tab).
    pub fn activate_tab_only(&mut self, page: PageId) {
        self.activate(page);
    }

    pub fn first_page(&mut self) -> PageId {
        self.doc().page_order[0]
    }

    /// Handle of an item on `page`.
    pub fn handle_of(&mut self, page: PageId, item: ecad_model::ItemId) -> Handle {
        self.p(|p| p.pages[&page].map.item(item)).expect("item mapped")
    }

    /// Every handle (items and markers) of a drawn representation.
    pub fn rep_handles(&mut self, page: PageId, component: DeviceId) -> Vec<String> {
        let doc = self.doc();
        let map = self.p(|p| p.pages[&page].map.clone());
        let map = &map;
        let mut out = Vec::new();
        for (rid, r) in doc.representations_of(ecad_model::Subject::Device(component)).filter(|(_, r)| r.page == page) {
            out.extend(r.items.iter().filter_map(|i| map.item(*i)).map(|h| h.hex()));
            out.extend(r.connection_points.keys().filter_map(|t| map.marker(rid, *t)).map(|h| h.hex()));
        }
        out
    }

    /// The route drawing wire `c` on `page` (`wires::route_of_wire_on`).
    pub fn route_of(&mut self, page: PageId, c: WireId) -> ecad_model::RouteId {
        ecad_cadcraft::wires::route_of_wire_on(&self.doc(), c, page).expect("route")
    }

    /// The handle of wire `c`'s line on `page`.
    pub fn route_handle(&mut self, page: PageId, c: WireId) -> Handle {
        let r = self.route_of(page, c);
        self.line_handle(page, r)
    }

    /// The handle of route `r`'s line on `page`.
    pub fn line_handle(&mut self, page: PageId, r: ecad_model::RouteId) -> Handle {
        let doc = self.doc();
        let (_, item, _) = ecad_cadcraft::wires::route_item(&doc, r).expect("route item");
        self.handle_of(page, item)
    }

    /// The first and last point of wire `c`'s line on `page`.
    pub fn route_ends(&mut self, page: PageId, c: WireId) -> (Pt, Pt) {
        let r = self.route_of(page, c);
        let [a, b] = self.doc().routes[&r].end_points().expect("points");
        (a, b)
    }

    pub fn terminal_pos(&mut self, page: PageId, t: TerminalId) -> Pt {
        ecad_model::derive::terminal_positions(&self.doc(), page).into_iter().find(|(x, _, _)| *x == t).map(|(_, _, p)| p).expect("terminal shown")
    }

    pub fn lines_on(&mut self, page: PageId) -> Vec<Handle> {
        self.pv(|p, pg| pg.entities_of(p, page))
            .iter()
            .filter(|e| matches!(e.kind, EntityKind::Line(_) | EntityKind::LwPolyline(_)))
            .map(|e| e.handle)
            .collect()
    }

    /// Proposal §17.4, checks 1-6 (6 with `allow_mismatch` false).
    pub fn check(&mut self) {
        self.check_with(false);
    }

    pub fn check_with(&mut self, allow_mismatch: bool) {
        // Unsaved state as the engine reports it; the project is not locked here (and cannot be:
        // `pv` holds the session).
        let unsaved_per_doc: Vec<(u64, bool)> = self.s.docs.iter().enumerate().map(|(i, d)| (d.uid, self.s.doc_unsaved(i))).collect();
        self.pv(|p, pg| check_project(p, pg, &unsaved_per_doc, allow_mismatch));
    }

    pub fn mapped(&mut self, page: PageId, h: Handle) -> Option<Mapped> {
        self.p(|p| p.pages[&page].map.get(h))
    }

    pub fn symbol_item(&mut self, c: DeviceId) -> ecad_model::ItemId {
        let doc = self.doc();
        let (_, r) = doc.representations_of(ecad_model::Subject::Device(c)).next().unwrap();
        let item = r.items[0];
        assert!(matches!(doc.items[&item].kind, ItemKind::Symbol(_)));
        item
    }
}

fn check_project(p: &Project, pg: &PagesRef<'_>, unsaved_per_doc: &[(u64, bool)], allow_mismatch: bool) {
    {
        let doc = p.ledger.document();
        // 2. replay equals the document.
        assert!(p.ledger.replay_matches().unwrap(), "replay differs from the live document");
        for (page, view) in &p.pages {
            let Some(uid) = view.uid else { continue };
            let Some(st) = pg.doc(uid) else { panic!("open page without a document") };
            if view.unavailable {
                continue;
            }
            // 1. projecting the document equals the engine drawing.
            let (d, map) =
                ecad_cadcraft::project::project_page_pending(doc, *page, &st.doc, &view.map, &p.views, p.tolerance, p.show_hidden_text, p.pending)
                    .unwrap();
            assert!(d.model == st.doc.model, "page {page}: drawing differs from the projection");
            assert_eq!(map, view.map, "page {page}: handle map differs from a fresh projection");
            // 3. the map is one-to-one and every mapped handle exists (and every entity is mapped).
            assert!(view.map.is_consistent());
            let model = st.doc.space(&Space::Model).unwrap();
            for h in view.map.handles() {
                assert!(model.contains(h), "mapped handle {h:?} missing from the drawing");
            }
            assert_eq!(model.len(), view.map.len(), "page {page}: unmapped entities in the drawing");
            // 4. the engine keeps no undo history for owned pages.
            assert!(st.undo.is_empty(), "engine undo stack is not empty");
            // 5. unsaved state follows head vs persisted, on every page tab.
            let shown = unsaved_per_doc.iter().find(|(u, _)| *u == uid).map(|(_, x)| *x);
            assert_eq!(shown, Some(p.ledger.is_unsaved()), "page tab unsaved marker");
            assert_eq!(st.is_dirty(), p.ledger.is_unsaved(), "the saved pointer is out of step");
        }
        // 6. no attached end away from its terminal.
        if !allow_mismatch {
            assert!(mismatches(doc, 1e-6).is_empty(), "mismatch: {:?}", mismatches(doc, 1e-6));
        }
        assert!(doc.check().is_ok());
    }
}

/// Close every document of `s`, as before reopening a saved project in a fresh session.
pub fn close_all(s: &mut Session) {
    while !s.docs.is_empty() {
        s.execute("close", &json!({"index": 0})).unwrap();
    }
}

/// A fresh temporary folder whose name has spaces and a non-ASCII letter, as on a Windows desktop
/// (`C:\Users\...\Schaltpläne 2026`). `tag` says which test made it.
pub fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad {tag} Schaltpl\u{e4}ne {}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Open the project file at `path` in a fresh session, with its trail started at the opened document.
pub fn reopen(path: &str, seed: u64) -> H {
    // As `H::new`: run alone (as nextest runs every test), nothing else has installed ECAD's commands.
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, path, seed).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let d = h.doc();
    h.trail.push(d);
    h
}

/// `place` a symbol at `at`; the command's JSON result (`component`, ...).
pub fn place(h: &mut H, symbol: &str, at: (f64, f64)) -> Value {
    h.run("place", json!({"symbol": symbol, "at": [at.0, at.1]})).unwrap()
}

impl H {
    /// Close every document of this session ([`close_all`]).
    pub fn close_all(&mut self) {
        close_all(&mut self.s);
    }
}

pub fn pts(v: &[(f64, f64)]) -> Value {
    json!(v.iter().map(|(x, y)| json!([x, y])).collect::<Vec<_>>())
}

/// A small project: a drawn component with a terminal, a placed symbol, a wire between them.
pub fn small_project(seed: u64) -> (H, DeviceId, DeviceId, WireId) {
    let mut h = H::new(seed);
    h.cmdline("line 0,0 40,0 40,20 0,20 c");
    let first = h.first_page();
    let lines = h.lines_on(first);
    let c1: DeviceId = id(&h.run("convertcomponent", json!({"handles": lines.iter().map(|x| x.hex()).collect::<Vec<_>>()})).unwrap(), "component");
    h.run("addterminal", json!({"handle": lines[1].hex(), "at": [40, 10]})).unwrap();
    let c2: DeviceId = id(&h.run("place", json!({"at": [100, 0], "rotation": 90})).unwrap(), "component");
    // Wire to the rotated symbol's first terminal, wherever the placement puts it.
    let doc = h.doc();
    let (rid, _) = doc.representations_of(ecad_model::Subject::Device(c2)).next().unwrap();
    let to = ecad_model::derive::terminal_positions(&doc, first).into_iter().find(|(_, r, _)| *r == rid).unwrap().2;
    let w: WireId = id(&h.run("wire", json!({"points": pts(&[(40.0, 10.0), (to.x, to.y)])})).unwrap(), "conductor");
    assert!(doc.wires.is_empty());
    assert!(h.doc().wires.contains_key(&w), "the wire between the two points");
    (h, c1, c2, w)
}

/// Ops drawing `n` wires on `page` straight to the ledger (fixtures too large to draw one by
/// one): one drawn device holding two connection points per wire, a route between them landing
/// on both (design 072 §4.3) with its line, and the wire. `line(i)` gives wire `i`'s two end
/// points. Returns the ops and, in order, each wire with its route and its line's item.
pub fn wired_lines(
    ids: &mut ecad_model::IdAllocator, page: PageId, style: ecad_model::Style, n: usize, line: impl Fn(usize) -> (Pt, Pt),
) -> (Vec<ecad_model::Op>, Vec<(WireId, ecad_model::RouteId, ecad_model::ItemId)>) {
    use ecad_model::{Device, Form, Op, Representation, RouteEnd, Subject, Terminal, Wire};
    let device: DeviceId = ids.fresh();
    let mut ops = vec![Op::CreateDevice { id: device, device: Device::default() }];
    let mut cps = std::collections::BTreeMap::new();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = line(i);
        let (ta, tb): (TerminalId, TerminalId) = (ids.fresh(), ids.fresh());
        for (t, at, k) in [(ta, a, 2 * i), (tb, b, 2 * i + 1)] {
            ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: device, label: Some(k.to_string()), key: None } });
            cps.insert(t, at);
        }
        let (r, item, add) = ecad_cadcraft::wires::add_route(ids, page, style.clone(), vec![a, b], [RouteEnd::Point(ta), RouteEnd::Point(tb)]);
        ops.extend(add);
        let w: WireId = ids.fresh();
        ops.push(Op::CreateWire { id: w, wire: Wire::new(ids.fresh(), ids.fresh(), ta, tb) });
        out.push((w, r, item));
    }
    let rep: ecad_model::RepresentationId = ids.fresh();
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Device(device),
            page,
            form: Form::Drawn,
            role: "main".into(),
            items: Vec::new(),
            connection_points: cps,
        },
    });
    (ops, out)
}

/// Wire `w`'s line on `page`: the item drawing its route there (`wires::route_of_wire_on`) and
/// the item's path (design 072 §4.3; what `derive::route_on_page` gave before routes).
pub fn wire_line(doc: &Document, w: WireId, page: PageId) -> Option<(ecad_model::ItemId, ecad_model::Primitive)> {
    let r = ecad_cadcraft::wires::route_of_wire_on(doc, w, page)?;
    let (_, item, it) = ecad_cadcraft::wires::route_item(doc, r)?;
    match &it.kind {
        ItemKind::Primitive(p) => Some((item, p.clone())),
        ItemKind::Symbol(_) => None,
    }
}
