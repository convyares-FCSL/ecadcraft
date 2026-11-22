//! The project as the engine's document owner: drawing commits, undo and redo.

use crate::project::*;

/// Commands refused on ECAD pages before they start (owner decision, 2026-10-31, brief 007): the
/// Block Editor and in-place reference editing swap or rewrite the page drawing and its blocks
/// outside the commit gate, and APPLOAD and LISP evaluate LISP (see [`LISP_REFUSED`]).
pub const REFUSED_COMMANDS: &[&str] = &["bedit", "bsave", "bsaveas", "bclose", "refedit", "refset", "refclose", "appload", "lisp"];

/// Why LISP is refused on ECAD pages: its built-ins change the drawing outside any command, and a
/// routine keeps an undo step of its own.
pub const LISP_REFUSED: &str = "LISP is not available on ECAD pages: it changes the drawing outside the commands ECAD records";

impl DocumentOwner for Project {
    fn refuse_start(&self, _s: &Session, uid: u64, start: Start<'_>) -> Option<String> {
        self.page_of(uid)?;
        match start {
            Start::Lisp => Some(LISP_REFUSED.into()),
            Start::Command("appload" | "lisp") => Some(LISP_REFUSED.into()),
            Start::Command("oops") => self.page_of(uid).and_then(|page| self.oops.refuse_start(uid, page)),
            Start::Command(id) if REFUSED_COMMANDS.contains(&id) => {
                Some(format!("{} is not available on ECAD pages: it edits blocks outside the commands ECAD records", id.to_ascii_uppercase()))
            }
            Start::Command(_) => None,
        }
    }

    fn commit(&mut self, s: &mut Session, c: &Commit<'_>) -> Verdict {
        let pg = &mut Pages(s);
        #[cfg(feature = "fault-injection")]
        if std::mem::take(&mut self.faults.panic_in_commit) {
            crate::fault_panic();
        }
        let head = self.ledger.head();
        let Some(page) = self.page_of(c.uid) else { return Verdict::Accept };
        if let Some(clip) = c.clipboard
            && clip.uid == c.uid
        {
            self.clips.insert(
                clip.serial,
                ClipSnapshot { doc: self.ledger.document().clone(), page, map: self.pages.get(&page).map(|v| v.map.clone()).unwrap_or_default() },
            );
            while self.clips.len() > 8 {
                self.clips.pop_first();
            }
            self.events.record(EventKind::ClipboardCaptured, Some(c.command), head, Some(page), format!("{} objects", clip.handles.len()));
        }
        let Some(st) = pg.doc(c.uid) else { return Verdict::Accept };
        let now = st.doc.clone();
        let Some(view) = self.pages.get(&page) else { return Verdict::Accept };
        let Some(last) = view.last.clone() else { return Verdict::Accept };
        if Arc::ptr_eq(&now, &last) {
            return Verdict::Accept;
        }
        if view.unavailable {
            self.restore(pg, c.uid, page);
            return Verdict::Refuse(crate::commands::UNAVAILABLE.into());
        }
        // OOPS brings back only the plain drawing the last erase on this page removed (brief 023).
        if c.command == "oops" && !self.oops.allows(c.uid, page, &last, &now) {
            self.restore(pg, c.uid, page);
            self.events.record(EventKind::Refused, Some(c.command), head, Some(page), crate::translate::oops::REFUSED);
            return Verdict::Refuse(crate::translate::oops::REFUSED.into());
        }
        let stash = crate::translate::oops::STASH_COMMANDS.contains(&c.command);
        self.tag_high.catch_up(&self.ledger);
        let t = translate::translate(&mut translate::Ctx {
            tags: &mut self.tag_high,
            ledger: &mut self.ledger,
            page,
            map: &view.map,
            last: &last,
            now: &now,
            provenance: c.provenance,
            clips: &self.clips,
            ids: &mut self.ids,
            tol: self.tolerance,
        });
        let t = match t {
            Ok(t) => t,
            Err(m) => {
                self.restore(pg, c.uid, page);
                if stash {
                    self.oops.note_refused(c.uid);
                }
                self.events.record(EventKind::Refused, Some(c.command), head, Some(page), m.clone());
                return Verdict::Refuse(m);
            }
        };
        if stash && !t.ops.is_empty() {
            self.oops.note_accepted(c.uid, page, self.ledger.document(), &view.map, &t.ops);
        }
        if t.ops.is_empty() {
            // Only differences that are not recorded (header variables such as the current layer).
            // A new draw order (TEXTTOFRONT, DRAWORDER) is not recorded either: the page is drawn
            // in the projection's order again (brief 018 put derived texts on every page).
            let reordered = !now.model.iter().map(|e| e.handle).eq(last.model.iter().map(|e| e.handle));
            if let Some(v) = self.pages.get_mut(&page) {
                v.last = Some(now);
            }
            if reordered {
                self.refresh_page(pg, page, Some(c.command), None, false);
            }
            self.events.record(EventKind::NoChange, Some(c.command), head, Some(page), "");
            return Verdict::Accept;
        }
        let label = c.command.to_ascii_uppercase();
        // Nothing is drawn on a generated page (brief 024, design 024 §5.3).
        if let Some(m) = ecad_electrical::pages::generated::refusal(self.ledger.document(), &t.ops) {
            self.restore(pg, c.uid, page);
            self.events.record(EventKind::Refused, Some(c.command), head, Some(page), m.clone());
            return Verdict::Refuse(m);
        }
        // Brief 052d (design 052 §5.4): the moved device symbols' contexts before the move.
        let moved = ecad_electrical::symbols::proposals::before_move(self.ledger.document(), &t.ops);
        let pending = self.derived.begin(self.ledger.document(), &t.ops);
        // The numbering pass (brief 022) extends the same transaction.
        if let Err(e) = self.ledger.commit_extended(self.author.clone(), &label, t.ops, now_ms(), ecad_model::connectivity::wiring::assign) {
            self.restore(pg, c.uid, page);
            let m = format!("not recorded: {e}");
            self.events.record(EventKind::Refused, Some(c.command), head, Some(page), m.clone());
            return Verdict::Refuse(m);
        }
        self.events.record(EventKind::Accepted, Some(c.command), self.ledger.head(), Some(page), t.summary);
        // Accepted: from here on consistency is restored by projecting the document (PROJ-02).
        if let Some(v) = self.pages.get_mut(&page) {
            for (id, h) in &t.new_items {
                v.map.insert_item(*id, *h);
            }
            for ((r, term), h) in &t.new_markers {
                v.map.insert_marker(*r, *term, *h);
            }
        }
        self.refresh_page(pg, page, Some(c.command), Some((&now, &t.amended)), false);
        // A reference end on another page may be drawn from this page's objects (brief 010). Only
        // the active page is drawn (brief 016); the change log says which other pages changed,
        // for a preview of one of them.
        let changes = self.finish_derived(pending);
        self.apply_changes(pg, c.command, changes, Path::Interactive { page, notes: &t.notes });
        self.move_follow_up(pg, &moved, &label);
        self.sync_saved(pg);
        Verdict::Accept
    }

