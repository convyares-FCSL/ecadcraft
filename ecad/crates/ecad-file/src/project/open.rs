//! Opening a project: the container facts, loading a branch from its latest snapshot and tail, and
//! creating a new file.

use std::collections::BTreeMap;
use std::sync::Arc;

use ecad_model::{Base, Document, History, Ledger, ProjectId, Rev};
use serde_json::{Value, json};

use super::lineage::{Line, Lineage, StackState, StoreHistory, encode_tx, line_high, line_state, lineage, load_snapshot, read_rows};
use super::report::{Damage, OpenError, Report, SaveError, Saved};
use super::save::revision_of;
use super::{Opened, Project, Snap};
use crate::FileError;
use crate::chain::{self, ChainKind, Hash};
use crate::chunks::{self, ChunkSet, Dirty, ModelMeta};
use crate::record::SCHEMA_VERSION;
use crate::schema::{
    BranchId, BranchRecord, BranchStatus, CODEC, CONTAINER_VERSION, Cache, FORMAT_PROJECT, Point, RowKind, SNAPSHOT_EVERY_RANGE, STACK_CAP,
    SnapshotRef, VersionRecord, auto_name,
};
use crate::store::{Reader, Store};
use ecad_model::naming::tag::HighWater;

/// The project ID and cache of a file, after checking its container facts.
pub(crate) fn read_facts_pub(r: &Reader) -> Result<(ProjectId, Cache), OpenError> {
    read_facts(r).map(|f| (f.project, f.cache))
}

/// The project ID of a file, after checking its container facts as opening does.
///
/// # Errors
/// Not a version 2 project file this build reads, or its facts are damaged.
pub fn project_id(r: &Reader) -> Result<ProjectId, OpenError> {
    read_facts(r).map(|f| f.project)
}

fn meta_json(r: &Reader, name: &'static str) -> Result<Option<Value>, OpenError> {
    match r.meta(name)? {
        None => Ok(None),
        Some(b) => serde_json::from_slice(&b).map(Some).map_err(|e| OpenError::Damaged(FileError::Meta { name, error: e.to_string() })),
    }
}

/// The container facts, checked as a container header is checked.
struct Facts {
    project: ProjectId,
    cache: Cache,
    created_ms: Option<u64>,
    /// `meta.active`: the branch the project was last shown on (`None`: not recorded or not read).
    active: Option<BranchId>,
    /// Why `meta.active` does not read, when it is recorded but unreadable.
    active_damage: Option<String>,
    /// The `meta` names this build does not read.
    unknown: Vec<ecad_model::document::strict::UnknownWord>,
    /// The file's `schema_version`; not [`SCHEMA_VERSION`] means it opens read-only
    /// ([`format_note`]).
    schema_version: u64,
}

/// `older format version (file 1, this build 2)` / `newer ...`, when `file` is not the build's
/// [`SCHEMA_VERSION`]. Such a file is opened read-only whatever replay found, and never called
/// damaged: its records may mean something else here.
pub(crate) fn format_note(file: u64) -> Option<String> {
    match file.cmp(&SCHEMA_VERSION) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Less => Some(format!("older format version (file {file}, this build {SCHEMA_VERSION})")),
        std::cmp::Ordering::Greater => Some(format!("newer format version (file {file}, this build {SCHEMA_VERSION})")),
    }
}

