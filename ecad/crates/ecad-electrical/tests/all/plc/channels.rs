//! Rules of `plc::channels` (moved from its unit tests): a document or text in, a result out.

use ecad_electrical::plc::channels::*;
use ecad_electrical::plc::rack::CardType;
use ecad_model::*;
use std::collections::BTreeSet;

#[test]
fn keys_parse_only_their_own_form() {
    let parse = |k: &str| k.parse::<PointKey>().ok().and_then(|k| k.channel());
    assert_eq!(parse(".3.in"), Some((3, ChannelRole::In)));
    assert_eq!(parse(".0.+"), Some((0, ChannelRole::Plus)));
    assert_eq!(parse(".2.C/Q"), Some((2, ChannelRole::Cq)));
    for bad in [".3", ".-1.in", ".3.nope", ".99999.in", "3.in", "..in", ".3.in.x", ""] {
        assert_eq!(parse(bad), None, "{bad}");
    }
    assert_eq!(key(3, ChannelRole::In).map(|k| k.to_string()).as_deref(), Some(".3.in"));
    assert_eq!(key(MAX_INDEX + 1, ChannelRole::In), None);
    assert_eq!(parse_ch_key("plc.ch.12.text"), Some((12, "text")));
    assert_eq!(parse_ch_key("plc.ch.x.text"), None);
    assert_eq!(parse_ch_key("plc.ch.1.other"), None);
}

#[test]
fn pin_layouts() {
    let doc = Document::default();
    let mut used = BTreeSet::new();
    assert_eq!(pins_for(&doc, None, CardType::Ai, 4, 0, &[ChannelRole::Plus, ChannelRole::Minus], &mut used), ["1", "5"]);
    assert_eq!(pins_for(&doc, None, CardType::Ai, 4, 1, &[ChannelRole::Plus, ChannelRole::Minus], &mut used), ["2", "6"]);
    let mut used = BTreeSet::new();
    assert_eq!(pins_for(&doc, None, CardType::Di, 16, 3, &[ChannelRole::In], &mut used), ["4"]);
    // A taken pin moves on to the next free whole number.
    let mut used: BTreeSet<String> = ["4".to_string()].into();
    assert_eq!(pins_for(&doc, None, CardType::Di, 16, 3, &[ChannelRole::In], &mut used), ["5"]);
}
