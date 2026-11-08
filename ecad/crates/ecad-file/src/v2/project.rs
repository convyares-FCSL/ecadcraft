//! A project in a version 2 file: opening from the latest snapshot and its tail, and saving
//! (EEP-0005 §5–§6).
//!
//! - **Open** reads `meta`, the branch row and its head transaction, the latest snapshot's parts
//!   (phase 1: the whole active snapshot, overseer 2026-11-08) and the transactions after it,
//!   each checked against its chain value and replayed with every check. It never reads other
//!   branches, older versions, transactions before the snapshot, or assets. Older transactions are
//!   read on demand through [`StoreHistory`] (undo past the snapshot, `state_at`).
//! - **Save** writes the pending transactions, the snapshots of any versions they name and of an
//!   automatic snapshot every `cache.every` transactions, the branch head and `meta` in one redb
//!   write transaction. Parts are built incrementally from the last snapshot ([`parts::Dirty`]).
//!   Nothing stored is read back to check it; its cost follows what is written.

use std::collections::BTreeMap;
use std::sync::Arc;

use ecad_model::{Base, Document, History, Ledger, ModelError, ObjectRef, Op, ProjectId, Transaction, TxId, TxKind};
use serde_json::{Value, json};

use super::pack;
use super::parts::{self, Dirty, PartSet};
use super::schema::{
    BranchId, BranchRecord, BranchStatus, CODEC, CONTAINER_VERSION, Cache, FORMAT_PROJECT, LinkV1, MAX_NAME,
    META_NAMES, Point, Release, SNAPSHOT_EVERY_RANGE, SnapshotRef, VersionKind, VersionRecord, auto_name,
};
use super::store::{Reader, Store, StoreError};
use crate::chain::{self, ChainKind, Hash};
use crate::record::{self, SCHEMA_VERSION};

/// Why a version 2 file could not be opened at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenErrorV2 {
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("{0}")]
    Unsupported(String),
    /// The container facts or the branch are unusable.
    #[error("the file is damaged: {0}")]
    Damaged(String),
}

/// Why a save did not complete. The file then holds exactly what it held before, and the
/// project is still unsaved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SaveErrorV2 {
    #[error("the file was opened read-only: {0}")]
    ReadOnly(String),
    #[error("the file was changed by something else ({0}); it was not modified; use Save As")]
    ExternalChange(String),
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("could not encode: {0}")]
    Encode(String),
}

/// What a successful save wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedV2 {
    pub first_seq: u64,
    pub last_seq: u64,
    /// Bytes of new rows and parts (before redb's own overhead).
    pub bytes: u64,
    pub parts_written: usize,
    /// Snapshots written (versions and automatic).
    pub snapshots: usize,
}

/// What a check found wrong while opening (EEP-0005 §5.8): the project opens read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageV2 {
    /// The transaction (or snapshot point) that failed, when known.
    pub seq: Option<u64>,
    pub detail: String,
}

/// What opening found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportV2 {
    /// The branch's recorded head.
    pub head: u64,
    /// The last transaction that verified (equal to `head` unless damaged).
    pub verified_seq: u64,
    /// The snapshot the project was opened from (0: the project root).
    pub snapshot_seq: u64,
    /// Transactions replayed after the snapshot.
    pub tail: u64,
    /// redb repaired the file while opening it (it was not closed cleanly).
    pub repaired: bool,
    pub damage: Option<DamageV2>,
    /// Converted from a version 1 file.
    pub link_v1: Option<LinkV1>,
}

impl ReportV2 {
    /// One paragraph for the user.
    pub fn describe(&self) -> String {
        let mut s = format!(
            "Opened: {} transactions (from the snapshot at {}, then {} replayed).",
            self.verified_seq, self.snapshot_seq, self.tail
        );
        if self.repaired {
            s.push_str(" The file was not closed cleanly last time and was recovered to its last save.");
        }
        if let Some(d) = &self.damage {
            s.push_str(&format!(
                " Opened read-only: {}{}. {} of {} transactions verified; use Save As to keep them.",
                d.seq.map_or(String::new(), |q| format!("at transaction {q}, ")),
                d.detail,
                self.verified_seq,
                self.head
            ));
        }
        s
    }
}

/// A transaction row decoded: the transaction, its record bytes and its chain value.
fn decode_row(seq: u64, flag: u8, stored_chain: &Hash, body: &[u8], prev: &Hash) -> Result<Transaction, String> {
    let record = pack::unpack(flag, body)?;
    if chain::tx_hash(&record) != *stored_chain {
        return Err("the record does not match its chain value".into());
    }
    let value: Value = serde_json::from_slice(&record).map_err(|e| format!("not a record: {e}"))?;
    record::check_limits(&value)?;
    let (rseq, rprev) = record::seq_and_prev(&value);
    if rseq != Some(seq) {
        return Err(format!("the row of {seq} holds transaction {rseq:?}"));
    }
    if rprev.and_then(chain::parse_hex) != Some(*prev) {
        return Err("prev does not link to the previous transaction".into());
    }
    record::decode_tx(&value).map_err(|e| format!("{e:?}"))
}

