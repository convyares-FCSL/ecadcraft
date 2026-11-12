//! Where the protocol reads from (design 029 §3.1, §4, §6): a [`Source`]. Today a
//! [`FileSource`], a `.ecad` file opened read-only by path or from dropped bytes. A live app
//! session can be another source later.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use ecad_file::v2::verify::{VerifyReport, verify_chain, verify_stoppable};
use ecad_file::v2::{ProjectV2, ReportV2, Store};
use ecad_model::Document;
use serde_json::{Value, json};

/// A refusal in words (design 029 §2.1): a stable `what`, a sentence `why` and a `way_out`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub what: &'static str,
    pub why: String,
    pub way_out: String,
}

impl Problem {
    pub fn new(what: &'static str, why: impl Into<String>, way_out: impl Into<String>) -> Self {
        Self { what, why: why.into(), way_out: way_out.into() }
    }

    /// The protocol's `error` object.
    pub fn json(&self) -> Value {
        json!({"what": self.what, "why": self.why, "way_out": self.way_out})
    }
}

/// What a source is (`hello`'s `source`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    /// `file` (served by path) or `bytes` (dropped on the page).
    pub kind: &'static str,
    pub name: String,
    pub size: u64,
}

/// The project as the app opens it (model views).
pub struct Opened {
    pub doc: Document,
    pub report: ReportV2,
    /// The branch it opened on.
    pub branch: String,
}

/// The Verify started on load (design 029 §3.4).
#[derive(Debug, Clone, PartialEq)]
pub enum VerifyState {
    /// Not run: the file has no store this build reads.
    NotRun,
    Running {
        elapsed_ms: u64,
    },
    Done {
        elapsed_ms: u64,
        report: VerifyReport,
    },
}

/// A source of everything the protocol shows.
pub trait Source: Send + Sync {
    fn describe(&self) -> SourceInfo;
    /// The file's own problem (design 029 §6 rows 1–8), if any: what still works is decided by
    /// [`Source::store`] and [`Source::opened`].
    fn problem(&self) -> Option<&Problem>;
    /// Raw views, history, chain and Verify; `None` when the bytes are not a database this build
    /// reads.
    fn store(&self) -> Option<&Store>;
    /// Model views.
    ///
    /// # Errors
    /// Why the project did not open.
    fn opened(&self) -> Result<&Opened, &Problem>;
    /// The chain check, computed once on load.
    fn chain(&self) -> Option<&VerifyReport>;
    fn verify(&self) -> VerifyState;
    /// The first bytes of the file (shown for unreadable files).
    fn head(&self) -> &[u8];
    /// The file changed on disk since it was opened (served by path only).
    fn changed(&self) -> bool;
}

/// How many bytes of a file are kept to show and to classify it.
const HEAD: usize = 4_096;

/// The meta names this build reads, and what it reads (`summary`).
const CONTAINER_VERSION: u64 = 2;
const SCHEMA_VERSION: u64 = 1;
const CODEC: &str = "json/1";

/// A `.ecad` file, read-only.
pub struct FileSource {
    info: SourceInfo,
    head: Vec<u8>,
    problem: Option<Problem>,
    store: Option<Arc<Store>>,
    opened: Result<Opened, Problem>,
    chain: Option<VerifyReport>,
    verify: Arc<Mutex<(Instant, VerifyState)>>,
    /// Set when this source is dropped (replaced by a dropped file or a reload): its Verify thread
    /// stops at its next check.
    stop: Arc<AtomicBool>,
    /// The path, length and modified time it was read with (served by path).
    identity: Option<(PathBuf, u64, Option<SystemTime>)>,
}

