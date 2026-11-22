//! The signal list's channel data (brief 056, design 056 §4): a channel's PLC tag and safe state,
//! what it drives, the Test column's states, and **Make tags**.
//!
//! # Semantics
//! - A channel's **tag** is the card property `plc.ch.<i>.tag` (FP1), at most [`MAX_TAG`]
//!   characters with no control character, unique per PLC compared without case
//!   ([`tag_holder`]). A PLC is a rack with its cards; the cards in no rack are one PLC together.
//! - A channel's **safe state** is `plc.ch.<i>.safe` (FP1), for outputs only: a digital output
//!   `off`, `on` or `hold`; an analogue (or IO-Link) output `hold`, `min`, `max` or a value of at
//!   most [`MAX_SAFE`] characters. Unset, the registry's `plc.safe.dq` / `plc.safe.ao` at the card's
//!   level applies ([`safe_default`]).
//! - **Drives** ([`drives`]) are the channel's targets (045: the tagged devices on its nets) less
//!   the pass-through devices: terminals and connector halves. Checked against the code (house rule
//!   3): a terminal's two sides and a plug's mating halves are one net already, joined by their
//!   definition's bridges (`ecad_model::connectivity`, join 3), so the end device is on the
//!   channel's own net and no hop across a pass-through is needed. With no other target the last
//!   pass-through is shown.
//! - [`TestState`] is defined here for the Tests brief (0.2.3); 056 produces only `None`.

use crate::CmdError;
use std::collections::BTreeSet;

use ecad_model::{DeviceId, Document, Op};
use serde_json::{Value, json};

use crate::defaults::plc::SafeState;
use crate::defaults::{self, Scope};
use crate::plc::iolist::Target;
use crate::plc::rack::{self, CardType, RackRef};

/// The field names of a channel's tag and safe state (`plc.ch.<i>.tag`, `plc.ch.<i>.safe`).
pub const TAG: &str = "tag";
pub const SAFE: &str = "safe";
/// Longest tag (design 056 §4.3).
pub(crate) const MAX_TAG: usize = 128;
/// Longest safe value (design 056 §4.4).
pub(crate) const MAX_SAFE: usize = 16;
/// The safe words of a digital output.
pub(crate) const DQ_SAFE: &[&str] = <SafeState as ecad_model::Word>::WORDS;
/// The safe words of an analogue output (besides a value).
pub(crate) const AO_SAFE: [&str; 3] = ["hold", "min", "max"];
/// Most channels **Make tags** fills in one call (bounded by the cards' channel counts anyway).
pub(crate) const MAX_MADE: usize = 20_000;

/// The value check of a tag (design 056 §4.3): trimmed, 1 to [`MAX_TAG`] characters, no control
/// character. Empty clears.
///
/// # Errors
/// Too long, or a control character.
pub(crate) fn tag_value(value: Option<&str>) -> Result<Option<String>, CmdError> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    if v.chars().count() > MAX_TAG || v.chars().any(char::is_control) {
        return Err(CmdError::refused(format!("A PLC tag is 1 to {MAX_TAG} characters, with no control characters.")));
    }
    Ok(Some(v.to_string()))
}

/// Whether a card of type `t` has a safe state (outputs: DQ, AO, IO-Link).
pub(crate) fn has_safe(t: CardType) -> bool {
    matches!(t, CardType::Dq | CardType::Ao | CardType::Iol)
}

/// The registry entry of a card type's safe state.
pub(crate) fn safe_entry(t: CardType) -> Option<&'static defaults::Entry> {
    match t {
        CardType::Dq => Some(&defaults::plc::SAFE_DQ),
        CardType::Ao | CardType::Iol => Some(&defaults::plc::SAFE_AO),
        _ => None,
    }
}

/// The value check of a safe state for a card of type `t` (`None`: any output, as `setprop` sees
/// a key without its card's type). Words are stored in lower case; a value is kept as typed,
/// trimmed. Empty clears.
///
/// # Errors
/// An input card, a word the type does not take, too long, or a control character.
pub fn safe_value(t: Option<CardType>, value: Option<&str>) -> Result<Option<String>, CmdError> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    if let Some(t) = t
        && !has_safe(t)
    {
        return Err(CmdError::refused(format!("A {} has no safe state; only outputs have one.", t.noun())));
    }
    let lower = v.to_lowercase();
    if t == Some(CardType::Dq) {
        return DQ_SAFE
            .iter()
            .find(|w| **w == lower)
            .map(|w| Some((*w).to_string()))
            .ok_or_else(|| CmdError::refused("A digital output's safe state is Off, On or Hold."));
    }
    if let Some(w) = DQ_SAFE.iter().chain(AO_SAFE.iter()).find(|w| **w == lower) {
        // `on` and `off` are a digital output's words; an analogue output takes a value instead.
        let digital_only = <SafeState as ecad_model::Word>::parse(w).is_some_and(|s| s != SafeState::Hold);
        if matches!(t, Some(CardType::Ao)) && digital_only {
            return Err(CmdError::refused("An analogue output's safe state is Hold, Min, Max or a value (`4 mA`)."));
        }
        return Ok(Some((*w).to_string()));
    }
    if v.chars().count() > MAX_SAFE || v.chars().any(char::is_control) {
        return Err(CmdError::refused(format!("A safe value is 1 to {MAX_SAFE} characters, with no control characters.")));
    }
    if t.is_none() || matches!(t, Some(CardType::Ao | CardType::Iol)) {
        return Ok(Some(v.to_string()));
    }
    Err(CmdError::refused("A digital output's safe state is Off, On or Hold."))
}

