//! Rules of `terminal_strips::terminal_blocks` (moved from its unit tests): a document or text in, a result out.

use ecad_electrical::symbols::StarterKey;
use ecad_electrical::terminal_strips::terminal_blocks::*;

/// Every starter key of the current terminal types, level symbols first.
fn starter_keys() -> Vec<StarterKey> {
    let mut out = Vec::new();
    for t in TYPES.iter().filter(|t| !t.legacy()) {
        if t.multi() {
            out.extend((1..=t.count()).map(|n| t.starter_key(Some(n))));
        }
        out.push(t.key);
    }
    out
}

#[test]
fn starters_and_codes() {
    assert_eq!(of_starter(StarterKey::TermlnpeL2).map(|(t, n)| (t.code, n)), Some(("lnpe", Some(2))));
    assert_eq!(of_starter(StarterKey::Termlnpe).map(|(t, n)| (t.code, n)), Some(("lnpe", None)));
    assert_eq!(of_starter(StarterKey::Coil), None);
    assert_eq!(of_starter(StarterKey::Terminalblockfuse).map(|(t, n)| (t.code, n)), Some(("fuse", Some(1))));
    for t in TYPES {
        assert_eq!(t.level_starters.len(), if t.multi() { t.levels.len() } else { 0 }, "{}", t.code);
        for (i, k) in t.level_starters.iter().enumerate() {
            assert_eq!(t.starter_key(Some(u32::try_from(i).unwrap() + 1)), *k);
        }
    }
    assert_eq!(starter_keys().len(), 20);
    assert!(
        is_level_starter(ecad_electrical::symbols::StarterKey::Term2bL1) && !is_level_starter(ecad_electrical::symbols::StarterKey::TerminalBlock)
    );
    assert_eq!(by_word("x2b").map(|t| t.key), Some(StarterKey::Term2b));
    assert_eq!(by_word("Feedthrough").map(|t| t.key), Some(StarterKey::TerminalBlock));
}

#[test]
fn offers_by_type() {
    let c = parse_counts("12+2PE+4x3").unwrap_or_default();
    assert_eq!(counts_text(&c), "12+2PE+4x3");
    assert_eq!(c.iter().sum::<u32>(), 18);
    assert_eq!(parse_counts("12 + 2pe").map(|c| counts_text(&c)), Ok("12+2PE".into()));
    assert_eq!(parse_counts("0+2PE").map(|c| counts_text(&c)), Ok("2PE".into()));
    assert!(parse_counts("6Q3").unwrap_err().to_string().contains("types: PE, x2"));
    assert!(parse_counts("999x3").is_ok());
    assert!(parse_counts("1000x2").is_err());
    assert!(parse_counts("990+10PE").is_err());
    assert!(parse_counts(&"9".repeat(10_000)).is_err());
    assert!(parse_counts("").is_err());
}

#[test]
fn jumper_bar_ends() {
    assert_eq!(jumper_bar_end("4", 1, None), "4");
    assert_eq!(jumper_bar_end("4", 2, None), "4.2");
    assert_eq!(jumper_bar_end("2.1", 1, None), "2.1.1");
    let order: Vec<String> = ["1", "2", "3", "4"].iter().map(|s| s.to_string()).collect();
    let sel: Vec<&String> = order.iter().collect();
    assert_eq!(jumper_bar_text(&sel, &order, 2, None), "1.2-4.2");
    assert_eq!(jumper_bar_text(&sel, &order, 1, None), "1-4");
    assert_eq!(jumper_bar_text(&sel, &order, 1, Some('o')), "1.1o-4.1o", "review 036, 2: the point is kept");
}
