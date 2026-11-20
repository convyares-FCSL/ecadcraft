//! The commit paths' shared half: applying edits, the change log, and refreshing and redrawing pages.

use super::*;

use ecad_electrical::derived::Changes;
use ecad_model::view::PageSet;

/// How many recent changes [`Project::changes_since`] can answer for.
const CHANGE_LOG: usize = 64;

/// One change the project went through (a transaction, an undo or redo, a view switch): its
/// number and the pages whose drawing it could change (`None`: any page). Session only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub n: u64,
    pub pages: Option<BTreeSet<PageId>>,
    /// What it could change in the Devices list (the Devices feed, `ecad_electrical::derived::DevicesFeed`).
    pub devices: crate::device_list::DeviceTouch,
    /// The objects whose Properties rows it could change ([`crate::props::touched`]); `None`: any.
    pub props: Option<BTreeSet<ecad_model::ObjectRef>>,
    /// Whether it could change the Pages tree (the tree feed, `ecad_electrical::derived::TreeFeed`).
    pub tree: bool,
}

impl Project {
    /// A number that changes exactly when [`Project::page_rows`] could differ (update map of the
    /// Project panel's rows, brief 016): a page added or removed (rename and move are a remove and
    /// an add, [`crate::device_list::DeviceTouch::pages`]), or a page becoming available or
    /// unavailable. Every other change (drawing edits, devices, undo or redo of those, view
    /// switches) leaves it alone, so the panel keeps its rows.
    pub fn page_list_version(&self) -> u64 {
        self.page_list_version
    }

    /// The kept connector index (brief 044, design 044 §11.1), kept by the views' pipeline
    /// (`ecad_electrical::derived::ConnectorsView`).
    pub fn connectors(&self) -> &crate::connectors::Index {
        self.derived.connectors()
    }

    /// The derived views, to put a kept view out of step on purpose (tests of verify mode only).
    #[cfg(feature = "test-support")]
    pub fn views_mut(&mut self) -> &mut ecad_electrical::derived::Views {
        &mut self.derived
    }

    /// Whether the kept frame is marked to be drawn again (tests of the frame's map only).
    #[cfg(feature = "test-support")]
    pub fn frame_is_stale(&self) -> bool {
        self.frame_stale
    }

    /// Number of the last change the project went through (a transaction, an undo or redo, a view
    /// switch); 0 before the first. Cheap: callers compare it every frame.
    pub fn change_count(&self) -> u64 {
        self.change_count
    }

    /// The pages whose drawing the changes after change `n` could have changed: `Some(empty)`
    /// when none, `None` when any page could have changed or the changes are too old to tell.
    pub fn changes_since(&self, n: u64) -> Option<BTreeSet<PageId>> {
        if n >= self.change_count {
            return Some(BTreeSet::new());
        }
        if self.changes.front().is_none_or(|c| c.n > n + 1) {
            return None;
        }
        let mut out = BTreeSet::new();
        for c in self.changes.iter().filter(|c| c.n > n) {
            out.extend(c.pages.as_ref()?.iter().copied());
        }
        Some(out)
    }

    /// What the changes after change `n` could change in the Devices list, together; `None` when
    /// the changes are too old to tell (rebuild the list).
    pub fn device_changes_since(&self, n: u64) -> Option<crate::device_list::DeviceTouch> {
        let mut out = crate::device_list::DeviceTouch::default();
        if n >= self.change_count {
            return Some(out);
        }
        if self.changes.front().is_none_or(|c| c.n > n + 1) {
            return None;
        }
        for c in self.changes.iter().filter(|c| c.n > n) {
            out.devices.extend(c.devices.devices.iter().copied());
            out.pages |= c.devices.pages;
            out.settings |= c.devices.settings;
        }
        Some(out)
    }

    /// The objects whose Properties rows the changes after change `n` could have changed, together;
    /// `None` when any could have, or the changes are too old to tell (rebuild the copy).
    pub fn props_changes_since(&self, n: u64) -> Option<BTreeSet<ecad_model::ObjectRef>> {
        if n >= self.change_count {
            return Some(BTreeSet::new());
        }
        if self.changes.front().is_none_or(|c| c.n > n + 1) {
            return None;
        }
        let mut out = BTreeSet::new();
        for c in self.changes.iter().filter(|c| c.n > n) {
            out.extend(c.props.as_ref()?.iter().copied());
        }
        Some(out)
    }