    fn undo(&mut self, s: &mut Session, _uid: u64) -> cadcraft_engine::Result<Option<String>> {
        self.history(&mut Pages(s), true)
    }

    fn redo(&mut self, s: &mut Session, _uid: u64) -> cadcraft_engine::Result<Option<String>> {
        self.history(&mut Pages(s), false)
    }

    fn save(&mut self, s: &mut Session, _uid: u64, path: Option<&str>, save_as: bool) -> cadcraft_engine::Result<Value> {
        let pg = &mut Pages(s);
        let head = self.ledger.head();
        if self.persistence == Persistence::File {
            return self.save_file(pg, path, save_as);
        }
        if self.persistence == Persistence::Unavailable {
            // Nothing is recorded as saved: no path, no store, no clean state.
            let what = if save_as { "saveas" } else { "qsave" };
            self.events.record(EventKind::SaveUnavailable, Some(what), head, None, SAVE_UNAVAILABLE);
            self.sync_saved(pg);
            return Err(EngineError::Other(SAVE_UNAVAILABLE.into()));
        }
        // The test store is in memory; the path is only recorded (no file is written), and only
        // once the save succeeded, so a failed Save As keeps the previous binding (§20.5).
        match self.store.save(self.id, &mut self.ledger, &writer()) {
            Ok(n) => {
                if let Some(p) = path {
                    self.path = Some(p.to_string());
                    for v in self.pages.values() {
                        if let Some(st) = v.uid.and_then(|u| pg.doc_mut(u)) {
                            st.path = self.path.clone();
                        }
                    }
                }
                self.events.record(EventKind::Saved, Some("qsave"), head, None, format!("persisted through seq {n}"));
                self.sync_saved(pg);
                Ok(json!({"saved": true, "persisted": n}))
            }
            Err(e) => {
                self.events.record(EventKind::SaveFailed, Some("qsave"), head, None, e.to_string());
                self.sync_saved(pg);
                Err(EngineError::Other(format!("{} was not saved: {e}", self.name)))
            }
        }
    }

