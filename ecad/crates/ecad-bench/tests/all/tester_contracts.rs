//! Tester's acceptance checks for brief 012, from the outside: the small tier itself (not only a
//! cut-down tier) is deterministic and its two histories replay to the same document; the options
//! and `--compare` behave as `cargo xtask bench` documents them.

use ecad_bench::generate::{Variant, generate, tier};
use ecad_bench::measure::Measured;
use ecad_bench::report::{Row, compare, json};
use ecad_bench::{TIERS, parse};
use ecad_file::{Outcome, open_project};

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
#[ignore = "slow: generates the small tier twice, about 2 s"]
fn the_small_tier_is_byte_identical_in_both_histories() {
    let t = tier("small").unwrap();
    for variant in Variant::BOTH {
        let a = generate(t, variant).unwrap().file_bytes().unwrap();
        let b = generate(t, variant).unwrap().file_bytes().unwrap();
        assert_eq!(a, b, "{variant:?}");
    }
}

#[test]
#[ignore = "slow: generates and replays the small tier, about 11 s"]
fn the_small_tier_lean_and_churned_files_replay_to_the_same_document() {
    let t = tier("small").unwrap();
    let mut docs = Vec::new();
    let mut heads = Vec::new();
    for variant in Variant::BOTH {
        let bytes = generate(t, variant).unwrap().file_bytes().unwrap();
        let o = open_project(&bytes, 7).unwrap();
        assert_eq!(o.report.outcome, Outcome::Clean, "{variant:?}: {}", o.report.describe());
        heads.push(o.ledger.head());
        docs.push(o.ledger.document().clone());
    }
    assert_eq!(docs[0], docs[1], "history alone differs");
    let (lean, churned) = (heads[0], heads[1]);
    assert!(churned >= 4 * lean && churned <= 6 * lean, "roughly 5×: {lean} lean, {churned} churned");
}

#[test]
fn options_are_parsed_as_documented() {
    let names = |o: ecad_bench::Options| o.tiers.iter().map(|t| t.name).collect::<Vec<_>>();
    assert_eq!(names(parse(&[]).unwrap()), ["small", "medium"], "default small,medium");
    assert_eq!(names(parse(&args(&["--tier", "small"])).unwrap()), ["small"]);
    assert_eq!(names(parse(&args(&["--tier", "all"])).unwrap()).len(), TIERS.len());
    assert_eq!(names(parse(&args(&["--tier", "large,small,small"])).unwrap()), ["large", "small"]);
    let o = parse(&args(&["--keep", "C:\\some dir\\kept", "--compare", "x.json"])).unwrap();
    assert_eq!(o.keep.unwrap().to_str(), Some("C:\\some dir\\kept"));
    for bad in [&["--tier", "huge"][..], &["--tier"], &["--keep"], &["--frobnicate"], &["--tier", ","]] {
        assert!(parse(&args(bad)).is_err(), "{bad:?} refused");
    }
}

fn row(variant: &str, open_ms: f64) -> Row {
    Row {
        tier: "small".into(),
        variant: variant.into(),
        measured: Measured {
            bytes: 1000,
            transactions: 10,
            open_ms,
            derive_ms: 1.0,
            xref_ms: None,
            first_page_ms: 2.0,
            edit_ms: 0.5,
            save_ms: 0.0,
            draw_move_ms: 3.0,
            draw_window_ms: 4.0,
            draw_wire_ms: 1.5,
            switch_ms: 2.5,
            preview_ms: 6.0,
            panel_ms: 0.05,
            devices_ms: 3.0,
            goto_ms: 2.0,
            devupd_ms: 0.1,
            place_ms: 1.0,
            join_ms: 1.0,
            tag_ms: 0.5,
            pnum_ms: 0.5,
            frame_ms: 0.1,
            tbupd_ms: 0.5,
            gridchange_ms: 1.0,
            revopen_ms: 10.0,
        },
    }
}

#[test]
fn compare_prints_the_change_per_row_and_refuses_a_foreign_file() {
    let earlier = json(&[row("lean", 10.0)], 5, "abc1234", "2026-11-07T00:00:00Z");
    let text = compare(&[row("lean", 15.0), row("churned", 1.0)], &earlier).unwrap();
    assert!(text.contains("abc1234"), "{text}");
    assert!(text.contains("10.0 → 15.0 (+50%)"), "{text}");
    assert!(text.contains("(not in the earlier run)"), "{text}");
    assert!(compare(&[row("lean", 1.0)], &serde_json::json!({"hello": 1})).is_err());
}
