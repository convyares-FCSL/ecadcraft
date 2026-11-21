//! Where a project is saved: the persistence modes, the in-memory store, and the save and branch plumbing.

use super::*;

/// Injected failures for the experiment (never set in normal use).
#[derive(Debug, Clone, Default)]
pub struct Faults {
    /// The next `n` projection attempts fail.
    pub fail_projections: u32,
    /// The next `n` cable view switches (`cableview` with a cable) fail before changing anything.
    pub fail_view_switches: u32,
    /// The next release fails at this step (brief 027's failure-injection tests): the step's own
    /// failure path runs, as when the disk or the save fails there.
    pub fail_release_at: Option<crate::release::Step>,
    /// The next commit callback panics (`fault-injection` feature only).
    #[cfg(feature = "fault-injection")]
    pub panic_in_commit: bool,
    /// The next ECAD command panics before committing (`fault-injection` feature only).
    #[cfg(feature = "fault-injection")]
    pub panic_in_command: bool,
    /// While set, the prompts and previews of the interactive forms panic (`fault-injection`
    /// feature only).
    #[cfg(feature = "fault-injection")]
    pub panic_in_form_preview: bool,
}

/// Whether a project can be saved, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persistence {
    /// Automated tests only: saving writes the real `.ecad` format (`ecad-file`) to a database
    /// in memory and marks the project clean. No file is written to disk.
    TestStore,
    /// No persistence: Save and Save As report that saving is unavailable, change nothing and
    /// never mark the project clean.
    Unavailable,
    /// Project files on disk (`.ecad`, persistence stage 2a): Save writes to the bound file,
    /// Save As (and the first save) writes a new one; see [`crate::file`].
    File,
}

/// The test-only project file: the real `.ecad` format (`ecad-file`), a redb database held in
/// memory. It is written on the first save; until then it is empty.
#[derive(Debug, Default)]
pub struct MemoryStore {
    /// The database's bytes (tests read them, or make the next write fail).
    pub(super) backend: MemoryBackend,
    /// The project in the database, once written.
    pub(super) project: Option<Box<ecad_file::ProjectV2>>,
}

impl MemoryStore {
    /// A copy of the database's bytes (empty before the first save): what a test reopens with
    /// [`Project::open_from`].
    pub fn bytes(&self) -> Vec<u8> {
        self.backend.bytes()
    }

    /// The database's size in bytes.
    pub fn byte_len(&self) -> usize {
        self.backend.byte_len()
    }

    /// Make the next write fail once (tests): the save in progress fails and nothing it wrote is
    /// acknowledged.
    pub fn fail_next_write(&self) {
        self.backend.fail_next_write();
    }

    /// The project in the database, once written.
    pub fn project(&self) -> Option<&ecad_file::ProjectV2> {
        self.project.as_deref()
    }

    /// Save: the first save writes the whole history (a failure leaves no database); later saves
    /// write the transactions after the database's head, in one transaction. Returns the persisted
    /// `seq`.
    pub(super) fn save(&mut self, project: ProjectId, ledger: &mut Ledger, writer: &str) -> Result<u64, String> {
        match &mut self.project {
            Some(p) => p.save(ledger).map_err(|e| e.to_string()).map(|_| ledger.persisted()),
            None => {
                let created = ecad_file::Store::with_backend(self.backend.clone()).map_err(|e| e.to_string()).and_then(|store| {
                    ecad_file::ProjectV2::create(store, project, ledger, now_ms(), writer, ecad_file::schema::DEFAULT_SNAPSHOT_EVERY)
                        .map_err(|e| e.to_string())
                });
                match created {
                    Ok((p, _)) => {
                        self.project = Some(Box::new(p));
                        ledger.mark_persisted(ledger.head()).map_err(|e| e.to_string())?;
                        Ok(ledger.persisted())
                    }
                    Err(e) => {
                        self.backend.clear();
                        Err(e)
                    }
                }
            }
        }
    }
}

/// What Save and Save As report when the project has no persistence.
pub const SAVE_UNAVAILABLE: &str = "Saving is not available in the ECAD desktop app: project persistence is not implemented. \
Nothing was written and the project is still unsaved. Closing it discards the design.";

