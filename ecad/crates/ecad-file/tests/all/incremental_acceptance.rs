//! Brief 013 acceptance (tester): behaviour that must match `dev` exactly.
//!
//! Every test here uses only API that exists on `dev` as well, so the same file runs on both
//! trees. With `ECAD_013_DUMP=<dir>` set, the tests also write what they observed (file bytes,
//! commit outcomes and messages, damaged-file reports) to that folder, so two runs can be
//! compared byte for byte.

use crate::common;

use common::*;
use ecad_file::chain;
use ecad_file::codec::{Codec, JsonLines};
use ecad_file::record::tx_value;
use ecad_file::{Access, Outcome, ProjectFile, open_project};
use ecad_model::*;
use ecad_store::{Medium, MemoryMedium};

fn dump(name: &str, bytes: &[u8]) {
    if let Some(dir) = std::env::var_os("ECAD_013_DUMP") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), bytes).unwrap();
    }
}

fn written(ledger: &mut Ledger) -> Vec<u8> {
    let file = ProjectFile::create(MemoryMedium::default(), PROJECT, ledger, Some(1_760_000_000_000), WRITER).unwrap();
    file.medium().read_all().unwrap()
}

/// Every terminal and conductor the document holds, with the query answers for each.
fn queries(doc: &Document) -> String {
    let mut s = String::new();
    for t in doc.terminals.keys() {
        s += &format!("ends_at {t}: {:?}\n", doc.ends_at(*t));
    }
    for c in doc.conductors.keys() {
        s += &format!("cables_of {c}: {:?}\n", doc.cables_of(*c));
    }
    s
}

/// The query answers of `doc` equal those of a copy rebuilt from its serialised form.
fn assert_queries_fresh(doc: &Document, context: &str) {
    let copy: Document = serde_json::from_value(serde_json::to_value(doc).unwrap()).unwrap();
    assert_eq!(&copy, doc, "{context}: serialisation round trip");
    assert_eq!(queries(doc), queries(&copy), "{context}: indexes answer as a rebuilt copy does");
}

fn doc_bytes(doc: &Document) -> Vec<u8> {
    serde_json::to_vec(doc).unwrap()
}

/// Small deterministic generator (no extra dependency).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// A variation of a recorded op: itself, its inverse, or one with a shifted index / swapped
/// before and after. Many of these fail their preconditions or the invariants.
fn vary(op: &Op, r: &mut Lcg) -> Op {
    match r.below(4) {
        0 => op.clone(),
        1 => op.inverse(),
        2 => match op.clone() {
            Op::AddPage { id, page, index } => Op::AddPage { id, page, index: index + 1 + r.below(3) },
            Op::RemovePage { id, page, index } => Op::RemovePage { id, page, index: index + 1 },
            Op::AddCableMember { cable, index, member } => Op::AddCableMember { cable, index: index + 1, member },
            Op::RemoveCableMember { cable, index, member } => Op::RemoveCableMember { cable, index: index + 1, member },
            Op::AttachEnd { conductor, end, before, after } => {
                Op::AttachEnd { conductor, end: if end == End::A { End::B } else { End::A }, before, after }
            }
            other => other.inverse(),
        },
        _ => match op.clone() {
            Op::SetProperty { object, key, before, after } => {
                Op::SetProperty { object, key, before: after, after: before }
            }
            Op::AttachEnd { conductor, end, before, after } => {
                Op::AttachEnd { conductor, end, before: after, after: before }
            }
            other => other,
        },
    }
}