    pub(super) fn record_change(
        &mut self, pages: Option<BTreeSet<PageId>>, devices: crate::device_list::DeviceTouch, props: Option<BTreeSet<ecad_model::ObjectRef>>,
        tree: bool,
    ) {
        self.goto_ring = None;
        self.refresh_pending();
        self.tag_high.catch_up(&self.ledger);
        if devices.pages || tree {
            self.page_list_version += 1;
        }
        self.change_count += 1;
        self.changes.push_back(Change { n: self.change_count, pages, devices, props, tree });
        while self.changes.len() > CHANGE_LOG {
            self.changes.pop_front();
        }
    }

    /// Bring every open page's drawing and map in line with the document (handles kept where
    /// objects remain). Pages that fail are rebuilt from scratch; if that fails too, the page view
    /// becomes unavailable (PROJ-02).
    pub(crate) fn refresh_pages(&mut self, pg: &mut Pages<'_>, command: Option<&str>) {
        // An unavailable page stays a placeholder until it is closed and reopened.
        let pages: Vec<PageId> = self.pages.iter().filter(|(_, v)| v.uid.is_some() && !v.unavailable).map(|(p, _)| *p).collect();
        for page in pages {
            self.refresh_page(pg, page, command, None, false);
        }
        self.sync_saved(pg);
    }

