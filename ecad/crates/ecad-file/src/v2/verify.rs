//! **Verify** (EEP-0005 §5.8, brief 029): check a whole file, in three parts.
//!
//! - **(a) Chain** ([`verify_chain`]): decode every row of every branch, check each chain value
//!   against the previous one, each head against its last row, each version's and merge row's
//!   chain value, and that every chunk a version names is present and hashes to its key. Linear
//!   in the file, with no replay.
//! - **(b) Replay** each branch, parents before children, from the root or from its parent's state
//!   at the fork point, with **only the commit-time check** per transaction
//!   ([`ecad_model::Check::Commit`]: the objects it touched, exactly as [`ecad_model::Ledger::commit`]
//!   checks them). A broken invariant can only come from a transaction, and that transaction
//!   touches the broken object (brief 013's contract, which every `cargo test` asserts), so the
//!   failure names it.
//! - **(c) Full check** ([`Document::check`] and the index check) only at each stored snapshot
//!   point and at each branch's end. At a snapshot the replayed state is also compared with the
//!   stored snapshot, its tag numbers and its undo stacks, which is what catches a row rewritten
//!   with valid but different ops.
//!
//! Brief 013's verify mode (the full check after every transaction) is never used here, whatever
//! the build, so Verify costs and decides the same in tests and in release.
//!
//! It reads everything, so it runs only on request (the debugger's Verify file, `ecadinspect
//! verify`, the inspector, before a revision is issued), never as part of open.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::tag::HighWater;
use ecad_model::{Check, Document, Ledger, ProjectId, Transaction, TxId, TxKind};

use super::chunks::{self, ChunkSet};
use super::project::Lineage;
use super::schema::{BranchId, BranchRecord, VersionKind, VersionRecord};
use super::store::{Reader, Store};
use crate::chain::{self, Hash};

/// What Verify found: one line per fact, and whether everything held.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifyReport {
    pub ok: bool,
    pub lines: Vec<String>,
    /// Main's transactions (read and chain-checked; replayed too by [`verify`]).
    pub transactions: u64,
    /// Snapshots checked against the replay.
    pub snapshots: usize,
    /// Chunks no version references (garbage until compaction).
    pub unreferenced_chunks: usize,
    /// Full document checks run: one per snapshot point and one per branch end, never one per
    /// transaction (0 for [`verify_chain`]).
    pub full_checks: usize,
    /// The failing lines only (also in `lines`, in order), each naming the branch and the
    /// transaction or snapshot.
    pub failures: Vec<String>,
}

impl VerifyReport {
    fn fail(&mut self, line: String) {
        self.ok = false;
        self.failures.push(line.clone());
        self.lines.push(line);
    }
}

/// One branch after the chain check: its rows decoded, their chain values, and its versions.
struct Rows {
    id: BranchId,
    rec: BranchRecord,
    line: Lineage,
    /// Decoded transactions `1..`; only the intact prefix when the chain broke.
    txs: Vec<Transaction>,
    /// Snapshots (user, revision and automatic versions) in point order, up to the head.
    versions: Vec<VersionRecord>,
    /// The merge index rows.
    merges: Vec<VersionRecord>,
    /// The rows, head and lineage are intact, so the branch can be replayed.
    intact: bool,
}

/// What part (a) leaves for parts (b) and (c).
struct Chained {
    reader: Reader,
    branches: Vec<Rows>,
}

/// Part (a) on its own: rows, chain values, heads, version and merge rows' chain values, and
/// chunks. Linear in the file; no replay, so `snapshots` and `full_checks` stay 0.
pub fn verify_chain(store: &Store) -> VerifyReport {
    let mut rep = VerifyReport { ok: true, ..VerifyReport::default() };
    chain_part(store, &mut rep);
    if rep.ok {
        rep.lines.push("the chain verifies".into());
    }
    rep
}

/// Verify the whole file: [`verify_chain`], then the replay with the commit-time check, then
/// the full check at every snapshot and branch end.
pub fn verify(store: &Store) -> VerifyReport {
    let mut rep = VerifyReport { ok: true, ..VerifyReport::default() };
    let Some(chained) = chain_part(store, &mut rep) else {
        return rep;
    };
    replay_part(chained, &mut rep);
    if rep.ok {
        rep.lines.push("everything verifies".into());
    }
    rep
}