fn read_facts(r: &Reader) -> Result<Facts, OpenError> {
    // Pre-v1 there is no forward compatibility: a `meta` name this build does not
    // read is an unknown word, so the project opens read-only and names it. Every name this build
    // needs is checked below.
    let unknown = r
        .meta_all()?
        .into_iter()
        .filter(|(name, _)| !crate::schema::META_NAMES.contains(&name.as_str()))
        .map(|(name, _)| ecad_model::document::strict::UnknownWord::MetaName { name })
        .collect();
    let format = meta_json(r, "format")?;
    if format.as_ref().and_then(Value::as_str) != Some(FORMAT_PROJECT) {
        return Err(OpenError::Unsupported(format!("not an ECAD project file (format {format:?})")));
    }
    let cv = meta_json(r, "container_version")?.and_then(|v| v.as_u64());
    if cv != Some(CONTAINER_VERSION) {
        return Err(OpenError::Unsupported(format!("container version {cv:?} is not supported (this build reads {CONTAINER_VERSION})")));
    }
    let sv = meta_json(r, "schema_version")?.and_then(|v| v.as_u64()).unwrap_or(0);
    if sv == 0 {
        return Err(OpenError::Unsupported("the file has no schema version".into()));
    }
    let codec = meta_json(r, "codec")?;
    if codec.as_ref().and_then(Value::as_str) != Some(CODEC) {
        return Err(OpenError::Unsupported(format!("codec {codec:?} is not supported (this build reads {CODEC:?})")));
    }
    let project = meta_json(r, "file_id")?.and_then(|v| v.as_str().and_then(ProjectId::parse)).ok_or(OpenError::Damaged(FileError::FileId))?;
    let cache: Cache = match meta_json(r, "cache")? {
        Some(v) => serde_json::from_value(v).map_err(|e| OpenError::Damaged(FileError::Meta { name: "cache", error: e.to_string() }))?,
        None => Cache::default(),
    };
    if !(SNAPSHOT_EVERY_RANGE.0..=SNAPSHOT_EVERY_RANGE.1).contains(&cache.every) {
        return Err(OpenError::Damaged(FileError::CacheEvery(cache.every)));
    }
    let created_ms = meta_json(r, "created_ms")?.and_then(|v| v.as_u64());
    // An `active` that does not read is damage, reported when the project opens (on `main`).
    let (active, active_damage) = match meta_json(r, "active") {
        Ok(None) => (None, None),
        Ok(Some(v)) => match v.as_str().and_then(BranchId::parse) {
            Some(b) => (Some(b), None),
            None => (None, Some(format!("`meta.active` is not a branch ID ({v})"))),
        },
        Err(e) => (None, Some(format!("`meta.active` does not read ({e})"))),
    };
    Ok(Facts { project, cache, created_ms, active, active_damage, unknown, schema_version: sv })
}

/// [`main_branch`], for Verify: the check that opening applies.
pub(crate) fn main_branch_pub(r: &Reader, project: ProjectId) -> Result<(), OpenError> {
    main_branch(r, project).map(|_| ())
}

/// The original branch, `main`.
fn main_branch(r: &Reader, project: ProjectId) -> Result<(BranchId, BranchRecord), OpenError> {
    let id = BranchId::main_of(project.0);
    let rec = r.branch(id)?.ok_or(OpenError::Damaged(FileError::NoMain))?;
    if rec.name != "main" || rec.parent.is_some() || rec.fork_point.is_some() || rec.status != BranchStatus::Open {
        return Err(OpenError::Damaged(FileError::MainInconsistent));
    }
    Ok((id, rec))
}

/// What loading one branch gave.
pub(super) struct Loaded {
    pub(super) ledger: Ledger,
    pub(super) report: Report,
    pub(super) lineage: Lineage,
    pub(super) head: (u64, Hash),
    pub(super) snap: Snap,
    pub(super) base_tag_high: HighWater,
    pub(super) snap_ref: Option<SnapshotRef>,
    pub(super) base_stacks: StackState,
    pub(super) damage: Option<String>,
}