/// A copy of the project's meaning taken when objects were put on the clipboard.
#[derive(Debug, Clone)]
pub struct ClipSnapshot {
    pub doc: Document,
    pub page: PageId,
    pub map: HandleMap,
}

impl Project {
    /// Where the project's file stands (`Persistence::File`).
    pub fn file_status(&self) -> crate::file::FileStatus {
        self.file.status()
    }

    /// Save now, as Ctrl+S does (brief 027's release, step 5), whatever the persistence: the file,
    /// the test store, or the refusal when there is none.
    ///
    /// # Errors
    /// The save failed, in words; the project stays as it is in memory.
    pub(crate) fn save_now(&mut self, pg: &mut Pages<'_>) -> Result<(), String> {
        match self.persistence {
            Persistence::File => self.save_file(pg, None, false).map(|_| ()).map_err(|e| e.to_string()),
            Persistence::Unavailable => Err(SAVE_UNAVAILABLE.into()),
            Persistence::TestStore => {
                let head = self.ledger.head();
                let r = self.store.save(self.id, &mut self.ledger, &writer());
                match &r {
                    Ok(n) => self.events.record(EventKind::Saved, Some("qsave"), head, None, format!("persisted through seq {n}")),
                    Err(e) => self.events.record(EventKind::SaveFailed, Some("qsave"), head, None, e.clone()),
                }
                self.sync_saved(pg);
                r.map(|_| ()).map_err(|e| format!("{} was not saved: {e}", self.name))
            }
        }
    }

    /// Save the pending transactions to the bound file (Save, as the branch actions of brief 020
    /// need before they write: owner answer 3, "Save and switch").
    ///
    /// # Errors
    /// No file, or the save failed (in words).
    pub(crate) fn save_pending(&mut self, pg: &mut Pages<'_>) -> Result<(), String> {
        if !self.unsaved() {
            return Ok(());
        }
        self.save_file(pg, None, false).map(|_| ()).map_err(|e| e.to_string())
    }

    /// Show another branch's state (design 020 §3.3): `ledger` replaces the project's, every view
    /// rebuilds once as on open, the active page stays if it exists on the new branch (else the
    /// first page in book order), and a read-only point view closes. Not a step.
    pub(crate) fn adopt_branch(&mut self, pg: &mut Pages<'_>, ledger: Ledger, tag_high: ecad_model::tag::HighWater) -> Result<(), String> {
        let seed = session_seed();
        let window = self.window();
        let doc = ledger.document();
        let target = self.active.filter(|p| doc.pages.contains_key(p)).or_else(|| ecad_model::book::Book::of(doc).order.first().copied());
        self.ids = IdAllocator::seeded_after(seed, ledger.log().last().map_or(0, |t| t.id.0) ^ u128::from(ledger.head()));
        self.baseline = ledger.head();
        let pages: Vec<PageId> = doc.page_order.clone();
        let cables: BTreeSet<CableId> = doc.cables.keys().copied().collect();
        self.ledger = ledger;
        self.derived.rebuild(self.ledger.document());
        self.tag_high = crate::devices::TagHigh::starting_at(tag_high);
        self.catch_up_tags();
        self.designation_uses.reset();
        self.conflicts.clear();
        self.views.retain(|c| cables.contains(c));
        self.checks = None;
        self.goto_ring = None;
        self.frame = None;
        self.frame_stale = false;
        self.pending_known = false;
        self.revision_view = None;
        self.clips.clear();
        self.last_placed = None;
        // Every change-driven copy (pages, devices, properties, checks) rebuilds once.
        self.changes.clear();
        self.change_count += 2;
        self.page_list_version += 1;
        let mut views: BTreeMap<PageId, PageView> = pages.iter().map(|p| (*p, PageView::default())).collect();
        if let Some((old, uid)) = window {
            self.oops.stale_doc(uid);
            if let Some(st) = pg.doc_mut(uid) {
                st.selection.clear();
                st.previous_selection.clear();
                st.undo.clear();
                st.redo.clear();
            }
            match target {
                Some(t) if t == old => {
                    // The same page: projected again, with fresh handles, in the same window.
                    views.insert(old, PageView { uid: Some(uid), ..PageView::default() });
                    self.pages = views;
                    if let Some(st) = pg.doc_mut(uid) {
                        st.doc = Arc::new(base_drawing());
                    }
                    self.refresh_page(pg, old, None, None, true);
                    if let Some(st) = pg.doc_mut(uid) {
                        st.title = self.page_title(old);
                    }
                }
                Some(t) => {
                    // Another page, shown in the same window; the old page's entry goes if the
                    // branch has no such page.
                    views.insert(old, PageView { uid: Some(uid), ..PageView::default() });
                    self.pages = views;
                    self.open_page(pg, t)?;
                    if !self.ledger.document().pages.contains_key(&old) {
                        self.pages.remove(&old);
                    }
                }
                None => {
                    self.pages = views;
                    self.active = None;
                }
            }
        } else {
            self.pages = views;
        }
        self.sync_saved(pg);
        let head = self.ledger.head();
        self.events.record(EventKind::ProjectOpened, Some("switchbranch"), head, None, "another branch shown");
        self.branch_gen += 1;
        Ok(())
    }

