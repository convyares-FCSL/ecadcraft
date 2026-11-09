//! Cable views (brief 005, design 005): whether each cable is shown **traditional** (every wire
//! drawn, the cable shown by its slash) or **fan** (one trunk that fans out to the wire ends), the
//! fan's geometry, and where each user's choice is remembered.
//!
//! # Semantics
//! - A view is presentation only. It is never part of the ledger, never a transaction, never moves
//!   the head and never marks the project unsaved (P5, SAVE-03). The project keeps the set of
//!   cables shown in fan view; traditional is the absence of an entry.
//! - The fan is computed, never stored: [`fan_on_page`] derives it from the cable's members and
//!   their current routes every time a page is projected (design §5).
//! - Views are remembered per user, outside the `.ecad` file, in `cable-views.json` in the ECAD
//!   settings folder ([`Settings`], design §7). A missing, unreadable or unwritable file never
//!   fails an open, a save or a switch: views then start traditional, or last for the session.
//!
//! This module records nothing in the document, has no UI and decides nothing about what the
//! commands do: programmatic calls act on IDs whatever the views are (owner answer O4).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use ecad_model::{CableId, Document, PageId, ProjectId};
use serde_json::{Map, Value, json};

/// The reserved layer the fan is drawn on (colour `ecad.cable`, plots).
pub const LAYER: &str = "ECAD-CABLES";
/// `ecad.cable`: ACI 6, magenta.
pub const CABLE_ACI: u8 = 6;
/// Refusal of an edit or a pick on a fan drawing (design §6.1).
pub const FAN_REFUSED: &str = "This cable is shown in fan view; switch to traditional view to edit.";
/// Refusal of new geometry drawn on [`LAYER`].
pub const LAYER_RESERVED: &str = "the ECAD-CABLES layer is reserved for cable views";
/// Said once when no settings folder exists (wasm, or no folder found).
pub const NOT_REMEMBERED: &str = "Cable views will not be remembered: no settings folder.";
/// Said at open when the settings file cannot be used.
pub const UNREADABLE: &str = "Cable views file could not be read; all cables start in traditional view.";
/// A cable of one wire is not offered fan view (owner answer Q4).
pub const ONE_WIRE: &str = "has one wire; fan view is for cables of two or more wires.";

/// The settings file's name in the settings folder.
pub const FILE_NAME: &str = "cable-views.json";
/// The settings file's `format`.
pub const FORMAT: &str = "ecad-cable-views/1";
/// Larger settings files are not read (treated as empty).
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Most cable IDs read or written for one project. Chosen so that it can be reached: a project at
/// the cap takes about 875 KiB, under [`MAX_FILE_BYTES`] with room for other projects.
pub const MAX_CABLES_PER_PROJECT: usize = 25_000;
/// Environment variable naming the settings folder (tests and manual trials).
pub const SETTINGS_ENV: &str = "ECAD_SETTINGS_DIR";

/// How one cable is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Traditional,
    Fan,
}

impl View {
    /// `traditional` or `fan`.
    pub fn name(self) -> &'static str {
        match self {
            View::Traditional => "traditional",
            View::Fan => "fan",
        }
    }

    /// The view named `text` (exactly `traditional` or `fan`).
    pub fn parse(text: &str) -> Option<View> {
        match text {
            "traditional" => Some(View::Traditional),
            "fan" => Some(View::Fan),
            _ => None,
        }
    }

    /// The other view.
    pub fn toggled(self) -> View {
        match self {
            View::Traditional => View::Fan,
            View::Fan => View::Traditional,
        }
    }
}

// ---------------------------------------------------------------- geometry (design 009 §4)

/// Refusal of an edit or a pick on a computed part of a traditional-view cable: a core label at
/// the slash, or a reference end's flag (design 009 §8).
pub const DRAWN_REFUSED: &str = "This is drawn from the cable; use Core Label to change it, or move the reference point.";

/// The refusal for an edit of computed drawing of `cable` on a page mapped by `map`.
pub fn refusal(map: &crate::project::HandleMap, cable: CableId) -> &'static str {
    if map.is_fan(cable) { FAN_REFUSED } else { DRAWN_REFUSED }
}

