//! Rules of `plc::address` (moved from its unit tests): a document or text in, a result out.

use ecad_electrical::plc::address::*;
use ecad_electrical::plc::rack::CardType;

#[test]
fn formats_follow_design_045_table_5_2() {
    let b = Block { input: Some((2, 2)), output: None, pinned: false, overflow: false };
    assert_eq!(automatic(Style::ByteBit, CardType::Di, &b, 3, 11, 11, 2, false).as_deref(), Some("I3.3"));
    assert_eq!(automatic(Style::Iec, CardType::Di, &b, 3, 11, 11, 2, false).as_deref(), Some("%IX3.3"));
    assert_eq!(automatic(Style::Slot, CardType::Di, &b, 3, 4, 4, 2, false).as_deref(), Some("I:3/04"));
    assert_eq!(automatic(Style::None, CardType::Di, &b, 3, 4, 4, 2, false), None);
    assert_eq!(automatic(Style::ByteBit, CardType::Di, &b, 3, 16, 16, 2, false), None, "past the block");
    let a = Block { input: Some((4, 8)), output: None, pinned: false, overflow: false };
    assert_eq!(automatic(Style::ByteBit, CardType::Ai, &a, 4, 2, 2, 2, false).as_deref(), Some("IW8"));
    assert_eq!(automatic(Style::Iec, CardType::Ai, &a, 4, 3, 3, 2, false).as_deref(), Some("%IW10"));
    let q = Block { input: None, output: Some((8, 2)), pinned: false, overflow: false };
    assert_eq!(automatic(Style::Slot, CardType::Dq, &q, 6, 1, 1, 2, true).as_deref(), Some("O:6/01"));
    assert_eq!(block_words(Style::ByteBit, false, (2, 2)), "I2–I3");
    // Hostile arithmetic: no panic, no address.
    let edge = Block { input: Some((MAX_BYTE, 1)), output: None, pinned: true, overflow: false };
    assert_eq!(automatic(Style::ByteBit, CardType::Ai, &edge, 1, 9, 9, 2, false), None);
}
