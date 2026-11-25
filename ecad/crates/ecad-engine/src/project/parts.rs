//! The groups of a [`Project`](super::Project)'s session state, each a field of it: the file side,
//! the display, the change counts, the frame, the indexes read from the log, the prompts'
//! defaults, the desktop app's asks and the caller.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use cadcraft_engine::doc::Drawing;
use ecad_model::{Author, CableId, PageId, Pt};

use super::{Change, MemoryStore, Persistence};

/// The project's file side: where it is saved and what opening reported.
pub struct Storage {
    /// The test-only project file in memory (`Persistence::TestStore`).
    pub store: MemoryStore,
    /// The project's file on disk (`Persistence::File`).
    pub file: ecad_file::binding::Binding,
    /// What opening reported, when the project was opened from a file on disk or from the test
    /// store.
    pub open_report: Option<ecad_file::Report>,
    /// How Save behaves; see [`Persistence`].
    pub persistence: Persistence,
    /// The head right after the project was created or opened: with no persistence, the project
    /// has unsaved design exactly when the head has moved from here.
    pub(super) baseline: u64,
    /// Points `DocState::saved` away from the drawing while the project is unsaved.
    pub(super) unsaved_marker: Arc<Drawing>,
}

/// What this session shows: never in the ledger or the file, never a transaction, never the
/// unsaved state.
pub struct Display {
    /// Cables shown in fan view; every other cable is traditional.
    pub views: BTreeSet<CableId>,
    /// Show Hidden Text: pages also draw their hidden derived texts, dimmed. Not undoable, never
    /// printed. Off when a project is opened.
    pub show_hidden_text: bool,
    /// Where this user's cable views are remembered ([`crate::cables::views::Settings`]).
    pub view_settings: crate::cables::views::Settings,
    /// Whether [`crate::cables::views::NOT_REMEMBERED`] was said in this session.
    pub(crate) said_not_remembered: bool,
    /// The page shown in the page window, if any.
    pub(super) active: Option<PageId>,
    /// Each page's views as the user left them.
    pub(super) page_views: HashMap<PageId, Vec<(cadcraft_engine::doc::Space, cadcraft_engine::View)>>,
    /// The sheet (0-based) each generated page of several sheets shows in the page window; the
    /// first when not set. Display only, like the views; clamped to the page's sheets when drawn.
    pub(super) sheets: HashMap<PageId, usize>,
    /// The Checks tab's go-to ring: page, point and the row's severity, until the next change.
    pub goto_ring: Option<(PageId, Pt, ecad_electrical::checks::Severity)>,
    /// The revision opened read-only.
    pub revision_view: Option<crate::pages::commands::sheets::RevisionView>,
    /// Closing the page window leaves the project open with no active page instead of closing the
    /// project. Off by default (tests, MCP); the desktop app turns it on and off again while it
    /// closes the project or quits, so unsaved work is always asked about.
    pub keep_open: bool,
}

/// The change log and the counts that say what could differ since a copy was read.
pub struct Counts {
    /// The last [`super::CHANGE_LOG`] changes, oldest first.
    pub(super) changes: std::collections::VecDeque<Change>,
    /// Number of the last change (0: none yet).
    pub(super) change_count: u64,
    /// Bumped whenever a page row could differ ([`super::Project::page_list_version`]).
    pub(super) page_list_version: u64,
    /// Counts branch actions (create, rename, switch, archive, reopen), however they were run:
    /// what shows the branch (title, bar, Revisions, History) refreshes when it moves.
    pub(super) branch_gen: u64,
    /// How many times a frame was worked out again (tests and bench: only when it can differ).
    pub frame_builds: u64,
    /// How many times a page in the window was projected again after a change or a view switch
    /// ([`super::Project::refresh_page`]; tests: a commit that touched another page projects
    /// nothing).
    pub projections: u64,
}