/// What a fan-view cable shows on one page.
#[derive(Debug, Clone, PartialEq)]
pub enum FanOnPage {
    /// No member has a route here: the page is unchanged (a slash stays drawn).
    Nothing,
    /// One of its wires is on this page: the cable is drawn traditional here (design 009 §4.1).
    TooFew,
    /// The fan cannot be drawn here (a reference route of no length, or values that are not
    /// finite): the cable is drawn traditional on this page.
    Failed,
    /// The fan (design 009 §4), with the items it hides.
    Drawn(Fan),
}

/// A cable's fan on one page (brief 009's look).
pub type Fan = crate::look::CableDrawing;

/// The fan of `cable` on `page` (design 009 §4), measured with the drafting font. Bounded by the
/// members and their route points; never panics.
pub fn fan_on_page(doc: &Document, page: PageId, cable: CableId, tol: f64) -> FanOnPage {
    let routed =
        doc.cables.get(&cable).is_some_and(|c| c.members.iter().any(|m| ecad_model::derive::route_on_page(doc, m.conductor, page).is_some()));
    if !routed {
        return FanOnPage::Nothing;
    }
    let d = crate::look::cable_on_page(doc, page, cable, true, tol, &crate::look::font_width);
    match d.state {
        crate::look::FanState::Drawn => FanOnPage::Drawn(d),
        crate::look::FanState::TooFew => FanOnPage::TooFew,
        _ => FanOnPage::Failed,
    }
}

/// Every cable's computed drawing on `page` (design 009): fans for the cables in `fan` where they
/// can be drawn, and core labels and reference ends for every cable, by cable.
pub fn drawings_on_page(doc: &Document, page: PageId, fan: &BTreeSet<CableId>, tol: f64) -> BTreeMap<CableId, crate::look::CableDrawing> {
    crate::look::cables_on_page(doc, page, fan, tol, &crate::look::font_width)
}

/// A cable after the word "Cable" in messages: its tag, else `8a3f19c0…`.
pub fn cable_title(doc: &Document, cable: CableId) -> String {
    match doc.cables.get(&cable).and_then(|c| c.properties.get("tag")) {
        Some(t) => t.clone(),
        None => format!("{}…", cable.hex().get(..8).unwrap_or_default()),
    }
}

/// `Cable C1 now shown in fan view.`
pub fn switched(doc: &Document, cable: CableId, view: View) -> String {
    format!("Cable {} now shown in {} view.", cable_title(doc, cable), view.name())
}

/// Whether `cable` may be switched to fan view: it has two or more members (owner answer Q4).
pub fn offers_fan(doc: &Document, cable: CableId) -> bool {
    doc.cables.get(&cable).is_some_and(|c| c.members.len() >= 2)
}

// ---------------------------------------------------------------- remembering (design §7)

/// Where cable views are remembered: the per-user ECAD settings folder, or none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    dir: Option<PathBuf>,
}

/// What reading the settings file found for one project.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Loaded {
    /// Cables remembered in fan view (not yet checked against the document).
    pub fan: BTreeSet<CableId>,
    /// The file exists but could not be used ([`UNREADABLE`] is said).
    pub unreadable: bool,
}

/// This process's settings folder, used where `ECAD_SETTINGS_DIR` is not set.
static PROCESS_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Use `dir` as this process's settings folder wherever `ECAD_SETTINGS_DIR` is not set, instead of
/// the user's own folder. The first call wins. Automated tests use it (through
/// [`isolate_for_tests`]) so that no test reads or writes the user's real `cable-views.json`:
/// setting the environment variable from inside a test process would need `unsafe`.
pub fn use_process_settings_dir(dir: impl Into<PathBuf>) {
    let _ = PROCESS_DIR.set(dir.into());
}

/// Automated tests: a settings folder of this process's own under the system temp folder
/// ([`use_process_settings_dir`]).
pub fn isolate_for_tests() {
    use_process_settings_dir(std::env::temp_dir().join(format!("ecad-test-settings-{}", process_id())));
}

