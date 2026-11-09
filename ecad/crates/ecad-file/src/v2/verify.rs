//! **Verify** (EEP-0005 §5.8): replay each branch from the root with every check, and check each
//! snapshot against the replay. It reads everything, so it runs only on request (the debugger's
//! Verify file, `ecadinspect verify`, before a revision is issued), never as part of open.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{Document, Ledger, TxId, TxKind};

use super::parts::{self, PartSet};
use super::schema::{VersionKind, VersionRecord};
use super::store::{Reader, Store};
use crate::chain::{self, ChainKind, Hash};

/// What Verify found: one line per fact, and whether everything held.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifyReport {
    pub ok: bool,
    pub lines: Vec<String>,
    /// The branch's verified transactions (main).
    pub transactions: u64,
    /// Snapshots checked against the replay.
    pub snapshots: usize,
    /// Parts no version references (garbage until compaction).
    pub unreferenced_parts: usize,
}

fn snapshot_doc(r: &Reader, v: &VersionRecord) -> Result<(Document, Option<parts::ModelMeta>), String> {
    let set = PartSet::from_record(&v.parts)?;
    let mut bodies = Vec::new();
    for (k, h) in &set.parts {
        bodies.push((k.clone(), r.part_body(h).map_err(|e| e.to_string())?));
    }
    let (mut d, meta) = parts::assemble(&set, bodies)?;
    d.rebuild_index();
    Ok((d, meta))
}

