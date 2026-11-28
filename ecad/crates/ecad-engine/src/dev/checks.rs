//! The debugger's checks on request (replay, projection, index, file), file reads and state at a step.

use std::path::Path;
use std::sync::Arc;

use cadcraft_engine::doc::Drawing;
use ecad_model::{Document, Ledger, PageId, Transaction};
use serde_json::Value;

use super::debug::*;
use crate::project::HandleMap;

/// What [`verify_projection`] needs, copied under the lock.
#[derive(Debug, Clone)]
pub struct ProjectionInputs {
    pub head: u64,
    pub doc: Document,
    pub pages: Vec<PageInputs>,
    /// Cables shown in fan view: the fresh projection uses the same views.
    pub views: std::collections::BTreeSet<ecad_model::CableId>,
    pub tolerance: f64,
    /// Show Hidden Text: the fresh projection draws hidden texts the same way.
    pub hidden_text: bool,
    /// The project's pending flag, so the fresh frame shows the same Rev box.
    pub pending: bool,
    /// The sheet the pages are shown on, so the fresh projection draws the same colours.
    pub colours: &'static ecad_model::palette::Sheet,
}

/// One open page's drawing and handle map.
#[derive(Debug, Clone)]
pub struct PageInputs {
    pub id: PageId,
    pub drawing: Arc<Drawing>,
    pub map: HandleMap,
    /// The sheet a generated page shows (0-based).
    pub sheet: usize,
}

/// The result of an explicit check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub what: &'static str,
    /// The head the check ran against.
    pub at_seq: u64,
    pub ok: bool,
    pub lines: Vec<String>,
}

/// Rebuild the reference index from a copy of the document and compare it with a copy
/// of the kept one (the debugger's **Check index**).
pub fn verify_index(doc: &Document, kept: &ecad_electrical::references::devices::index::RefIndex, head: u64) -> Check {
    let (members, groups, edges) = kept.sizes();
    match kept.verify(doc) {
        Ok(()) => Check {
            what: "reference index",
            at_seq: head,
            ok: true,
            lines: vec![format!("{members} members in {groups} groups, {edges} edges: the same as a rebuild.")],
        },
        Err(e) => Check { what: "reference index", at_seq: head, ok: false, lines: vec![e.to_string()] },
    }
}

/// Replay the history into a fresh document and compare it with the live one (copies of both),
/// then check the document's invariants.
///
/// The replay runs in verify mode: every transaction is checked in full as well as by the scoped
/// check, and any disagreement fails it.
pub fn verify_replay(log: &[Transaction], live: &Document) -> Check {
    let at_seq = log.len() as u64;
    let mut lines = Vec::new();
    let ok = match Ledger::replay_verified(log.to_vec(), 0) {
        Ok(l) if l.document() == live => {
            lines.push(format!("replaying {at_seq} transactions gives the live document"));
            true
        }
        Ok(_) => {
            lines.push(format!("replaying {at_seq} transactions gives a DIFFERENT document"));
            false
        }
        Err(e) => {
            lines.push(format!("replay failed: {e}"));
            false
        }
    };
    let invariants = live.check();
    match &invariants {
        Ok(()) => lines.push("document invariants hold".into()),
        Err(e) => lines.push(format!("document invariants fail: {e}")),
    }
    Check { what: "replay", at_seq, ok: ok && invariants.is_ok(), lines }
}

