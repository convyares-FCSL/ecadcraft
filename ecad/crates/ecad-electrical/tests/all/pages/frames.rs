//! How a frame fits long text into its box.

use ecad_electrical::pages::frames::fit;

#[test]
fn long_text_is_narrowed_then_cut() {
    assert_eq!(fit("abc", 2.5, Some(100.0)), ("abc".to_string(), 1.0));
    let (s, wf) = fit("abcdefghij", 2.5, Some(15.0));
    assert_eq!(s, "abcdefghij");
    assert!((wf - 0.75).abs() < 1e-9);
    let (s, wf) = fit(&"x".repeat(100), 2.5, Some(20.0));
    assert!(s.ends_with('…') && s.chars().count() < 20);
    assert!((wf - 0.6).abs() < 1e-9);
}