/// The canonical record and chain value of `tx` after `prev`.
fn encode_tx(tx: &Transaction, prev: &Hash) -> Result<(Vec<u8>, Hash), String> {
    let value = record::tx_value(tx, prev)?;
    let canonical = crate::canonical::canonical(&value).map_err(|e| e.to_string())?;
    let hash = chain::tx_hash(&canonical);
    Ok((canonical, hash))
}

/// The undo and redo stacks after `txs`, starting from `stacks` (only kinds and IDs are read).
fn advance_stacks(stacks: &mut (Vec<(u64, TxId)>, Vec<(u64, TxId)>), txs: &[Transaction]) {
    for tx in txs {
        match tx.kind {
            TxKind::Edit => {
                stacks.0.push((tx.seq, tx.id));
                stacks.1.clear();
            }
            TxKind::Undo { .. } => {
                stacks.0.pop();
                stacks.1.push((tx.seq, tx.id));
            }
            TxKind::Redo { .. } => {
                stacks.1.pop();
                stacks.0.push((tx.seq, tx.id));
            }
        }
    }
}

fn stacks_record(s: &[(u64, TxId)]) -> Vec<(u64, String)> {
    s.iter().map(|(q, id)| (*q, format!("{:032x}", id.0))).collect()
}

fn stacks_from_record(s: &[(u64, String)]) -> Result<Vec<(u64, TxId)>, String> {
    s.iter()
        .map(|(q, id)| TxId::parse(id).map(|t| (*q, t)).ok_or_else(|| format!("{id:?} is not a transaction ID")))
        .collect()
}

/// Read a snapshot's document: every part, each checked against its hash.
fn load_snapshot(r: &Reader, v: &VersionRecord) -> Result<Document, String> {
    let set = PartSet::from_record(&v.parts)?;
    let mut bodies = Vec::with_capacity(set.parts.len());
    for (key, hash) in &set.parts {
        bodies.push((key.clone(), r.part_body(hash).map_err(|e| e.to_string())?));
    }
    parts::assemble(&set, bodies)
}

/// Rows `from..=to` of `branch`, decoded and chain-checked from `prev`. Stops at the first bad
/// row: the good prefix, and the failure.
fn read_rows(
    r: &Reader,
    branch: BranchId,
    from: u64,
    to: u64,
    mut prev: Hash,
) -> Result<(Vec<Transaction>, Hash, Option<DamageV2>), StoreError> {
    let rows = r.tx_range(branch, from..=to)?;
    let mut out = Vec::with_capacity(rows.len());
    let mut expected = from;
    for (seq, flag, stored, body) in rows {
        if seq != expected {
            return Ok((
                out,
                prev,
                Some(DamageV2 { seq: Some(expected), detail: "the transaction is missing".into() }),
            ));
        }
        match decode_row(seq, flag, &stored, &body, &prev) {
            Ok(tx) => {
                out.push(tx);
                prev = stored;
            }
            Err(detail) => return Ok((out, prev, Some(DamageV2 { seq: Some(seq), detail }))),
        }
        expected += 1;
    }
    if expected <= to {
        return Ok((out, prev, Some(DamageV2 { seq: Some(expected), detail: "the transaction is missing".into() })));
    }
    Ok((out, prev, None))
}

/// Reads the history before a ledger's base from the file, on demand (never on open).
#[derive(Debug)]
pub struct StoreHistory {
    store: Arc<Store>,
    branch: BranchId,
    seed: Hash,
}

impl StoreHistory {
    fn read(&self) -> ecad_model::Result<Reader> {
        self.store.reader().map_err(|e| ModelError::CorruptLog(e.to_string()))
    }

    /// The chain value before `seq` (the seed before 1).
    fn prev_of(&self, r: &Reader, seq: u64) -> ecad_model::Result<Hash> {
        if seq <= 1 {
            return Ok(self.seed);
        }
        r.tx(self.branch, seq - 1)
            .map_err(|e| ModelError::CorruptLog(e.to_string()))?
            .map(|(_, c, _)| c)
            .ok_or_else(|| ModelError::CorruptLog(format!("transaction {} is missing", seq - 1)))
    }
}

impl History for StoreHistory {
    fn transaction(&self, seq: u64) -> ecad_model::Result<Transaction> {
        let r = self.read()?;
        let prev = self.prev_of(&r, seq)?;
        let (flag, chain, body) = r
            .tx(self.branch, seq)
            .map_err(|e| ModelError::CorruptLog(e.to_string()))?
            .ok_or_else(|| ModelError::CorruptLog(format!("transaction {seq} is missing")))?;
        decode_row(seq, flag, &chain, &body, &prev)
            .map_err(|e| ModelError::CorruptLog(format!("transaction {seq}: {e}")))
    }

