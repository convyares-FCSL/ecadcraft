//! Transaction records as JSON values: the flat op form `{"op": name, "v": n, …}`, version
//! checks, upcasts and input caps (proposal §20.2, §20.7, §20.11). The project file stores every
//! transaction as one such record, in canonical form ([`crate::canonical`]).

use ecad_model as m;
use ecad_model::TxId;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::FileError;
use crate::canonical::MAX_SAFE_INTEGER;
use crate::chain::{self, Hash};
use crate::records;

/// Meaning of record contents this build reads and writes.
pub const SCHEMA_VERSION: u64 = 1;

/// Longest string in a record (1 MiB).
pub const MAX_STRING: usize = 1 << 20;
/// Most elements in one array (ops per transaction, vertices per polyline, …).
pub const MAX_ARRAY: usize = 1_000_000;
/// Most entries in one object (properties per object, …).
pub const MAX_OBJECT: usize = 100_000;
/// Deepest nesting of arrays and objects.
pub const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TxTag {
    Tx,
}

/// A transaction record before its ops are decoded.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TxRaw {
    #[serde(rename = "type")]
    tag: TxTag,
    seq: u64,
    id: TxId,
    kind: records::TxKind,
    author: records::Author,
    time_ms: Option<u64>,
    label: String,
    prev: String,
    ops: Vec<Value>,
}

/// Names of the ops schema version 1 knows.
pub const OP_NAMES: &[&str] = &[
    "AddPage",
    "RemovePage",
    "ModifyPage",
    "AddLayer",
    "RemoveLayer",
    "AddItem",
    "ModifyItem",
    "RemoveItem",
    "CreateDevice",
    "DeleteDevice",
    "ModifyDevice",
    "SetDeviceDefinition",
    "AddTerminal",
    "RemoveTerminal",
    "LinkRepresentation",
    "ModifyRepresentation",
    "UnlinkRepresentation",
    "CreateWire",
    "DeleteWire",
    "ModifyWire",
    "AttachEnd",
    "CreateCable",
    "DeleteCable",
    "AddCableMember",
    "RemoveCableMember",
    "SetProperty",
    "EmbedDefinition",
    "RemoveDefinition",
    "SetProjectSettings",
    "NameVersion",
    "UnnameVersion",
    "AddFolder",
    "RemoveFolder",
    "ModifyFolder",
    "SetStyle",
    "SetStandard",
    "ModifyProject",
];

/// Why a record could not be decoded (§20.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Not the expected record type (including the reserved `checkpoint`).
    RecordType(String),
    /// Fields missing, unknown or of the wrong type, or values the model cannot hold.
    Parse(String),
    /// An op this build does not know.
    UnknownOp(String),
    /// An op version that is newer, or older with no upcast.
    Upcast(String),
    /// A size or integer limit.
    Limits(String),
    /// An embedded definition's recorded content hash does not match its content (§20.3).
    ContentHash(String),
}

/// Encode a model op in the flat form.
///
/// # Errors
/// An integer outside the limits.
pub fn op_value(op: &m::Op) -> Result<Value, FileError> {
    let rec = records::Op::from(op);
    let tagged = serde_json::to_value(&rec).map_err(|e| FileError::json(&e))?;
    let Value::Object(outer) = tagged else { return Err(FileError::OpNotObject) };
    let Some((name, Value::Object(fields))) = outer.into_iter().next() else {
        return Err(FileError::OpNotTagged);
    };
    let mut flat = Map::new();
    flat.insert("op".into(), Value::String(name));
    flat.insert("v".into(), Value::from(records::OP_VERSION));
    flat.extend(fields);
    check_integers(&rec)?;
    Ok(Value::Object(flat))
}

/// Encode a transaction record chained to `prev`.
///
/// # Errors
/// An integer outside the limits.
pub fn tx_value(tx: &m::Transaction, prev: &Hash) -> Result<Value, FileError> {
    if tx.seq > MAX_SAFE_INTEGER || tx.time_ms.is_some_and(|t| t > MAX_SAFE_INTEGER) {
        return Err(FileError::TxLimits(tx.seq));
    }
    let raw = TxRaw {
        tag: TxTag::Tx,
        seq: tx.seq,
        id: tx.id,
        kind: (&tx.kind).into(),
        author: (&tx.author).into(),
        time_ms: tx.time_ms,
        label: tx.label.clone(),
        prev: chain::hex(prev),
        ops: tx.ops.iter().map(op_value).collect::<Result<_, _>>()?,
    };
    serde_json::to_value(&raw).map_err(|e| FileError::json(&e))
}