/// A long scripted session: the shared fixture, then random multi-op transactions built from
/// variations of recorded ops (many fail), with undo and redo runs interleaved. Returns the
/// ledger and a transcript of every attempt's outcome.
fn session(steps: usize) -> (Ledger, String) {
    let mut f = fixture();
    let pool: Vec<Op> = f.ledger.log().iter().flat_map(|t| t.ops.clone()).collect();
    let mut r = Lcg(0x013);
    let mut transcript = String::new();
    let mut time = 1_770_000_000_000u64;
    for step in 0..steps {
        time += 1000;
        let before = f.ledger.document().clone();
        let head = f.ledger.head();
        let outcome = match r.below(10) {
            0..=1 => f.ledger.undo_at(Author::User, Some(time)).map(|id| format!("undo {id}")),
            2 => f.ledger.redo_at(Author::User, Some(time)).map(|id| format!("redo {id}")),
            _ => {
                let n = 1 + r.below(4);
                let ops: Vec<Op> = (0..n).map(|_| vary(&pool[r.below(pool.len())], &mut r)).collect();
                f.ledger.commit_at(Author::User, "random", ops, Some(time)).map(|id| format!("commit {id}"))
            }
        };
        match &outcome {
            Ok(text) => transcript += &format!("{step}: ok {text}\n"),
            Err(e) => {
                transcript += &format!("{step}: err {e}\n");
                assert_eq!(f.ledger.head(), head, "step {step}: a failed transaction appends nothing");
                assert_eq!(doc_bytes(f.ledger.document()), doc_bytes(&before), "step {step}: rolled back");
            }
        }
        transcript += &format!("   undo={:?} redo={:?}\n", f.ledger.undo_label(), f.ledger.redo_label());
        assert_queries_fresh(f.ledger.document(), &format!("step {step}"));
    }
    (f.ledger, transcript)
}

#[test]
fn a_scripted_session_records_the_same_bytes_and_outcomes() {
    let (mut ledger, transcript) = session(400);
    let ok = transcript.lines().filter(|l| l.contains(": ok ")).count();
    let err = transcript.lines().filter(|l| l.contains(": err ")).count();
    assert!(ok > 50 && err > 50, "the session mixes successes ({ok}) and failures ({err})");
    let bytes = written(&mut ledger);
    dump("session.ecad", &bytes);
    dump("session-transcript.txt", transcript.as_bytes());
    dump("session-document.json", &doc_bytes(ledger.document()));

    // Save and reopen: same log, same document, same undo/redo state.
    let opened = open_project(&bytes, SEED).unwrap();
    assert_eq!(opened.report.outcome, Outcome::Clean);
    assert_eq!(opened.ledger.log(), ledger.log());
    assert_eq!(opened.ledger.document(), ledger.document());
    assert_eq!(opened.ledger.undo_label(), ledger.undo_label());
    assert_eq!(opened.ledger.redo_label(), ledger.redo_label());
    assert_queries_fresh(opened.ledger.document(), "reopened");

    // Every state in history is reachable and equals replaying that prefix.
    for seq in (0..=ledger.head()).step_by(7) {
        let prefix = Ledger::replay(ledger.log()[..seq as usize].to_vec(), SEED).unwrap();
        assert_eq!(&ledger.state_at(seq).unwrap(), prefix.document(), "state_at {seq}");
    }
}

#[test]
fn undo_and_redo_across_many_steps_survive_save_and_reopen() {
    let mut f = fixture();
    let start = f.ledger.document().clone();
    let base = f.ledger.head();
    // Thirty edits that each add a page, then undo all of them and the fixture's own steps.
    let mut pages = Vec::new();
    for i in 0..30 {
        let id: PageId = f.ids.fresh();
        pages.push(id);
        f.commit("page", vec![Op::AddPage { id, page: Page::new(format!("p{i}")), index: 1 }]);
    }
    let full = f.ledger.document().clone();
    let mut undone = 0;
    while f.ledger.undo_label().is_some() {
        f.time += 1000;
        f.ledger.undo_at(Author::User, Some(f.time)).unwrap();
        undone += 1;
        assert_queries_fresh(f.ledger.document(), &format!("undo {undone}"));
    }
    assert!(undone > 30, "undid every step ({undone})");
    assert_eq!(f.ledger.document(), &Document::default());

    // Save at the bottom, reopen, and redo everything on the reopened ledger.
    let bytes = written(&mut f.ledger);
    dump("undo-bottom.ecad", &bytes);
    let mut reopened = open_project(&bytes, SEED).unwrap().ledger;
    assert_eq!(reopened.redo_label(), f.ledger.redo_label());
    let mut redone = 0;
    while reopened.redo_label().is_some() {
        f.time += 1000;
        reopened.redo_at(Author::User, Some(f.time)).unwrap();
        f.ledger.redo_at(Author::User, Some(f.time)).unwrap();
        redone += 1;
        assert_eq!(reopened.document(), f.ledger.document(), "redo {redone}");
    }
    assert_eq!(redone, undone);
    assert_eq!(reopened.document(), &full);
    // Transaction IDs after a reopen come from the file's seed, so only the documents compare.
    assert_eq!(reopened.head(), f.ledger.head());
    assert_queries_fresh(reopened.document(), "after redo");
    // Back to the fixture's end state by undoing the thirty pages.
    for _ in 0..30 {
        f.time += 1000;
        reopened.undo_at(Author::User, Some(f.time)).unwrap();
    }
    assert_eq!(reopened.document(), &start);
    assert_eq!(reopened.head(), base + 30 + undone * 2 + 30);
    dump("undo-redo.ecad", &written(&mut reopened));
}