    fn state_at(&self, seq: u64) -> ecad_model::Result<Document> {
        let r = self.read()?;
        let err = |e: String| ModelError::CorruptLog(e);
        // The nearest snapshot at or before `seq`: an automatic one, or a named version.
        let mut best: Option<VersionRecord> = r.auto_at_or_before(self.branch, seq).map_err(|e| err(e.to_string()))?;
        for kind in [VersionKind::User, VersionKind::Revision] {
            for v in r.versions(self.branch, kind).map_err(|e| err(e.to_string()))? {
                if v.point.seq <= seq && best.as_ref().is_none_or(|b| b.point.seq < v.point.seq) {
                    best = Some(v);
                }
            }
        }
        let (mut doc, from, prev) = match &best {
            Some(v) => (load_snapshot(&r, v).map_err(err)?, v.point.seq, v.point.hash().map_err(err)?),
            None => (Document::default(), 0, self.seed),
        };
        let (txs, _, damage) = read_rows(&r, self.branch, from + 1, seq, prev).map_err(|e| err(e.to_string()))?;
        if let Some(d) = damage {
            return Err(err(format!("transaction {:?}: {}", d.seq, d.detail)));
        }
        doc.rebuild_index();
        for tx in &txs {
            for op in &tx.ops {
                op.apply(&mut doc)?;
            }
        }
        doc.check()?;
        Ok(doc)
    }
}

/// The in-memory snapshot the next one is built from.
#[derive(Debug, Clone)]
struct Snap {
    seq: u64,
    set: PartSet,
}

/// A project bound to a version 2 database.
#[derive(Debug)]
pub struct ProjectV2 {
    store: Arc<Store>,
    project: ProjectId,
    branch: BranchId,
    /// The branch head in the file: `seq` and chain value.
    head: (u64, Hash),
    cache: Cache,
    snap: Snap,
    /// The `versions` row of the snapshot opened from or last written (`None`: the root).
    snap_ref: Option<SnapshotRef>,
    /// The undo and redo stacks at the ledger's base (the snapshot opened from).
    base_stacks: (Vec<(u64, TxId)>, Vec<(u64, TxId)>),
    base_seq: u64,
    writer: String,
    read_only: Option<String>,
    link_v1: Option<LinkV1>,
    created_ms: Option<u64>,
    last_save: Option<SavedV2>,
}

/// A project opened from a version 2 database.
#[derive(Debug)]
pub struct OpenedV2 {
    pub project: ProjectV2,
    pub ledger: Ledger,
    pub report: ReportV2,
}

fn meta_json(r: &Reader, name: &str) -> Result<Option<Value>, OpenErrorV2> {
    match r.meta(name)? {
        None => Ok(None),
        Some(b) => serde_json::from_slice(&b).map(Some).map_err(|e| OpenErrorV2::Damaged(format!("meta {name}: {e}"))),
    }
}

/// The container facts, checked as E2 §5 checks a header.
struct Facts {
    project: ProjectId,
    cache: Cache,
    link_v1: Option<LinkV1>,
    created_ms: Option<u64>,
}

fn read_facts(r: &Reader) -> Result<Facts, OpenErrorV2> {
    for (name, _) in r.meta_all()? {
        if !META_NAMES.contains(&name.as_str()) {
            return Err(OpenErrorV2::Unsupported(format!("meta {name:?} is not something this build reads")));
        }
    }
    let format = meta_json(r, "format")?;
    if format.as_ref().and_then(Value::as_str) != Some(FORMAT_PROJECT) {
        return Err(OpenErrorV2::Unsupported(format!("not an ECAD project file (format {format:?})")));
    }
    let cv = meta_json(r, "container_version")?.and_then(|v| v.as_u64());
    if cv != Some(CONTAINER_VERSION) {
        return Err(OpenErrorV2::Unsupported(format!(
            "container version {cv:?} is not supported (this build reads {CONTAINER_VERSION})"
        )));
    }
    let sv = meta_json(r, "schema_version")?.and_then(|v| v.as_u64()).unwrap_or(0);
    if sv == 0 || sv > SCHEMA_VERSION {
        return Err(OpenErrorV2::Unsupported(format!(
            "schema version {sv} is not supported (this build reads 1 to {SCHEMA_VERSION})"
        )));
    }
    let codec = meta_json(r, "codec")?;
    if codec.as_ref().and_then(Value::as_str) != Some(CODEC) {
        return Err(OpenErrorV2::Unsupported(format!("codec {codec:?} is not supported (this build reads {CODEC:?})")));
    }
    let project = meta_json(r, "file_id")?
        .and_then(|v| v.as_str().and_then(ProjectId::parse))
        .ok_or_else(|| OpenErrorV2::Damaged("meta file_id is missing or malformed".into()))?;
    let cache: Cache = match meta_json(r, "cache")? {
        Some(v) => serde_json::from_value(v).map_err(|e| OpenErrorV2::Damaged(format!("meta cache: {e}")))?,
        None => Cache::default(),
    };
    if !(SNAPSHOT_EVERY_RANGE.0..=SNAPSHOT_EVERY_RANGE.1).contains(&cache.every) {
        return Err(OpenErrorV2::Damaged(format!("meta cache: snapshot interval {} is out of range", cache.every)));
    }
    let link_v1 = match meta_json(r, "link_v1")? {
        Some(v) => Some(serde_json::from_value(v).map_err(|e| OpenErrorV2::Damaged(format!("meta link_v1: {e}")))?),
        None => None,
    };
    let created_ms = meta_json(r, "created_ms")?.and_then(|v| v.as_u64());
    Ok(Facts { project, cache, link_v1, created_ms })
}

