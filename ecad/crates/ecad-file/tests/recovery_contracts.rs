//! Recovery on open (proposal §20.6): an unterminated trailing write is distinguished from a
//! complete corrupt record, every check fails the same way at the last position as in the middle,
//! and the report names what was omitted.

mod common;

use common::*;
use ecad_file::chain;
use ecad_file::codec::{Codec, JsonLines};
use ecad_file::record::tx_value;
use ecad_file::{Access, Check, OpenError, Outcome, ProjectFile, open_project};
use ecad_model::*;
use ecad_store::{Medium, MemoryMedium};
use serde_json::Value;

struct File {
    bytes: Vec<u8>,
    /// (offset, len) of every line, header first.
    lines: Vec<(usize, usize)>,
    ledger: Ledger,
}

fn good() -> File {
    let mut f = fixture();
    let file = ProjectFile::create(MemoryMedium::default(), PROJECT, &mut f.ledger, None, WRITER).unwrap();
    let bytes = file.medium().read_all().unwrap();
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            lines.push((start, i + 1 - start));
            start = i + 1;
        }
    }
    File { bytes, lines, ledger: f.ledger }
}

/// The record value at line `n` (1 = first transaction).
fn record(file: &File, n: usize) -> Value {
    let (o, l) = file.lines[n];
    let frame = &file.bytes[o..o + l - 1];
    serde_json::from_slice(JsonLines.payload(frame).unwrap()).unwrap()
}

/// Replace line `n` by `line` (which must end in a newline).
fn with_line(file: &File, n: usize, line: &[u8]) -> Vec<u8> {
    let (o, l) = file.lines[n];
    let mut b = file.bytes[..o].to_vec();
    b.extend_from_slice(line);
    b.extend_from_slice(&file.bytes[o + l..]);
    b
}

/// The chain head after line `n - 1`.
fn prev_of(file: &File, n: usize) -> chain::Hash {
    let (o, _) = file.lines[n];
    open_project(&file.bytes[..o], SEED).unwrap().chain_head
}

/// A complete record for line `n` holding `tx`, correctly framed and chained.
fn tx_line(file: &File, n: usize, tx: &Transaction) -> Vec<u8> {
    JsonLines.encode(&tx_value(tx, &prev_of(file, n)).unwrap()).unwrap()
}

fn corrupt(bytes: &[u8]) -> (ecad_file::Opened, ecad_file::Corruption) {
    let o = open_project(bytes, SEED).unwrap();
    let Outcome::Corrupt(c) = o.report.outcome.clone() else { panic!("expected corrupt, got {:?}", o.report.outcome) };
    assert_eq!(o.report.access, Access::ReadOnly);
    (o, c)
}

#[test]
fn a_clean_file_opens_writable() {
    let f = good();
    let o = open_project(&f.bytes, SEED).unwrap();
    assert_eq!(o.report.outcome, Outcome::Clean);
    assert_eq!(o.report.access, Access::Writable);
    assert!(o.report.describe().starts_with("Opened:"));
}

#[test]
fn an_unterminated_trailing_write_opens_writable_and_is_reported() {
    let f = good();
    let last = f.lines.len() - 1;
    let (o_last, l_last) = f.lines[last];
    for cut in [1, l_last / 2, l_last - 1] {
        // An interrupted append of the last record: everything but its tail end is there.
        let bytes = &f.bytes[..o_last + cut];
        let o = open_project(bytes, SEED).unwrap();
        assert_eq!(o.report.access, Access::Writable, "cut {cut}");
        assert_eq!(o.report.verified_seq, last as u64 - 1);
        let Outcome::IncompleteTail { offset, len, ref preview } = o.report.outcome else { panic!("cut {cut}") };
        assert_eq!((offset, len), (o_last as u64, cut as u64));
        assert!(!preview.is_empty());
        assert!(o.report.describe().contains("incomplete trailing write"));
        assert_eq!(o.fragment.as_deref(), Some(&bytes[o_last..]));
    }
    // Zeros a crash can leave past the end, with no terminator, are an incomplete write too.
    let mut zeros = f.bytes.clone();
    zeros.extend_from_slice(&[0u8; 512]);
    let o = open_project(&zeros, SEED).unwrap();
    assert!(matches!(o.report.outcome, Outcome::IncompleteTail { len: 512, .. }));
    assert_eq!(o.report.verified_seq, f.ledger.head());
    // A complete record missing only its terminator is still not durable: incomplete.
    let o = open_project(&f.bytes[..f.bytes.len() - 1], SEED).unwrap();
    assert!(matches!(o.report.outcome, Outcome::IncompleteTail { .. }));
    assert_eq!(o.report.verified_seq, f.ledger.head() - 1);
}