/// The `type` of a record value.
pub fn record_type(v: &Value) -> Option<&str> {
    v.get("type").and_then(Value::as_str)
}

/// The `seq` and `prev` of a transaction value, read before full decoding so the chain can be
/// checked on the stored form.
pub fn seq_and_prev(v: &Value) -> (Option<u64>, Option<&str>) {
    (v.get("seq").and_then(Value::as_u64), v.get("prev").and_then(Value::as_str))
}

/// Decode a transaction record that has passed the frame and chain checks: check op versions,
/// upcast, and convert to the model.
///
/// # Errors
/// [`DecodeError`] naming the first problem.
pub fn decode_tx(v: &Value) -> Result<m::Transaction, DecodeError> {
    match record_type(v) {
        Some("tx") => {}
        Some("checkpoint") => {
            return Err(DecodeError::RecordType("checkpoint records are not supported (checkpoints are disabled, §20.8)".into()));
        }
        Some(other) => return Err(DecodeError::RecordType(format!("unknown record type {other:?}"))),
        None => return Err(DecodeError::RecordType("record has no type".into())),
    }
    let raw: TxRaw = serde_json::from_value(v.clone()).map_err(|e| DecodeError::Parse(e.to_string()))?;
    if raw.seq > MAX_SAFE_INTEGER || raw.time_ms.is_some_and(|t| t > MAX_SAFE_INTEGER) {
        return Err(DecodeError::Limits("seq or time exceeds 2^53 - 1".into()));
    }
    let mut ops = Vec::with_capacity(raw.ops.len().min(MAX_ARRAY));
    for (i, op) in raw.ops.into_iter().enumerate() {
        ops.push(decode_op(op).map_err(|e| match e {
            DecodeError::Parse(m) => DecodeError::Parse(format!("op {}: {m}", i + 1)),
            other => other,
        })?);
    }
    Ok(m::Transaction { id: raw.id, seq: raw.seq, author: raw.author.into(), label: raw.label, kind: raw.kind.into(), time_ms: raw.time_ms, ops })
}

fn decode_op(v: Value) -> Result<m::Op, DecodeError> {
    let Value::Object(mut obj) = v else { return Err(DecodeError::Parse("an op is not an object".into())) };
    let name = match obj.remove("op") {
        Some(Value::String(s)) => s,
        _ => return Err(DecodeError::Parse("an op has no name".into())),
    };
    if !OP_NAMES.contains(&name.as_str()) {
        return Err(DecodeError::UnknownOp(format!("op {name:?} is not known to this build")));
    }
    let version = obj.remove("v").and_then(|v| v.as_u64()).ok_or_else(|| DecodeError::Parse(format!("op {name} has no version")))?;
    upcast_op(&name, version, &mut obj).map_err(|e| DecodeError::Upcast(e.to_string()))?;
    let mut tagged = Map::new();
    tagged.insert(name, Value::Object(obj));
    let rec: records::Op = serde_json::from_value(Value::Object(tagged)).map_err(|e| DecodeError::Parse(e.to_string()))?;
    check_integers(&rec).map_err(|e| DecodeError::Limits(e.to_string()))?;
    let op = m::Op::try_from(rec).map_err(DecodeError::Parse)?;
    check_content_hash(&op)?;
    Ok(op)
}

/// A content hash is computed, never trusted from the file (§20.3): an embedded definition's
/// recorded `origin.content_hash` must equal the hash of its content.
fn check_content_hash(op: &m::Op) -> Result<(), DecodeError> {
    let (m::Op::EmbedDefinition { id, definition } | m::Op::RemoveDefinition { id, definition }) = op else {
        return Ok(());
    };
    let Some(origin) = &definition.origin else { return Ok(()) };
    let computed = definition_hash(definition).map_err(|e| DecodeError::Parse(e.to_string()))?;
    if computed != origin.content_hash {
        return Err(DecodeError::ContentHash(format!(
            "definition {id}: recorded content hash {} does not match its content ({})",
            origin.content_hash.hex(),
            computed.hex()
        )));
    }
    Ok(())
}

