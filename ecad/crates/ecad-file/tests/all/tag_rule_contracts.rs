//! Tag rule (brief 029, FP1): a snapshot's `model:meta` tag marks are a cache of what the history
//! gives under [`ecad_model::tag::TAG_RULE`]. A file written before brief 026 (rule 1) cached marks
//! without cable tags; it opens with the marks recomputed, Verify reports that as information, and
//! a re-save writes the current rule. The rule is in the version row, which builds before 029 read
//! (they ignore fields they do not know).
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::path::Path;

use crate::common::*;
use crate::container_v2::TempDir;
use ecad_file::chain;
use ecad_file::v2::chunks::{META_KEY, ModelMeta};
use ecad_file::v2::schema::{self, BranchId, VersionKind};
use ecad_file::v2::{ProjectV2, Store};
use ecad_model::tag::{HighWater, TAG_RULE, note_ops};
use ecad_model::*;

fn cable(tag: &str) -> Cable {
    let mut properties = ecad_model::doc::Properties::new();
    properties.insert("tag".into(), tag.into());
    Cable { properties, members: Vec::new() }
}

/// Cables `-W01` and `-W02`, then `-W02` deleted, then lines: the snapshot (every 16) is after the
/// deletion.
fn history() -> Ledger {
    let mut f = fixture();
    let (w1, w2): (CableId, CableId) = (f.ids.fresh(), f.ids.fresh());
    f.commit("Cables", vec![Op::CreateCable { id: w1, cable: cable("-W01") }, Op::CreateCable { id: w2, cable: cable("-W02") }]);
    f.commit("Delete", vec![Op::DeleteCable { id: w2, cable: cable("-W02") }]);
    for i in 0..20 {
        let id: ItemId = f.ids.fresh();
        let kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(f64::from(i), 0.0), b: Pt::new(0.0, 1.0) });
        f.commit("LINE", vec![Op::AddItem { id, item: Item { page: f.page, style: style(f.layer), kind } }]);
    }
    f.ledger
}

fn create(path: &Path, ledger: &Ledger) {
    let store = Store::create(path).unwrap();
    ProjectV2::create(store, PROJECT, ledger, None, None, Some(1), WRITER, 16).unwrap();
}

/// The marks as a build before brief 026 counted them (rule 1): no cable tags, no resets.
pub fn rule_1(ops: &[Op], high: &mut HighWater) {
    let kept: Vec<Op> = ops
        .iter()
        .filter(|op| match op {
            Op::CreateCable { .. } | Op::DeleteCable { .. } => false,
            Op::SetProperty { key, .. } => !key.starts_with("numbering.high."),
            _ => true,
        })
        .cloned()
        .collect();
    note_ops(high, &kept);
}

/// Make the file at `path` (written from `ledger`) look as a build before brief 026 wrote it:
/// every snapshot's `model:meta` holds rule 1 marks, and its row names no rule.
fn make_rule_1(path: &Path, ledger: &Ledger) {
    let store = Store::open(path, true).unwrap();
    let main = BranchId::main_of(PROJECT.0);
    let rows: Vec<schema::VersionRecord> = {
        let r = store.reader().unwrap();
        [VersionKind::User, VersionKind::Revision, VersionKind::Auto].into_iter().flat_map(|k| r.versions(main, k).unwrap()).collect()
    };
    assert!(!rows.is_empty());
    let mut w = store.writer().unwrap();
    for mut rec in rows {
        let mut high = HighWater::new();
        for tx in ledger.log().iter().take(rec.point.seq as usize) {
            rule_1(&tx.ops, &mut high);
        }
        let body = serde_json::to_vec(&ModelMeta { tag_high: high }).unwrap();
        let h = schema::chunk_hash(&body);
        w.put_chunk(&h, &body).unwrap();
        rec.chunks.insert(META_KEY.into(), chain::hex(&h));
        rec.tag_rule = None;
        w.put_version(main, &rec).unwrap();
    }
    w.commit().unwrap();
}

/// Each snapshot row of main and its `model:meta`.
fn snapshots(path: &Path) -> Vec<(schema::VersionRecord, ModelMeta)> {
    let s = Store::open(path, false).unwrap();
    let r = s.reader().unwrap();
    let main = BranchId::main_of(PROJECT.0);
    let mut out = Vec::new();
    for k in [VersionKind::User, VersionKind::Revision, VersionKind::Auto] {
        for v in r.versions(main, k).unwrap() {
            let h = chain::parse_hex(&v.chunks[META_KEY]).unwrap();
            let meta: ModelMeta = serde_json::from_slice(&r.chunk_body(&h).unwrap()).unwrap();
            out.push((v, meta));
        }
    }
    out
}

#[test]
fn a_save_records_the_current_rule_with_cable_marks() {
    let dir = TempDir::new("tag-rule-new");
    let p = dir.path("p.ecad");
    let l = history();
    create(&p, &l);
    let mut with_cables = 0;
    for (v, meta) in snapshots(&p) {
        assert_eq!(v.tag_rule, Some(TAG_RULE), "{}", v.name);
        let mut want = HighWater::new();
        for tx in l.log().iter().take(v.point.seq as usize) {
            note_ops(&mut want, &tx.ops);
        }
        assert_eq!(meta.tag_high, want, "{}", v.name);
        with_cables += usize::from(meta.tag_high.get("W") == Some(&2));
    }
    assert!(with_cables > 0, "a snapshot after the deletion counts -W02");
}