/// The 1-based record line `n` replaced by `tx`, chained correctly.
fn with_tx(bytes: &[u8], n: usize, tx: &Transaction) -> Vec<u8> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            lines.push((start, i + 1));
            start = i + 1;
        }
    }
    let (o, e) = lines[n];
    let prev: chain::Hash = open_project(&bytes[..o], SEED).unwrap().chain_head;
    let line = JsonLines.encode(&tx_value(tx, &prev).unwrap()).unwrap();
    let mut out = bytes[..o].to_vec();
    out.extend_from_slice(&line);
    out.extend_from_slice(&bytes[e..]);
    out
}

#[test]
fn damaged_files_open_read_only_at_the_last_good_transaction_with_the_same_report() {
    let mut f = fixture();
    let bytes = written(&mut f.ledger);
    // The log as read back from the file: the canonical form writes -0.0 as 0, so the documents
    // compared are those of transactions read from bytes, as `open_project` sees them.
    let log = open_project(&bytes, SEED).unwrap().ledger.log().to_vec();
    let mut reports = String::new();
    let page = f.page;
    let w = f.ledger.document().conductors.keys().next().copied().unwrap();
    let cable = f.ledger.document().cables.keys().next().copied().unwrap();
    for n in [2, log.len() / 2, log.len()] {
        let seq = n as u64;
        let prior = Ledger::replay(log[..n - 1].to_vec(), SEED).unwrap();
        let doc = prior.document();
        let mut bad_ops: Vec<(&str, Vec<Op>)> = vec![
            (
                "fails at op 3 (precondition)",
                vec![
                    Op::AddPage { id: PageId(0xa1), page: Page::new("x"), index: 0 },
                    Op::AddLayer { id: LayerId(0xa2), layer: Layer { name: "x".into() } },
                    Op::AddPage { id: PageId(0xa3), page: Page::new("y"), index: 99 },
                ],
            ),
            (
                "end state breaks an invariant after three ops",
                vec![
                    Op::AddPage { id: PageId(0xa1), page: Page::new("x"), index: 0 },
                    Op::AddLayer { id: LayerId(0xa2), layer: Layer { name: "x".into() } },
                    Op::AddItem {
                        id: ItemId(0xa4),
                        item: Item {
                            page: PageId(0xfeed),
                            style: style(LayerId(0xa2)),
                            kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(0.0, 0.0) }),
                        },
                    },
                ],
            ),
            (
                "conductor end to a missing terminal",
                vec![
                    Op::CreateConductor { id: ConductorId(0xb1), conductor: Conductor::default() },
                    Op::AttachEnd {
                        conductor: ConductorId(0xb1),
                        end: End::A,
                        before: None,
                        after: Some(TerminalId(0xdead)),
                    },
                ],
            ),
            (
                "cable member that is not a conductor",
                vec![
                    Op::CreateCable { id: CableId(0xc1), cable: Cable::default() },
                    Op::AddCableMember {
                        cable: CableId(0xc1),
                        index: 0,
                        member: CableMember { conductor: ConductorId(0xdead), core: None },
                    },
                ],
            ),
            (
                "existing cable member removed at the wrong index",
                if doc.cables.contains_key(&cable) {
                    vec![Op::RemoveCableMember {
                        cable,
                        index: 5,
                        member: CableMember { conductor: w, core: Some("1".into()) },
                    }]
                } else {
                    vec![Op::RemoveLayer { id: LayerId(0xdead), layer: Layer { name: "0".into() } }]
                },
            ),
        ];
        if doc.items.values().any(|i| i.page == page) {
            bad_ops.push((
                "removes the only page under its items",
                vec![Op::RemovePage { id: page, page: doc.pages[&page].clone(), index: 0 }],
            ));
        }
        for (name, ops) in bad_ops {
            let tx = Transaction {
                id: TxId(0xdead),
                seq,
                author: Author::User,
                label: "bad".into(),
                kind: TxKind::Edit,
                time_ms: None,
                ops,
            };
            let damaged = with_tx(&bytes, n, &tx);
            let o = open_project(&damaged, SEED).unwrap();
            assert_eq!(o.report.access, Access::ReadOnly, "{name} at {n}");
            assert_eq!(o.report.verified_seq, seq - 1, "{name} at {n}");
            assert_eq!(o.ledger.log(), &log[..n - 1], "{name} at {n}");
            assert_eq!(o.ledger.document(), prior.document(), "{name} at {n}: last good state");
            assert_eq!(doc_bytes(o.ledger.document()), doc_bytes(prior.document()), "{name} at {n}");
            assert_queries_fresh(o.ledger.document(), name);
            let Outcome::Corrupt(c) = &o.report.outcome else { panic!("{name} at {n}: {:?}", o.report.outcome) };
            reports +=
                &format!("{name} at {n}: {:?} seq={:?} detail={}\n{}\n", c.check, c.seq, c.detail, o.report.describe());
            // The read-only ledger can still be edited in memory and saved as a copy.
            let mut ledger = o.ledger;
            ledger
                .commit(
                    Author::User,
                    "rescue",
                    vec![Op::AddLayer { id: LayerId(0x5a), layer: Layer { name: "r".into() } }],
                )
                .unwrap();
            assert_queries_fresh(ledger.document(), "rescued");
        }
    }
    dump("damaged-reports.txt", reports.as_bytes());
}

