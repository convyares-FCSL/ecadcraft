//! Brief 044 (overseer condition on FP2): invariant I7 refuses a definition whose bridge names a
//! key it has no terminal for and no placement writes; side keys and level keys are allowed.

use ecad_model::{Author, Definition, DefinitionId, Ledger, Op, Pt, TerminalDef};

fn def(bridges: &[&[&str]]) -> Definition {
    Definition {
        name: "test".into(),
        origin: None,
        graphics: Vec::new(),
        terminals: vec![TerminalDef { key: "1".into(), label: Some("1".into()), at: Pt::new(0.0, 0.0) }],
        bridges: bridges.iter().map(|g| g.iter().map(|k| k.to_string()).collect()).collect(),
        ..Default::default()
    }
}

fn embed(bridges: &[&[&str]]) -> Result<(), String> {
    let mut l = Ledger::new(44);
    l.commit(Author::User, "embed", vec![Op::EmbedDefinition { id: DefinitionId(7), definition: def(bridges) }])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[test]
fn a_bridge_names_only_keys_a_placement_can_write() {
    assert!(embed(&[]).is_ok());
    assert!(embed(&[&["1", ".s"]]).is_ok(), "own key and a side key");
    assert!(embed(&[&[".s", ".p"]]).is_ok(), "the connector halves' bridge (FP2)");
    assert!(embed(&[&[".1", ".2"]]).is_ok(), "036's level keys");
    assert!(embed(&[&[".1i", ".1o"]]).is_ok(), "036's in and out points");
    let e = embed(&[&[".s", ".x"]]).unwrap_err();
    assert!(e.contains(".x"), "{e}");
    assert!(embed(&[&["1", "2"]]).is_err(), "a plain key the definition lacks");
    assert!(embed(&[&[".100"]]).is_err(), "a level beyond the range");
    assert!(ecad_model::invariant::is_placement_key(".p") && !ecad_model::invariant::is_placement_key(".q"));
}
