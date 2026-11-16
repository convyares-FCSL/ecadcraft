//! `ecad` has no commands in this version (brief 069 removed `ecad upgrade`); `ecad inspect`
//! points at the developer tool `ecadinspect` (brief 029).

fn run(f: fn(&[String], &mut dyn std::io::Write) -> std::result::Result<(), String>, args: &[&str]) -> std::result::Result<String, String> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut out = Vec::new();
    f(&args, &mut out).map(|()| String::from_utf8(out).unwrap())
}

#[test]
fn ecad_inspect_points_at_the_developer_tool() {
    let e = run(ecad_cli::ecad, &["inspect", "demo.ecad", "tables"]).unwrap_err();
    assert!(e.contains("ecadinspect") && e.contains("cargo run -p ecad-inspector --bin ecadinspect"), "{e}");
}

#[test]
fn upgrade_is_gone_and_anything_else_prints_the_usage() {
    for args in [&["upgrade", "old.ecad"][..], &[], &["anything"]] {
        assert_eq!(run(ecad_cli::ecad, args).unwrap_err(), ecad_cli::USAGE);
    }
}