#[test]
fn every_recorded_op_and_its_inverse_fail_or_succeed_as_on_dev_at_op_3() {
    // For each op recorded in the fixture, retry it (and its inverse) as op 3 of a transaction
    // whose ops 1 and 2 touch pages, properties and the cable and conductor indexes.
    let f = fixture();
    let doc = f.ledger.document().clone();
    let w = doc.conductors.keys().next().copied().unwrap();
    let cable = doc.cables.keys().next().copied().unwrap();
    let t_free = f.t[5];
    let pool: Vec<Op> = f.ledger.log().iter().flat_map(|t| t.ops.clone()).collect();
    let mut out = String::new();
    for (i, op) in pool.iter().enumerate() {
        for (kind, third) in [("op", op.clone()), ("inverse", op.inverse())] {
            let w2 = ConductorId(0x7700 + i as u128);
            let ops = vec![
                Op::CreateConductor { id: w2, conductor: Conductor::default() },
                Op::AttachEnd { conductor: w2, end: End::A, before: None, after: Some(t_free) },
                third,
            ];
            let extra = vec![
                Op::AddCableMember { cable, index: 1, member: CableMember { conductor: w, core: Some("2".into()) } },
                Op::AddPage { id: PageId(0x8800 + i as u128), page: Page::new("z"), index: 0 },
            ];
            for (variant, ops) in [
                ("conductor first", ops.clone()),
                ("cable first", {
                    let mut v = extra.clone();
                    v.push(ops[2].clone());
                    v
                }),
            ] {
                let mut l = f.ledger.clone();
                let before = doc_bytes(l.document());
                let before_q = queries(l.document());
                let r = l.commit_at(Author::User, "probe", ops, Some(1));
                out += &format!(
                    "{i} {} {kind} {variant}: {:?}\n",
                    op.name(),
                    r.as_ref().map(|_| ()).map_err(|e| e.to_string())
                );
                if r.is_err() {
                    assert_eq!(doc_bytes(l.document()), before, "{i} {kind} {variant}: rolled back");
                    assert_eq!(queries(l.document()), before_q, "{i} {kind} {variant}: indexes rolled back");
                    // The next commit sees correct answers.
                    l.commit_at(
                        Author::User,
                        "next",
                        vec![
                            Op::CreateConductor { id: w2, conductor: Conductor::default() },
                            Op::AttachEnd { conductor: w2, end: End::B, before: None, after: Some(t_free) },
                            Op::AddCableMember { cable, index: 0, member: CableMember { conductor: w2, core: None } },
                        ],
                        Some(2),
                    )
                    .unwrap();
                    assert_eq!(l.document().cables_of(w2), vec![cable], "{i} {kind} {variant}");
                    assert!(l.document().ends_at(t_free).contains(&(w2, End::B)), "{i} {kind} {variant}");
                    assert!(!l.document().ends_at(t_free).contains(&(w2, End::A)), "{i} {kind} {variant}");
                }
                assert_queries_fresh(l.document(), &format!("{i} {kind} {variant}"));
            }
        }
    }
    dump("op3-outcomes.txt", out.as_bytes());
}