    /// Project one page; `committed` is the engine's drawing (with its new handles already in the
    /// map) after a CADCraft commit, used as the base and for the normal-path fidelity check.
    /// `recovery` marks a reprojection with fresh handles (owner recovery); it and any rebuild are
    /// checked by [`verify_page`] instead, which does not rely on handle numbers.
    pub(crate) fn refresh_page(
        &mut self, pg: &mut Pages<'_>, page: PageId, command: Option<&str>, committed: Option<(&Arc<Drawing>, &BTreeSet<Handle>)>, recovery: bool,
    ) {
        // One sheet for the projection, any rebuild and the check (see `look::pin_sheet`).
        let _sheet = crate::look::pin_sheet();
        let Some(view) = self.pages.get(&page) else { return };
        let Some(uid) = view.uid else { return };
        let Some(current) = pg.doc(uid).map(|st| st.doc.clone()) else { return };
        let current = Some(current);
        let base = committed.map(|(d, _)| d.clone()).or(current).unwrap_or_else(|| Arc::new(base_drawing()));
        let prev = view.map.clone();
        let head = self.ledger.head();
        self.projections += 1;
        let first = if self.take_projection_fault() { Err("injected projection failure".to_string()) } else { self.project(page, &base, &prev) };
        // Only a normal projection keeps the engine's handles, so only then is fidelity comparable.
        let normal = first.is_ok();
        let result = match first {
            Ok(r) => Ok(r),
            Err(e) => {
                self.events.record(EventKind::ProjectionFailed, command, head, Some(page), e);
                let rebuilt = if self.take_projection_fault() {
                    Err("injected rebuild failure".to_string())
                } else {
                    // Rebuild from scratch: fresh handles, nothing trusted from the failed state.
                    self.project(page, &base_drawing(), &HandleMap::default())
                };
                match &rebuilt {
                    Ok(_) => self.events.record(EventKind::ProjectionRebuilt, command, head, Some(page), "drawing and map rebuilt from the document"),
                    Err(e) => self.events.record(EventKind::PageUnavailable, command, head, Some(page), e.clone()),
                }
                rebuilt
            }
        };
        // Recovery path: fresh handles, so the check is by content against the document.
        let result = match result {
            Ok((d, map)) if recovery || !normal => {
                let v = crate::verify::verify_page_pending(
                    self.ledger.document(),
                    page,
                    &d,
                    &map,
                    &self.views,
                    self.tolerance,
                    self.show_hidden_text,
                    self.pending,
                );
                if v.ok() {
                    self.events.record(EventKind::RecoveryVerified, command, head, Some(page), v.summary());
                    Ok((d, map))
                } else {
                    self.events.record(EventKind::RecoveryVerificationFailed, command, head, Some(page), v.summary());
                    self.events.record(EventKind::PageUnavailable, command, head, Some(page), "rebuilt page did not verify");
                    Err(v.summary())
                }
            }
            r => r,
        };
        let title = self.page_title(page);
        let Some(st) = pg.doc_mut(uid) else { return };
        match result {
            Ok((d, map)) => {
                if let Some((committed, amended)) = committed.filter(|_| normal) {
                    // Fan lines are recomputed from the document (a terminal moved by M2 moves
                    // them), so they are not compared with what the engine committed.
                    let mut amended = amended.clone();
                    amended.extend(prev.fan_handles().chain(map.fan_handles()));
                    // Reference texts are derived too (brief 010): moving a reference point or a
                    // member route moves or rewrites them.
                    amended.extend(prev.ref_text_handles().chain(map.ref_text_handles()));
                    // Tag texts and terminal labels follow their symbols (brief 018).
                    amended.extend(prev.tag_text_handles().chain(map.tag_text_handles()));
                    // Wire labels follow their routes and nets (brief 022).
                    amended.extend(prev.label_handles().chain(map.label_handles()));
                    // Contact texts and mirror tables follow their device (brief 025).
                    amended.extend(prev.xref_handles().chain(map.xref_handles()));
                    let mismatched = fidelity_mismatches(committed, &d, &amended);
                    if !mismatched.is_empty() {
                        self.events.record(
                            EventKind::FidelityMismatch,
                            command,
                            head,
                            Some(page),
                            format!("{} objects differ from the projection; the projection was kept", mismatched.len()),
                        );
                    }
                }
                let changed = st.doc.model != d.model || st.doc.groups != d.groups || st.doc.blocks != d.blocks || st.doc.layers != d.layers;
                if changed {
                    let keep: BTreeSet<Handle> = map.handles().collect();
                    st.selection.retain(|h| keep.contains(h));
                    st.doc = Arc::new(d);
                    st.revision += 1;
                }
                st.title = title;
                if let Some(v) = self.pages.get_mut(&page) {
                    v.map = map;
                    v.last = Some(st.doc.clone());
                    if v.unavailable {
                        v.unavailable = false;
                        self.page_list_version += 1;
                    }
                }
            }
            Err(_) => {
                st.doc = Arc::new(placeholder_drawing());
                st.title = format!("{title}{UNAVAILABLE_SUFFIX}");
                st.selection.clear();
                st.revision += 1;
                if let Some(v) = self.pages.get_mut(&page) {
                    v.map = HandleMap::default();
                    v.last = Some(st.doc.clone());
                    if !v.unavailable {
                        v.unavailable = true;
                        self.page_list_version += 1;
                    }
                }
                pg.echo("This page could not be drawn; it is unavailable until it is closed and reopened. The project is intact and can be saved.");
            }
        }
    }

    /// After a transaction that is not a drawing commit (an ECAD command's JSON form, undo, redo)
    /// or a view switch: redraw the active page, the only page drawn (brief 016). Other pages are
    /// projected when they become active, so nothing else is marked or redrawn.
    pub(crate) fn redraw_window(&mut self, pg: &mut Pages<'_>, command: Option<&str>) {
        if let Some(page) = self.active_page().filter(|p| self.pages.get(p).is_some_and(|v| !v.unavailable)) {
            self.refresh_page(pg, page, command, None, false);
        }
        self.sync_saved(pg);
    }

    /// After a commit, undo or redo whose touched pages are `pages` (`None`: any page): redraw the
    /// active page only when the change could alter it (overseer item O1, design ui-architecture
    /// §8.4). The touched set names the page's own objects, its references, its numbered wires and
    /// the generated pages whose content changed ([`Project::generated_touched`]); the kept frame
    /// (title block, Total, the pending Rev box) has its own update map, `frame_stale`. A frame
    /// kept for another page means the active page's frame is not kept, so it is drawn.
    pub(super) fn redraw_touched(&mut self, pg: &mut Pages<'_>, command: Option<&str>, pages: Option<&BTreeSet<PageId>>) {
        let active = self.active_page();
        let frame_other = self.frame.as_ref().is_none_or(|(p, _)| Some(*p) != active);
        let touched = match (pages, active) {
            (Some(set), Some(a)) => set.contains(&a),
            _ => true,
        };
        if touched || self.frame_stale || frame_other {
            self.redraw_window(pg, command);
        } else {
            self.sync_saved(pg);
        }
    }

