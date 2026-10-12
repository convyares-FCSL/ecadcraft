//! Shared harness for the integration experiment: a CADCraft session with one ECAD project, and
//! the checks of proposal §17.4 run after every step.

#![allow(dead_code)]

use std::sync::{Arc, Mutex, MutexGuard};

use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle, Space};
use ecad_cadcraft::{Mapped, Project, project_page};
use ecad_model::derive::mismatches;
use ecad_model::{ComponentId, ConductorId, Document, ItemKind, PageId, Pt, TerminalId};
use serde_json::{Value, json};

pub struct H {
    pub s: Session,
    pub project: Arc<Mutex<Project>>,
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
        let mut s = Session::empty();
        let project = Project::create(&mut s, "Demo", seed).unwrap();
        let mut h = H { s, project, trail: Vec::new() };
        let d = h.p().ledger.document().clone();
        h.trail.push(d);
        h.check();
        h
    }

    pub fn p(&self) -> MutexGuard<'_, Project> {
        self.project.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn head(&self) -> u64 {
        self.p().ledger.head()
    }

    pub fn doc(&self) -> Document {
        self.p().ledger.document().clone()
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
        assert!(head <= before + 1, "one command appends at most one transaction");
        if self.s.running.is_some() {
            // Mid-command the drawing holds uncommitted work; it is checked at the commit.
            return;
        }
        if head > before {
            let d = self.doc();
            let last_kind = self.p().ledger.log().last().map(|t| t.kind);
            match last_kind {
                Some(ecad_model::TxKind::Undo { .. }) => {
                    self.trail.pop();
                }
                _ => self.trail.push(d),
            }
        }
        self.check();
    }

    pub fn page_index(&self, page: PageId) -> Option<usize> {
        let uid = self.p().pages.get(&page)?.uid?;
        self.s.docs.iter().position(|d| d.uid == uid)
    }

    pub fn activate(&mut self, page: PageId) {
        let i = self.page_index(page).expect("page open");
        self.s.active = i;
    }

    pub fn first_page(&self) -> PageId {
        self.doc().page_order[0]
    }

    /// Handle of an item on `page`.
    pub fn handle_of(&self, page: PageId, item: ecad_model::ItemId) -> Handle {
        self.p().pages[&page].map.item(item).expect("item mapped")
    }

    /// Every handle (items and markers) of a drawn representation.
    pub fn rep_handles(&self, page: PageId, component: ComponentId) -> Vec<String> {
        let doc = self.doc();
        let p = self.p();
        let map = &p.pages[&page].map;
        let mut out = Vec::new();
        for (rid, r) in
            doc.representations_of(ecad_model::Subject::Component(component)).filter(|(_, r)| r.page == page)
        {
            out.extend(r.items.iter().filter_map(|i| map.item(*i)).map(|h| h.hex()));
            out.extend(r.connection_points.keys().filter_map(|t| map.marker(rid, *t)).map(|h| h.hex()));
        }
        out
    }

    pub fn route_handle(&self, page: PageId, c: ConductorId) -> Handle {
        let doc = self.doc();
        let (item, _) = ecad_model::derive::route_on_page(&doc, c, page).expect("route");
        self.handle_of(page, item)
    }

    pub fn route_ends(&self, page: PageId, c: ConductorId) -> (Pt, Pt) {
        let doc = self.doc();
        let (_, prim) = ecad_model::derive::route_on_page(&doc, c, page).expect("route");
        prim.path_ends().unwrap()
    }

    pub fn terminal_pos(&self, page: PageId, t: TerminalId) -> Pt {
        ecad_model::derive::terminal_positions(&self.doc(), page)
            .into_iter()
            .find(|(x, _, _)| *x == t)
            .map(|(_, _, p)| p)
            .expect("terminal shown")
    }

    pub fn lines_on(&self, page: PageId) -> Vec<Handle> {
        self.p()
            .entities_of(&self.s, page)
            .iter()
            .filter(|e| matches!(e.kind, EntityKind::Line(_) | EntityKind::LwPolyline(_)))
            .map(|e| e.handle)
            .collect()
    }

    /// Proposal §17.4, checks 1-6 (6 with `allow_mismatch` false).
    pub fn check(&self) {
        self.check_with(false);
    }

    pub fn check_with(&self, allow_mismatch: bool) {
        // Unsaved state as the engine reports it, asked while the project is not locked here.
        let unsaved_per_doc: Vec<(u64, bool)> =
            self.s.docs.iter().enumerate().map(|(i, d)| (d.uid, self.s.doc_unsaved(i))).collect();
        let p = self.p();
        let doc = p.ledger.document();
        // 2. replay equals the document.
        assert!(p.ledger.replay_matches().unwrap(), "replay differs from the live document");
        for (page, view) in &p.pages {
            let Some(uid) = view.uid else { continue };
            let Some(st) = self.s.docs.iter().find(|d| d.uid == uid) else { panic!("open page without a document") };
            if view.unavailable {
                continue;
            }
            // 1. projecting the document equals the engine drawing.
            let (d, map) = project_page(doc, *page, &st.doc, &view.map).unwrap();
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

    pub fn mapped(&self, page: PageId, h: Handle) -> Option<Mapped> {
        self.p().pages[&page].map.get(h)
    }

    pub fn symbol_item(&self, c: ComponentId) -> ecad_model::ItemId {
        let doc = self.doc();
        let (_, r) = doc.representations_of(ecad_model::Subject::Component(c)).next().unwrap();
        let item = r.items[0];
        assert!(matches!(doc.items[&item].kind, ItemKind::Symbol(_)));
        item
    }
}

pub fn pts(v: &[(f64, f64)]) -> Value {
    json!(v.iter().map(|(x, y)| json!([x, y])).collect::<Vec<_>>())
}