/// Part (a). `None` when the file cannot be read far enough to list its branches.
fn chain_part(store: &Store, rep: &mut VerifyReport) -> Option<Chained> {
    let r = match store.reader() {
        Ok(r) => r,
        Err(e) => {
            rep.fail(format!("the file cannot be read: {e}"));
            return None;
        }
    };
    let project = match super::project::read_facts_pub(&r) {
        Ok((p, _)) => p,
        Err(e) => {
            rep.fail(format!("container facts: {e}"));
            return None;
        }
    };
    let listed = match r.branches() {
        Ok(b) => b,
        Err(e) => {
            rep.fail(format!("branches: {e}"));
            return None;
        }
    };
    rep.lines.push(format!("{} branch(es)", listed.len()));
    // The check opening applies to main's row (tester 020b: Verify must refuse what open refuses).
    if let Err(e) = super::project::main_branch_pub(&r, project) {
        rep.fail(format!("main: {e}"));
    }
    let mut referenced: BTreeSet<Hash> = BTreeSet::new();
    let mut checked: BTreeSet<Hash> = BTreeSet::new();
    let mut branches = Vec::with_capacity(listed.len());
    for (id, rec) in listed {
        if let Some(rows) = chain_branch(&r, project, id, rec, rep, &mut referenced, &mut checked) {
            branches.push(rows);
        }
    }
    match r.chunk_list() {
        Ok(all) => {
            rep.unreferenced_chunks = all.iter().filter(|(h, _, _)| !referenced.contains(h)).count();
            rep.lines.push(format!("{} chunk(s), {} not referenced by any version (removed by compaction)", all.len(), rep.unreferenced_chunks));
        }
        Err(e) => rep.fail(format!("chunks: {e}")),
    }
    Some(Chained { reader: r, branches })
}

/// Part (a) for one branch. `None` when not even its lineage reads.
fn chain_branch(
    r: &Reader, project: ProjectId, id: BranchId, rec: BranchRecord, rep: &mut VerifyReport, referenced: &mut BTreeSet<Hash>,
    checked: &mut BTreeSet<Hash>,
) -> Option<Rows> {
    let name = rec.name.clone();
    let line = match super::project::lineage(r, project, id) {
        Ok(l) => l,
        Err(e) => {
            rep.fail(format!("{name}: {e}"));
            return None;
        }
    };
    let Some(seed) = line.first().map(|l| l.seed) else {
        rep.fail(format!("{name}: an empty lineage"));
        return None;
    };
    let last = r.last_seq(id).ok().flatten().unwrap_or(0);
    let mut intact = true;
    let mut prev = seed;
    let mut chains = vec![prev];
    let mut txs = Vec::new();
    // Read in slices, so a long history never holds every stored row at once.
    let mut from = 1u64;
    'rows: while from <= last {
        let to = last.min(from.saturating_add(4_095));
        let rows = match r.tx_range(id, from..=to) {
            Ok(x) => x,
            Err(e) => {
                rep.fail(format!("{name}: rows: {e}"));
                intact = false;
                break;
            }
        };
        for (seq, flag, stored, body) in rows {
            let want = txs.len() as u64 + 1;
            if seq != want {
                rep.fail(format!("{name}: transaction {want} is missing"));
                intact = false;
                break 'rows;
            }
            match super::project::decode_row_pub(seq, flag, &stored, &body, &prev) {
                Ok(tx) => txs.push(tx),
                Err(e) => {
                    rep.fail(format!("{name}: transaction {seq}: {e}"));
                    intact = false;
                    break 'rows;
                }
            }
            prev = stored;
            chains.push(prev);
        }
        if txs.len() as u64 != to {
            let want = txs.len() as u64 + 1;
            rep.fail(format!("{name}: transaction {want} is missing"));
            intact = false;
            break;
        }
        let Some(next) = to.checked_add(1) else { break };
        from = next;
    }
    if intact && (rec.head.seq != last || rec.head.hash().ok() != Some(prev)) {
        rep.fail(format!("{name}: the head ({}) is not the last transaction ({last})", rec.head.seq));
        intact = false;
    }
    let n = txs.len() as u64;
    if line.len() == 1 {
        rep.transactions = n;
    }
    if intact {
        rep.lines.push(format!("{name}: {n} transactions, chain and head intact"));
    }
    // Versions: their chain values, and their chunks present and hashing to their keys.
    let mut versions: Vec<VersionRecord> = Vec::new();
    for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto] {
        match r.versions(id, kind) {
            Ok(v) => versions.extend(v),
            Err(e) => rep.fail(format!("{name}: versions: {e}")),
        }
    }
    versions.sort_by_key(|v| v.point.seq);
    let mut kept = Vec::with_capacity(versions.len());
    for v in versions {
        if v.point.seq > n {
            if intact {
                rep.fail(format!("{name}: version {:?} is after the head", v.name));
            }
            continue;
        }
        if chains.get(v.point.seq as usize).map(chain::hex) != Some(v.point.chain.clone()) {
            rep.fail(format!("{name}: version {:?}: its chain value differs from the log's", v.name));
        }
        for h in v.chunks.values() {
            let Some(h) = chain::parse_hex(h) else {
                rep.fail(format!("{name}: version {:?} names a chunk that is not a hash", v.name));
                continue;
            };
            referenced.insert(h);
            if checked.insert(h)
                && let Err(e) = r.chunk_body(&h)
            {
                rep.fail(format!("{name}: version {:?}: {e}", v.name));
            }
        }
        kept.push(v);
    }
    // Merge index rows: decodable facts and the log's chain value (part (c) compares them with the
    // merges the replay finds live).
    let merges = match r.versions(id, VersionKind::Merge) {
        Ok(rows) => rows,
        Err(e) => {
            rep.fail(format!("{name}: merge rows: {e}"));
            Vec::new()
        }
    };
    for v in &merges {
        let at = v.point.seq;
        let Some(m) = &v.merge else {
            rep.fail(format!("{name}: the merge row at {at} has no merge facts"));
            continue;
        };
        if m.source_id().is_none() || m.ancestor_id().is_none() || m.source_head.hash().is_err() || chain::parse_hex(&m.ancestor.chain).is_none() {
            rep.fail(format!("{name}: the merge row at {at} does not decode"));
            continue;
        }
        if chains.get(at as usize).map(chain::hex) != Some(v.point.chain.clone()) {
            rep.fail(format!("{name}: the merge row at {at}: its chain value differs from the log's"));
        }
    }
    Some(Rows { id, rec, line, txs, versions: kept, merges, intact })
}