fn identity_of(path: &Path) -> (u64, Option<SystemTime>) {
    match std::fs::metadata(path) {
        Ok(m) => (m.len(), m.modified().ok()),
        Err(_) => (u64::MAX, None),
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

/// A JSON meta value of `store`.
fn meta(store: &Store, name: &str) -> Option<Value> {
    store.reader().ok()?.meta(name).ok().flatten().and_then(|b| serde_json::from_slice(&b).ok())
}

/// A readable size: `1.4 GB`, `323 KB`.
pub fn size_text(n: u64) -> String {
    let f = n as f64;
    if f >= 1e9 {
        format!("{:.1} GB", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.1} MB", f / 1e6)
    } else if f >= 1e3 {
        format!("{:.0} KB", f / 1e3)
    } else {
        format!("{n} bytes")
    }
}

/// What the first bytes say, before any database is opened (design 029 §6 rows 1–4).
fn classify_head(name: &str, size: u64, head: &[u8]) -> Option<Problem> {
    if size == 0 || name.to_ascii_lowercase().ends_with(".lock") {
        let open = name.strip_suffix(".lock").unwrap_or(name);
        return Some(Problem::new(
            "empty",
            format!("This is an empty file (`{name}` is the app's lock file when it ends in .lock)."),
            format!("Open `{open}` instead."),
        ));
    }
    match ecad_file::v2::convert::sniff_bytes(head) {
        ecad_file::v2::convert::Major::V2 => None,
        ecad_file::v2::convert::Major::V1 => {
            let text = String::from_utf8_lossy(head);
            if text.contains("\"ecad-inspect/") {
                return Some(Problem::new(
                    "snapshot",
                    "This is an inspector snapshot, not a project file.",
                    "The page opens it in Snapshots mode itself.",
                ));
            }
            let stem = name.strip_suffix(".ecad").unwrap_or(name);
            Some(Problem::new(
                "version-1",
                "This is a version 1 project file; the inspector reads version 2.",
                format!("Upgrade a copy: `ecad upgrade {name}` writes `{stem}.v2.ecad` beside it and never changes the original."),
            ))
        }
        ecad_file::v2::convert::Major::Unknown => Some(Problem::new(
            "not-ecad",
            format!("Not an ECAD project file (it starts with `{}`).", hex(head.get(..4).unwrap_or(head))),
            "Open a `.ecad` file.",
        )),
    }
}

/// What the meta rows say (design 029 §6 rows 5–7).
fn classify_meta(store: &Store) -> Option<Problem> {
    let writer = meta(store, "writer").and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "unknown".into());
    let format = meta(store, "format");
    if format.as_ref().and_then(Value::as_str) != Some("ecad-project") {
        let f = format.as_ref().and_then(Value::as_str).unwrap_or("none");
        return Some(Problem::new("not-project", format!("An ECAD database, but its format is `{f}`."), "The inspector reads projects only."));
    }
    let cv = meta(store, "container_version").and_then(|v| v.as_u64());
    if cv != Some(CONTAINER_VERSION) {
        let shown = cv.map_or("unknown".into(), |v| v.to_string());
        return Some(Problem::new(
            "container-version",
            format!("Container version {shown}; this inspector reads {CONTAINER_VERSION}."),
            format!("Run the `ecadinspect` from the ECAD version that wrote it (writer: `{writer}`)."),
        ));
    }
    let sv = meta(store, "schema_version").and_then(|v| v.as_u64()).unwrap_or(0);
    let codec = meta(store, "codec");
    if sv == 0 || sv > SCHEMA_VERSION || codec.as_ref().and_then(Value::as_str) != Some(CODEC) {
        let c = codec.as_ref().and_then(Value::as_str).unwrap_or("none");
        return Some(Problem::new(
            "schema-version",
            format!("Written by a newer ECAD (schema {sv}, codec {c}): rows shown raw, model not opened."),
            format!("Run the `ecadinspect` from the ECAD version that wrote it (writer: `{writer}`)."),
        ));
    }
    None
}

/// The way out when the model did not open.
const OPEN_WAY_OUT: &str = "The Tables and History tabs still show the stored rows; the Checks tab has the chain check.";

impl FileSource {
    /// Open `path` read-only (`Store::open(path, false)`), as `ecadinspect serve <file>` does.
    pub fn from_path(path: &Path) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
        let (size, modified) = identity_of(path);
        let mut head = vec![0u8; HEAD];
        let n = std::fs::File::open(path).and_then(|mut f| std::io::Read::read(&mut f, &mut head)).unwrap_or(0);
        head.truncate(n);
        let info = SourceInfo { kind: "file", name, size: if size == u64::MAX { 0 } else { size } };
        let mut me = Self::empty(info, head);
        me.identity = Some((path.to_path_buf(), size, modified));
        if size == u64::MAX {
            me.problem = Some(Problem::new("unreadable", format!("The file could not be read: {}.", path.display()), "Check the path."));
            return me;
        }
        if let Some(p) = classify_head(&me.info.name, size, &me.head) {
            me.problem = Some(p);
            return me;
        }
        let open = || Store::open(path, false);
        me.load(open);
        me
    }

    /// Bytes dropped on the page, held in memory only.
    pub fn from_bytes(name: &str, bytes: Vec<u8>) -> Self {
        let size = bytes.len() as u64;
        let head = bytes.get(..HEAD.min(bytes.len())).unwrap_or_default().to_vec();
        let mut me = Self::empty(SourceInfo { kind: "bytes", name: name.to_string(), size }, head);
        if let Some(p) = classify_head(name, size, &me.head) {
            me.problem = Some(p);
            return me;
        }
        // One copy into the database; a second only if the project does not open (below).
        let bytes = Arc::new(bytes);
        me.load(|| Store::from_bytes(bytes.as_ref().clone()));
        me
    }

    fn empty(info: SourceInfo, head: Vec<u8>) -> Self {
        let no = Problem::new("unavailable", "The project was not opened.", OPEN_WAY_OUT);
        Self {
            info,
            head,
            problem: None,
            store: None,
            opened: Err(no),
            chain: None,
            verify: Arc::new(Mutex::new((Instant::now(), VerifyState::NotRun))),
            stop: Arc::new(AtomicBool::new(false)),
            identity: None,
        }
    }

    /// Open the store, classify its meta, run the chain check, open the project on the same store
    /// (a second handle only when the project does not open, for the raw views), and start
    /// Verify.
    fn load(&mut self, open: impl Fn() -> Result<Store, ecad_file::v2::StoreError>) {
        let first = match open() {
            Ok(s) => s,
            Err(e) => {
                self.problem = Some(Problem::new(
                    "unreadable",
                    format!("The database could not be read: {e}."),
                    "Check the app opens it; otherwise report the file.",
                ));
                return;
            }
        };
        if let Some(p) = classify_meta(&first) {
            self.problem = Some(p);
            self.store = Some(Arc::new(first));
            return;
        }
        self.chain = Some(verify_chain(&first));
        let store = match ProjectV2::open(first, 0x1A5_EC7, "ecadinspect") {
            Ok(o) => {
                let branch = o.project.branch_record().name.clone();
                self.opened = Ok(Opened { doc: o.ledger.document().clone(), report: o.report, branch });
                o.project.store().clone()
            }
            Err(e) => {
                self.opened = Err(Problem::new("unavailable", format!("The project did not open: {e}."), OPEN_WAY_OUT));
                match open() {
                    Ok(s) => Arc::new(s),
                    Err(_) => return,
                }
            }
        };
        self.store = Some(store.clone());
        // Verify on its own thread (design 029 §3.4): nothing waits for it. A replaced source drops
        // its result with it.
        let slot = self.verify.clone();
        let stop = self.stop.clone();
        if let Ok(mut g) = slot.lock() {
            *g = (Instant::now(), VerifyState::Running { elapsed_ms: 0 });
        }
        let spawned = std::thread::Builder::new().name("ecadinspect-verify".into()).spawn(move || {
            let started = Instant::now();
            // Stopped when the source is dropped: nobody reads the result any more.
            let Some(report) = verify_stoppable(&store, &|| stop.load(Ordering::Relaxed)) else { return };
            let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            if let Ok(mut g) = slot.lock() {
                g.1 = VerifyState::Done { elapsed_ms, report };
            }
        });
        if spawned.is_err()
            && let Ok(mut g) = self.verify.lock()
        {
            g.1 = VerifyState::NotRun;
        }
    }

    /// Wait for Verify to finish (tests and `ecadinspect verify`-like callers), at most `limit`.
    pub fn wait_verified(&self, limit: std::time::Duration) -> VerifyState {
        // A limit too far ahead to add is no limit.
        let until = Instant::now().checked_add(limit);
        loop {
            let s = self.verify();
            if !matches!(s, VerifyState::Running { .. }) || until.is_some_and(|u| Instant::now() >= u) {
                return s;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Whether this source asked its Verify thread to stop (tests).
    pub fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// The path it was opened from, when served by path.
    pub fn path(&self) -> Option<&Path> {
        self.identity.as_ref().map(|(p, _, _)| p.as_path())
    }
}

impl Drop for FileSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Source for FileSource {
    fn describe(&self) -> SourceInfo {
        self.info.clone()
    }

    fn problem(&self) -> Option<&Problem> {
        self.problem.as_ref()
    }

    fn store(&self) -> Option<&Store> {
        self.store.as_deref()
    }

    fn opened(&self) -> Result<&Opened, &Problem> {
        if let Some(p) = &self.problem {
            return Err(p);
        }
        self.opened.as_ref()
    }

    fn chain(&self) -> Option<&VerifyReport> {
        self.chain.as_ref()
    }

    fn verify(&self) -> VerifyState {
        match self.verify.lock() {
            Ok(g) => match &g.1 {
                VerifyState::Running { .. } => VerifyState::Running { elapsed_ms: u64::try_from(g.0.elapsed().as_millis()).unwrap_or(u64::MAX) },
                other => other.clone(),
            },
            Err(_) => VerifyState::NotRun,
        }
    }

    fn head(&self) -> &[u8] {
        &self.head
    }

    fn changed(&self) -> bool {
        match &self.identity {
            Some((path, len, modified)) => identity_of(path) != (*len, *modified),
            None => false,
        }
    }
}