    /// Commit ECAD-made ops (Path A) and refresh the open pages.
    ///
    /// # Errors
    /// Validation failure; nothing changes.
    pub(crate) fn apply_edit(&mut self, pg: &mut Pages<'_>, command: &str, label: &str, ops: Vec<Op>) -> Result<ecad_model::TxId, CmdError> {
        self.apply_edit_with(pg, command, label, ops, true)
    }

    /// [`Project::apply_edit`], with the numbering pass (brief 022) only when `numbering`: a
    /// Restore (brief 020) records exactly the state it returns to, wire numbers included.
    pub(crate) fn apply_edit_with(
        &mut self, pg: &mut Pages<'_>, command: &str, label: &str, ops: Vec<Op>, numbering: bool,
    ) -> Result<ecad_model::TxId, CmdError> {
        self.apply_tx(pg, command, label, ops, numbering, None).map_err(CmdError::refused)
    }

    /// Commit a merge (brief 020b): `ops` as one transaction of `kind` (a merge), updated in the
    /// views as any commit (their update maps; no special path).
    ///
    /// # Errors
    /// As [`Project::apply_edit`].
    pub(crate) fn apply_merge(
        &mut self, pg: &mut Pages<'_>, label: &str, kind: ecad_model::TxKind, ops: Vec<Op>,
    ) -> Result<ecad_model::TxId, String> {
        self.apply_tx(pg, "merge", label, ops, false, Some(kind))
    }

    fn apply_tx(
        &mut self, pg: &mut Pages<'_>, command: &str, label: &str, ops: Vec<Op>, numbering: bool, kind: Option<ecad_model::TxKind>,
    ) -> Result<ecad_model::TxId, String> {
        if pg.command_running() {
            return Err("finish or cancel the current command first (TX-05)".into());
        }
        // Brief 052d (design 052 §5.5): any other edit is a Leave of the conflicts waiting.
        self.conflicts.clear();
        // Nothing is drawn on a generated page (brief 024, design 024 §5.3).
        if let Some(m) = crate::generated::refusal(self.ledger.document(), &ops) {
            return Err(m);
        }
        // Brief 056 (design 056 §8): the first rack brings the book's rack overview page with it,
        // in the same transaction (not for a merge or a Restore, which record exactly the state
        // they bring back).
        let mut ops = ops;
        let rack_page = if kind.is_none() && numbering {
            let (id, more) = crate::cmd::plc::page::rack_page_ops(self, &ops).map_err(String::from)?;
            ops.extend(more);
            id
        } else {
            None
        };
        #[cfg(feature = "fault-injection")]
        if std::mem::take(&mut self.faults.panic_in_command) {
            crate::fault_panic();
        }
        // References are derived for the cables and devices the ops could touch only (briefs 014,
        // 025).
        let reuse = self.xref_reuse();
        let watch = crate::xref::Watch::take(self.ledger.document(), &self.xref, &ops, reuse, &crate::draw::text_style::EngineMetrics);
        let pages = pages_touched(self.ledger.document(), &ops, self.outputs.as_ref().map(|o| &o.plc));
        let props = crate::props::touched(self.ledger.document(), &ops);
        let checks = self.checks.as_ref().map(|c| c.scope(self.ledger.document(), &ops));
        let outputs = self.outputs.as_ref().map(|o| o.scope(self.ledger.document(), &ops));
        let n = ops.len();
        // The numbering pass (brief 022) extends the same transaction.
        let pass = |d: &ecad_model::Document, o: &[Op]| if numbering { ecad_model::wiring::assign(d, o) } else { Vec::new() };
        let pending = self.derived.begin(self.ledger.document(), &ops);
        let committed = match kind {
            Some(k) => self.ledger.commit_merge(self.author.clone(), label, k, ops, now_ms()),
            None => self.ledger.commit_extended(self.author.clone(), label, ops, now_ms(), pass),
        };
        let tx = committed.map_err(|e| {
            self.events.record(EventKind::Refused, Some(command), self.ledger.head(), None, e.to_string());
            e.to_string()
        })?;
        self.events.record(EventKind::Accepted, Some(command), self.ledger.head(), None, label);
        if let Some(id) = rack_page {
            self.pages.entry(id).or_default();
        }
        let refs = self.finish_watch(watch);
        let numbered = self.numbered_pages(n);
        let props = self.with_numbered(props, n);
        self.update_checks(checks);
        let generated = self.update_outputs(outputs);
        let generated = Some(self.generated_touched(generated));
        let pages = touched(touched(touched(pages, refs.pages), numbered), generated);
        let mut changes = self.finish_derived(pending);
        pass_through(&mut changes, Old { pages, named: BTreeSet::new(), props, echo: refs.orphaned.into_iter().collect() });
        self.apply_changes(pg, command, changes, Path::Command);
        Ok(tx)
    }