/// The safe state channels of card `c` (type `t`) take when they have none: the registry's at the
/// card's level. `None` for inputs.
pub fn safe_default(doc: &Document, c: DeviceId, t: CardType) -> Option<String> {
    let e = safe_entry(t)?;
    let at = Scope { component: Some(c), ..Scope::project() };
    Some(defaults::resolve(doc, e, at).text()).filter(|w| !w.is_empty())
}

/// A safe value in words: `Off`, `On`, `Hold`, `Min`, `Max`, or the value as stored.
pub fn safe_words(v: &str) -> String {
    match <SafeState as ecad_model::Word>::parse(v) {
        Some(SafeState::Off) => return "Off".into(),
        Some(SafeState::On) => return "On".into(),
        Some(SafeState::Hold) => return "Hold".into(),
        None => {}
    }
    match v {
        "min" => "Min".into(),
        "max" => "Max".into(),
        other => other.to_string(),
    }
}

/// The PLC a card belongs to: its rack, or `None` for the cards in no rack (one PLC together).
pub(crate) fn plc_of(doc: &Document, c: DeviceId) -> Option<DeviceId> {
    match rack::rack_ref(doc, c) {
        RackRef::Rack(r) => Some(r),
        _ => None,
    }
}

/// The tag key compared for uniqueness (PLC tools compare names without case).
pub fn name_key(tag: &str) -> String {
    tag.trim().to_lowercase()
}

/// The tags of every channel in the PLC `plc` (a rack, or `None`: the cards in no rack), as (card,
/// index, tag). One walk over the project's devices.
pub(crate) fn tags_of_plc(doc: &Document, plc: Option<DeviceId>) -> Vec<(DeviceId, u32, String)> {
    let mut out = Vec::new();
    for (id, x) in &doc.devices {
        let ecad_model::DeviceType::PlcModule(m) = &x.device_type else { continue };
        let tags: Vec<(u32, String)> = m.channels().into_iter().filter_map(|(i, ch)| ch.tag.as_ref().map(|t| (i, t.to_string()))).collect();
        if tags.is_empty() || plc_of(doc, *id) != plc {
            continue;
        }
        out.extend(tags.into_iter().map(|(i, t)| (*id, i, t)));
    }
    out
}

/// Another channel of the same PLC as card `c` that holds `tag` (compared without case), other than
/// channel `i` of `c`.
pub(crate) fn tag_holder(doc: &Document, c: DeviceId, i: u32, tag: &str) -> Option<(DeviceId, u32)> {
    let key = name_key(tag);
    tags_of_plc(doc, plc_of(doc, c)).into_iter().find(|(x, j, t)| !(*x == c && *j == i) && name_key(t) == key).map(|(x, j, _)| (x, j))
}

/// `Tag "Valve_Inlet" is already -A03 channel 2 (%Q2.2).`
pub(crate) fn duplicate_words(doc: &Document, tag: &str, holder: (DeviceId, u32)) -> String {
    let (c, j) = holder;
    let (auto, _) = crate::plc::address::of_channel(doc, c, j, None, &mut std::collections::BTreeMap::new());
    let (addr, _) = crate::plc::address::shown(doc, c, j, auto);
    let at = addr.map(|a| format!(" ({a})")).unwrap_or_default();
    format!("Tag \"{tag}\" is already {}{at}.", crate::plc::channels::channel_words(doc, c, j))
}

/// A tag made from a description (design 056 §4.3): spaces and dashes become `_`, other characters
/// are dropped, a digit first gets `_` before it. `None` when nothing is left.
pub(crate) fn make_tag(description: &str) -> Option<String> {
    let mut s = String::new();
    for ch in description.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            s.push(ch);
        } else if (ch == ' ' || ch == '-' || ch == '_') && !s.ends_with('_') && !s.is_empty() {
            s.push('_');
        }
        if s.len() >= MAX_TAG {
            break;
        }
    }
    let s = s.trim_end_matches('_').to_string();
    if s.is_empty() {
        return None;
    }
    let s = if s.starts_with(|c: char| c.is_ascii_digit()) { format!("_{s}") } else { s };
    Some(s.chars().take(MAX_TAG).collect())
}