/// Load `branch`: its latest snapshot and the transactions after it; a
/// branch with no snapshot of its own starts from its parent's state at the fork point. Damage
/// found loads the last state that verifies, with a report.
pub(super) fn load_branch(
    store: &Arc<Store>, project: ProjectId, cache: &Cache, branch: BranchId, rec: &BranchRecord, seed: u64,
) -> Result<Loaded, OpenError> {
    let r = store.reader()?;
    let lineage = lineage(&r, project, branch).map_err(OpenError::Damaged)?;
    let own = lineage.first().cloned().ok_or(OpenError::Damaged(FileError::EmptyLineage))?;
    let seed_chain = own.seed;
    let head = rec.head.seq;
    let head_chain = rec.head.hash().map_err(OpenError::Damaged)?;
    let mut damage = None;
    // The head must be the branch's last row, with the recorded chain value.
    let last = r.last_seq(branch)?.unwrap_or(0);
    if last != head {
        damage = Some(Damage { seq: Some(head), detail: format!("the head is {head} but the last stored transaction is {last}") });
    } else if head > 0 && r.tx(branch, head)?.map(|(_, c, _)| c) != Some(head_chain) {
        damage = Some(Damage { seq: Some(head), detail: "the head transaction does not match the branch head".into() });
    } else if head == 0 && head_chain != seed_chain {
        damage = Some(Damage { seq: Some(0), detail: "the branch head does not match its fork point".into() });
    }
    // The latest snapshot at or before the head.
    let latest = cache.latest.get(&branch.hex()).filter(|s| s.seq <= head);
    let mut loaded = None;
    let mut snap_ref = None;
    if let Some(s) = latest {
        match r.version(branch, s.kind, &s.name)? {
            Some(v) if v.point.seq == s.seq => match load_snapshot(&r, &v) {
                Ok((doc, meta)) => {
                    // Every invariant of the snapshot is checked in debug builds and after a
                    // crash repair (about 38 ms on the large project, so not on every normal
                    // release open). Its chunks were checked against their hashes in any case,
                    // and Verify checks everything on request.
                    let checked = if cfg!(debug_assertions) || store.repaired() { doc.check() } else { Ok(()) };
                    match checked {
                        Ok(()) => {
                            loaded = Some((v, doc, meta));
                            snap_ref = Some(s.clone());
                        }
                        Err(e) => damage = damage.or(Some(Damage { seq: Some(s.seq), detail: format!("snapshot breaks an invariant: {e}") })),
                    }
                }
                Err(e) => damage = damage.or(Some(Damage { seq: Some(s.seq), detail: format!("snapshot: {e}") })),
            },
            _ => damage = damage.or(Some(Damage { seq: Some(s.seq), detail: "the latest snapshot is missing".into() })),
        }
    }
    // Start from the snapshot; if anything about it fails (its point row, its stack record,
    // the rows under it), open from the branch's start instead (the project root, or the
    // parent's state at the fork point), read-only with the damage reported.
    let to = head.min(last);
    let (ledger, failure, snap_seq, snap_chain, stacks, set, base_tag_high, tail_len, verified_chain, row_damage) = loop {
        let using = loaded.take();
        let from_snapshot = using.as_ref().map(|(v, _, _)| v.point.seq);
        let attempt = (|| -> Result<_, FileError> {
            let (snap_seq, snap_chain, doc, stacks, set, high) = match using {
                Some((v, doc, meta)) => {
                    let chain = v.point.hash()?;
                    let stacks = StackState::from_record(&v)?;
                    let set = ChunkSet::from_record(&v.chunks)?;
                    // The snapshot's point must be the stored row's chain value.
                    if r.tx(branch, v.point.seq)?.map(|(_, c, _)| c) != Some(chain) {
                        return Err(FileError::SnapshotPoint);
                    }
                    // The tag high-water mark at the snapshot: carried by `model:meta`; a
                    // snapshot written without it is scanned once (a cache, rebuilt from the
                    // history).
                    // Marks of an older rule (written before cable tags were counted) are
                    // recomputed once, the same way.
                    let high = match meta {
                        Some(m) if v.tag_marks_current() => m.tag_high,
                        _ => line_high(&r, &lineage, 0, v.point.seq)?,
                    };
                    (v.point.seq, chain, doc, stacks, Some(set), high)
                }
                None => match own.fork {
                    // A branch without a snapshot of its own: its parent's state at the fork.
                    Some((pi, fseq)) => {
                        let (doc, high) = line_state(&r, &lineage, pi, fseq)?;
                        (0, seed_chain, doc, StackState::default(), None, high)
                    }
                    None => (0, seed_chain, Document::default(), StackState::default(), Some(ChunkSet::default()), HighWater::new()),
                },
            };
            let (tail, verified_chain, row_damage) = read_rows(&r, branch, snap_seq.saturating_add(1), to, snap_chain)?;
            let history: Arc<dyn History> =
                Arc::new(StoreHistory { store: Arc::downgrade(store), branch, seed: seed_chain, lineage: lineage.clone() });
            let base = Base {
                seq: snap_seq,
                doc,
                undo: stacks.undo.clone(),
                redo: stacks.redo.clone(),
                undo_hidden: stacks.undo_hidden,
                redo_hidden: stacks.redo_hidden,
                stack_cap: stacks.cap,
                history,
            };
            let tail_len = tail.len() as u64;
            let (ledger, failure) = Ledger::from_base(base, tail, seed).map_err(|error| FileError::SnapshotAt { seq: snap_seq, error })?;
            Ok((ledger, failure, snap_seq, snap_chain, stacks, set, high, tail_len, verified_chain, row_damage))
        })();
        match (attempt, from_snapshot) {
            (Ok(done), _) => break done,
            (Err(e), Some(seq)) => {
                damage = damage.or(Some(Damage { seq: Some(seq), detail: format!("snapshot: {e}") }));
                snap_ref = None;
            }
            (Err(e), None) => return Err(OpenError::Damaged(e)),
        }
    };
    drop(r);
    let damage = damage.or(row_damage);
    let damage =
        damage.or(failure.map(|(i, e)| Damage { seq: snap_seq.checked_add(i as u64).and_then(|n| n.checked_add(1)), detail: e.to_string() }));
    let verified = ledger.head();
    let verified_chain = if verified == snap_seq + tail_len {
        verified_chain
    } else {
        // Replay stopped early: the chain value of the last good row.
        store.reader()?.tx(branch, verified)?.map(|(_, c, _)| c).unwrap_or(snap_chain)
    };
    let damage_text = damage.as_ref().map(|d| format!("the file is damaged ({})", d.detail));
    let report = Report {
        head,
        verified_seq: verified,
        snapshot_seq: snap_seq,
        tail: verified - snap_seq,
        repaired: store.repaired(),
        damage,
        schema_version: 0,
    };
    Ok(Loaded {
        ledger,
        report,
        lineage,
        head: (verified, verified_chain),
        snap: Snap { seq: snap_seq, set },
        base_tag_high,
        snap_ref,
        base_stacks: stacks,
        damage: damage_text,
    })
}

