//! Creating and opening a project.

use crate::project::*;

impl Project {
    /// Create a project with one page, open that page in `s`, and attach the project as its owner.
    ///
    /// # Errors
    /// Projection failure (not expected for an empty project).
    pub fn create(s: &mut Session, name: &str, seed: u64) -> Result<ProjectHandle, String> {
        Self::create_with(s, name, seed, Persistence::TestStore)
    }

    /// [`Project::create`] with a chosen [`Persistence`].
    ///
    /// # Errors
    /// Projection failure (not expected for an empty project).
    pub fn create_with(s: &mut Session, name: &str, seed: u64, persistence: Persistence) -> Result<ProjectHandle, String> {
        let mut ids = IdAllocator::seeded(seed);
        let mut ledger = Ledger::new(seed.rotate_left(32));
        let (page, layer): (PageId, LayerId) = (ids.fresh(), ids.fresh());
        ledger
            .commit_at(
                Author::User,
                "New project",
                vec![
                    // The first page is numbered 1: stored, so moving pages never
                    // renumbers it.
                    Op::AddPage { id: page, page: first_page().map_err(|e| e.to_string())?, index: 0 },
                    Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
                    // The length unit is recorded at creation.
                    Op::SetProjectSettings {
                        before: ProjectSettings::default(),
                        after: ProjectSettings { length_unit: LengthUnit::Millimetre, ..ProjectSettings::default() },
                    },
                ],
                now_ms(),
            )
            .map_err(|e| e.to_string())?;
        let baseline = ledger.head();
        let id: ProjectId = ids.fresh();
        let derived = views_for(ledger.document());
        let project = Arc::new(Mutex::new(Project {
            name: name.to_string(),
            path: None,
            ledger,
            ids,
            id,
            pages: BTreeMap::new(),
            events: EventLog::default(),
            clips: BTreeMap::new(),
            oops: crate::translate::oops::Records::default(),
            faults: Faults::default(),
            tolerance: 1e-6,
            default_layer: layer,
            me: Weak::new(),
            conflicts: Vec::new(),
            merge_cache: None,
            derived,
            storage: super::parts::Storage {
                store: MemoryStore::default(),
                file: ecad_file::binding::Binding::default(),
                open_report: None,
                persistence,
                baseline,
                unsaved_marker: Arc::new(Drawing::new_metric()),
            },
            display: super::parts::Display {
                views: BTreeSet::new(),
                show_hidden_text: false,
                view_settings: crate::cables::views::Settings::none(),
                said_not_remembered: false,
                keep_open: false,
                active: None,
                page_views: HashMap::new(),
                sheets: HashMap::new(),
                goto_ring: None,
                revision_view: None,
            },
            counts: super::parts::Counts {
                changes: std::collections::VecDeque::new(),
                change_count: 0,
                page_list_version: 0,
                frame_builds: 0,
                projections: 0,
                branch_gen: 0,
            },
            frame: super::parts::FrameCache { drawn: None, stale: false, pending: false, pending_known: false },
            indexes: super::parts::Indexes {
                tag_high: ecad_electrical::devices::tags::TagHigh::default(),
                designation_uses: ecad_electrical::locations::designations::Uses::default(),
            },
            defaults: super::parts::Defaults::default(),
            asks: super::parts::AppAsks {
                open_reports: false,
                agent_access_request: None,
                agent_access_status: None,
                confirm_issue_undo: false,
                issue_undo_question: None,
                confirm_merge_undo: false,
                merge_undo_question: None,
            },
            caller: super::parts::Caller { author: Author::User, tx_budget: None },
        }));
        let mut p = crate::access::try_lock(&project)?;
        p.me = Arc::downgrade(&project);
        // Read the history once, so tag numbering never reads the log during a prompt.
        p.catch_up_tags();
        // Automated tests (the test store) never touch the user's settings unless they choose a
        // folder; a new project has no remembered views to read.
        if persistence != Persistence::TestStore {
            p.display.view_settings = crate::cables::views::Settings::from_env();
        }
        p.pages.insert(page, PageView::default());
        p.open_page(&mut Pages(s), page)?;
        drop(p);
        Ok(ProjectHandle::new(project))
    }