/// `base`, else `base_2`, `base_3`… : the first not in `taken` (keys without case).
pub(crate) fn unique_tag(base: &str, taken: &BTreeSet<String>) -> String {
    if !taken.contains(&name_key(base)) {
        return base.to_string();
    }
    for n in 2..100_000u32 {
        let suffix = format!("_{n}");
        let keep = MAX_TAG.saturating_sub(suffix.len());
        let t = format!("{}{suffix}", base.chars().take(keep).collect::<String>());
        if !taken.contains(&name_key(&t)) {
            return t;
        }
    }
    base.to_string()
}

/// Whether device `c` passes a conductor through (a terminal or a connector half): it is not what a
/// channel drives.
pub fn is_pass_through(doc: &Document, c: DeviceId) -> bool {
    use ecad_model::DeviceType;
    doc.devices.get(&c).is_some_and(|d| matches!(d.device_type, DeviceType::TerminalStrip(_) | DeviceType::Connector(_)))
}

/// What a channel drives (design 056 §4.5), from its row's targets: the targets that are not
/// pass-throughs; with none of those, the last pass-through (shown faint).
pub fn drives(targets: &[Target]) -> (Vec<&Target>, bool) {
    let kept: Vec<&Target> = targets.iter().filter(|t| !t.through).collect();
    if !kept.is_empty() {
        return (kept, false);
    }
    (targets.iter().rev().take(1).collect(), true)
}

/// `-K03:A1`, `-K03:A1 +1`; empty when nothing.
pub fn drives_words(targets: &[Target]) -> String {
    let (d, _) = drives(targets);
    match d.as_slice() {
        [] => String::new(),
        [one] => format!("{}:{}", one.device, one.point),
        [first, rest @ ..] => format!("{}:{} +{}", first.device, first.point, rest.len()),
    }
}

/// The Test column's state of a channel (design 056 §4.6), filled by the Tests brief (0.2.3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TestState {
    /// No test yet: `—`.
    #[default]
    None,
    /// Passed at a revision: `✓ C`.
    Passed(String),
    /// Passed, then something it depends on changed: `↻ retest`.
    Retest,
    /// The last result failed: `⊘ failed`.
    Failed,
    /// Cannot run yet: `⊘ blocked`.
    Blocked,
}

impl TestState {
    /// The cell's words.
    pub fn words(&self) -> String {
        match self {
            TestState::None => "\u{2014}".into(),
            TestState::Passed(rev) => format!("\u{2713} {rev}"),
            TestState::Retest => "\u{21bb} retest".into(),
            TestState::Failed => "\u{2298} failed".into(),
            TestState::Blocked => "\u{2298} blocked".into(),
        }
    }

    /// As `iolist` returns it (`null` for none).
    pub fn json(&self) -> Value {
        match self {
            TestState::None => Value::Null,
            TestState::Passed(r) => json!({"state": "passed", "revision": r}),
            TestState::Retest => json!({"state": "retest"}),
            TestState::Failed => json!({"state": "failed"}),
            TestState::Blocked => json!({"state": "blocked"}),
        }
    }
}

/// The ops of **Make tags** on `cards`: every used or reserved channel with no tag gets one from its
/// description, unique in its PLC. Returns the ops and how many tags per card.
pub(crate) fn make_tags_ops(doc: &Document, cards: &[DeviceId]) -> (Vec<Op>, Vec<(DeviceId, usize)>) {
    let mut ops = Vec::new();
    let mut made = 0usize;
    let mut counts = Vec::new();
    let mut taken_by_plc: std::collections::BTreeMap<Option<DeviceId>, BTreeSet<String>> = std::collections::BTreeMap::new();
    for c in cards {
        let plc = plc_of(doc, *c);
        let taken = taken_by_plc.entry(plc).or_insert_with(|| tags_of_plc(doc, plc).into_iter().map(|(_, _, t)| name_key(&t)).collect());
        let chans = crate::plc::channel_data(doc, *c);
        let mut n = 0;
        for (i, ch) in &chans {
            if made >= MAX_MADE {
                break;
            }
            if ch.tag.is_some() || !(ch.used || ch.text.is_some()) {
                continue;
            }
            let Some(base) = ch.text.as_deref().and_then(make_tag) else { continue };
            let t = unique_tag(&base, taken);
            taken.insert(name_key(&t));
            if crate::plc::channels::set_field(doc, &mut ops, *c, *i, TAG, Some(&t)).is_err() {
                continue;
            }
            n += 1;
            made += 1;
        }
        counts.push((*c, n));
    }
    (ops, counts)
}