/// Each way a complete record can fail, as (name, check, replacement line for position `n`).
fn failures(f: &File, n: usize) -> Vec<(&'static str, Check, Vec<u8>)> {
    let (o, l) = f.lines[n];
    let original = &f.bytes[o..o + l];
    let value = record(f, n);
    let seq = n as u64;
    let mut out: Vec<(&'static str, Check, Vec<u8>)> = Vec::new();
    // CRC: change a payload byte, keep the stored CRC.
    let mut crc = original.to_vec();
    let i = crc.len() - 3;
    crc[i] = if crc[i] == b'0' { b'1' } else { b'0' };
    out.push(("crc", Check::Crc, crc));
    // Frame shape.
    out.push(("frame", Check::Frame, b"not a frame\n".to_vec()));
    // Unparsable payload with a valid CRC.
    let payload = b"{\"type\":\"tx\",";
    let mut p = format!("{{\"crc\":\"{:08x}\",\"rec\":", ecad_file::crc32c(payload)).into_bytes();
    p.extend_from_slice(payload);
    p.extend_from_slice(b"}\n");
    out.push(("parse", Check::Parse, p));
    // Wrong record type: a (disabled) checkpoint.
    out.push(("checkpoint", Check::RecordType, JsonLines.encode(&serde_json::json!({"type": "checkpoint"})).unwrap()));
    // Wrong sequence number (framed and chained correctly).
    let mut v = value.clone();
    v["seq"] = Value::from(seq + 1);
    out.push(("sequence", Check::Sequence, JsonLines.encode(&v).unwrap()));
    // Broken chain.
    let mut v = value.clone();
    v["prev"] = Value::from(chain::hex(&[7u8; 32]));
    out.push(("chain", Check::Chain, JsonLines.encode(&v).unwrap()));
    // Unknown op and newer op version.
    let mut v = value.clone();
    v["ops"][0]["op"] = Value::from("TeleportComponent");
    out.push(("unknown op", Check::UnknownOp, JsonLines.encode(&v).unwrap()));
    let mut v = value.clone();
    v["ops"][0]["v"] = Value::from(2u64);
    out.push(("newer op", Check::Upcast, JsonLines.encode(&v).unwrap()));
    // An unknown field.
    let mut v = value.clone();
    v["extra"] = Value::from(1);
    out.push(("unknown field", Check::Parse, JsonLines.encode(&v).unwrap()));
    // An integer beyond 2^53 - 1 where the schema needs an exact integer.
    let mut v = value.clone();
    v["time_ms"] = Value::from(9_007_199_254_740_992u64);
    out.push(("integer limit", Check::Limits, JsonLines.encode(&v).unwrap()));
    // Precondition: remove an item that does not exist.
    let ghost = Item {
        page: f.ledger.document().page_order[0],
        style: style(LayerId(1)),
        kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(0.0, 0.0) }),
    };
    let tx = Transaction {
        id: TxId(0xdead),
        seq,
        author: Author::User,
        label: "bad".into(),
        kind: TxKind::Edit,
        time_ms: None,
        ops: vec![Op::RemoveItem { id: ItemId(0xbeef), item: ghost.clone() }],
    };
    out.push(("precondition", Check::Precondition, tx_line(f, n, &tx)));
    // Invariant: an item on a page that does not exist.
    let tx = Transaction {
        ops: vec![Op::AddItem { id: ItemId(0xbeef), item: Item { page: PageId(0xfeed), ..ghost } }],
        ..tx
    };
    out.push(("invariant", Check::Invariant, tx_line(f, n, &tx)));
    // An embedded definition whose recorded content hash is not its content's.
    let mut definition = f.ledger.document().definitions.values().find(|d| d.origin.is_some()).unwrap().clone();
    if let Some(origin) = definition.origin.as_mut() {
        origin.content_hash = ContentHash([9; 32]);
    }
    let tx = Transaction { ops: vec![Op::EmbedDefinition { id: DefinitionId(0xd0d0), definition }], ..tx };
    out.push(("content hash", Check::ContentHash, tx_line(f, n, &tx)));
    out
}