/// A state kept for a child branch: the document and tag numbers after a parent's transaction.
type ForkState = (Document, HighWater);

/// Parts (b) and (c), for every intact branch, parents before children.
fn replay_part(chained: Chained, rep: &mut VerifyReport) {
    let Chained { reader, mut branches } = chained;
    let r = &reader;
    // The fork states children need, by parent branch and seq.
    let mut wanted: BTreeMap<BranchId, BTreeSet<u64>> = BTreeMap::new();
    for b in &branches {
        if let Some((pi, fseq)) = b.line.first().and_then(|l| l.fork)
            && let Some(parent) = b.line.get(pi)
        {
            wanted.entry(parent.branch).or_default().insert(fseq);
        }
    }
    let mut kept: BTreeMap<(BranchId, u64), ForkState> = BTreeMap::new();
    // A parent's lineage is one shorter than its child's.
    branches.sort_by_key(|b| b.line.len());
    for b in branches {
        if !b.intact {
            rep.lines.push(format!("{}: not replayed (its chain is broken)", b.rec.name));
            continue;
        }
        let start = match b.line.first().and_then(|l| l.fork) {
            None => Ok((Document::default(), HighWater::new())),
            Some((pi, fseq)) => match b.line.get(pi).map(|p| p.branch).and_then(|p| kept.get(&(p, fseq))) {
                Some(s) => Ok(s.clone()),
                // The parent could not be replayed this far: its stored state, as open reads it.
                None => super::project::line_state(r, &b.line, pi, fseq),
            },
        };
        let (start_doc, start_high) = match start {
            Ok(s) => s,
            Err(e) => {
                rep.fail(format!("{}: its parent's state at the fork: {e}", b.rec.name));
                continue;
            }
        };
        let want = wanted.get(&b.id).cloned().unwrap_or_default();
        replay_branch(r, b, start_doc, start_high, &want, &mut kept, rep);
    }
}

/// The stack state after each transaction, as the ledger derives it.
#[derive(Default)]
struct Stacks {
    undo: Vec<(u64, TxId)>,
    redo: Vec<(u64, TxId)>,
}

impl Stacks {
    fn step(&mut self, tx: &Transaction) {
        match tx.kind {
            TxKind::Edit | TxKind::Merge { .. } => {
                self.undo.push((tx.seq, tx.id));
                self.redo.clear();
            }
            TxKind::Undo { .. } => {
                self.undo.pop();
                self.redo.push((tx.seq, tx.id));
            }
            TxKind::Redo { .. } => {
                self.redo.pop();
                self.undo.push((tx.seq, tx.id));
            }
        }
    }
}

