//! Text views of a version 2 file (brief 021 scope 7; moved here from `ecad_file::inspect`
//! by brief 029): its tables, branches, log, versions and chunks. Reads only; never changes the
//! file.

use ecad_file::chain;
use ecad_file::pack;
use ecad_file::schema::{BranchId, VersionKind, split_asset_value, split_tx_value};
use ecad_file::{Reader, Store, StoreError};
use serde_json::{Value, json};

/// Lower-case hex of `b`.
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The name of a stored compression flag.
pub fn flag_name(f: u8) -> &'static str {
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

/// The branch `text` names: its ID (32 hex digits), or its name. A name that more than one
/// branch has (an archived one and an open one may share it) is refused, listing their IDs:
/// never a guess.
pub fn branch_named(r: &Reader, text: &str) -> Result<BranchId, StoreError> {
    let all = r.branches()?;
    if let Some(id) = BranchId::parse(text)
        && all.iter().any(|(i, _)| *i == id)
    {
        return Ok(id);
    }
    let named: Vec<&(BranchId, ecad_file::schema::BranchRecord)> = all.iter().filter(|(_, b)| b.name == text).collect();
    match named.as_slice() {
        [(id, _)] => Ok(*id),
        [] => Err(StoreError::Damaged(format!("there is no branch named {text:?}"))),
        many => {
            let ids: Vec<String> = many.iter().map(|(id, b)| format!("{} ({:?})", id.hex(), b.status)).collect();
            Err(StoreError::Refused(format!("{} branches are named {text:?}; give one of their IDs: {}", many.len(), ids.join(", "))))
        }
    }
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
        let record = pack::unpack(flag, &body).map_err(|e| StoreError::Damaged(e.to_string()))?;
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
        for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto, VersionKind::Merge] {
            for v in r.versions(id, kind)? {
                if let Some(m) = &v.merge {
                    s.push_str(&format!("{} Merge at {} from {} at {}\n", b.name, v.point.seq, m.source, m.source_head.seq));
                    continue;
                }
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

/// A stored value's flag and readable form, by table.
///
/// # Errors
/// The value does not decode as its table's rows do.
pub fn decode_raw(table: &str, value: &[u8]) -> Result<(Option<u8>, Value), String> {
    let json_of = |b: &[u8]| serde_json::from_slice::<Value>(b).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(b).into()));
    Ok(match table {
        "meta" | "branches" | "versions" => (None, json_of(value)),
        "transactions" => {
            let (flag, c, body) = split_tx_value(value).map_err(|e| e.to_string())?;
            let record = pack::unpack(flag, body).map_err(|e| e.to_string())?;
            (Some(flag), json!({"chain": chain::hex(&c), "record": json_of(&record)}))
        }
        "chunks" => {
            let flag = *value.first().ok_or("an empty chunk row")?;
            let body = pack::unpack(flag, value.get(1..).unwrap_or_default()).map_err(|e| e.to_string())?;
            (Some(flag), json_of(&body))
        }
        "assets" => {
            let (media, bytes) = split_asset_value(value).map_err(|e| e.to_string())?;
            (None, json!({"media_type": media, "bytes": bytes.len()}))
        }
        other => return Err(format!("unknown table {other}")),
    })
}