/// [`verify_replay`] for a copy of the history that starts after `base` (a project opened from a
/// version 2 snapshot): the state at `base` is read from `history`, then the copy is applied.
pub fn verify_replay_from(base: u64, history: Option<&Arc<dyn ecad_model::History>>, log: &[Transaction], live: &Document) -> Check {
    if base == 0 {
        return verify_replay(log, live);
    }
    let at_seq = base + log.len() as u64;
    let mut lines = Vec::new();
    let replayed = history
        .ok_or_else(|| "no history to read the snapshot from".to_string())
        .and_then(|h| h.state_at(base).map_err(|e| e.to_string()))
        .and_then(|mut doc| {
            lines.push(format!("the state at the snapshot ({base}) was rebuilt from the file"));
            for tx in log {
                for op in &tx.ops {
                    op.apply(&mut doc).map_err(|e| format!("transaction {}: {e}", tx.seq))?;
                }
            }
            Ok(doc)
        });
    let ok = match replayed {
        Ok(d) if &d == live => {
            lines.push(format!("replaying the {} transactions after it gives the live document", log.len()));
            true
        }
        Ok(_) => {
            lines.push(format!("replaying the {} transactions after it gives a DIFFERENT document", log.len()));
            false
        }
        Err(e) => {
            lines.push(format!("replay failed: {e}"));
            false
        }
    };
    let invariants = live.check();
    match &invariants {
        Ok(()) => lines.push("document invariants hold".into()),
        Err(e) => lines.push(format!("document invariants fail: {e}")),
    }
    Check { what: "replay", at_seq, ok: ok && invariants.is_ok(), lines }
}

/// **Verify file** for a version 2 file: replay the branch from the root with
/// every check and check every snapshot against the replay, then compare the file's rows with the
/// copied history (after `base`) up to the saved position. Reads only, on request.
pub fn verify_store(store: &ecad_file::Store, base: u64, log: &[Transaction], persisted: u64) -> Check {
    let at_seq = base + log.len() as u64;
    let v = ecad_file::verify::verify(store);
    let mut ok = v.ok;
    let mut lines = v.lines;
    let rows = store.reader().map_err(|e| e.to_string()).and_then(|r| {
        let branch = r.branches().map_err(|e| e.to_string())?.into_iter().find(|(_, b)| b.is_main()).map(|(id, _)| id).ok_or("no main branch")?;
        let head = r.last_seq(branch).map_err(|e| e.to_string())?.unwrap_or(0);
        Ok((r, branch, head))
    });
    match rows {
        Ok((r, branch, head)) => {
            if head < persisted {
                ok = false;
                lines.push(format!("the file ends at transaction {head}, before the saved position {persisted}"));
            }
            let mut differ = None;
            for tx in log.iter().filter(|t| t.seq <= head.min(persisted)) {
                let row = r.tx(branch, tx.seq).ok().flatten();
                let same = row.and_then(|(flag, _, body)| ecad_file::pack::unpack(flag, &body).ok()).is_some_and(|rec| {
                    serde_json::from_slice::<Value>(&rec).ok().and_then(|v| ecad_file::record::decode_tx(&v).ok()).is_some_and(|t| &t == tx)
                });
                if !same {
                    differ = Some(tx.seq);
                    break;
                }
            }
            match differ {
                Some(seq) => {
                    ok = false;
                    lines.push(format!("transaction {seq} in the file DIFFERS from the project's"));
                }
                None => lines.push(format!(
                    "the file's rows match the project's history; saved through {persisted}, {} pending in memory",
                    at_seq.saturating_sub(persisted)
                )),
            }
        }
        Err(e) => {
            ok = false;
            lines.push(format!("the file's rows could not be read: {e}"));
        }
    }
    Check { what: "file", at_seq, ok, lines }
}

