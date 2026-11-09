//! Seeing a version 2 file (brief 021 scope 7): readable views of its tables, branches, log,
//! versions and chunks, and `export-jsonl`, the full history as a version 1 style JSON Lines file
//! that a version 1 reader opens. Reads only; never changes the file.

use std::io::Write;

use serde_json::{Value, json};

use super::pack;
use super::schema::{BranchId, VersionKind, split_asset_value, split_tx_value};
use super::store::{Reader, Store, StoreError};
use crate::chain;
use crate::codec::{Codec, JsonLines};
use crate::record::Header;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn flag_name(f: u8) -> &'static str {
    match f {
        pack::RAW => "raw",
        pack::ZSTD => "zstd",
        _ => "unknown",
    }
}

/// Row counts and sizes of every table, one line each.
///
/// # Errors
/// The file cannot be read.
pub fn tables(store: &Store) -> Result<String, StoreError> {
    let r = store.reader()?;
    let mut s = format!("{:<13} {:>10} {:>14} {:>12} {:>12}\n", "table", "rows", "stored bytes", "metadata", "fragmented");
    for t in r.stats()? {
        s.push_str(&format!("{:<13} {:>10} {:>14} {:>12} {:>12}\n", t.name, t.rows, t.stored_bytes, t.metadata_bytes, t.fragmented_bytes));
    }
    s.push_str("\nmeta:\n");
    for (k, v) in r.meta_all()? {
        s.push_str(&format!("  {k} = {}\n", String::from_utf8_lossy(&v)));
    }
    Ok(s)
}

/// The branch tree: each branch with its parent, fork point, head and status.
///
/// # Errors
/// The file cannot be read.
pub fn branches(store: &Store) -> Result<String, StoreError> {
    let r = store.reader()?;
    let mut s = String::new();
    for (id, b) in r.branches()? {
        let rows = r.tx_count(id)?;
        s.push_str(&format!(
            "{} ({}) {:?}{}\n  head {} {}\n  {} rows{}\n",
            b.name,
            id.hex(),
            b.status,
            if b.release_line { ", release line" } else { "" },
            b.head.seq,
            b.head.chain,
            rows,
            match (&b.parent, &b.fork_point) {
                (Some(p), Some(f)) => format!(", forked from {p} after {} ({})", f.seq, f.chain),
                _ => ", from the project root".into(),
            }
        ));
    }
    Ok(s)
}

/// The branch named `name`.
fn branch_named(r: &Reader, name: &str) -> Result<BranchId, StoreError> {
    r.branches()?
        .into_iter()
        .find(|(_, b)| b.name == name)
        .map(|(id, _)| id)
        .ok_or_else(|| StoreError::Damaged(format!("there is no branch named {name:?}")))
}

/// The last `last` transactions of branch `name`, decompressed and readable (one JSON record a
/// line, with its stored size and compression).
///
/// # Errors
/// The file cannot be read, or there is no such branch.
pub fn log(store: &Store, name: &str, last: u64) -> Result<String, StoreError> {
    let r = store.reader()?;
    let id = branch_named(&r, name)?;
    let head = r.last_seq(id)?.unwrap_or(0);
    let from = head.saturating_sub(last.max(1)) + 1;
    let mut s = String::new();
    for (seq, flag, chain_value, body) in r.tx_range(id, from..=head)? {
        let record = pack::unpack(flag, &body).map_err(StoreError::Damaged)?;
        s.push_str(&format!(
            "# {seq}: {} stored bytes ({}), {} bytes of record, chain {}\n{}\n",
            body.len() + 33,
            flag_name(flag),
            record.len(),
            chain::hex(&chain_value),
            String::from_utf8_lossy(&record)
        ));
    }
    Ok(s)
}

/// Every version row: kind, name, point, number of chunks, release data.
///
/// # Errors
/// The file cannot be read.
pub fn versions(store: &Store) -> Result<String, StoreError> {
    let r = store.reader()?;
    let mut s = String::new();
    for (id, b) in r.branches()? {
        for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto] {
            for v in r.versions(id, kind)? {
                s.push_str(&format!(
                    "{} {:?} {:?} at {} ({} chunks, undo {} / redo {}){}\n",
                    b.name,
                    v.kind,
                    v.name,
                    v.point.seq,
                    v.chunks.len(),
                    v.undo.len(),
                    v.redo.len(),
                    v.release.as_ref().map_or(String::new(), |rel| format!(" release {:?}", rel.fields))
                ));
            }
        }
    }
    Ok(s)
}