#[test]
fn a_transaction_failing_at_op_3_leaves_the_document_and_indexes_as_before() {
    let mut f = fixture();
    let doc = f.ledger.document().clone();
    let w = doc.conductors.keys().next().copied().unwrap();
    let cable = doc.cables.keys().next().copied().unwrap();
    let ta = doc.conductors[&w].end(End::A).unwrap();
    let before = doc_bytes(&doc);
    let before_q = queries(&doc);
    let w2: ConductorId = f.ids.fresh();
    // Ops 1 and 2 change both indexes for an existing conductor; op 3 fails its precondition.
    let cases: Vec<Vec<Op>> = vec![
        vec![
            Op::RemoveCableMember { cable, index: 0, member: CableMember { conductor: w, core: Some("1".into()) } },
            Op::AttachEnd { conductor: w, end: End::A, before: Some(ta), after: None },
            Op::RemoveItem { id: ItemId(0xbeef), item: doc.items.values().next().unwrap().clone() },
        ],
        vec![
            Op::CreateConductor { id: w2, conductor: Conductor::default() },
            Op::AddCableMember { cable, index: 1, member: CableMember { conductor: w2, core: None } },
            Op::AttachEnd { conductor: w2, end: End::A, before: Some(ta), after: None },
        ],
        // Op 3 applies but leaves the end state invalid: all three are rolled back.
        vec![
            Op::CreateConductor { id: w2, conductor: Conductor::default() },
            Op::AttachEnd { conductor: w2, end: End::A, before: None, after: Some(ta) },
            Op::AddCableMember { cable, index: 1, member: CableMember { conductor: ConductorId(0xdead), core: None } },
        ],
    ];
    for (k, ops) in cases.into_iter().enumerate() {
        let head = f.ledger.head();
        assert!(f.ledger.commit(Author::User, "bad", ops).is_err(), "case {k} fails");
        assert_eq!(f.ledger.head(), head, "case {k}");
        assert_eq!(doc_bytes(f.ledger.document()), before, "case {k}: document byte-identical");
        assert_eq!(queries(f.ledger.document()), before_q, "case {k}: cables_of and ends_at as before");
        assert_queries_fresh(f.ledger.document(), &format!("case {k}"));
    }
    // The next commit sees correct answers.
    f.commit(
        "good",
        vec![
            Op::CreateConductor { id: w2, conductor: Conductor::default() },
            Op::AttachEnd { conductor: w2, end: End::A, before: None, after: Some(ta) },
            Op::AddCableMember { cable, index: 1, member: CableMember { conductor: w2, core: None } },
            Op::RemoveCableMember { cable, index: 0, member: CableMember { conductor: w, core: Some("1".into()) } },
        ],
    );
    let d = f.ledger.document();
    assert_eq!(d.cables_of(w2), vec![cable]);
    assert_eq!(d.cables_of(w), Vec::<CableId>::new());
    assert_eq!(d.ends_at(ta), vec![(w, End::A), (w2, End::A)]);
    assert_queries_fresh(d, "after the next commit");
    f.ledger.undo(Author::User).unwrap();
    assert_eq!(doc_bytes(f.ledger.document()), before, "undo restores");
    assert_eq!(queries(f.ledger.document()), before_q);
}