    /// The After phase and the views' updates on the transaction just committed (design 064 §5).
    pub(super) fn finish_derived(&mut self, pending: ecad_electrical::derived::Pending) -> Changes {
        let ops = self.ledger.log().last().map(|t| t.ops.as_slice()).unwrap_or_default();
        self.derived.finish(self.ledger.document(), ops, pending)
    }

    /// What follows every commit, on all three paths (design 064 §5): the change log entry, the
    /// redraw the path calls for, and the lines for the command line.
    pub(super) fn apply_changes(&mut self, pg: &mut Pages<'_>, command: &str, changes: Changes, path: Path<'_>) {
        let Changes { mut pages, named, devices, props, list, frame_stale, echo, problems } = changes;
        // Verify mode: a kept view that differed from a fresh build (it was rebuilt).
        let head = self.ledger.head();
        for line in problems {
            self.events.record(EventKind::IndexRebuilt, None, head, None, line);
        }
        if let Path::Interactive { page } = path {
            // The edited page was drawn by the commit itself.
            pages.insert(page);
        }
        if frame_stale {
            self.frame_stale = true;
        }
        let pages = pages.into_option();
        // Which page an undo or redo brings to the front is read before the change is recorded.
        let front = match path {
            Path::History { order_before } => {
                let was_active = self.pages.iter().find(|(_, v)| v.uid.is_some()).map(|(p, _)| *p);
                let target = front_after_history(order_before, &self.ledger.document().page_order, Some(&named), self.active_page());
                Some((was_active, target))
            }
            Path::Command | Path::Interactive { .. } => None,
        };
        self.record_change(pages.clone(), devices, props, list);
        match (path, front) {
            (Path::Command, _) => self.redraw_touched(pg, Some(command), pages.as_ref()),
            (Path::Interactive { .. }, _) => {}
            (Path::History { .. }, Some((was_active, Some(page)))) if self.open_page(pg, page).is_ok() => {
                // The page the undo or redo changed comes to the front, so the change is seen
                // (brief 016, AC8). A page that cannot be shown leaves the active page as it is.
                // A page already in the window was only brought to the front: draw it.
                if was_active == Some(page) {
                    self.redraw_window(pg, Some(command));
                }
                let words = crate::references::page_words(self.ledger.document(), page);
                pg.echo(format!("{} brought to the front.", words.replacen("page", "Page", 1)));
            }
            (Path::History { .. }, _) => self.redraw_touched(pg, Some(command), pages.as_ref()),
        }
        for line in echo {
            pg.echo(line);
        }
    }

    /// Finish a commit's [`crate::xref::Watch`] with the last transaction: the reference index is
    /// brought up to date and the pages whose references changed are named.
    pub(super) fn finish_watch(&mut self, watch: crate::xref::Watch) -> crate::xref::Outcome {
        let ledger = &self.ledger;
        let ops = ledger.log().last().map(|t| t.ops.as_slice()).unwrap_or_default();
        let mut out = watch.finish(ledger.document(), &mut self.xref, ops, &crate::draw::text_style::EngineMetrics);
        let head = ledger.head();
        for line in std::mem::take(&mut out.problems) {
            self.events.record(EventKind::IndexRebuilt, None, head, None, line);
        }
        self.xref_shown = Some((self.state_key(), std::mem::take(&mut out.shown)));
        out
    }

