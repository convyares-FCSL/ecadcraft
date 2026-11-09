//! Brief 013 acceptance (tester): `ECAD_VERIFY_INVARIANTS=1` turns on the full check in a
//! release build, and the private indexes stay consistent after rollback.
//!
//! The switch is read once per process, so the outer test re-runs this test binary with and
//! without it and inspects the child's verdict. The observable: a checkpoint document that is
//! already invalid somewhere a new transaction does not touch. The scoped check does not look
//! there; the full check does.

use std::process::Command;

use ecad_model::*;

/// A document whose only fault is an item on a page that does not exist.
fn invalid_checkpoint() -> Document {
    let mut doc = Document::default();
    let layer = LayerId(1);
    doc.layers.insert(layer, Layer { name: "0".into() });
    doc.items.insert(
        ItemId(2),
        Item {
            page: PageId(0xfeed),
            style: Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 },
            kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(0.0, 0.0) }),
        },
    );
    doc.rebuild_index();
    doc
}

/// Child: prints whether an unrelated commit on the invalid checkpoint was refused.
#[test]
#[ignore = "run by full_check_runs_with_the_switch_in_release"]
fn child_probe() {
    // Only as a child of its test, which sets `ECAD_TEST_CHILD`; run directly, it does nothing.
    if std::env::var_os("ECAD_TEST_CHILD").is_none() {
        return;
    }
    let doc = invalid_checkpoint();
    assert!(doc.check().is_err(), "the checkpoint is invalid");
    let mut ledger = Ledger::from_checkpoint(doc, 0, Vec::new(), 1).unwrap();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ledger.commit(Author::User, "unrelated", vec![Op::AddPage { id: PageId(9), page: Page::new("1"), index: 0 }])
    }));
    let verdict = match r {
        Ok(Ok(_)) => "VERDICT=accepted".to_string(),
        Ok(Err(e)) => format!("VERDICT=refused: {e}"),
        Err(_) => "VERDICT=panicked".to_string(),
    };
    println!("{verdict}");
}

fn run_child(switch: Option<&str>) -> String {
    let exe = std::env::current_exe().unwrap();
    let mut cmd = Command::new(exe);
    cmd.args(["--ignored", "--exact", "verify_mode_acceptance::child_probe", "--nocapture", "--test-threads=1"]);
    cmd.env("ECAD_TEST_CHILD", "1");
    cmd.env_remove("ECAD_VERIFY_INVARIANTS");
    if let Some(v) = switch {
        cmd.env("ECAD_VERIFY_INVARIANTS", v);
    }
    let out = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    // The harness prints "test child_probe ... " without a newline before the test's own output.
    stdout.lines().find_map(|l| l.find("VERDICT=").map(|i| l[i..].to_string())).unwrap_or_else(|| format!("VERDICT=missing: {stdout}"))
}

#[test]
fn full_check_runs_with_the_switch_in_release() {
    let on = run_child(Some("1"));
    let off = run_child(None);
    let zero = run_child(Some("0"));
    eprintln!("switch on: {on}\nswitch off: {off}\nswitch 0: {zero}");
    if cfg!(feature = "verify-invariants") {
        // Test builds always verify (the feature, through dev-dependencies): the disagreement is
        // caught, by the debug assertion or a refusal. The switch itself is unit-tested in ledger.rs.
        for v in [&on, &off, &zero] {
            assert!(v.starts_with("VERDICT=refused") || v == "VERDICT=panicked", "debug: {v}");
        }
    } else {
        assert!(on.starts_with("VERDICT=refused"), "release with ECAD_VERIFY_INVARIANTS=1 runs the full check: {on}");
        assert!(on.contains("page"), "the full check's message names the fault: {on}");
        assert_eq!(off, "VERDICT=accepted", "release without the switch runs only the scoped check");
        assert_eq!(zero, "VERDICT=accepted", "only the value 1 turns it on");
    }
}

#[test]
fn replay_verified_runs_the_full_check_in_any_build() {
    // Verify replay of a log is the debugger's switch; it must agree with plain replay on a
    // valid log and refuse the same invalid one.
    let mut l = Ledger::new(5);
    l.commit(Author::User, "p", vec![Op::AddPage { id: PageId(1), page: Page::new("1"), index: 0 }]).unwrap();
    l.commit(Author::User, "l", vec![Op::AddLayer { id: LayerId(2), layer: Layer { name: "0".into() } }]).unwrap();
    l.undo(Author::User).unwrap();
    l.redo(Author::User).unwrap();
    let a = Ledger::replay(l.log().to_vec(), 5).unwrap();
    let b = Ledger::replay_verified(l.log().to_vec(), 5).unwrap();
    assert_eq!(a.document(), b.document());
    a.document().check_index().unwrap();
    b.document().check_index().unwrap();
}

#[test]
fn indexes_match_a_rebuild_after_every_rolled_back_transaction() {
    let mut l = Ledger::new(7);
    let (page, layer, c, t1, t2, w, cable) = (PageId(1), LayerId(2), ComponentId(3), TerminalId(4), TerminalId(5), ConductorId(6), CableId(7));
    l.commit(
        Author::User,
        "setup",
        vec![
            Op::AddPage { id: page, page: Page::new("1"), index: 0 },
            Op::AddLayer { id: layer, layer: Layer { name: "0".into() } },
            Op::CreateComponent { id: c, component: Component::default() },
            Op::AddTerminal { id: t1, terminal: Terminal { component: c, label: None, key: None } },
            Op::AddTerminal { id: t2, terminal: Terminal { component: c, label: None, key: None } },
            Op::CreateConductor { id: w, conductor: Conductor::default() },
            Op::AttachEnd { conductor: w, end: End::A, before: None, after: Some(t1) },
            Op::CreateCable { id: cable, cable: Cable::default() },
            Op::AddCableMember { cable, index: 0, member: CableMember { conductor: w, core: None } },
        ],
    )
    .unwrap();
    let before = serde_json::to_string(l.document()).unwrap();
    let fails: Vec<Vec<Op>> = vec![
        vec![
            Op::AttachEnd { conductor: w, end: End::B, before: None, after: Some(t2) },
            Op::RemoveCableMember { cable, index: 0, member: CableMember { conductor: w, core: None } },
            Op::DeleteCable { id: cable, cable: Cable { properties: Default::default(), members: vec![] } },
            Op::RemoveTerminal { id: TerminalId(99), terminal: Terminal { component: c, label: None, key: None } },
        ],
        vec![
            Op::AttachEnd { conductor: w, end: End::A, before: Some(t1), after: None },
            Op::RemoveTerminal { id: t1, terminal: Terminal { component: c, label: None, key: None } },
            Op::AttachEnd { conductor: w, end: End::A, before: None, after: Some(t1) },
        ],
        // Valid ops, invalid end state: a terminal removed under a conductor end.
        vec![
            Op::RemoveCableMember { cable, index: 0, member: CableMember { conductor: w, core: None } },
            Op::RemoveTerminal { id: t1, terminal: Terminal { component: c, label: None, key: None } },
        ],
    ];
    for (k, ops) in fails.into_iter().enumerate() {
        assert!(l.commit(Author::User, "bad", ops).is_err(), "case {k}");
        assert_eq!(serde_json::to_string(l.document()).unwrap(), before, "case {k}");
        l.document().check_index().unwrap_or_else(|e| panic!("case {k}: {e}"));
        assert_eq!(l.document().ends_at(t1), vec![(w, End::A)], "case {k}");
        assert!(l.document().ends_at(t2).is_empty(), "case {k}");
        assert_eq!(l.document().cables_of(w), vec![cable], "case {k}");
    }
}