/// Read transaction `seq`'s row from the project file (no project lock) and check that it decodes
/// to exactly `expected`. `shown` is the record, decompressed; `stored` is its size in the file.
///
/// # Errors
/// The file cannot be read, or holds no such row.
pub fn read_row(store: &ecad_file::Store, seq: u64, expected: Option<&Transaction>) -> Result<RecordBytes, String> {
    let r = store.reader().map_err(|e| e.to_string())?;
    let branch = r.branches().map_err(|e| e.to_string())?.into_iter().find(|(_, b)| b.is_main()).map(|(id, _)| id).ok_or("no main branch")?;
    let (flag, chain, body) = r.tx(branch, seq).map_err(|e| e.to_string())?.ok_or(format!("no row for {seq}"))?;
    let record = ecad_file::pack::unpack(flag, &body).map_err(|e| e.to_string())?;
    let check = (|| {
        if ecad_file::chain::tx_hash(&record) != chain {
            return Err("the record does not match its chain value".to_string());
        }
        let v: Value = serde_json::from_slice(&record).map_err(|e| format!("not JSON: {e}"))?;
        let tx = ecad_file::record::decode_tx(&v).map_err(|e| format!("not a transaction record: {e:?}"))?;
        match expected {
            Some(e) if *e != tx => Err(format!("transaction {seq} in the file differs from the one in memory")),
            _ => Ok(()),
        }
    })();
    let stored = (body.len() + 33) as u64;
    let mut shown = record;
    let truncated = shown.len() > MAX_RECORD_SHOWN;
    shown.truncate(MAX_RECORD_SHOWN);
    Ok(RecordBytes { seq, stored, shown, truncated, check })
}

/// What the Storage and Branches views read from a version 2 file, on request: table sizes, the
/// file's size, branches and version rows (without their chunk maps).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageRead {
    pub file_bytes: u64,
    pub tables: Vec<ecad_file::TableStat>,
    /// Name, ID, status, head, rows, parent and fork point.
    pub branches: Vec<BranchRow>,
    /// Branch name, kind, name, point and chunk count.
    pub versions: Vec<(String, String, String, u64, usize)>,
}

/// One branch, for the Branches view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRow {
    pub name: String,
    pub id: String,
    pub status: String,
    pub head: u64,
    pub head_chain: String,
    pub rows: u64,
    pub release_line: bool,
    /// Parent branch and fork point, for a branch that is not the original.
    pub fork: Option<(String, u64)>,
}

/// The branches of a version 2 file (a few rows and one key read each; read after each save, so
/// its cost never depends on the history's length).
///
/// # Errors
/// The file cannot be read.
pub fn read_branches(store: &ecad_file::Store) -> Result<Vec<BranchRow>, String> {
    let r = store.reader().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (id, b) in r.branches().map_err(|e| e.to_string())? {
        out.push(BranchRow {
            name: b.name.clone(),
            id: id.hex(),
            status: format!("{:?}", b.status),
            head: b.head.seq,
            head_chain: b.head.chain.clone(),
            // The last stored `seq`, one key read: a branch's rows run 1..=last without gaps, so
            // it is the row count. Counting the rows would read the whole log after every save.
            rows: r.last_seq(id).map_err(|e| e.to_string())?.unwrap_or(0),
            release_line: b.release_line,
            fork: b.parent.map(|p| p.hex()).zip(b.fork_point.as_ref().map(|f| f.seq)),
        });
    }
    Ok(out)
}

/// Read the Storage view's facts (on request: table statistics walk the tables).
///
/// # Errors
/// The file cannot be read.
pub fn read_storage(store: &ecad_file::Store, path: Option<&Path>) -> Result<StorageRead, String> {
    let r = store.reader().map_err(|e| e.to_string())?;
    let tables = r.stats().map_err(|e| e.to_string())?;
    let mut versions = Vec::new();
    for (id, b) in r.branches().map_err(|e| e.to_string())? {
        for kind in [ecad_file::schema::RowKind::VERSION, ecad_file::schema::RowKind::REVISION, ecad_file::schema::RowKind::Auto] {
            for v in r.versions(id, kind).map_err(|e| e.to_string())? {
                versions.push((b.name.clone(), format!("{kind:?}"), v.name.clone(), v.point.seq, v.chunks.len()));
            }
        }
    }
    drop(r);
    Ok(StorageRead {
        file_bytes: path.and_then(|p| std::fs::metadata(p).ok()).map_or(0, |m| m.len()),
        tables,
        branches: read_branches(store)?,
        versions,
    })
}