/// The original branch, `main`.
fn main_branch(r: &Reader, project: ProjectId) -> Result<(BranchId, BranchRecord), OpenErrorV2> {
    let id = BranchId::main_of(project.0);
    let rec = r.branch(id)?.ok_or_else(|| OpenErrorV2::Damaged("the file has no main branch".into()))?;
    if rec.name != "main" || rec.parent.is_some() || rec.fork_point.is_some() || rec.status != BranchStatus::Open {
        return Err(OpenErrorV2::Damaged("the main branch's record is inconsistent".into()));
    }
    Ok((id, rec))
}

impl ProjectV2 {
    /// Open the project in `store` (EEP-0005 §5). `seed` seeds IDs allocated after opening.
    /// Damage found while loading opens read-only, at the last state that verifies, with a report.
    ///
    /// # Errors
    /// [`OpenErrorV2`] when the container facts or the branch row are unusable.
    pub fn open(store: Store, seed: u64, writer: &str) -> Result<OpenedV2, OpenErrorV2> {
        let store = Arc::new(store);
        let r = store.reader()?;
        let facts = read_facts(&r)?;
        let (branch, rec) = main_branch(&r, facts.project)?;
        let seed_chain = chain::seed(ChainKind::Project, facts.project.0);
        let head = rec.head.seq;
        let head_chain = rec.head.hash().map_err(OpenErrorV2::Damaged)?;
        let mut damage = None;
        // The head must be the branch's last row, with the recorded chain value.
        let last = r.last_seq(branch)?.unwrap_or(0);
        if last != head {
            damage = Some(DamageV2 {
                seq: Some(head),
                detail: format!("the head is {head} but the last stored transaction is {last}"),
            });
        } else if head > 0 && r.tx(branch, head)?.map(|(_, c, _)| c) != Some(head_chain) {
            damage = Some(DamageV2 {
                seq: Some(head),
                detail: "the head transaction does not match the branch head".into(),
            });
        }
        // The latest snapshot at or before the head.
        let latest = facts.cache.latest.get(&branch.hex()).filter(|s| s.seq <= head);
        let mut loaded = None;
        let mut snap_ref = None;
        if let Some(s) = latest {
            match r.version(branch, s.kind, &s.name)? {
                Some(v) if v.point.seq == s.seq => match load_snapshot(&r, &v) {
                    Ok(doc) => {
                        loaded = Some((v, doc));
                        snap_ref = Some(s.clone());
                    }
                    Err(e) => damage = damage.or(Some(DamageV2 { seq: Some(s.seq), detail: format!("snapshot: {e}") })),
                },
                _ => {
                    damage =
                        damage.or(Some(DamageV2 { seq: Some(s.seq), detail: "the latest snapshot is missing".into() }))
                }
            }
        }
        let (snap_seq, snap_chain, doc, stacks, set) = match loaded {
            Some((v, doc)) => {
                let chain = v.point.hash().map_err(OpenErrorV2::Damaged)?;
                let undo = stacks_from_record(&v.undo).map_err(OpenErrorV2::Damaged)?;
                let redo = stacks_from_record(&v.redo).map_err(OpenErrorV2::Damaged)?;
                let set = PartSet::from_record(&v.parts).map_err(OpenErrorV2::Damaged)?;
                (v.point.seq, chain, doc, (undo, redo), set)
            }
            None => (0, seed_chain, Document::default(), (Vec::new(), Vec::new()), PartSet::default()),
        };
        // The snapshot's point must be the stored row's chain value.
        if snap_seq > 0 && r.tx(branch, snap_seq)?.map(|(_, c, _)| c) != Some(snap_chain) {
            damage = damage.or(Some(DamageV2 {
                seq: Some(snap_seq),
                detail: "the snapshot's point does not match its transaction".into(),
            }));
        }
        let to = head.min(last);
        let (tail, verified_chain, row_damage) = read_rows(&r, branch, snap_seq + 1, to, snap_chain)?;
        drop(r);
        let damage = damage.or(row_damage);
        let history: Arc<dyn History> = Arc::new(StoreHistory { store: store.clone(), branch, seed: seed_chain });
        let base = Base { seq: snap_seq, doc, undo: stacks.0.clone(), redo: stacks.1.clone(), history };
        let tail_len = tail.len() as u64;
        let (ledger, failure) = Ledger::from_base(base, tail, seed)
            .map_err(|e| OpenErrorV2::Damaged(format!("the snapshot at {snap_seq}: {e}")))?;
        let damage =
            damage.or(failure.map(|(i, e)| DamageV2 { seq: Some(snap_seq + i as u64 + 1), detail: e.to_string() }));
        let verified = ledger.head();
        let verified_chain = if verified == snap_seq + tail_len {
            verified_chain
        } else {
            // Replay stopped early: the chain value of the last good row.
            store.reader()?.tx(branch, verified)?.map(|(_, c, _)| c).unwrap_or(snap_chain)
        };
        let read_only = damage.as_ref().map(|d| format!("the file is damaged ({})", d.detail));
        let read_only = read_only.or_else(|| (!store.writable()).then(|| "it is open in another session".to_string()));
        let report = ReportV2 {
            head,
            verified_seq: verified,
            snapshot_seq: snap_seq,
            tail: verified - snap_seq,
            repaired: store.repaired(),
            damage,
            link_v1: facts.link_v1.clone(),
        };
        let project = ProjectV2 {
            store,
            project: facts.project,
            branch,
            head: (verified, verified_chain),
            cache: facts.cache,
            snap: Snap { seq: snap_seq, set },
            snap_ref,
            base_stacks: stacks,
            base_seq: snap_seq,
            writer: writer.to_string(),
            read_only,
            link_v1: facts.link_v1,
            created_ms: facts.created_ms,
            last_save: None,
        };
        Ok(OpenedV2 { project, ledger, report })
    }