    /// How many branch actions this session has seen ([`Project`]'s `branch_gen`).
    pub fn branch_gen(&self) -> u64 {
        self.branch_gen
    }

    /// A branch action happened (brief 020a): every view of the branches refreshes.
    pub(crate) fn note_branch_change(&mut self) {
        self.branch_gen += 1;
    }

    /// Bind the project to the file at `path`: its name, and every open page's path and title.
    fn bind(&mut self, pg: &mut Pages<'_>, path: &str) {
        self.path = Some(path.to_string());
        self.name = file_stem(path);
        let pages: Vec<(PageId, u64)> = self.pages.iter().filter_map(|(p, v)| v.uid.map(|u| (*p, u))).collect();
        for (page, uid) in pages {
            let title = self.page_title(page);
            if let Some(st) = pg.doc_mut(uid) {
                st.path = self.path.clone();
                st.title = title;
            }
        }
    }

    /// Save or Save As to disk (`Persistence::File`).
    pub(super) fn save_file(&mut self, pg: &mut Pages<'_>, path: Option<&str>, save_as: bool) -> cadcraft_engine::Result<Value> {
        let head = self.ledger.head();
        let what = if save_as { "saveas" } else { "qsave" };
        let target = path.filter(|p| !p.trim().is_empty());
        let r: Result<(Option<String>, Option<String>), String> = match target {
            None if save_as => Err(crate::file::SAVE_AS_NEEDS_PATH.into()),
            None => self.file.save(&mut self.ledger).map(|()| (None, None)).map_err(|e| e.to_string()),
            Some(p) => crate::file::normalize(p).map_err(|e| e.to_string()).and_then(|p| {
                let w = self.file.save_as(&p, self.id, &mut self.ledger, &writer(), now_ms()).map_err(|e| e.to_string())?;
                Ok((
                    Some(p),
                    match w {
                        crate::file::Written::Confirmed => None,
                        crate::file::Written::Unconfirmed(m) => Some(m),
                    },
                ))
            }),
        };
        let r = match r {
            Ok((new_path, unconfirmed)) => {
                if let Some(p) = &new_path {
                    self.bind(pg, p);
                }
                match unconfirmed {
                    None => Ok(()),
                    Some(m) => Err(m),
                }
            }
            Err(e) => Err(e),
        };
        let shown = self.path.clone().unwrap_or_default();
        let result = match r {
            Ok(()) => {
                let n = self.ledger.persisted();
                self.events.record(EventKind::Saved, Some(what), head, None, format!("{shown} through transaction {n}"));
                Ok(json!({"saved": true, "persisted": n, "path": shown}))
            }
            Err(e) => {
                self.events.record(EventKind::SaveFailed, Some(what), head, None, e.clone());
                Err(EngineError::Other(format!("{} was not saved: {e}", self.name)))
            }
        };
        self.sync_saved(pg);
        result
    }

    /// Keep each open page's `saved` pointer in step with the project's unsaved state, so the
    /// engine's own pointer comparison agrees with SAVE-01.
    pub(crate) fn sync_saved(&self, pg: &mut Pages<'_>) {
        let unsaved = self.unsaved();
        for v in self.pages.values() {
            if let Some(st) = v.uid.and_then(|u| pg.doc_mut(u)) {
                st.saved = if unsaved { self.unsaved_marker.clone() } else { st.doc.clone() };
            }
        }
    }
}