/// Parts (b) and (c) for one branch.
fn replay_branch(
    r: &Reader, mut b: Rows, start_doc: Document, start_high: HighWater, want: &BTreeSet<u64>, kept: &mut BTreeMap<(BranchId, u64), ForkState>,
    rep: &mut VerifyReport,
) {
    let name = &b.rec.name;
    let n = b.txs.len() as u64;
    // Snapshots by point.
    let mut at: BTreeMap<u64, Vec<&VersionRecord>> = BTreeMap::new();
    for v in &b.versions {
        at.entry(v.point.seq).or_default().push(v);
    }
    let mut high = start_high.clone();
    let mut stacks = Stacks::default();
    let mut snapshots = 0usize;
    let mut full_checks = 0usize;
    let mut lines: Vec<String> = Vec::new();
    let mut info: Vec<String> = Vec::new();
    if want.contains(&0) {
        kept.insert((b.id, 0), (start_doc.clone(), start_high));
    }
    // A snapshot at 0 (a branch saved before its first edit) is the fork state itself.
    if let Some(vs) = at.get(&0) {
        check_point(r, name, 0, vs, &start_doc, &high, &stacks, &mut snapshots, &mut full_checks, &mut lines, &mut info);
    }
    let named_before = start_doc.versions.clone();
    let txs = std::mem::take(&mut b.txs);
    let name = &b.rec.name;
    let replayed = Ledger::replay_checked_visit(start_doc, txs, 0, Check::Commit, |tx, doc| {
        ecad_model::tag::note_ops(&mut high, &tx.ops);
        stacks.step(tx);
        if want.contains(&tx.seq) {
            kept.insert((b.id, tx.seq), (doc.clone(), high.clone()));
        }
        if let Some(vs) = at.get(&tx.seq) {
            check_point(r, name, tx.seq, vs, doc, &high, &stacks, &mut snapshots, &mut full_checks, &mut lines, &mut info);
        }
    });
    rep.snapshots += snapshots;
    rep.full_checks += full_checks;
    let failed = !lines.is_empty();
    rep.lines.extend(info);
    for l in lines {
        rep.fail(l);
    }
    let ledger = match replayed {
        Ok(l) => l,
        Err(e) => {
            rep.fail(format!("{name}: replay failed: {e}"));
            return;
        }
    };
    let from = if b.line.len() == 1 { "the root" } else { "the fork point" };
    rep.lines.push(format!("{name}: {n} transactions replay from {from} with the commit check"));
    // (c) at the branch's end.
    rep.full_checks += 1;
    if let Err(e) = full_check(ledger.document()) {
        rep.fail(format!("{name}: the state at the end ({n}) fails the full check: {e}"));
    }
    // The merge index (FQ2, reviewer 020b finding 6): its rows decide the next merge's ancestor,
    // so they must be exactly the live merges of the replay, with their facts.
    let live = live_merges(ledger.log());
    let mut seen = BTreeSet::new();
    for v in &b.merges {
        let at = v.point.seq;
        seen.insert(at);
        let Some(m) = &v.merge else { continue };
        match live.get(&at) {
            Some(want) if want == m => {}
            Some(_) => rep.fail(format!("{name}: the merge row at {at} differs from its merge transaction")),
            None => rep.fail(format!("{name}: the merge row at {at} names no live merge in the history")),
        }
    }
    for at in live.keys().filter(|s| !seen.contains(*s)) {
        rep.fail(format!("{name}: the live merge at {at} has no merge row"));
    }
    // Versions named on this branch (those inherited from the parent are its rows).
    let named: BTreeMap<&String, &u64> = ledger.document().versions.iter().filter(|(k, at)| named_before.get(*k) != Some(*at)).collect();
    for v in b.versions.iter().filter(|v| v.kind != VersionKind::Auto) {
        if named.get(&v.name) != Some(&&v.point.seq) {
            rep.fail(format!("{name}: version {:?} is not named at {} in the history", v.name, v.point.seq));
        }
    }
    let rows_named = b.versions.iter().filter(|v| v.kind != VersionKind::Auto).count();
    if rows_named != named.len() {
        rep.fail(format!("{name}: {rows_named} version rows, {} versions named in the history", named.len()));
    }
    if !failed {
        rep.lines.push(format!("{name}: {snapshots} snapshot(s) equal the replay at their points, and pass the full check"));
    }
}

/// The full document and index check (part (c)).
fn full_check(doc: &Document) -> ecad_model::Result<()> {
    doc.check().and_then(|()| doc.check_index())
}