    /// The ledger's state: its head and last transaction ID (unique, so a state reached again
    /// by another route never matches).
    fn state_key(&self) -> (u64, u128) {
        (self.ledger.head(), self.ledger.log().last().map_or(0, |t| t.id.0))
    }

    /// What the last commit derived, when the state has not moved since (brief 025: a device
    /// edited twice in a row is derived once per commit, not twice).
    pub(super) fn xref_reuse(&mut self) -> crate::xref::Shown {
        match self.xref_shown.take() {
            Some((key, shown)) if key == self.state_key() => shown,
            _ => crate::xref::Shown::new(),
        }
    }

    /// `props` with the wires the extension of the last transaction (its ops after the first `n`)
    /// made, removed, changed or renumbered: the wire rewrite (design 072 §4.4) and the numbering
    /// pass (brief 022); the Properties view of a wire reads its number, net and ends.
    pub(super) fn with_numbered(&self, props: Option<BTreeSet<ecad_model::ObjectRef>>, n: usize) -> Option<BTreeSet<ecad_model::ObjectRef>> {
        let mut props = props?;
        let extra = self.ledger.log().last().map(|t| t.ops.get(n..).unwrap_or_default()).unwrap_or_default();
        let doc = self.ledger.document();
        for op in extra {
            match op {
                Op::SetProperty { object, .. } => {
                    props.insert(*object);
                }
                Op::CreateWire { id, wire } | Op::DeleteWire { id, wire } | Op::ModifyWire { id, after: wire, .. } => {
                    props.insert(ecad_model::ObjectRef::Wire(*id));
                    // The devices at its ends show its net (brief 022).
                    props.extend(wire.ends.iter().filter_map(|t| doc.terminals.get(t)).map(|t| ecad_model::ObjectRef::Device(t.component)));
                }
                _ => {}
            }
        }
        Some(props)
    }

    /// The pages whose labels the extension of the last transaction changed: the pages of the
    /// wires its ops (after the first `n`, the command's own) made, removed or renumbered (the
    /// wire rewrite, design 072 §4.4, and the numbering pass, brief 022).
    pub(super) fn numbered_pages(&self, n: usize) -> Option<BTreeSet<PageId>> {
        let doc = self.ledger.document();
        let extra = self.ledger.log().last().map(|t| t.ops.get(n..).unwrap_or_default()).unwrap_or_default();
        let mut out = BTreeSet::new();
        for op in extra {
            match op {
                Op::SetProperty { object: ecad_model::ObjectRef::Wire(c), .. } | Op::ModifyWire { id: c, .. } => {
                    out.extend(crate::wires::wire_pages(doc, *c));
                }
                // The rewrite's wires (design 072 §4.4): labelled on the routes at their ends.
                Op::CreateWire { wire, .. } | Op::DeleteWire { wire, .. } => out.extend(super::points_pages(doc, &wire.ends)),
                _ => {}
            }
        }
        Some(out)
    }
}

/// Which commit path a change came from: what [`Project::apply_changes`] redraws.
#[derive(Debug, Clone, Copy)]
pub(super) enum Path<'a> {
    /// A command's transaction, a merge or a Restore: the touched pages on screen are drawn.
    Command,
    /// A drawing commit on `page`, which the commit drew itself.
    Interactive { page: PageId },
    /// An undo or redo: a page it changed may come to the front. `order_before` is the page order
    /// before it.
    History { order_before: &'a [PageId] },
}

/// What the functions not yet moved into a view worked out (design 064 §9: each view step deletes
/// one of them and its field here).
pub(super) struct Old {
    pub pages: Option<BTreeSet<PageId>>,
    pub named: BTreeSet<PageId>,
    pub props: Option<BTreeSet<ecad_model::ObjectRef>>,
    pub echo: Vec<String>,
}