/// A non-empty environment variable as a path (no settings folder is looked up on wasm).
#[cfg(not(target_arch = "wasm32"))]
fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

impl Settings {
    /// The folder for this user: `ECAD_SETTINGS_DIR` when set; else the folder given to
    /// [`use_process_settings_dir`]; else `%APPDATA%\ECAD` on Windows,
    /// `$XDG_CONFIG_HOME/ecad` or `$HOME/.config/ecad` elsewhere; none on wasm.
    pub fn from_env() -> Settings {
        #[cfg(target_arch = "wasm32")]
        {
            Settings::none()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(d) = env_dir(SETTINGS_ENV) {
                return Settings::at(d);
            }
            if let Some(d) = PROCESS_DIR.get() {
                return Settings::at(d.clone());
            }
            #[cfg(windows)]
            let dir = env_dir("APPDATA").map(|d| d.join("ECAD"));
            #[cfg(not(windows))]
            let dir = env_dir("XDG_CONFIG_HOME").map(|d| d.join("ecad")).or_else(|| env_dir("HOME").map(|d| d.join(".config").join("ecad")));
            Settings { dir }
        }
    }

    /// Views remembered in folder `dir`.
    pub fn at(dir: impl Into<PathBuf>) -> Settings {
        Settings { dir: Some(dir.into()) }
    }

    /// Views last for the session only.
    pub fn none() -> Settings {
        Settings { dir: None }
    }

    /// The settings folder, if any.
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// The settings file, if there is a folder.
    pub fn file(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(FILE_NAME))
    }

    /// What the settings file holds now.
    fn read_all(&self) -> FileState {
        let Some(path) = self.file() else { return FileState::Missing };
        let f = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return FileState::Missing,
            Err(e) => return FileState::Unusable(format!("the settings file cannot be read: {e}")),
        };
        let mut bytes = Vec::new();
        if let Err(e) = f.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes) {
            return FileState::Unusable(format!("the settings file cannot be read: {e}"));
        }
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return FileState::Unusable("the settings file is larger than 1 MiB".into());
        }
        let Ok(v) = serde_json::from_slice::<Value>(&bytes) else { return FileState::NotJson };
        let Value::Object(top) = v else { return FileState::Unusable(OTHER_FORMAT.into()) };
        if top.get("format").and_then(Value::as_str) != Some(FORMAT) || !top.get("projects").is_some_and(Value::is_object) {
            return FileState::Unusable(OTHER_FORMAT.into());
        }
        FileState::Usable(top)
    }

    /// The cables remembered in fan view for `project`. Never fails: a missing file means none, an
    /// unusable one (unreadable, not JSON, another format, over 1 MiB) is reported and means none.
    pub fn load(&self, project: ProjectId) -> Loaded {
        match self.read_all() {
            FileState::Missing => Loaded::default(),
            FileState::NotJson | FileState::Unusable(_) => Loaded { fan: BTreeSet::new(), unreadable: true },
            FileState::Usable(top) => {
                let list = top.get("projects").and_then(|p| p.get(project.hex())).and_then(|p| p.get("fan")).and_then(Value::as_array);
                let fan = list.into_iter().flatten().take(MAX_CABLES_PER_PROJECT).filter_map(|v| v.as_str().and_then(CableId::parse)).collect();
                Loaded { fan, unreadable: false }
            }
        }
    }

    /// Remember `fan` as `project`'s cables in fan view: read the file again, change only this
    /// project's entry (removed when `fan` is empty), write a temporary file in the same folder and
    /// rename it over the old one.
    ///
    /// # Semantics
    /// - Only a missing file or one that is not JSON is (re)written from scratch. A file that cannot
    ///   be read for another reason (locked by another program, another format such as a newer
    ///   version, over 1 MiB) is left alone and nothing is remembered.
    /// - Each project's entry records when it was last written (`used`, ms since the epoch). The
    ///   file written is never one [`Settings::load`] would refuse: when it would exceed
    ///   [`MAX_FILE_BYTES`], the least recently used other projects are dropped first; if this
    ///   project's entry alone is too large, or lists more than [`MAX_CABLES_PER_PROJECT`]
    ///   cables, nothing is written.
    /// - Two windows writing at the same moment may lose one of the two changes (no lock: views
    ///   are presentation only).
    ///
    /// # Errors
    /// Nothing was remembered: no settings folder, a file left alone, too many cables, or the
    /// folder or file cannot be written. The reason in words.
    pub fn store(&self, project: ProjectId, fan: &BTreeSet<CableId>) -> Result<(), String> {
        let (Some(dir), Some(path)) = (self.dir.as_ref(), self.file()) else {
            return Err("no settings folder".into());
        };
        if fan.len() > MAX_CABLES_PER_PROJECT {
            return Err(format!("more than {MAX_CABLES_PER_PROJECT} cables of this project are in fan view"));
        }
        let mut top = match self.read_all() {
            FileState::Missing | FileState::NotJson => Map::new(),
            FileState::Unusable(reason) => return Err(format!("{reason}; it was left as it is")),
            FileState::Usable(top) => top,
        };
        top.insert("format".into(), Value::String(FORMAT.into()));
        let mut projects = match top.remove("projects") {
            Some(Value::Object(m)) => m,
            _ => Map::new(),
        };
        let key = project.hex();
        if fan.is_empty() {
            projects.remove(&key);
        } else {
            let ids: Vec<Value> = fan.iter().map(|k| Value::String(k.hex())).collect();
            projects.insert(key.clone(), json!({ "fan": ids, "used": crate::project::now_ms().unwrap_or(0) }));
        }
        let text = fit(top, projects, &key)?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let tmp = dir.join(format!("{FILE_NAME}.{}.{}.tmp", process_id(), next_serial()));
        let written = std::fs::write(&tmp, text.as_bytes()).and_then(|()| std::fs::rename(&tmp, &path));
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.to_string());
        }
        Ok(())
    }
}