#[test]
fn a_complete_corrupt_record_opens_read_only_wherever_it_is() {
    let f = good();
    let last = f.lines.len() - 1;
    for n in [1, last / 2, last] {
        for (name, check, line) in failures(&f, n) {
            let bytes = with_line(&f, n, &line);
            let (o, c) = corrupt(&bytes);
            assert_eq!(c.check, check, "{name} at record {n}");
            assert_eq!(o.report.verified_seq, n as u64 - 1, "{name} at record {n}: verified prefix");
            assert_eq!(c.offset, f.lines[n].0 as u64, "{name} at record {n}: offset");
            assert_eq!(c.omitted_records, (f.lines.len() - n) as u64, "{name} at record {n}: omitted records");
            assert_eq!(c.omitted_bytes, (bytes.len() - f.lines[n].0) as u64, "{name} at record {n}: omitted bytes");
            assert_eq!(o.ledger.log(), &f.ledger.log()[..n - 1]);
            let text = o.report.describe();
            assert!(text.contains("read-only") && text.contains("not changed"), "{name}: {text}");
            if n == last {
                // The final record being complete is what matters: it is not treated as an
                // interrupted write.
                assert!(o.fragment.is_none());
            }
        }
    }
}

#[test]
fn the_source_is_preserved_and_a_read_only_file_refuses_save() {
    let f = good();
    let last = f.lines.len() - 1;
    let (_, _, line) = failures(&f, last).remove(0);
    let bytes = with_line(&f, last, &line);
    let (mut file, mut ledger, report) = ProjectFile::open(MemoryMedium::from_bytes(bytes.clone()), SEED).unwrap();
    assert_eq!(report.access, Access::ReadOnly);
    ledger
        .commit(
            Author::User,
            "edit",
            vec![Op::AddPage { id: PageId(0x99), page: Page { title: "x".into() }, index: 1 }],
        )
        .unwrap();
    assert!(matches!(file.save(&mut ledger), Err(ecad_file::SaveError::ReadOnly)));
    assert_eq!(file.medium().read_all().unwrap(), bytes, "nothing written or truncated");
    // Save As rescues the verified history into a new file.
    let rescued = ProjectFile::create(MemoryMedium::default(), PROJECT, &mut ledger, None, WRITER).unwrap();
    let o = open_project(&rescued.medium().read_all().unwrap(), SEED).unwrap();
    assert_eq!(o.report.outcome, Outcome::Clean);
    assert_eq!(o.ledger.log(), ledger.log());
}

#[test]
fn headers_that_cannot_be_trusted_refuse_to_open() {
    assert_eq!(open_project(b"", SEED).unwrap_err(), OpenError::Empty);
    let f = good();
    let (o, l) = f.lines[0];
    assert!(matches!(open_project(&f.bytes[..l - 1], SEED), Err(OpenError::Header(_))), "incomplete header");
    let header: Value = record(&f, 0);
    let cases: Vec<(&str, Value)> = vec![
        ("container version", {
            let mut h = header.clone();
            h["container_version"] = 2.into();
            h
        }),
        ("codec", {
            let mut h = header.clone();
            h["codec"] = "cbor/1".into();
            h
        }),
        ("schema version", {
            let mut h = header.clone();
            h["schema_version"] = 2.into();
            h
        }),
        ("library", {
            let mut h = header.clone();
            h["container"] = "ecad-library".into();
            h.as_object_mut().unwrap().remove("project_id");
            h["library_id"] = LIBRARY.hex().into();
            h
        }),
        ("not ecad", {
            let mut h = header.clone();
            h["container"] = "zip".into();
            h
        }),
    ];
    for (name, h) in cases {
        let mut b = JsonLines.encode(&h).unwrap();
        b.extend_from_slice(&f.bytes[o + l..]);
        assert!(matches!(open_project(&b, SEED), Err(OpenError::Unsupported(_))), "{name}");
    }
    let mut b = f.bytes.clone();
    b[10] ^= 1;
    assert!(matches!(open_project(&b, SEED), Err(OpenError::Header(_))), "header CRC");
}