    fn is_unsaved(&self) -> bool {
        self.unsaved()
    }

    fn close_needs_prompt(&self, _uid: u64) -> bool {
        // Closing the page window closes the project (experiment choice, proposal §18.4), which
        // prompts when unsaved; with `keep_open` (the desktop app, design 016 §4.1) it only hides the
        // page and the project stays open, so nothing is lost and nothing is asked.
        self.unsaved() && self.open_pages() <= 1 && !self.keep_open
    }

    fn closed(&mut self, s: &mut Session, uid: u64) -> bool {
        self.oops.forget(uid);
        if let Some(page) = self.page_of(uid) {
            // The page's view is remembered for when it is shown again (design 016 §4.1).
            if let Some(st) = s.docs.iter().find(|d| d.uid == uid) {
                self.page_views.insert(page, st.views.clone());
            }
            if let Some(v) = self.pages.get_mut(&page) {
                *v = PageView::default();
            }
            self.events.record(EventKind::PageClosed, Some("close"), self.ledger.head(), Some(page), "");
        }
        let done = self.open_pages() == 0 && !self.keep_open;
        if done {
            // The project is closed: let go of its file and writer lock.
            self.file.release();
        }
        done
    }

    fn drag_extra(&self, s: &Session, uid: u64, drag: &cadcraft_engine::Drag) -> Vec<cadcraft_engine::doc::EntityKind> {
        let Some(page) = self.page_of(uid) else { return Vec::new() };
        let Some(view) = self.pages.get(&page) else { return Vec::new() };
        if drag.clipboard {
            return Vec::new();
        }
        let doc = self.ledger.document();
        let dragged: BTreeSet<Handle> = drag.handles.iter().copied().collect();
        let mut moved: HashMap<ConnectionPointId, Pt> = HashMap::new();
        for (t, rep, at) in ecad_model::connectivity::derive::connection_point_positions(doc, page) {
            let carried = match doc.representations.get(&rep).map(|r| r.form) {
                Some(Form::Drawn) => view.map.marker(rep, t).is_some_and(|h| dragged.contains(&h)),
                Some(Form::Symbol) => {
                    doc.representations.get(&rep).and_then(|r| r.items.first()).and_then(|i| view.map.item(*i)).is_some_and(|h| dragged.contains(&h))
                }
                None => false,
            };
            if carried {
                let p = drag.matrix.apply(cadcraft_engine::geom::Vec2::new(at.x, at.y));
                moved.insert(t, Pt::new(p.x, p.y));
            }
        }
        let _ = s;
        if moved.is_empty() {
            return Vec::new();
        }
        self.following_routes(page, &|t| moved.get(&t).copied())
    }

    /// Terminal rings and wire-end marks over an open page (brief 008, design §4.1).
    /// Only while the Model tab is shown: the marks are at model positions, and a paper-space
    /// layout (also while working in one of its viewports) shows the page at another place and
    /// scale, or not at all.
    fn overlay(&self, s: &Session, uid: u64) -> Vec<cadcraft_engine::ext::Mark> {
        if s.layout_space() != cadcraft_engine::doc::Space::Model {
            return Vec::new();
        }
        crate::interact::overlay_marks(self, uid)
    }

    fn recover(&mut self, s: &mut Session) {
        let pg = &mut Pages(s);
        // Rebuild every open page from the authoritative document with fresh maps.
        for v in self.pages.values_mut() {
            v.map = HandleMap::default();
            v.last = None;
        }
        let pages: Vec<PageId> = self.pages.iter().filter(|(_, v)| v.uid.is_some()).map(|(p, _)| *p).collect();
        for page in pages {
            if let Some(uid) = self.pages.get(&page).and_then(|v| v.uid)
                && let Some(st) = pg.doc_mut(uid)
            {
                st.doc = Arc::new(base_drawing());
            }
            self.refresh_page(pg, page, None, None, true);
        }
        self.sync_saved(pg);
        self.events.record(EventKind::Recovered, None, self.ledger.head(), None, "owner callback failed; pages rebuilt from the document");
    }