#[test]
fn intermediate_states_may_break_invariants_and_later_ops_see_earlier_ones() {
    let mut f = fixture();
    let layer = f.layer;
    let page = f.page;
    // Op 1 adds an item on a page that only op 2 creates.
    let (p2, item): (PageId, ItemId) = (f.ids.fresh(), f.ids.fresh());
    let ops = vec![
        Op::AddItem {
            id: item,
            item: Item {
                page: p2,
                style: style(layer),
                kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(1.0, 2.0) }),
            },
        },
        Op::AddPage { id: p2, page: Page::new("2"), index: 1 },
    ];
    f.commit("forward reference", ops);
    assert!(f.ledger.document().items.contains_key(&item));
    // Rename a page with items on it: remove it, then add it back with a new title.
    let old = f.ledger.document().pages[&page].clone();
    f.commit(
        "rename",
        vec![
            Op::RemovePage { id: page, page: old, index: 0 },
            Op::AddPage { id: page, page: Page::new("renamed"), index: 0 },
        ],
    );
    assert_eq!(f.ledger.document().pages[&page].title, "renamed");
    // Op 2 and later use IDs that op 1 created.
    let (w, cable): (ConductorId, CableId) = (f.ids.fresh(), f.ids.fresh());
    let t = f.t[4];
    f.commit(
        "dependent",
        vec![
            Op::CreateConductor { id: w, conductor: Conductor::default() },
            Op::AttachEnd { conductor: w, end: End::A, before: None, after: Some(t) },
            Op::CreateCable { id: cable, cable: Cable::default() },
            Op::AddCableMember { cable, index: 0, member: CableMember { conductor: w, core: None } },
            Op::SetProperty {
                object: ObjectRef::Cable(cable),
                key: "type".into(),
                before: None,
                after: Some("x".into()),
            },
        ],
    );
    assert_eq!(f.ledger.document().cables_of(w), vec![cable]);
    assert!(f.ledger.document().ends_at(t).contains(&(w, End::A)));
    // A conductor removed and re-created in one transaction (delete-then-add of one ID).
    let cond = f.ledger.document().conductors[&w].clone();
    let member = CableMember { conductor: w, core: None };
    f.commit(
        "replace",
        vec![
            Op::RemoveCableMember { cable, index: 0, member: member.clone() },
            Op::AttachEnd { conductor: w, end: End::A, before: Some(t), after: None },
            Op::DeleteConductor { id: w, conductor: Conductor { ends: [None, None], ..cond } },
            Op::CreateConductor { id: w, conductor: Conductor::default() },
            Op::AddCableMember { cable, index: 0, member },
        ],
    );
    assert_eq!(f.ledger.document().cables_of(w), vec![cable]);
    assert!(f.ledger.document().ends_at(t).is_empty());
    assert_queries_fresh(f.ledger.document(), "after intra-transaction cases");
    // Undo all four, redo all four.
    let end = f.ledger.document().clone();
    for _ in 0..4 {
        f.ledger.undo(Author::User).unwrap();
        assert_queries_fresh(f.ledger.document(), "undo");
    }
    assert_eq!(f.ledger.document(), fixture().ledger.document());
    for _ in 0..4 {
        f.ledger.redo(Author::User).unwrap();
    }
    assert_eq!(f.ledger.document(), &end);
    let mut l = f.ledger;
    let bytes = written(&mut l);
    let o = open_project(&bytes, SEED).unwrap();
    assert_eq!(o.report.outcome, Outcome::Clean);
    assert_eq!(o.ledger.document(), &end);
    dump("intra.ecad", &bytes);
}