/// Verify the whole file.
pub fn verify(store: &Store) -> VerifyReport {
    let mut rep = VerifyReport { ok: true, ..VerifyReport::default() };
    let fail = |rep: &mut VerifyReport, line: String| {
        rep.ok = false;
        rep.lines.push(line);
    };
    let r = match store.reader() {
        Ok(r) => r,
        Err(e) => {
            fail(&mut rep, format!("the file cannot be read: {e}"));
            return rep;
        }
    };
    let opened = super::project::read_facts_pub(&r);
    let (project, _) = match opened {
        Ok(f) => f,
        Err(e) => {
            fail(&mut rep, format!("container facts: {e}"));
            return rep;
        }
    };
    let branches = match r.branches() {
        Ok(b) => b,
        Err(e) => {
            fail(&mut rep, format!("branches: {e}"));
            return rep;
        }
    };
    rep.lines.push(format!("{} branch(es)", branches.len()));
    let mut referenced: BTreeSet<Hash> = BTreeSet::new();
    for (id, b) in &branches {
        if b.parent.is_some() {
            fail(&mut rep, format!("branch {} has a parent; only main exists in this version", b.name));
            continue;
        }
        let last = r.last_seq(*id).ok().flatten().unwrap_or(0);
        let rows = match r.tx_range(*id, 1..=last) {
            Ok(x) => x,
            Err(e) => {
                fail(&mut rep, format!("{}: rows: {e}", b.name));
                continue;
            }
        };
        let mut prev = chain::seed(ChainKind::Project, project.0);
        let mut chains = vec![prev];
        let mut txs = Vec::with_capacity(rows.len());
        let mut broken = false;
        for (i, (seq, flag, stored, body)) in rows.into_iter().enumerate() {
            if seq != i as u64 + 1 {
                fail(&mut rep, format!("{}: transaction {} is missing", b.name, i + 1));
                broken = true;
                break;
            }
            match super::project::decode_row_pub(seq, flag, &stored, &body, &prev) {
                Ok(tx) => txs.push(tx),
                Err(e) => {
                    fail(&mut rep, format!("{}: transaction {seq}: {e}", b.name));
                    broken = true;
                    break;
                }
            }
            prev = stored;
            chains.push(prev);
        }
        if !broken && (b.head.seq != last || b.head.hash().ok() != Some(prev)) {
            fail(&mut rep, format!("{}: the head ({}) is not the last transaction ({last})", b.name, b.head.seq));
        }
        let n = txs.len() as u64;
        rep.transactions = n;
        let ledger = match Ledger::replay_verified(txs, 0) {
            Ok(l) => l,
            Err(e) => {
                fail(&mut rep, format!("{}: replay failed: {e}", b.name));
                continue;
            }
        };
        rep.lines.push(format!("{}: {n} transactions replay from the root with every check", b.name));
        // Snapshots, in point order, against one forward replay.
        let mut versions: Vec<VersionRecord> = Vec::new();
        for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto] {
            match r.versions(*id, kind) {
                Ok(v) => versions.extend(v),
                Err(e) => fail(&mut rep, format!("{}: versions: {e}", b.name)),
            }
        }
        versions.sort_by_key(|v| v.point.seq);
        let named: BTreeMap<&String, &u64> = ledger.document().versions.iter().collect();
        let mut doc = Document::default();
        let mut high = ecad_model::tag::HighWater::new();
        let mut done = 0u64;
        let mut stacks: (Vec<(u64, TxId)>, Vec<(u64, TxId)>) = (Vec::new(), Vec::new());
        for v in &versions {
            if v.point.seq > n {
                fail(&mut rep, format!("{}: version {:?} is after the head", b.name, v.name));
                continue;
            }
            for tx in ledger.log().iter().skip(done as usize).take((v.point.seq - done) as usize) {
                for op in &tx.ops {
                    let _ = op.apply(&mut doc);
                    ecad_model::tag::note_ops(&mut high, std::slice::from_ref(op));
                }
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
            done = v.point.seq;
            if chains.get(v.point.seq as usize).map(chain::hex) != Some(v.point.chain.clone()) {
                fail(&mut rep, format!("{}: version {:?}: its chain value differs from the log's", b.name, v.name));
            }
            for h in v.parts.values() {
                if let Some(h) = chain::parse_hex(h) {
                    referenced.insert(h);
                }
            }
            match snapshot_doc(&r, v) {
                Ok((d, meta)) if d == doc => {
                    rep.snapshots += 1;
                    // `model:meta` is a cache: it must equal what the history gives.
                    if let Some(m) = meta
                        && m.tag_high != high
                    {
                        fail(
                            &mut rep,
                            format!("{}: the tag numbers of {:?} differ from the history's", b.name, v.name),
                        );
                    }
                }
                Ok(_) => fail(&mut rep, format!("{}: the snapshot of {:?} DIFFERS from the replay", b.name, v.name)),
                Err(e) => fail(&mut rep, format!("{}: the snapshot of {:?}: {e}", b.name, v.name)),
            }
            let recorded = (
                v.undo.iter().map(|(q, i)| (*q, TxId::parse(i))).collect::<Vec<_>>(),
                v.redo.iter().map(|(q, i)| (*q, TxId::parse(i))).collect::<Vec<_>>(),
            );
            let replayed = (
                stacks.0.iter().map(|(q, i)| (*q, Some(*i))).collect::<Vec<_>>(),
                stacks.1.iter().map(|(q, i)| (*q, Some(*i))).collect::<Vec<_>>(),
            );
            if recorded != replayed && !(v.undo.is_empty() && v.redo.is_empty() && v.kind != VersionKind::Auto) {
                fail(&mut rep, format!("{}: the undo stacks of {:?} differ from the replay", b.name, v.name));
            }
            if v.kind != VersionKind::Auto && named.get(&v.name) != Some(&&v.point.seq) {
                fail(
                    &mut rep,
                    format!("{}: version {:?} is not named at {} in the history", b.name, v.name, v.point.seq),
                );
            }
        }
        let rows_named = versions.iter().filter(|v| v.kind != VersionKind::Auto).count();
        if rows_named != named.len() {
            fail(
                &mut rep,
                format!("{}: {} version rows, {} versions named in the history", b.name, rows_named, named.len()),
            );
        }
        rep.lines.push(format!("{}: {} snapshot(s) equal the replay at their points", b.name, rep.snapshots));
    }
    match r.part_list() {
        Ok(all) => {
            rep.unreferenced_parts = all.iter().filter(|(h, _, _)| !referenced.contains(h)).count();
            rep.lines.push(format!(
                "{} part(s), {} not referenced by any version (removed by compaction)",
                all.len(),
                rep.unreferenced_parts
            ));
        }
        Err(e) => fail(&mut rep, format!("parts: {e}")),
    }
    if rep.ok {
        rep.lines.push("everything verifies".into());
    }
    rep
}