    /// Reopen a project from a copy of the test store's bytes ([`MemoryStore::bytes`]), from its
    /// latest snapshot. A damaged file opens read-only at the last state that verifies;
    /// the report is kept in the storage's `open_report` ([`Project::storage`]).
    ///
    /// # Errors
    /// The bytes are not a project file this build supports, or the project cannot be shown.
    pub(crate) fn open_from(s: &mut Session, name: &str, bytes: Vec<u8>, seed: u64) -> Result<ProjectHandle, String> {
        let backend = MemoryBackend::from_bytes(bytes);
        let store = ecad_file::Store::with_backend(backend.clone()).map_err(|e| e.to_string())?;
        let opened = ecad_file::Project::open(store, seed, &writer()).map_err(|e| e.to_string())?;
        let (file, ledger, report) = (opened.project, opened.ledger, opened.report);
        let id = file.project_id();
        let tag_high = file.base_tag_high().clone();
        let default_layer = ledger.document().layers().iter().find(|(_, l)| l.name == "0").map(|(id, _)| *id).ok_or("no layer 0")?;
        let pages: Vec<PageId> = ledger.document().page_order().to_vec();
        // A fresh ID stream even when the caller reuses the creation seed.
        let ids = IdAllocator::seeded_after(seed, ledger.log().last().map_or(0, |t| t.id.0) ^ u128::from(ledger.head()));
        let baseline = ledger.head();
        let derived = views_for(ledger.document());
        let project = Arc::new(Mutex::new(Project {
            name: name.to_string(),
            path: None,
            ledger,
            ids,
            id,
            pages: pages.iter().map(|p| (*p, PageView::default())).collect(),
            events: EventLog::default(),
            clips: BTreeMap::new(),
            oops: crate::translate::oops::Records::default(),
            faults: Faults::default(),
            tolerance: 1e-6,
            default_layer,
            me: Weak::new(),
            conflicts: Vec::new(),
            merge_cache: None,
            derived,
            storage: super::parts::Storage {
                store: MemoryStore { backend, project: Some(Box::new(file)) },
                file: ecad_file::binding::Binding::default(),
                open_report: Some(report),
                persistence: Persistence::TestStore,
                baseline,
                unsaved_marker: Arc::new(Drawing::new_metric()),
            },
            display: super::parts::Display {
                views: BTreeSet::new(),
                show_hidden_text: false,
                view_settings: crate::cables::views::Settings::none(),
                said_not_remembered: false,
                keep_open: false,
                active: None,
                page_views: HashMap::new(),
                sheets: HashMap::new(),
                goto_ring: None,
                revision_view: None,
            },
            counts: super::parts::Counts {
                changes: std::collections::VecDeque::new(),
                change_count: 0,
                page_list_version: 0,
                frame_builds: 0,
                projections: 0,
                branch_gen: 0,
            },
            frame: super::parts::FrameCache { drawn: None, stale: false, pending: false, pending_known: false },
            indexes: super::parts::Indexes {
                tag_high: ecad_electrical::devices::tags::TagHigh::starting_at(tag_high),
                designation_uses: ecad_electrical::locations::designations::Uses::default(),
            },
            defaults: super::parts::Defaults::default(),
            asks: super::parts::AppAsks {
                open_reports: false,
                agent_access_request: None,
                agent_access_status: None,
                confirm_issue_undo: false,
                issue_undo_question: None,
                confirm_merge_undo: false,
                merge_undo_question: None,
            },
            caller: super::parts::Caller { author: Author::User, tx_budget: None },
        }));
        let mut p = crate::access::try_lock(&project)?;
        p.me = Arc::downgrade(&project);
        // Read the history once, so tag numbering never reads the log during a prompt.
        p.catch_up_tags();
        if let Some(first) = pages.first() {
            p.open_page(&mut Pages(s), *first)?;
        }
        drop(p);
        Ok(ProjectHandle::new(project))
    }

    /// Open a project file from disk (`Persistence::File`): take its writer lock and replay it.
    /// A damaged file opens its verified prefix read-only; a file another session is
    /// editing opens read-only. Either can be saved as another file. Returns the handle and what
    /// opening found, in words (also recorded as a [`EventKind::ProjectOpened`] event).
    ///
    /// # Errors
    /// The file cannot be read, or is not a project file this build supports.
    pub fn open_file(s: &mut Session, path: &str, seed: u64) -> Result<(ProjectHandle, String), String> {
        Self::open_file_in(s, path, seed, crate::cables::views::Settings::from_env())
    }

    /// [`Project::open_file`], remembering cable views in `settings` instead of this user's
    /// settings folder (tests, and trials that must not touch it). The views remembered for the
    /// project are read here; an unusable settings file adds [`crate::cables::views::UNREADABLE`] to what
    /// opening reports and never fails the open.
    ///
    /// # Errors
    /// As [`Project::open_file`].
    pub fn open_file_in(s: &mut Session, path: &str, seed: u64, settings: crate::cables::views::Settings) -> Result<(ProjectHandle, String), String> {
        Self::open_file_with(s, path, seed, settings, false)
    }

