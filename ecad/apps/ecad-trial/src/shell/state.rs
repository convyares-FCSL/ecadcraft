//! What the shell remembers (design `ui-architecture.md` §5.4): the [`ShellState`] (workspace,
//! panels, the developer switch) and, per project, each page's zoom and centre, the last page and
//! the last workspace ([`Views`]). Both live in eframe's app storage, never in the `.ecad` file:
//! a view is how one person looks at a project, not part of it.
//!
//! An unreadable stored value falls back to the default and says so in one line; it never stops
//! start-up. Hover, selection, open flyouts and scroll positions are not kept.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The eframe storage keys.
pub const SHELL_KEY: &str = "shell";
pub const VIEWS_KEY: &str = "views";
/// At most this many projects' views are kept (the least recently opened is dropped).
pub const MAX_PROJECTS: usize = 32;
/// At most this many pages' views are kept per project.
pub const MAX_PAGES: usize = 4096;
/// The companion's default share of the main view.
pub const COMPANION_RATIO: f32 = 0.5;

/// The four workspaces (spec §2): a workspace changes the main view and the tool strip only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Workspace {
    #[default]
    Draw,
    Lists,
    Library,
    Publish,
}

impl Workspace {
    pub const ALL: [Workspace; 4] = [Workspace::Draw, Workspace::Lists, Workspace::Library, Workspace::Publish];

    /// Its name in the segmented control.
    pub fn label(self) -> &'static str {
        match self {
            Workspace::Draw => "Draw",
            Workspace::Lists => "Lists",
            Workspace::Library => "Library",
            Workspace::Publish => "Publish",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Workspace::Draw => 0,
            Workspace::Lists => 1,
            Workspace::Library => 2,
            Workspace::Publish => 3,
        }
    }

    pub fn from_index(i: usize) -> Option<Workspace> {
        Workspace::ALL.get(i).copied()
    }
}

/// The left panel's tabs: navigation only (spec §2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeftTab {
    #[default]
    Pages,
    Devices,
    History,
}

impl LeftTab {
    pub const ALL: [LeftTab; 3] = [LeftTab::Pages, LeftTab::Devices, LeftTab::History];

    pub fn index(self) -> usize {
        match self {
            LeftTab::Pages => 0,
            LeftTab::Devices => 1,
            LeftTab::History => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LeftTab::Pages => crate::project_panel::PAGES_TAB,
            LeftTab::Devices => crate::project_panel::DEVICES_TAB,
            LeftTab::History => crate::history_panel::HISTORY_TAB,
        }
    }
}

/// The left panel's state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LeftState {
    pub tab: LeftTab,
    pub collapsed: bool,
    /// The width of each tab, in [`LeftTab::index`] order.
    pub width_per_tab: [f32; 3],
}

impl Default for LeftState {
    fn default() -> Self {
        LeftState { tab: LeftTab::Pages, collapsed: false, width_per_tab: LeftTab::ALL.map(super::layout::default_width) }
    }
}

/// What the companion half shows (design §5.5): Reports beside the page in Draw, the page beside
/// Reports in Lists.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Companion {
    /// The main view's share of the width (the companion takes the rest).
    pub ratio: f32,
}

impl Default for Companion {
    fn default() -> Self {
        Companion { ratio: COMPANION_RATIO }
    }
}

/// The shell's remembered state (design §5.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellState {
    pub workspace: Workspace,
    pub left: LeftState,
    pub drawer_open: bool,
    pub right_collapsed: bool,
    pub companion: Option<Companion>,
    /// The Text panel over the right panel (Ctrl+/).
    pub text_panel: bool,
    /// The developer switch: CADCraft's own interface inside the main view.
    pub cad_chrome: bool,
    /// Brief 040: the sheet the page is drawn on (`ui.sheet`), per person.
    pub sheet: crate::canvas::Sheet,
    /// Brief 048: the Library shows the symbol builder (its tool strip, and the 380 px right
    /// panel). View state, never stored.
    #[serde(skip)]
    pub builder: bool,
}

impl Default for ShellState {
    fn default() -> Self {
        ShellState {
            workspace: Workspace::Draw,
            left: LeftState::default(),
            drawer_open: true,
            right_collapsed: false,
            companion: None,
            text_panel: false,
            cad_chrome: false,
            sheet: crate::canvas::Sheet::Paper,
            builder: false,
        }
    }
}

impl ShellState {
    /// The state stored under [`SHELL_KEY`], or the default and the line saying why when the
    /// value cannot be read. `None` (nothing stored) is the default without a line.
    pub fn from_stored(stored: Option<&str>) -> (ShellState, Option<String>) {
        read(stored, "shell layout")
    }

    /// The text stored under [`SHELL_KEY`].
    pub fn to_stored(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// One page's view: centre and visible height, in drawing units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PageView {
    pub cx: f64,
    pub cy: f64,
    pub height: f64,
}

impl PageView {
    /// Usable as a view: finite, with a positive height.
    pub fn usable(&self) -> bool {
        self.cx.is_finite() && self.cy.is_finite() && self.height.is_finite() && self.height > 0.0
    }
}

/// One project's views.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectViews {
    /// By page ID (hex).
    pub pages: BTreeMap<String, PageView>,
    pub last_page: Option<String>,
    pub last_workspace: Option<Workspace>,
    /// When it was last opened, as a counter (larger is more recent).
    pub opened: u64,
}

/// Every project's views, by project ID (hex), at most [`MAX_PROJECTS`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Views {
    pub projects: BTreeMap<String, ProjectViews>,
}

impl Views {
    /// The views stored under [`VIEWS_KEY`], as [`ShellState::from_stored`].
    pub fn from_stored(stored: Option<&str>) -> (Views, Option<String>) {
        let (mut v, line): (Views, _) = read(stored, "page views");
        v.cap();
        (v, line)
    }

    pub fn to_stored(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// The project's views, marked as the most recently opened; the oldest project is dropped
    /// beyond [`MAX_PROJECTS`].
    pub fn open(&mut self, project: &str) -> &mut ProjectViews {
        let next = self.projects.values().map(|p| p.opened).max().unwrap_or(0).saturating_add(1);
        let entry = self.projects.entry(project.to_string()).or_default();
        entry.opened = next;
        self.cap();
        self.projects.entry(project.to_string()).or_default()
    }

    /// Keep the [`MAX_PROJECTS`] most recently opened projects and [`MAX_PAGES`] pages each.
    fn cap(&mut self) {
        while self.projects.len() > MAX_PROJECTS {
            let Some(oldest) = self.projects.iter().min_by_key(|(_, p)| p.opened).map(|(k, _)| k.clone()) else { break };
            self.projects.remove(&oldest);
        }
        for p in self.projects.values_mut() {
            p.pages.retain(|_, v| v.usable());
            while p.pages.len() > MAX_PAGES {
                if p.pages.pop_last().is_none() {
                    break;
                }
            }
        }
    }
}

/// Read a stored JSON value, or the default and one line naming `what` when it is unreadable.
fn read<T: Default + for<'de> Deserialize<'de>>(stored: Option<&str>, what: &str) -> (T, Option<String>) {
    match stored {
        None => (T::default(), None),
        Some(text) => match serde_json::from_str(text) {
            Ok(v) => (v, None),
            Err(e) => (T::default(), Some(format!("The saved {what} could not be read ({e}); starting with the default."))),
        },
    }
}