/// The active page's frame as last drawn, and whether the drawing is pending a revision.
pub struct FrameCache {
    /// The page, the sheet shown and their frame's parts. What it showed is the frame view's
    /// (`ecad_electrical::views::FrameView`), whose update map sets `stale`.
    pub(super) drawn: Option<(PageId, usize, Arc<Vec<crate::pages::frame::Part>>)>,
    /// An effect of the frame's update map was committed since the frame was drawn.
    pub(super) stale: bool,
    /// The drawing changed since the newest issued revision, as the title block shows it; worked
    /// out on first use and after every change.
    pub pending: bool,
    pub(super) pending_known: bool,
}

/// What is read from the log as it grows; rebuilt on a branch switch.
pub(crate) struct Indexes {
    /// The highest number each tag prefix has had.
    pub(crate) tag_high: ecad_electrical::devices::tags::TagHigh,
    /// The use index of the project's list of functions and locations.
    pub(crate) designation_uses: ecad_electrical::locations::designations::Uses,
}

/// What this session's prompts offer as their defaults, per project (a window and an agent
/// session in one process keep their own).
#[derive(Default)]
pub struct Defaults {
    /// What the last placement placed (starter, device).
    pub(crate) last_placed: Option<ecad_electrical::commands::run::LastPlaced>,
    /// The last continuation name placed.
    pub(crate) last_continuation: Option<String>,
    /// The last caption given, Caption Continuations' default.
    pub(crate) last_caption: Option<String>,
    /// The Wire tool's potential for new wires; `None` is `auto`.
    pub wire_potential: Option<String>,
    /// The Location Box and Function Box tools draw a closed polyline (`true`) or a rectangle
    /// (`false`, the default): the drawing mode last chosen.
    pub(crate) box_polyline: bool,
    /// The last location the Location Box tool gave, its next prompt's default.
    pub(crate) last_location: Option<String>,
    /// The last function the Function Box tool gave, its next prompt's default.
    pub(crate) last_function: Option<String>,
}

impl Defaults {
    /// The value the box tool of `aspect` last gave (Location Box, Function Box).
    pub(crate) fn last_box_value(&self, aspect: ecad_electrical::locations::designations::Aspect) -> &Option<String> {
        match aspect {
            ecad_electrical::locations::designations::Aspect::Function => &self.last_function,
            ecad_electrical::locations::designations::Aspect::Location => &self.last_location,
        }
    }

    /// [`Defaults::last_box_value`], to change.
    pub(crate) fn last_box_value_mut(&mut self, aspect: ecad_electrical::locations::designations::Aspect) -> &mut Option<String> {
        match aspect {
            ecad_electrical::locations::designations::Aspect::Function => &mut self.last_function,
            ecad_electrical::locations::designations::Aspect::Location => &mut self.last_location,
        }
    }
}

/// What a command asks of the desktop app, and the app's answers.
pub struct AppAsks {
    /// `reports` asked for the Reports window (the desktop app opens it and clears this).
    pub open_reports: bool,
    /// `agentaccess` asked for this agent access preference (the desktop app applies it and
    /// clears this).
    pub agent_access_request: Option<crate::agent::access::Preference>,
    /// The desktop app's agent access state as `agentaccess` reports it (set by the desktop app).
    pub agent_access_status: Option<crate::agent::access::Status>,
    /// Plain `u` across an issued revision stops and asks: set by the desktop app's
    /// confirmation, used by the next undo only.
    pub confirm_issue_undo: bool,
    /// The question an undo across an issue is waiting on, for the desktop app's popup.
    pub issue_undo_question: Option<String>,
    /// Plain `u` across a merge stops and asks: set by `undomerge`, used by the next undo only.
    pub confirm_merge_undo: bool,
    /// The question an undo across a merge is waiting on, for the desktop app's popup.
    pub merge_undo_question: Option<String>,
}

/// Who is calling, and what the call may record.
pub struct Caller {
    /// Who the project's edits, undos and redos are recorded as: [`Author::User`] by default,
    /// `Author::Agent(<client name>)` when an agent drives the session. A session setting, not
    /// part of the design: set it through [`crate::access::ProjectHandle::configure`].
    pub author: Author,
    /// How many transactions the command now running may record, when its caller caps them (an
    /// agent's call through MCP); `None` for no cap. Commands that write files check it before
    /// they write (a release), so nothing reaches the disk that the cap would take back.
    pub tx_budget: Option<usize>,
}