/// One chunk, decompressed and pretty-printed, with its stored size and compression.
///
/// # Errors
/// The file cannot be read, the hash is malformed, or no such chunk is stored.
pub fn chunk(store: &Store, hash: &str) -> Result<String, StoreError> {
    let h = chain::parse_hex(hash).ok_or_else(|| StoreError::Damaged(format!("{hash:?} is not a chunk hash")))?;
    let r = store.reader()?;
    let (flag, stored) = r.chunk(&h)?.ok_or_else(|| StoreError::Damaged(format!("no chunk {hash}")))?;
    let body = r.chunk_body(&h)?;
    let pretty = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| String::from_utf8_lossy(&body).into_owned());
    Ok(format!("chunk {hash}: {} stored bytes ({}), {} bytes uncompressed, hash verified\n{pretty}\n", stored.len() + 1, flag_name(flag), body.len()))
}

/// Write the whole history as a version 1 style JSON Lines file to `out` (EEP-0005; brief 021):
/// a version 1 header, then branch `main`'s records exactly as stored, each framed as version 1
/// frames them. A version 1 reader opens it, with the same chain values.
///
/// With `with_raw`, every row of every table is written instead, one JSON object a line:
/// `{"table", "key" (hex), "stored" (hex of the stored bytes), "flag", "decoded"}`, read-only
/// material for the HTML inspector's raw/readable toggle; that form is not a version 1 file.
/// Returns the number of lines written.
///
/// # Errors
/// The file cannot be read, a row does not decode, or `out` cannot be written.
pub fn export_jsonl(store: &Store, with_raw: bool, out: &mut dyn Write) -> Result<u64, String> {
    let r = store.reader().map_err(|e| e.to_string())?;
    let mut lines = 0u64;
    if with_raw {
        r.raw_rows(|table, key, value| {
            let (flag, decoded) = decode_raw(table, value)?;
            let line = json!({"table": table, "key": hex(key), "stored": hex(value), "flag": flag, "decoded": decoded});
            writeln!(out, "{line}").map_err(|e| e.to_string())?;
            lines += 1;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
        return Ok(lines);
    }
    let (project, _) = super::project::read_facts_pub(&r).map_err(|e| e.to_string())?;
    let created = r.meta("created_ms").map_err(|e| e.to_string())?;
    let created_ms = created.and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v.as_u64());
    let writer = r
        .meta("writer")
        .map_err(|e| e.to_string())?
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    let header = Header::project(project, created_ms, &format!("{writer} (export-jsonl)"));
    let codec = JsonLines;
    let hv = serde_json::to_value(&header).map_err(|e| e.to_string())?;
    out.write_all(&codec.encode(&hv).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    lines += 1;
    let id = branch_named(&r, "main").map_err(|e| e.to_string())?;
    let head = r.last_seq(id).map_err(|e| e.to_string())?.unwrap_or(0);
    // In chunks, so a long history never sits in memory at once.
    let mut from = 1;
    while from <= head {
        let to = head.min(from.saturating_add(4_095));
        let rows = r.tx_range(id, from..=to).map_err(|e| e.to_string())?;
        // Rows run 1..=head with no gap: a gap is damage, never a walk over empty ranges.
        if rows.len() as u64 != to.saturating_sub(from).saturating_add(1) {
            return Err(format!("transactions {from} to {to} are not all stored"));
        }
        for (_, flag, _, body) in rows {
            let record = pack::unpack(flag, &body)?;
            let v: Value = serde_json::from_slice(&record).map_err(|e| e.to_string())?;
            out.write_all(&codec.encode(&v).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            lines += 1;
        }
        let Some(next) = to.checked_add(1) else { break };
        from = next;
    }
    Ok(lines)
}

/// A stored value's flag and readable form, by table.
fn decode_raw(table: &str, value: &[u8]) -> Result<(Option<u8>, Value), String> {
    let json_of = |b: &[u8]| serde_json::from_slice::<Value>(b).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(b).into()));
    Ok(match table {
        "meta" | "branches" | "versions" => (None, json_of(value)),
        "transactions" => {
            let (flag, c, body) = split_tx_value(value)?;
            let record = pack::unpack(flag, body)?;
            (Some(flag), json!({"chain": chain::hex(&c), "record": json_of(&record)}))
        }
        "chunks" => {
            let flag = *value.first().ok_or("an empty chunk row")?;
            let body = pack::unpack(flag, value.get(1..).unwrap_or_default())?;
            (Some(flag), json_of(&body))
        }
        "assets" => {
            let (media, bytes) = split_asset_value(value)?;
            (None, json!({"media_type": media, "bytes": bytes.len()}))
        }
        other => return Err(format!("unknown table {other}")),
    })
}