/// Add `old` to the views' `changes`.
pub(super) fn pass_through(changes: &mut Changes, old: Old) {
    changes.pages.union(PageSet::from_option(old.pages));
    changes.named.extend(old.named);
    changes.props = match (changes.props.take(), old.props) {
        (Some(mut a), Some(b)) => {
            a.extend(b);
            Some(a)
        }
        (None, _) | (_, None) => None,
    };
    changes.echo.extend(old.echo);
}

/// Handles whose entity differs between the engine's committed drawing and the projection, other
/// than those the owner amended on purpose.
fn fidelity_mismatches(committed: &Drawing, projected: &Drawing, amended: &BTreeSet<Handle>) -> Vec<Handle> {
    let a: HashMap<Handle, &Arc<Entity>> = committed.model.iter().map(|e| (e.handle, e)).collect();
    let b: HashMap<Handle, &Arc<Entity>> = projected.model.iter().map(|e| (e.handle, e)).collect();
    let mut out: Vec<Handle> = a.iter().filter(|(h, e)| !amended.contains(h) && b.get(h).is_none_or(|x| x != *e)).map(|(h, _)| *h).collect();
    out.extend(b.keys().filter(|h| !a.contains_key(h) && !amended.contains(h)));
    out.sort();
    out
}

/// The page an undo or redo brings to the front (design 016 §4.3), or `None` to stay: a page it
/// adds; the page before the active page when it removes the active page (else the first page);
/// otherwise, when it changed pages that do not include the active page, the first of them in
/// page order. Rename and move (a page removed and added again) stay; so does a change that could
/// be on any page (`touched` `None`), and reference texts changing elsewhere never switch.
pub(crate) fn front_after_history(before: &[PageId], after: &[PageId], touched: Option<&BTreeSet<PageId>>, active: Option<PageId>) -> Option<PageId> {
    if let Some(added) = after.iter().find(|p| !before.contains(p)) {
        return Some(*added);
    }
    if let Some(a) = active
        && !after.contains(&a)
    {
        let at = before.iter().position(|p| *p == a).unwrap_or(0);
        let earlier = before.get(..at).unwrap_or_default().iter().rev().find(|p| after.contains(p));
        return earlier.or(after.first()).copied();
    }
    let touched = touched?;
    if active.is_some_and(|a| touched.contains(&a)) {
        return None;
    }
    after.iter().find(|p| touched.contains(p)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Design 016 section 4.3, table-driven: which page an undo or redo brings to the front.
    #[test]
    fn the_page_an_undo_or_redo_brings_to_the_front() {
        let [a, b, c] = [PageId(1), PageId(2), PageId(3)];
        let set = |p: &[PageId]| p.iter().copied().collect::<BTreeSet<_>>();
        let cases: &[(&str, &[PageId], &[PageId], Option<BTreeSet<PageId>>, Option<PageId>, Option<PageId>)] = &[
            ("edit on the active page stays", &[a, b], &[a, b], Some(set(&[a])), Some(a), None),
            ("edit on another page switches", &[a, b], &[a, b], Some(set(&[b])), Some(a), Some(b)),
            ("first touched in page order", &[a, b, c], &[a, b, c], Some(set(&[c, b])), Some(a), Some(b)),
            ("nothing drawn changed stays", &[a, b], &[a, b], Some(set(&[])), Some(a), None),
            ("unknown pages stay", &[a, b], &[a, b], None, Some(a), None),
            ("a page added becomes active", &[a, b], &[a, b, c], Some(set(&[])), Some(a), Some(c)),
            ("active removed: the page before", &[a, b, c], &[a, b], Some(set(&[])), Some(c), Some(b)),
            ("active first removed: the first", &[a, b], &[b], Some(set(&[])), Some(a), Some(b)),
            ("rename or move stays", &[a, b], &[b, a], Some(set(&[])), Some(a), None),
            ("no window: touched page", &[a, b], &[a, b], Some(set(&[b])), None, Some(b)),
        ];
        for (what, before, after, touched, active, want) in cases {
            assert_eq!(front_after_history(before, after, touched.as_ref(), *active), *want, "{what}");
        }
    }
}