/// Reproject every open page from the document and compare (same handles), then check each page
/// without relying on handle numbers ([`super::verify::verify_page`]). Runs on copies, outside the lock.
pub fn verify_projection(inputs: &ProjectionInputs) -> Check {
    let mut lines = Vec::new();
    let mut ok = true;
    if inputs.pages.is_empty() {
        lines.push("no page is open".into());
    }
    // The report models of this copy, worked out once for every page.
    let reports = crate::project::engine_views().report_models(&inputs.doc);
    for p in &inputs.pages {
        let title = inputs.doc.pages().get(&p.id).map_or_else(|| p.id.short(), |x| x.title.to_string());
        let same = crate::project::project_page_pending(
            &inputs.doc,
            p.id,
            &p.drawing,
            &p.map,
            &inputs.views,
            inputs.tolerance,
            inputs.hidden_text,
            inputs.pending,
            p.sheet,
            reports.as_ref(),
            inputs.colours,
        )
        .map(|(d, m)| d.model == p.drawing.model && m == p.map && p.map.is_consistent());
        match same {
            Ok(true) => lines.push(format!("page {title}: drawing equals a fresh projection ({} handles)", p.map.len())),
            Ok(false) => {
                ok = false;
                lines.push(format!("page {title}: drawing DIFFERS from a fresh projection"));
            }
            Err(e) => {
                ok = false;
                lines.push(format!("page {title}: projection failed: {e}"));
            }
        }
        let v = crate::dev::verify::verify_page_pending(
            &inputs.doc,
            p.id,
            &p.drawing,
            &p.map,
            &inputs.views,
            inputs.tolerance,
            inputs.hidden_text,
            inputs.pending,
            p.sheet,
            reports.as_ref(),
            inputs.colours,
        );
        ok &= v.ok();
        lines.push(format!("page {title}: {}", v.summary()));
    }
    Check { what: "projection", at_seq: inputs.head, ok, lines }
}

/// One transaction's stored record, read from the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordBytes {
    pub seq: u64,
    /// Bytes the row takes in the file (compressed, with its flag and chain value).
    pub stored: u64,
    /// The first [`MAX_RECORD_SHOWN`] bytes of the record.
    pub shown: Vec<u8>,
    pub truncated: bool,
    /// Whether the record matches its chain value and decodes to exactly the expected transaction.
    pub check: Result<(), String>,
}

/// The document after transaction `seq`, rebuilt from a copy of the history. The live project is
/// not involved.
///
/// # Errors
/// `seq` past the end of `log`, or replay failure.
pub fn state_at_from(base: u64, history: Option<&Arc<dyn ecad_model::History>>, log: &[Transaction], seq: u64) -> Result<Document, String> {
    if base == 0 {
        return state_at(log, seq);
    }
    let h = history.ok_or("no history to read older states from")?;
    if seq <= base {
        return h.state_at(seq).map_err(|e| e.to_string());
    }
    let n = usize::try_from(seq - base).map_err(|e| e.to_string())?;
    let tail = log.get(..n).ok_or_else(|| format!("transaction {seq} is past the head {}", base + log.len() as u64))?;
    let mut doc = h.state_at(base).map_err(|e| e.to_string())?;
    for tx in tail {
        for op in &tx.ops {
            op.apply(&mut doc).map_err(|e| format!("transaction {}: {e}", tx.seq))?;
        }
    }
    Ok(doc)
}

/// The document after transaction `seq`, replayed from a copy of the history from the root.
///
/// # Errors
/// Past the copy's end, or replay failed.
pub fn state_at(log: &[Transaction], seq: u64) -> Result<Document, String> {
    let n = usize::try_from(seq).map_err(|e| e.to_string())?;
    let prefix = log.get(..n).ok_or_else(|| format!("transaction {seq} is past the head {}", log.len()))?;
    Ledger::replay(prefix.to_vec(), 0).map(|l| l.document().clone()).map_err(|e| e.to_string())
}