    fn label(&self) -> String {
        self.name.clone()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Project {
    /// One undo, as `u` does (used by `undoissue` after its confirmation).
    pub(crate) fn undo_step(&mut self, pg: &mut Pages<'_>) -> Result<(), String> {
        self.history(pg, true).map(|_| ()).map_err(|e| e.to_string())
    }

    fn history(&mut self, pg: &mut Pages<'_>, undo: bool) -> cadcraft_engine::Result<Option<String>> {
        let command = if undo { "undo" } else { "redo" };
        self.oops.stale();
        // Brief 052d (tester 052d, finding 1): an undo or redo changes what the waiting
        // conflicts were about; they are left.
        self.conflicts.clear();
        if self.pages.values().any(|v| v.unavailable) {
            return Err(EngineError::Other("close and reopen the unavailable page first".into()));
        }
        // The ops the undo or redo will apply, scoped as a commit's are (brief 014).
        let ops = if undo { self.ledger.undo_ops() } else { self.ledger.redo_ops() }.unwrap_or_default();
        let order_before = self.ledger.document().page_order.clone();
        // Undoing an issued revision stops and asks first (owner answer Q7).
        if undo
            && let Some((rev, n)) = ecad_electrical::releases::revisions::issue_in(&ecad_electrical::releases::revisions::inverse(&ops))
            && !std::mem::take(&mut self.confirm_issue_undo)
        {
            let row = ecad_electrical::releases::revisions::list(self.ledger.document()).into_iter().find(|r| r.n == n);
            let date = row.as_ref().map(|r| r.get("date").to_string()).filter(|d| !d.is_empty());
            let rev = self.ledger.document().project.rev_scheme.show(rev);
            let mut q = crate::pages::commands::sheets::issue_undo_question(&rev, date.as_deref().unwrap_or_default());
            // Brief 027 (design 027 §5.2, Q10): undo never deletes files.
            if row.as_ref().is_some_and(|r| r.released.is_some()) {
                q.push_str(" Its release folder stays on disk.");
            }
            self.issue_undo_question = Some(q.clone());
            return Err(EngineError::Other(format!("{q} Use Undo Issue (`undoissue`) to confirm.")));
        }
        self.issue_undo_question = None;
        // Undoing a merge stops and asks first (design 020 §4.3).
        if undo
            && !std::mem::take(&mut self.confirm_merge_undo)
            && let Some(q) = crate::history::merging::undo_question(self)
        {
            self.merge_undo_question = Some(q.clone());
            return Err(EngineError::Other(format!("{} Use Undo Merge (`undomerge`) to confirm.", q.replace('\n', " "))));
        }
        self.merge_undo_question = None;
        let pending = self.derived.begin(self.ledger.document(), &ops);
        let r = if undo { self.ledger.undo_at(self.author.clone(), now_ms()) } else { self.ledger.redo_at(self.author.clone(), now_ms()) };
        match r {
            Ok(_) => {
                let label = self.ledger.log().last().map(|t| t.label.clone()).unwrap_or_default();
                let kind = if undo { EventKind::Undo } else { EventKind::Redo };
                self.events.record(kind, Some(command), self.ledger.head(), None, label.clone());
                let changes = self.finish_derived(pending);
                self.apply_changes(pg, command, changes, Path::History { order_before: &order_before });
                Ok(Some(label))
            }
            Err(ecad_model::ModelError::NothingTo(_)) => Ok(None),
            Err(e) => Err(EngineError::Other(e.to_string())),
        }
    }

    /// Brief 052d (design 052 §5.4, Q3): after a move, the moved symbols whose context changed
    /// propose it. Unset devices take it in one follow-up transaction (`u` undoes it first, then
    /// the move); devices with another value wait in [`Project::conflicts`] (Leave by default).
    fn move_follow_up(&mut self, pg: &mut Pages<'_>, moved: &[ecad_electrical::symbols::proposals::Moved], label: &str) {
        if moved.is_empty() {
            return;
        }
        let doc = self.ledger.document();
        let planned = match ecad_electrical::symbols::proposals::after_move(doc, &mut self.ids, moved) {
            Ok(p) => p,
            Err(e) => {
                pg.echo(e);
                return;
            }
        };
        let words = ecad_electrical::symbols::proposals::conflict_words(doc, &planned.conflicts);
        let conflicts = planned.conflicts.clone();
        if !planned.ops.is_empty() {
            let follow = ecad_electrical::symbols::proposals::follow_up_label(&planned);
            let set: Vec<String> =
                planned.set.iter().map(|(c, _, v)| format!("{} is now in {v}", ecad_electrical::devices::tags::tag_words(doc, *c))).collect();
            match self.apply_edit(pg, "move", &follow, planned.ops) {
                Ok(_) => pg.echo(format!("{label}; {}.", set.join("; "))),
                Err(e) => pg.echo(format!("Not set ({e}).")),
            }
        }
        self.conflicts = conflicts;
        for w in words {
            pg.echo(w);
        }
    }
}