/// Bring an op stored in an older version up to [`records::OP_VERSION`], on its JSON fields.
/// Runs only after the record's integrity has been verified on its stored form (§20.7).
#[cfg_attr(not(test), allow(unused_variables))]
fn upcast_op(name: &str, version: u64, obj: &mut Map<String, Value>) -> Result<(), FileError> {
    if version == records::OP_VERSION {
        return Ok(());
    }
    if version > records::OP_VERSION {
        return Err(FileError::OpNewer { name: name.to_string(), version });
    }
    match (name, version) {
        // A synthetic older version, compiled only into the crate's own tests, proving the
        // upcast path: in "v0" a page's title was called "name".
        #[cfg(test)]
        ("AddPage", 0) => {
            let page = obj.get_mut("page").and_then(Value::as_object_mut).ok_or(FileError::OpUpcastShape("AddPage v0 has no page"))?;
            let title = page.remove("name").ok_or(FileError::OpUpcastShape("AddPage v0 page has no name"))?;
            page.insert("title".into(), title);
            Ok(())
        }
        _ => Err(FileError::OpNoUpcast { name: name.to_string(), version }),
    }
}

/// Integer fields of ops must be exact as doubles (§20.10).
fn check_integers(op: &records::Op) -> Result<(), FileError> {
    let n = match op {
        records::Op::AddPage { index, .. }
        | records::Op::RemovePage { index, .. }
        | records::Op::AddCableMember { index, .. }
        | records::Op::RemoveCableMember { index, .. }
        | records::Op::AddFolder { index, .. }
        | records::Op::RemoveFolder { index, .. } => *index,
        records::Op::NameVersion { at, .. } | records::Op::UnnameVersion { at, .. } => *at,
        records::Op::EmbedDefinition { definition, .. } | records::Op::RemoveDefinition { definition, .. } => {
            definition.origin.as_ref().map_or(0, |o| o.library_seq)
        }
        _ => 0,
    };
    if n > MAX_SAFE_INTEGER { Err(FileError::Integer(n)) } else { Ok(()) }
}

/// Refuse values beyond the caps before typed decoding (§20.11).
///
/// # Errors
/// A message naming the cap.
pub fn check_limits(v: &Value) -> Result<(), FileError> {
    fn walk(v: &Value, depth: usize) -> Result<(), FileError> {
        if depth > MAX_DEPTH {
            return Err(FileError::Depth);
        }
        match v {
            Value::String(s) if s.len() > MAX_STRING => Err(FileError::LongString),
            Value::Array(a) if a.len() > MAX_ARRAY => Err(FileError::LongArray),
            Value::Array(a) => a.iter().try_for_each(|x| walk(x, depth + 1)),
            Value::Object(o) if o.len() > MAX_OBJECT => Err(FileError::BigObject),
            Value::Object(o) => o.iter().try_for_each(|(k, x)| {
                if k.len() > MAX_STRING {
                    return Err(FileError::LongKey);
                }
                walk(x, depth + 1)
            }),
            _ => Ok(()),
        }
    }
    walk(v, 0)
}

/// The content hash of a definition (§20.9): SHA-256 over the canonical form of its record with
/// `origin` cleared, because the origin records the hash itself.
///
/// # Errors
/// A number without canonical form (not possible for a valid definition).
pub fn definition_hash(d: &m::Definition) -> Result<m::ContentHash, FileError> {
    let mut rec = records::Definition::from(d);
    rec.origin = None;
    let v = serde_json::to_value(&rec).map_err(|e| FileError::json(&e))?;
    let bytes = crate::canonical::canonical(&v)?;
    Ok(m::ContentHash(chain::content_hash(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A transaction whose `AddPage` is stored in the synthetic version 0 is upcast on decoding.
    #[test]
    fn old_op_versions_are_upcast() {
        let tx = m::Transaction {
            id: TxId(1),
            seq: 1,
            author: m::Author::User,
            label: "v0 page".into(),
            kind: m::TxKind::Edit,
            time_ms: None,
            ops: vec![m::Op::AddPage { id: m::PageId(9), page: m::Page::titled("1").unwrap(), index: 0 }],
        };
        let mut v = tx_value(&tx, &[0; 32]).unwrap();
        let op = v["ops"][0].as_object_mut().unwrap();
        op.insert("v".into(), Value::from(0u64));
        let page = op.get_mut("page").unwrap().as_object_mut().unwrap();
        let title = page.remove("title").unwrap();
        page.insert("name".into(), title);
        let back = decode_tx(&v).unwrap();
        let m::Op::AddPage { page, .. } = &back.ops[0] else { panic!("not AddPage") };
        assert_eq!(page.title.as_str(), "1");
    }

    #[test]
    fn newer_op_versions_are_refused() {
        let mut obj = Map::new();
        assert!(upcast_op("AddPage", 2, &mut obj).unwrap_err().to_string().contains("newer"));
        assert!(upcast_op("AddItem", 0, &mut obj).unwrap_err().to_string().contains("no upcast"));
    }
}