#[test]
fn a_rule_1_file_opens_with_its_cable_marks_recomputed() {
    let dir = TempDir::new("tag-rule-old");
    let p = dir.path("p.ecad");
    let l = history();
    create(&p, &l);
    make_rule_1(&p, &l);
    for (v, meta) in snapshots(&p) {
        assert_eq!((v.tag_rule, meta.tag_high.get("W")), (None, None), "the file now looks as before brief 026");
    }
    {
        let o = ProjectV2::open(Store::open(&p, false).unwrap(), 1, WRITER).unwrap();
        assert!(o.report.damage.is_none(), "{:?}", o.report);
        assert!(o.report.snapshot_seq > 0, "opened from a snapshot");
        assert_eq!(o.project.base_tag_high().get("W"), Some(&2), "-W02 was deleted before the snapshot: its number stays used");
        assert_eq!(o.ledger.document(), l.document());
    }
    // A branch forked after the snapshot starts from the recomputed marks too.
    let mut proj = ProjectV2::open(Store::open(&p, true).unwrap(), 1, WRITER).unwrap();
    let main = proj.project.branch();
    let head = proj.ledger.head();
    let (b, _) = proj.project.create_branch("B", main, head, None, None).unwrap();
    let (_, _) = proj.project.switch_to(&proj.ledger, b, 2).unwrap();
    assert_eq!(proj.project.base_tag_high().get("W"), Some(&2));
}

#[test]
fn verify_reports_rule_1_marks_as_recomputed_not_as_a_failure() {
    let dir = TempDir::new("tag-rule-verify");
    let p = dir.path("p.ecad");
    let l = history();
    create(&p, &l);
    make_rule_1(&p, &l);
    let v = ecad_file::v2::verify::verify(&Store::open(&p, false).unwrap());
    assert!(v.ok, "{:#?}", v.lines);
    assert!(v.lines.iter().any(|x| x.contains("recomputed on open (rule 1 → 2)")), "{:#?}", v.lines);
    // A rule 2 snapshot whose marks differ is still a failure.
    let mut stale = Vec::new();
    {
        let store = Store::open(&p, true).unwrap();
        let main = BranchId::main_of(PROJECT.0);
        let mut rows = store.reader().unwrap().versions(main, VersionKind::Auto).unwrap();
        let mut w = store.writer().unwrap();
        for rec in &mut rows {
            rec.tag_rule = Some(TAG_RULE);
            w.put_version(main, rec).unwrap();
            stale.push(rec.name.clone());
        }
        w.commit().unwrap();
    }
    let v = ecad_file::v2::verify::verify(&Store::open(&p, false).unwrap());
    assert!(!v.ok && stale.iter().all(|n| v.failures.iter().any(|f| f.contains(n.as_str()) && f.contains("tag numbers"))), "{:#?}", v.lines);
}

#[test]
fn a_resave_writes_the_current_rule_and_verifies() {
    let dir = TempDir::new("tag-rule-resave");
    let p = dir.path("p.ecad");
    let l = history();
    create(&p, &l);
    make_rule_1(&p, &l);
    let mut o = ProjectV2::open(Store::open(&p, true).unwrap(), 1, WRITER).unwrap();
    let at = o.ledger.head();
    o.ledger.commit(Author::User, "Version", vec![Op::NameVersion { name: "Again".into(), at, description: None }]).unwrap();
    o.project.save(&mut o.ledger).unwrap();
    drop(o);
    let again = snapshots(&p).into_iter().find(|(v, _)| v.name == "Again").unwrap();
    assert_eq!(again.0.tag_rule, Some(TAG_RULE));
    assert_eq!(again.1.tag_high.get("W"), Some(&2), "the new snapshot carries the recomputed marks");
    let v = ecad_file::v2::verify::verify(&Store::open(&p, false).unwrap());
    assert!(v.ok, "{:#?}", v.lines);
    let o = ProjectV2::open(Store::open(&p, false).unwrap(), 1, WRITER).unwrap();
    assert_eq!(o.project.base_tag_high().get("W"), Some(&2));
}

/// The row and `model:meta` as a build before 029 parses them.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct OldVersionRecord {
    kind: VersionKind,
    name: String,
    point: schema::Point,
    chunks: BTreeMap<String, String>,
    undo: Vec<(u64, String)>,
    redo: Vec<(u64, String)>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct OldModelMeta {
    tag_high: BTreeMap<String, u64>,
}

#[test]
fn a_build_before_029_reads_the_rows_and_meta_029_writes() {
    let dir = TempDir::new("tag-rule-old-reader");
    let p = dir.path("p.ecad");
    create(&p, &history());
    let s = Store::open(&p, false).unwrap();
    let r = s.reader().unwrap();
    let mut seen = 0;
    r.raw_rows(|table, _, value| {
        if table == "versions" {
            let old: OldVersionRecord = serde_json::from_slice(value).map_err(|e| e.to_string())?;
            if old.kind != VersionKind::Merge {
                assert_eq!(old.extra.get("tag_rule"), Some(&serde_json::json!(TAG_RULE)), "kept as an unknown field");
                let h = chain::parse_hex(&old.chunks[META_KEY]).ok_or("hash")?;
                let body = r.chunk_body(&h).map_err(|e| e.to_string())?;
                serde_json::from_slice::<OldModelMeta>(&body).map_err(|e| format!("an old reader refuses model:meta: {e}"))?;
                seen += 1;
            }
        }
        Ok(())
    })
    .unwrap();
    assert!(seen > 0);
}