    /// Write `ledger`'s whole history into the empty database `store` as a new project file:
    /// `main` from the project root, a snapshot for every named version, and a snapshot at the
    /// head. `link` names the version 1 history it continues, when converted; `records` are then
    /// the version 1 records exactly as stored (canonical), so the chain is version 1's.
    ///
    /// # Errors
    /// [`SaveErrorV2`]; nothing is committed.
    pub fn create(
        store: Store,
        project: ProjectId,
        ledger: &Ledger,
        records: Option<&[Vec<u8>]>,
        link: Option<LinkV1>,
        created_ms: Option<u64>,
        writer: &str,
        every: u64,
    ) -> Result<(ProjectV2, SavedV2), SaveErrorV2> {
        if ledger.base() != 0 {
            return Err(SaveErrorV2::Encode("a new file needs the whole history in memory".into()));
        }
        let store = Arc::new(store);
        let branch = BranchId::main_of(project.0);
        let seed = chain::seed(ChainKind::Project, project.0);
        let mut w = store.writer()?;
        let mut saved = SavedV2 { first_seq: 1, last_seq: ledger.head(), ..SavedV2::default() };
        // Rows, and the chain value of every point.
        let mut chains = Vec::with_capacity(ledger.log().len() + 1);
        chains.push(seed);
        let mut prev = seed;
        for (i, tx) in ledger.log().iter().enumerate() {
            let (bytes, hash) = match records.and_then(|r| r.get(i)) {
                Some(b) => (b.clone(), chain::tx_hash(b)),
                None => encode_tx(tx, &prev).map_err(SaveErrorV2::Encode)?,
            };
            saved.bytes += w.append_tx(branch, tx.seq, &hash, &bytes)?;
            chains.push(hash);
            prev = hash;
        }
        // Version snapshots, then the head: one forward pass over the log.
        let doc_end = ledger.document();
        let mut points: Vec<(u64, VersionKind, String)> = Vec::new();
        for (name, at) in &doc_end.versions {
            let kind = if revision_of(doc_end, name).is_some() { VersionKind::Revision } else { VersionKind::User };
            points.push((*at, kind, name.clone()));
        }
        points.push((ledger.head(), VersionKind::Auto, auto_name(ledger.head())));
        points.sort();
        let mut doc = Document::default();
        let mut stacks = (Vec::new(), Vec::new());
        let mut done = 0u64;
        let mut snap = Snap { seq: 0, set: PartSet::default() };
        let mut first = true;
        let mut latest: Option<SnapshotRef> = None;
        for (at, kind, name) in points {
            let range = ledger.log().get(done as usize..at as usize).unwrap_or_default();
            let mut dirty = Dirty::default();
            for tx in range {
                for op in &tx.ops {
                    op.apply(&mut doc).map_err(|e| SaveErrorV2::Encode(format!("replaying {}: {e}", tx.seq)))?;
                }
                dirty.add_ops(&tx.ops);
            }
            advance_stacks(&mut stacks, range);
            done = at;
            let (set, new) =
                if first { parts::build_full(&doc) } else { parts::build_incremental(&doc, &snap.set, &dirty) }
                    .map_err(SaveErrorV2::Encode)?;
            first = false;
            for p in &new {
                let n = w.put_part(&p.hash, &p.body)?;
                saved.bytes += n;
                saved.parts_written += usize::from(n > 0);
            }
            let chain = chains.get(at as usize).copied().unwrap_or(seed);
            let release = (kind == VersionKind::Revision).then(|| revision_of(doc_end, &name)).flatten();
            w.put_version(
                branch,
                &VersionRecord {
                    kind,
                    name: name.clone(),
                    point: Point::new(at, &chain),
                    parts: set.to_record(),
                    release,
                    undo: stacks_record(&stacks.0),
                    redo: stacks_record(&stacks.1),
                },
            )?;
            saved.snapshots += 1;
            latest = Some(SnapshotRef { kind, name, seq: at });
            snap = Snap { seq: at, set };
        }
        let mut cache = Cache { every, ..Cache::default() };
        if let Some(l) = &latest {
            cache.latest.insert(branch.hex(), l.clone());
        }
        let head = (ledger.head(), prev);
        w.put_branch(
            branch,
            &BranchRecord {
                name: "main".into(),
                status: BranchStatus::Open,
                parent: None,
                fork_point: None,
                head: Point::new(head.0, &head.1),
                release_line: true,
            },
        )?;
        w.set_meta("format", &json!(FORMAT_PROJECT))?;
        w.set_meta("container_version", &json!(CONTAINER_VERSION))?;
        w.set_meta("schema_version", &json!(SCHEMA_VERSION))?;
        w.set_meta("codec", &json!(CODEC))?;
        w.set_meta("file_id", &json!(project.hex()))?;
        w.set_meta("writer", &json!(writer))?;
        w.set_meta("created_ms", &json!(created_ms))?;
        w.set_meta("cache", &serde_json::to_value(&cache).map_err(|e| SaveErrorV2::Encode(e.to_string()))?)?;
        if let Some(l) = &link {
            w.set_meta("link_v1", &serde_json::to_value(l).map_err(|e| SaveErrorV2::Encode(e.to_string()))?)?;
        }
        w.commit()?;
        let me = ProjectV2 {
            store,
            project,
            branch,
            head,
            cache,
            snap: snap.clone(),
            snap_ref: latest,
            base_stacks: (Vec::new(), Vec::new()),
            base_seq: 0,
            writer: writer.to_string(),
            read_only: None,
            link_v1: link,
            created_ms,
            last_save: Some(saved.clone()),
        };
        Ok((me, saved))
    }