    /// [`Project::open_file_in`], for reading only when `reading`: no writer lock is taken and
    /// the file is never changed (an agent with writes off).
    ///
    /// # Errors
    /// As [`Project::open_file`].
    pub fn open_file_with(
        s: &mut Session, path: &str, seed: u64, settings: crate::cables::views::Settings, reading: bool,
    ) -> Result<(ProjectHandle, String), String> {
        let path = crate::file::normalize(path).map_err(|e| e.to_string())?;
        let mut o = if reading {
            ecad_file::binding::Binding::open_read_only(&path, seed, &writer())
        } else {
            ecad_file::binding::Binding::open(&path, seed, &writer())
        }
        .map_err(|e| e.to_string())?;
        let ledger = o.ledger;
        // Words in values that stay text: read once on open, never per commit.
        o.binding.refuse_unknown_words(ecad_electrical::checks::words_check::check(ledger.document(), &ecad_drawing_style::REGISTRY));
        let default_layer = ledger.document().layers().iter().find(|(_, l)| l.name == "0").map(|(id, _)| *id).ok_or("no layer 0")?;
        let pages: Vec<PageId> = ledger.document().page_order().to_vec();
        // A fresh ID stream for this session, never reissuing the file's IDs.
        let ids = IdAllocator::seeded_after(seed, ledger.log().last().map_or(0, |t| t.id.0) ^ u128::from(ledger.head()));
        let baseline = ledger.head();
        let mut summary = vec![format!("Opened {path} at transaction {}.", ledger.head())];
        if o.report.damage.is_some() || o.report.repaired {
            summary.push(o.report.describe());
        }
        if let Some(r) = &o.reclaimed_lock {
            summary.push(format!(
                "A lock left by a session that ended without closing the project ({r}) was reclaimed; nothing else is editing the file."
            ));
        }
        if let Some(r) = o.binding.status().read_only {
            summary.push(format!("Opened read-only: {r}. Save As can save a copy."));
        }
        // Cable views remembered by this user; IDs not at the head are ignored.
        let loaded = settings.load(o.project);
        if loaded.unreadable {
            summary.push(crate::cables::views::UNREADABLE.into());
        }
        let views: BTreeSet<CableId> = loaded.fan.into_iter().filter(|k| ledger.document().cables().contains_key(k)).collect();
        let summary = summary.join("\n");
        let derived = views_for(ledger.document());
        let project = Arc::new(Mutex::new(Project {
            name: file_stem(&path),
            path: Some(path.clone()),
            ledger,
            ids,
            id: o.project,
            pages: pages.iter().map(|p| (*p, PageView::default())).collect(),
            events: EventLog::default(),
            clips: BTreeMap::new(),
            oops: crate::translate::oops::Records::default(),
            faults: Faults::default(),
            tolerance: 1e-6,
            default_layer,
            me: Weak::new(),
            conflicts: Vec::new(),
            merge_cache: None,
            derived,
            storage: super::parts::Storage {
                store: MemoryStore::default(),
                file: o.binding,
                open_report: Some(o.report),
                persistence: Persistence::File,
                baseline,
                unsaved_marker: Arc::new(Drawing::new_metric()),
            },
            display: super::parts::Display {
                views: BTreeSet::new(),
                show_hidden_text: false,
                view_settings: crate::cables::views::Settings::none(),
                said_not_remembered: false,
                keep_open: false,
                active: None,
                page_views: HashMap::new(),
                sheets: HashMap::new(),
                goto_ring: None,
                revision_view: None,
            },
            counts: super::parts::Counts {
                changes: std::collections::VecDeque::new(),
                change_count: 0,
                page_list_version: 0,
                frame_builds: 0,
                projections: 0,
                branch_gen: 0,
            },
            frame: super::parts::FrameCache { drawn: None, stale: false, pending: false, pending_known: false },
            indexes: super::parts::Indexes {
                tag_high: ecad_electrical::devices::tags::TagHigh::starting_at(std::mem::take(&mut o.tag_high)),
                designation_uses: ecad_electrical::locations::designations::Uses::default(),
            },
            defaults: super::parts::Defaults::default(),
            asks: super::parts::AppAsks {
                open_reports: false,
                agent_access_request: None,
                agent_access_status: None,
                confirm_issue_undo: false,
                issue_undo_question: None,
                confirm_merge_undo: false,
                merge_undo_question: None,
            },
            caller: super::parts::Caller { author: Author::User, tx_budget: None },
        }));
        let mut p = crate::access::try_lock(&project)?;
        p.me = Arc::downgrade(&project);
        // Read the history once, so tag numbering never reads the log during a prompt.
        p.catch_up_tags();
        p.display.views = views;
        p.display.view_settings = settings;
        let head = p.ledger.head();
        p.events.record(EventKind::ProjectOpened, None, head, None, summary.clone());
        if let Some(first) = pages.first() {
            p.open_page(&mut Pages(s), *first)?;
        }
        drop(p);
        Ok((ProjectHandle::new(project), summary))
    }
}