impl Project {
    /// Open the project in `store` on the branch it was last shown on
    /// (`meta.active`), or `main` when that is absent, unknown or not open. `seed` seeds IDs
    /// allocated after opening. Damage found while loading opens read-only, at the last state that
    /// verifies, with a report; so does a `meta.active` that does not read (on `main`).
    ///
    /// # Errors
    /// [`OpenError`] when the container facts or the branch row are unusable.
    pub fn open(store: Store, seed: u64, writer: &str) -> Result<Opened, OpenError> {
        let store = Arc::new(store);
        let r = store.reader()?;
        let facts = read_facts(&r)?;
        let (main, main_rec) = main_branch(&r, facts.project)?;
        let (branch, rec) = match facts.active.filter(|a| *a != main) {
            Some(a) => match r.branch(a)? {
                Some(rec) if rec.status == BranchStatus::Open => (a, rec),
                _ => (main, main_rec),
            },
            None => (main, main_rec),
        };
        drop(r);
        let note = format_note(facts.schema_version);
        let mut l = match load_branch(&store, facts.project, &facts.cache, branch, &rec, seed) {
            Ok(l) => l,
            // A file of another format that cannot be read at all is refused in the same words.
            Err(e) => match &note {
                Some(n) => return Err(OpenError::Unsupported(format!("{n}: {e}"))),
                None => return Err(e),
            },
        };
        l.report.schema_version = facts.schema_version;
        if let Some(detail) = facts.active_damage
            && l.damage.is_none()
        {
            l.damage = Some(format!("the file is damaged ({detail})"));
            l.report.damage = Some(Damage { seq: None, detail });
        }
        // Unknown `meta` names keep the file exactly as it is: nothing writes to it, not even a
        // branch switch's `meta.active`.
        let unknown_words = facts.unknown;
        let unknown_read_only = (!unknown_words.is_empty()).then(|| crate::disk::ReadOnly::UnknownWords(unknown_words.clone()).to_string());
        let file_read_only = (!store.writable()).then(|| "it is open in another session".to_string()).or(unknown_read_only);
        // Another format version is read-only whatever replay found: the version comes first, and
        // nothing writes to the file, not even a branch switch's `meta.active`.
        let file_read_only = note.clone().or(file_read_only);
        let read_only = note.or_else(|| l.damage.clone()).or_else(|| file_read_only.clone());
        let project = Project {
            store,
            project: facts.project,
            branch,
            rec,
            lineage: l.lineage,
            head: l.head,
            cache: facts.cache,
            snap: l.snap,
            base_tag_high: l.base_tag_high,
            snap_ref: l.snap_ref,
            base_stacks: l.base_stacks,
            base_seq: l.report.snapshot_seq,
            writer: writer.to_string(),
            read_only,
            file_read_only,
            created_ms: facts.created_ms,
            last_save: None,
            wrote_writer: false,
            closing: BTreeMap::new(),
            reopened: BTreeMap::new(),
        };
        Ok(Opened { project, ledger: l.ledger, report: l.report, unknown_words })
    }