/// Part (c) at one snapshot point: the full check once, then each snapshot taken there against
/// the replayed state, its tag numbers and its undo stacks.
#[allow(clippy::too_many_arguments)]
fn check_point(
    r: &Reader, branch: &str, seq: u64, vs: &[&VersionRecord], doc: &Document, high: &HighWater, stacks: &Stacks, snapshots: &mut usize,
    full_checks: &mut usize, lines: &mut Vec<String>, info: &mut Vec<String>,
) {
    *full_checks += 1;
    if let Err(e) = full_check(doc) {
        lines.push(format!("{branch}: the state at {seq} fails the full check: {e}"));
    }
    for v in vs {
        match snapshot_doc(r, v) {
            Ok((d, meta)) if d == *doc => {
                *snapshots += 1;
                // `model:meta` is a cache: it must equal what the history gives, under the rule it
                // was computed with. Marks of an older rule are not compared: opening recomputes
                // them (brief 029, FP1), which Verify reports as information.
                if !v.tag_marks_current() {
                    info.push(format!(
                        "{branch}: the tag numbers of {:?} were written under tag rule {}; recomputed on open (rule {} → {})",
                        v.name,
                        v.tag_rule(),
                        v.tag_rule(),
                        ecad_model::tag::TAG_RULE
                    ));
                } else if let Some(m) = meta
                    && m.tag_high != *high
                {
                    lines.push(format!("{branch}: the tag numbers of {:?} differ from the history's", v.name));
                }
            }
            Ok((d, _)) => lines.push(format!("{branch}: snapshot {:?} at {seq} DIFFERS from the replay: {}", v.name, first_difference(doc, &d))),
            Err(e) => lines.push(format!("{branch}: the snapshot of {:?}: {e}", v.name)),
        }
        // The recorded top of each stack, and the count below it, against the replay's.
        let matches = |rec: &[(u64, String)], hidden: u64, all: &[(u64, TxId)]| {
            let rec: Vec<(u64, Option<TxId>)> = rec.iter().map(|(q, i)| (*q, TxId::parse(i))).collect();
            let all: Vec<(u64, Option<TxId>)> = all.iter().map(|(q, i)| (*q, Some(*i))).collect();
            rec.len() as u64 + hidden == all.len() as u64 && all.ends_with(&rec)
        };
        if !matches(&v.undo, v.undo_hidden, &stacks.undo) || !matches(&v.redo, v.redo_hidden, &stacks.redo) {
            lines.push(format!("{branch}: the undo stacks of {:?} differ from the replay", v.name));
        }
    }
}

/// The first difference between the replayed state and a snapshot, in words.
fn first_difference(replayed: &Document, snapshot: &Document) -> String {
    let ops = ecad_model::diff::ops_between(replayed, snapshot, &|_| true);
    let Some(op) = ops.first() else {
        return if replayed.versions != snapshot.versions { "the named versions".into() } else { "the derived index".into() };
    };
    let what = ecad_model::diff::objects(std::slice::from_ref(op)).keys().next().map(|(k, id)| format!("{k} {id}")).unwrap_or_default();
    match op {
        ecad_model::Op::SetProperty { key, .. } => format!("{what}, property `{key}`"),
        op => format!("{what} ({})", op.name()),
    }
}

fn snapshot_doc(r: &Reader, v: &VersionRecord) -> Result<(Document, Option<chunks::ModelMeta>), String> {
    let set = ChunkSet::from_record(&v.chunks)?;
    let mut bodies = Vec::new();
    for (k, h) in &set.chunks {
        bodies.push((k.clone(), r.chunk_body(h).map_err(|e| e.to_string())?));
    }
    let (mut d, meta) = chunks::assemble(&set, bodies)?;
    d.rebuild_index();
    Ok((d, meta))
}

/// The live merges after `log` (a branch's whole log), by the step that made each live: a merge,
/// removed by its undo, live again at the redo of that undo.
fn live_merges(log: &[ecad_model::Transaction]) -> BTreeMap<u64, super::schema::MergeRow> {
    let mut live: BTreeMap<u64, super::schema::MergeRow> = BTreeMap::new();
    let mut undone: BTreeMap<u64, super::schema::MergeRow> = BTreeMap::new();
    let (mut undo, mut redo): (Vec<u64>, Vec<u64>) = (Vec::new(), Vec::new());
    for tx in log {
        match tx.kind {
            TxKind::Edit | TxKind::Merge { .. } => {
                if let Some(row) = super::schema::MergeRow::of(&tx.kind) {
                    live.insert(tx.seq, row);
                }
                undo.push(tx.seq);
                redo.clear();
            }
            TxKind::Undo { .. } => {
                if let Some(x) = undo.pop()
                    && let Some(row) = live.remove(&x)
                {
                    undone.insert(tx.seq, row);
                }
                redo.push(tx.seq);
            }
            TxKind::Redo { .. } => {
                if let Some(u) = redo.pop()
                    && let Some(row) = undone.remove(&u)
                {
                    live.insert(tx.seq, row);
                }
                undo.push(tx.seq);
            }
        }
    }
    live
}