/// Said when the file is JSON but not this format (for example a newer version's).
const OTHER_FORMAT: &str = "the settings file has another format";

/// What reading the settings file found.
enum FileState {
    Missing,
    /// Not JSON at all: replaced at the next write.
    NotJson,
    /// Exists but cannot be used, and must not be overwritten: why.
    Unusable(String),
    Usable(Map<String, Value>),
}

/// The file's text (LF-terminated) with `projects` under `top`, at most [`MAX_FILE_BYTES`]: the
/// least recently used projects other than `keep` are dropped until it fits.
fn fit(mut top: Map<String, Value>, mut projects: Map<String, Value>, keep: &str) -> Result<String, String> {
    let used = |v: &Value| v.get("used").and_then(Value::as_u64).unwrap_or(0);
    // Oldest first; equal times by key, so the order is total and repeatable.
    let mut others: Vec<(u64, String)> = projects.iter().filter(|(k, _)| k.as_str() != keep).map(|(k, v)| (used(v), k.clone())).collect();
    others.sort();
    let mut others = others.into_iter();
    loop {
        top.insert("projects".into(), Value::Object(projects.clone()));
        let mut text = serde_json::to_string(&Value::Object(top.clone())).map_err(|e| e.to_string())?;
        text.push('\n');
        if text.len() as u64 <= MAX_FILE_BYTES {
            return Ok(text);
        }
        // Drop old projects in steps sized to the excess, so this ends after few passes.
        let mut excess = text.len() as u64 - MAX_FILE_BYTES;
        let mut dropped = false;
        while excess > 0 {
            let Some((_, k)) = others.next() else { break };
            let size = projects.get(&k).map_or(0, |v| v.to_string().len() + k.len() + 4) as u64;
            projects.remove(&k);
            excess = excess.saturating_sub(size);
            dropped = true;
        }
        if !dropped {
            return Err("too many cables of this project are in fan view to remember (the settings file would be \
larger than 1 MiB)"
                .into());
        }
    }
}

fn process_id() -> u32 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::process::id()
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

fn next_serial() -> u64 {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