    /// Write `ledger`'s whole history into the empty database `store` as a new project file:
    /// `main` from the project root, a snapshot for every named version, and a snapshot at the
    /// head.
    ///
    /// # Errors
    /// [`SaveError`]; nothing is committed. `every` outside [`SNAPSHOT_EVERY_RANGE`] is refused
    /// (an open would refuse the file).
    pub fn create(
        store: Store, project: ProjectId, ledger: &Ledger, created_ms: Option<u64>, writer: &str, every: u64,
    ) -> Result<(Project, Saved), SaveError> {
        if ledger.base() != 0 {
            return Err(SaveError::Encode(FileError::WholeHistory));
        }
        if !(SNAPSHOT_EVERY_RANGE.0..=SNAPSHOT_EVERY_RANGE.1).contains(&every) {
            return Err(SaveError::Encode(FileError::SnapshotEvery));
        }
        let store = Arc::new(store);
        let branch = BranchId::main_of(project.0);
        let seed = chain::seed(ChainKind::Project, project.0);
        let mut w = store.writer()?;
        let mut saved = Saved { first_seq: 1, last_seq: ledger.head(), ..Saved::default() };
        // Rows, and the chain value of every point.
        let mut chains = Vec::with_capacity(ledger.log().len() + 1);
        chains.push(seed);
        let mut prev = seed;
        for tx in ledger.log() {
            let (bytes, hash) = encode_tx(tx, &prev).map_err(SaveError::Encode)?;
            saved.bytes += w.append_tx(branch, tx.seq, &hash, &bytes)?;
            chains.push(hash);
            prev = hash;
        }
        // Version snapshots, then the head: one forward pass over the log.
        let doc_end = ledger.document();
        let mut points: Vec<(u64, RowKind, String)> = Vec::new();
        for (name, at) in doc_end.versions() {
            points.push((*at, RowKind::of(name), name.key()));
        }
        points.push((ledger.head(), RowKind::Auto, auto_name(ledger.head())));
        points.sort();
        let mut doc = Document::default();
        let mut stacks = StackState::default();
        let mut done = 0u64;
        let mut snap = (0u64, ChunkSet::default());
        let mut first = true;
        let mut latest: Option<SnapshotRef> = None;
        let mut high = HighWater::new();
        for (at, kind, name) in points {
            let range = ledger.log().get(done as usize..at as usize).unwrap_or_default();
            let mut dirty = Dirty::default();
            for tx in range {
                for op in &tx.ops {
                    op.apply(&mut doc).map_err(|error| SaveError::Encode(FileError::Replay { seq: tx.seq, error }))?;
                }
                dirty.add_ops(&tx.ops);
                ecad_model::naming::tag::note_ops(&mut high, &tx.ops);
            }
            stacks.advance(range);
            done = at;
            let meta = ModelMeta { tag_high: high.clone() };
            let (set, new) = if first { chunks::build_full(&doc, &meta) } else { chunks::build_incremental(&doc, &snap.1, &dirty, &meta) }
                .map_err(SaveError::Encode)?;
            first = false;
            for p in &new {
                let n = w.put_chunk(&p.hash, &p.body)?;
                saved.bytes += n;
                saved.chunks_written += usize::from(n > 0);
            }
            let chain = chains.get(at as usize).copied().unwrap_or(seed);
            let release = Rev::from_key(&name).filter(|_| kind == RowKind::REVISION).and_then(|rev| revision_of(doc_end, rev));
            let rec = stacks.record();
            w.put_version(
                branch,
                &VersionRecord {
                    kind,
                    name: name.clone(),
                    point: Point::new(at, &chain),
                    chunks: set.to_record(),
                    release,
                    undo: rec.0,
                    redo: rec.1,
                    undo_hidden: rec.2,
                    redo_hidden: rec.3,
                    stack_cap: STACK_CAP,
                    description: None,
                    merge: None,
                    tag_rule: Some(ecad_model::naming::tag::TAG_RULE),
                },
            )?;
            saved.snapshots += 1;
            latest = Some(SnapshotRef { kind, name, seq: at });
            snap = (at, set);
        }
        let mut cache = Cache { every, ..Cache::default() };
        if let Some(l) = &latest {
            cache.latest.insert(branch.hex(), l.clone());
        }
        let head = (ledger.head(), prev);
        let rec = BranchRecord::main(Point::new(head.0, &head.1));
        w.put_branch(branch, &rec)?;
        w.set_meta("format", &json!(FORMAT_PROJECT))?;
        w.set_meta("container_version", &json!(CONTAINER_VERSION))?;
        w.set_meta("schema_version", &json!(SCHEMA_VERSION))?;
        w.set_meta("codec", &json!(CODEC))?;
        w.set_meta("file_id", &json!(project.hex()))?;
        w.set_meta("writer", &json!(writer))?;
        w.set_meta("created_ms", &json!(created_ms))?;
        w.set_meta("cache", &serde_json::to_value(&cache).map_err(|e| SaveError::Encode(FileError::json(&e)))?)?;
        w.commit()?;
        let lineage = Arc::new(vec![Line { branch, seed, fork: None }]);
        let me = Project {
            store,
            project,
            branch,
            rec,
            lineage,
            head,
            cache,
            snap: Snap { seq: snap.0, set: Some(snap.1) },
            base_tag_high: HighWater::new(),
            snap_ref: latest,
            base_stacks: StackState::default(),
            base_seq: 0,
            writer: writer.to_string(),
            read_only: None,
            file_read_only: None,
            created_ms,
            last_save: Some(saved.clone()),
            wrote_writer: true,
            closing: BTreeMap::new(),
            reopened: BTreeMap::new(),
        };
        Ok((me, saved))
    }
}