    /// Save the transactions after the file's head (EEP-0005 §6), with the snapshots of any
    /// versions they name and an automatic snapshot when due, in one write transaction. On
    /// success the ledger is marked persisted.
    ///
    /// # Errors
    /// [`SaveErrorV2`]; the file and the ledger are then unchanged.
    pub fn save(&mut self, ledger: &mut Ledger) -> Result<SavedV2, SaveErrorV2> {
        if let Some(r) = &self.read_only {
            return Err(SaveErrorV2::ReadOnly(r.clone()));
        }
        let (head, mut prev) = self.head;
        if ledger.head() == head {
            if ledger.persisted() != head {
                ledger.mark_persisted(head).map_err(|e| SaveErrorV2::Encode(e.to_string()))?;
            }
            return Ok(SavedV2 { first_seq: head + 1, last_seq: head, ..SavedV2::default() });
        }
        if ledger.base() > head || ledger.head() < head {
            return Err(SaveErrorV2::Encode(format!(
                "the project's history (base {}, head {}) does not continue the file's head {head}",
                ledger.base(),
                ledger.head()
            )));
        }
        let start = usize::try_from(head - ledger.base()).map_err(|e| SaveErrorV2::Encode(e.to_string()))?;
        let pending: Vec<Transaction> = ledger.log().get(start..).unwrap_or_default().to_vec();
        let mut w = self.store.writer()?;
        // Another writer cannot exist (locks); check anyway, it costs one row read.
        {
            let r = self.store.reader()?;
            match r.branch(self.branch)? {
                Some(b) if b.head.seq == head && b.head.hash().ok() == Some(prev) => {}
                Some(b) => return Err(SaveErrorV2::ExternalChange(format!("the branch head is now {}", b.head.seq))),
                None => return Err(SaveErrorV2::ExternalChange("the branch is gone".into())),
            }
        }
        let mut saved = SavedV2 { first_seq: head + 1, last_seq: ledger.head(), ..SavedV2::default() };
        let mut chains: BTreeMap<u64, Hash> = BTreeMap::new();
        chains.insert(head, prev);
        for tx in &pending {
            let (bytes, hash) = encode_tx(tx, &prev).map_err(SaveErrorV2::Encode)?;
            saved.bytes += w.append_tx(self.branch, tx.seq, &hash, &bytes)?;
            chains.insert(tx.seq, hash);
            prev = hash;
        }
        // Versions named or unnamed by these transactions, in order; then an automatic snapshot.
        let mut snap = self.snap.clone();
        let mut cache = self.cache.clone();
        for tx in &pending {
            for op in &tx.ops {
                match op {
                    Op::NameVersion { name, at } => {
                        let kind = if issues(tx, name) { VersionKind::Revision } else { VersionKind::User };
                        // Replaced if it existed under the other kind.
                        for k in [VersionKind::User, VersionKind::Revision] {
                            w.remove_version(self.branch, k, name)?;
                        }
                        let release = if kind == VersionKind::Revision {
                            Some(release_at(ledger, tx, name).map_err(SaveErrorV2::Encode)?)
                        } else {
                            None
                        };
                        let chain = self.chain_at(*at, &chains)?;
                        self.write_snapshot(
                            &mut w, ledger, &mut snap, &mut cache, *at, chain, kind, name, release, &mut saved,
                        )?;
                    }
                    Op::UnnameVersion { name, .. } => {
                        for k in [VersionKind::User, VersionKind::Revision] {
                            w.remove_version(self.branch, k, name)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        let end = ledger.head();
        if end.saturating_sub(snap.seq) >= cache.every {
            let chain = prev;
            self.write_snapshot(
                &mut w,
                ledger,
                &mut snap,
                &mut cache,
                end,
                chain,
                VersionKind::Auto,
                &auto_name(end),
                None,
                &mut saved,
            )?;
        }
        w.put_branch(
            self.branch,
            &BranchRecord {
                name: "main".into(),
                status: BranchStatus::Open,
                parent: None,
                fork_point: None,
                head: Point::new(end, &prev),
                release_line: true,
            },
        )?;
        w.set_meta("writer", &json!(self.writer))?;
        if cache != self.cache {
            w.set_meta("cache", &serde_json::to_value(&cache).map_err(|e| SaveErrorV2::Encode(e.to_string()))?)?;
        }
        w.commit()?;
        self.head = (end, prev);
        self.snap = snap;
        self.snap_ref = cache.latest.get(&self.branch.hex()).cloned();
        self.cache = cache;
        ledger.mark_persisted(end).map_err(|e| SaveErrorV2::Encode(e.to_string()))?;
        self.last_save = Some(saved.clone());
        Ok(saved)
    }

    /// The chain value at `seq`: from this save's rows, or the stored row.
    fn chain_at(&self, seq: u64, chains: &BTreeMap<u64, Hash>) -> Result<Hash, SaveErrorV2> {
        if let Some(c) = chains.get(&seq) {
            return Ok(*c);
        }
        if seq == 0 {
            return Ok(chain::seed(ChainKind::Project, self.project.0));
        }
        let r = self.store.reader()?;
        r.tx(self.branch, seq)?
            .map(|(_, c, _)| c)
            .ok_or_else(|| SaveErrorV2::Encode(format!("transaction {seq} is not stored")))
    }

    /// Build and write the snapshot at `at` (a point at or before the ledger's head), relative to
    /// `snap` when the transactions between them are in memory.
    #[allow(clippy::too_many_arguments)]
    fn write_snapshot(
        &self,
        w: &mut super::store::Writer,
        ledger: &mut Ledger,
        snap: &mut Snap,
        cache: &mut Cache,
        at: u64,
        chain: Hash,
        kind: VersionKind,
        name: &str,
        release: Option<Release>,
        saved: &mut SavedV2,
    ) -> Result<(), SaveErrorV2> {
        let enc = SaveErrorV2::Encode;
        let base = ledger.base();
        let (lo, hi) = (at.min(snap.seq), at.max(snap.seq));
        let in_memory = lo >= base;
        let built = if in_memory {
            let mut dirty = Dirty::default();
            for seq in lo + 1..=hi {
                if let Some(tx) = ledger.transaction(seq) {
                    dirty.add_ops(&tx.ops);
                }
            }
            let undo_after: Vec<Op> = ledger
                .log()
                .iter()
                .rev()
                .take_while(|t| t.seq > at)
                .flat_map(|t| t.ops.iter().rev().map(Op::inverse))
                .collect();
            let prev = snap.set.clone();
            if undo_after.is_empty() {
                parts::build_incremental(ledger.document(), &prev, &dirty)
            } else {
                ledger
                    .with_applied(&undo_after, |d| parts::build_incremental(d, &prev, &dirty))
                    .map_err(|e| enc(e.to_string()))?
            }
        } else {
            let doc = ledger.state_at(at).map_err(|e| enc(e.to_string()))?;
            parts::build_full(&doc)
        }
        .map_err(enc)?;
        let (set, new) = built;
        for p in &new {
            let n = w.put_part(&p.hash, &p.body)?;
            saved.bytes += n;
            saved.parts_written += usize::from(n > 0);
        }
        // The stacks at `at`, from those at the base (known only from the base on).
        let mut stacks = self.base_stacks.clone();
        if at >= self.base_seq && self.base_seq >= base {
            let from = usize::try_from(self.base_seq - base).unwrap_or(usize::MAX);
            let to = usize::try_from(at - base).unwrap_or(usize::MAX);
            advance_stacks(&mut stacks, ledger.log().get(from..to).unwrap_or_default());
        } else {
            stacks = (Vec::new(), Vec::new());
        }
        w.put_version(
            self.branch,
            &VersionRecord {
                kind,
                name: name.to_string(),
                point: Point::new(at, &chain),
                parts: set.to_record(),
                release,
                undo: stacks_record(&stacks.0),
                redo: stacks_record(&stacks.1),
            },
        )?;
        saved.snapshots += 1;
        // The latest snapshot (the one opening uses) only moves forward.
        let latest = cache.latest.get(&self.branch.hex()).map_or(0, |s| s.seq);
        if at >= latest {
            cache.latest.insert(self.branch.hex(), SnapshotRef { kind, name: name.to_string(), seq: at });
        }
        if in_memory {
            *snap = Snap { seq: at, set };
        }
        Ok(())
    }

    /// After a rescue copy (Save As of a damaged file, rows after the verified head left out):
    /// set the branch head to the verified one, drop versions after it, and point the cache at
    /// the snapshot this project opened from (none: the root).
    ///
    /// # Errors
    /// The engine failed.
    pub fn write_rescue(&self, w: &mut super::store::Writer, r: &Reader) -> Result<(), SaveErrorV2> {
        let (seq, chain) = self.head;
        w.put_branch(
            self.branch,
            &BranchRecord {
                name: "main".into(),
                status: BranchStatus::Open,
                parent: None,
                fork_point: None,
                head: Point::new(seq, &chain),
                release_line: true,
            },
        )?;
        for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto] {
            for v in r.versions(self.branch, kind)? {
                if v.point.seq > seq {
                    w.remove_version(self.branch, kind, &v.name)?;
                }
            }
        }
        let mut cache = self.cache.clone();
        cache.latest.remove(&self.branch.hex());
        if let Some(s) = self.snap_ref.as_ref().filter(|s| s.seq <= seq) {
            cache.latest.insert(self.branch.hex(), s.clone());
        }
        w.set_meta("cache", &serde_json::to_value(&cache).map_err(|e| SaveErrorV2::Encode(e.to_string()))?)?;
        Ok(())
    }

    /// The database.
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// The project ID.
    pub fn project_id(&self) -> ProjectId {
        self.project
    }

    /// The branch this project edits (`main`).
    pub fn branch(&self) -> BranchId {
        self.branch
    }

    /// The head in the file: `seq` and chain value.
    pub fn head(&self) -> (u64, Hash) {
        self.head
    }

    /// The `seq` of the latest snapshot.
    pub fn snapshot_seq(&self) -> u64 {
        self.snap.seq
    }

    /// Transactions between automatic snapshots.
    pub fn snapshot_every(&self) -> u64 {
        self.cache.every
    }

    /// Set the automatic snapshot interval (written with the next save).
    ///
    /// # Errors
    /// Out of range.
    pub fn set_snapshot_every(&mut self, every: u64) -> Result<(), String> {
        if !(SNAPSHOT_EVERY_RANGE.0..=SNAPSHOT_EVERY_RANGE.1).contains(&every) {
            return Err(format!(
                "the snapshot interval must be between {} and {}",
                SNAPSHOT_EVERY_RANGE.0, SNAPSHOT_EVERY_RANGE.1
            ));
        }
        self.cache.every = every;
        Ok(())
    }

    /// Why the project cannot be saved to this file, if it cannot.
    pub fn read_only(&self) -> Option<&str> {
        self.read_only.as_deref()
    }

    /// Make the project read-only (another session holds the file's lock).
    pub fn set_read_only(&mut self, why: String) {
        self.read_only = Some(why);
    }

    /// The version 1 history this file continues, if converted.
    pub fn link_v1(&self) -> Option<&LinkV1> {
        self.link_v1.as_ref()
    }

    /// When the file was created (informational).
    pub fn created_ms(&self) -> Option<u64> {
        self.created_ms
    }

    /// What the last save wrote.
    pub fn last_save(&self) -> Option<&SavedV2> {
        self.last_save.as_ref()
    }
}

/// Whether `tx` issues revision `name`: it sets some `rev.<n>.issued` to `name` (brief 019).
fn issues(tx: &Transaction, name: &str) -> bool {
    tx.ops.iter().any(|op| {
        matches!(op, Op::SetProperty { object: ObjectRef::Project, key, after: Some(v), .. }
            if key.starts_with("rev.") && key.ends_with(".issued") && v == name)
    })
}

/// The revision row (`rev.<n>.*`) whose `issued` is `name`, in `doc`, as release data.
fn revision_of(doc: &Document, name: &str) -> Option<Release> {
    let key = doc.properties.iter().find(|(k, v)| k.starts_with("rev.") && k.ends_with(".issued") && *v == name)?.0;
    let prefix = key.strip_suffix("issued")?;
    let fields = doc
        .properties
        .range(prefix.to_string()..)
        .take_while(|(k, _)| k.starts_with(prefix))
        .filter_map(|(k, v)| k.strip_prefix(prefix).filter(|f| *f != "issued").map(|f| (f.to_string(), v.clone())))
        .collect();
    Some(Release { fields, assets: Vec::new() })
}

/// The release data of revision `name` as issued by `tx`: the revision row's fields once `tx`
/// applied (fields are never changed after issuing, so the document after `tx` is read).
fn release_at(ledger: &mut Ledger, tx: &Transaction, name: &str) -> Result<Release, String> {
    if name.len() > MAX_NAME {
        return Err("a revision name is too long".into());
    }
    let undo_after: Vec<Op> = ledger
        .log()
        .iter()
        .rev()
        .take_while(|t| t.seq > tx.seq)
        .flat_map(|t| t.ops.iter().rev().map(Op::inverse))
        .collect();
    let release = if undo_after.is_empty() {
        revision_of(ledger.document(), name)
    } else {
        ledger.with_applied(&undo_after, |d| revision_of(d, name)).map_err(|e| e.to_string())?
    };
    Ok(release.unwrap_or_default())
}
