//! Device tags (brief 018): what makes two tags the same, and how a typed tag is written.
//!
//! A component's tag is its stored `tag` property. By the concept spec a tag **is** an identity:
//! the same tag anywhere means the same device. Two tags are the same when their [`key`]s are
//! equal: case is ignored, a leading `-` is optional, and in the usual form (letters, then a
//! number) leading zeros of the number are ignored, so `-K1`, `k01` and `-K001` are one tag.
//!
//! IEC 81346 is the default, never the law: any letters are accepted (`-SR01`, `-TB01`, `N01`),
//! and a tag that does not have the usual form is kept as typed (in upper case) and compared as a
//! whole.
//!
//! This module holds no index and reads no document; [`crate::Document::components_tagged`] finds
//! the components holding a tag through the document's index.

/// The property key a component's tag is stored under.
pub const TAG_KEY: &str = "tag";

/// Most characters in a tag.
pub const MAX_TAG_CHARS: usize = 40;

/// A device's tag as stored (design 072 §2.1): validated text of 1 to [`MAX_TAG_CHARS`]
/// characters.
pub type Tag = crate::naming::text::Text<MAX_TAG_CHARS>;

/// Most digits in a tag's number (larger numbers are compared as text).
pub const MAX_TAG_DIGITS: usize = 9;

/// The highest number a tag counts with ([`MAX_TAG_DIGITS`] nines).
pub const MAX_TAG_NUMBER: u64 = 999_999_999;

/// Most digits a project may pad tag numbers to.
pub const MAX_PADDING: u8 = 6;

/// The `setprop` key of a device's or cable's tag state (S6, #3): `automatic`, `fixed` or
/// `provisional` ([`TagState`]). Always stored; it replaces the fixed flag (brief 026, FP1).
pub const STATE_KEY: &str = "tag_state";

crate::word_enum! {
    /// What the tag of a device or cable is (S6, #3). It is stored beside the tag text, never worked
    /// out from how the text is spelt: a provisional tag is stored bare (`-K3`) and only drawn with
    /// its `?` (`-K?3`, [`shown`]); a provisional `-K3` and a real `-K3` are different tags
    /// ([`Key`]).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub enum TagState {
        /// Given by numbering (or a tag no person typed); Renumber may change it.
        #[default]
        Automatic,
        /// Typed by a person; Renumber and Finalise never change it (brief 026, FP1).
        Fixed,
        /// A placeholder number (`-K?3`, brief 026, FP3) that Finalise turns into an automatic tag.
        Provisional,
    }
}

impl TagState {
    /// Whether this is [`TagState::Provisional`].
    pub fn is_provisional(self) -> bool {
        self == TagState::Provisional
    }
}

/// The identity a tag has in the tag indexes: its [`key`] and whether it is provisional. A
/// provisional `-K3` and a real `-K3` are different tags (S6, #3).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    /// Whether the tag is provisional (so it sorts after every real key).
    pub provisional: bool,
    /// The tag's [`key`].
    pub text: String,
}

impl Key {
    /// The key of stored tag text `tag` in `state`.
    pub fn of(tag: &str, state: TagState) -> Key {
        Key { provisional: state.is_provisional(), text: key(tag) }
    }

    /// The key of a real (not provisional) tag.
    pub fn real(tag: &str) -> Key {
        Key::of(tag, TagState::Fixed)
    }
}

/// A tag as drawn and spoken: the stored text, with the `?` of a provisional tag put before its
/// number (`-K3` provisional is `-K?3`). The one place the `?` is made.
pub fn shown(tag: &str, state: TagState) -> String {
    if !state.is_provisional() {
        return tag.to_string();
    }
    let dash = usize::from(tag.starts_with('-'));
    let split = tag.get(dash..).and_then(|t| t.find(|c: char| !c.is_ascii_alphabetic())).map_or(tag.len(), |i| i + dash);
    format!("{}?{}", &tag[..split], &tag[split..])
}

/// A tag to store: its bare text and its state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Given {
    /// The bare text (`-K3` also for the provisional `-K?3`).
    pub text: String,
    pub state: TagState,
}

impl Given {
    /// A tag in `state`.
    pub fn new(text: impl Into<String>, state: TagState) -> Given {
        Given { text: text.into(), state }
    }

    /// As a new object takes it (Place, a new cable): a typed real tag is automatic, not fixed
    /// (brief 026: only `setprop tag` fixes), a provisional one stays provisional.
    pub fn placed(self) -> Given {
        let state = if self.state.is_provisional() { TagState::Provisional } else { TagState::Automatic };
        Given { state, ..self }
    }

    /// As drawn ([`shown`]).
    pub fn shown(&self) -> String {
        shown(&self.text, self.state)
    }

    /// Its identity in the tag indexes.
    pub fn key(&self) -> Key {
        Key::of(&self.text, self.state)
    }
}

/// A tag as typed, read once where the text comes in (S6, #3): the bare text and its state, or
/// the request for the next provisional number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A tag to store: typed real tags are [`TagState::Fixed`], `-K?3` is [`TagState::Provisional`]
    /// with the bare text `-K3`.
    Tag(Given),
    /// `-K?`: the next provisional number of this prefix (upper case, without `-`); never stored.
    Next(String),
}

/// Read typed `text` ([`normalise`] and the `?`).
///
/// # Errors
/// As [`normalise`].
pub fn enter(text: &str, style: impl Into<Style>) -> Result<Entry, crate::ParseError> {
    let style = style.into();
    let t = normalise_any(text, style)?;
    Ok(match provisional(&t) {
        Some((prefix, 0)) => Entry::Next(prefix),
        Some((prefix, n)) => Entry::Tag(Given::new(style.provisional(&prefix, n), TagState::Provisional)),
        None => Entry::Tag(Given::new(t, TagState::Fixed)),
    })
}

/// The project property turning provisional tags on (brief 026, FP2): `"yes"`, absent = off.
pub const PROVISIONAL_SETTING: &str = "numbering.provisional";

/// The stored value of a high-water reset: `"<n>;<was>;<generation>"`, where `n` is the new mark,
/// `was` the mark it replaced and `generation` one more than the reset it overwrites (tester 026:
/// the generation tells a reset, which raises it, from its undo, which lowers it, so an undone
/// reset never counts as a new one; it also makes every reset a change, even to the same `n`).
pub fn reset_value(n: u64, was: u64, generation: u64) -> String {
    format!("{n};{was};{generation}")
}

/// A stored reset value read back: `(n, was, generation)`. A plain number (written before the
/// generation existed) reads as `(n, 0, 0)`.
pub fn parse_reset(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.trim().split(';');
    let n = parts.next()?.trim().parse::<u64>().ok()?;
    let (was, generation) = match (parts.next(), parts.next(), parts.next()) {
        (None, ..) => (0, 0),
        (Some(w), Some(g), None) => (w.trim().parse::<u64>().ok()?, g.trim().parse::<u64>().ok()?),
        _ => return None,
    };
    Some((n.min(MAX_TAG_NUMBER), was.min(MAX_TAG_NUMBER), generation))
}

/// Refusal of a tag that cannot be stored.
pub const BAD_TAG: &str = "A tag is 1 to 40 printable characters, for example -K01, -SR01 or -TB01.";

/// The usual form of a tag: its letter prefix (upper case, without `-`) and its number.
///
/// # Semantics
/// After trimming and dropping one leading `-`: one or more ASCII letters, then 1 to
/// [`MAX_TAG_DIGITS`] ASCII digits, nothing else. `None` for anything else.
///
/// # Example
/// ```
/// assert_eq!(ecad_model::naming::tag::parse("-k01"), Some(("K".to_string(), 1)));
/// assert_eq!(ecad_model::naming::tag::parse("TB12"), Some(("TB".to_string(), 12)));
/// assert_eq!(ecad_model::naming::tag::parse("-K1A"), None);
/// ```
pub fn parse(tag: &str) -> Option<(String, u64)> {
    let t = tag.trim();
    let t = t.strip_prefix('-').unwrap_or(t);
    let split = t.find(|c: char| !c.is_ascii_alphabetic())?;
    let (letters, digits) = t.split_at(split);
    if letters.is_empty() || digits.is_empty() || digits.len() > MAX_TAG_DIGITS {
        return None;
    }
    if !crate::naming::words::is_digits(digits) {
        return None;
    }
    let n = digits.parse::<u64>().ok()?;
    Some((letters.to_ascii_uppercase(), n))
}

/// The provisional form of a tag (brief 026, FP3): its prefix (upper case, without `-`) and its
/// provisional number, for `-K?3`; `None` for anything else. `-K?` (no number) is `Some(("K", 0))`:
/// the request for the next provisional number, never a stored tag.
///
/// # Example
/// ```
/// assert_eq!(ecad_model::naming::tag::provisional("-k?3"), Some(("K".to_string(), 3)));
/// assert_eq!(ecad_model::naming::tag::provisional("-W?"), Some(("W".to_string(), 0)));
/// assert_eq!(ecad_model::naming::tag::provisional("-K03"), None);
/// ```
pub fn provisional(tag: &str) -> Option<(String, u64)> {
    let t = tag.trim();
    let t = t.strip_prefix('-').unwrap_or(t);
    let (letters, digits) = t.split_once('?')?;
    if letters.is_empty() || !letters.bytes().all(|b| b.is_ascii_alphabetic()) || digits.len() > MAX_TAG_DIGITS {
        return None;
    }
    if !digits.is_empty() && !crate::naming::words::is_digits(digits) {
        return None;
    }
    let n = if digits.is_empty() { 0 } else { digits.parse::<u64>().ok()? };
    Some((letters.to_ascii_uppercase(), n))
}

/// The letter prefix of a tag in the usual form.
pub fn prefix_of(tag: &str) -> Option<String> {
    parse(tag).map(|(p, _)| p)
}

/// The highest number each tag prefix has had in a history (brief 018: the next automatic number
/// is the highest + 1, so a deleted tag is never given again). Version 2 snapshots carry it
/// (brief 021, the `model:meta` chunk), so a project opened from a snapshot keeps it.
pub type HighWater = std::collections::BTreeMap<String, u64>;

/// Count tag `t` in `high`.
pub fn note(high: &mut HighWater, t: &str) {
    if let Some((prefix, n)) = parse(t) {
        let e = high.entry(prefix).or_insert(0);
        *e = (*e).max(n);
    }
}

/// Count the tag an object holds, unless it is provisional.
fn note_held(high: &mut HighWater, tag: Option<&str>, state: TagState) {
    if let (Some(t), false) = (tag, state.is_provisional()) {
        note(high, t);
    }
}

/// The version of the rule [`note_ops`] counts by (brief 029, FP1): 1 counted device tags only
/// (before brief 026); 2 also counts cable tags and `numbering.high.<P>` resets. Rule 2 also skips provisional
/// tags (S6, #3): no file written before the typed tag state exists to be read, so the rule keeps its number. A file records the
/// rule its cached marks were computed with, and marks of an older rule are recomputed on open.
pub const TAG_RULE: u32 = 2;

/// Count every tag `ops` write or remove (undone ones included), in order.
///
/// # Semantics
/// Device and cable tags share one mark per prefix (brief 026, Q7): `CreateDevice`,
/// `DeleteDevice`, `ModifyDevice`, `CreateCable`, `DeleteCable`, `ModifyCable` and any `SetProperty`
/// of `tag` count (FP7). A
/// `SetProperty` of project property `numbering.high.<P>` to a newer generation (a renumber, or
/// its redo) sets the mark of `P` to its number (FP4: overwrite, not max), so new tags follow a
/// renumbered set; a later tag write raises it again as usual. One back to an older generation or
/// to nothing (the renumber's undo) is not a reset: the mark becomes at least the one the undone
/// reset replaced, so every number held before or since it stays used (tester 026, 3-6). A
/// provisional tag is never counted (S6, #3): its number is a placeholder, not a number used.
pub fn note_ops(high: &mut HighWater, ops: &[crate::Op]) {
    for op in ops {
        match op {
            crate::Op::CreateDevice { device, .. } | crate::Op::DeleteDevice { device, .. } => {
                note_held(high, device.tag(), device.tag_state);
            }
            crate::Op::ModifyDevice { before, after, .. } => {
                note_held(high, before.tag(), before.tag_state);
                note_held(high, after.tag(), after.tag_state);
            }
            crate::Op::CreateCable { cable, .. } | crate::Op::DeleteCable { cable, .. } => {
                note_held(high, cable.tag(), cable.tag_state);
            }
            crate::Op::ModifyCable { before, after, .. } => {
                note_held(high, before.tag(), before.tag_state);
                note_held(high, after.tag(), after.tag_state);
            }
            crate::Op::SetProperty { object: crate::ObjectRef::Project, key, before, after }
                if crate::naming::keys::NUMBERING_HIGH.parse(key).is_some() =>
            {
                let prefix =
                    crate::naming::keys::NUMBERING_HIGH.parse(key).and_then(|p| p.first().map(|p| p.to_ascii_uppercase())).unwrap_or_default();
                if prefix.is_empty() || !prefix.bytes().all(|b| b.is_ascii_alphabetic()) {
                    continue;
                }
                let b = before.as_deref().and_then(parse_reset);
                let a = after.as_deref().and_then(parse_reset);
                match (b, a) {
                    // A reset: nothing before, or a newer generation (equal only for plain
                    // numbers written before the generation existed).
                    (None, Some((n, _, _))) => {
                        high.insert(prefix, n);
                    }
                    (Some((_, _, gb)), Some((n, _, ga))) if ga >= gb => {
                        high.insert(prefix, n);
                    }
                    // An undone reset: back to at least the mark it replaced.
                    (Some((_, was, _)), _) => {
                        let e = high.entry(prefix).or_insert(0);
                        *e = (*e).max(was);
                    }
                    (None, None) => {}
                }
            }
            crate::Op::SetProperty { key, before, after, .. } if key == TAG_KEY => {
                for t in before.iter().chain(after) {
                    note(high, t);
                }
            }
            _ => {}
        }
    }
}

/// The identity of a tag: equal keys are the same tag.
///
/// # Example
/// ```
/// use ecad_model::naming::tag::key;
/// assert_eq!(key("-K1"), key("k01"));
/// assert_ne!(key("-K1"), key("-K10"));
/// ```
pub fn key(tag: &str) -> String {
    match parse(tag) {
        Some((prefix, n)) => format!("{prefix}{n}"),
        None => {
            let t = tag.trim();
            t.strip_prefix('-').unwrap_or(t).to_uppercase()
        }
    }
}

/// The project property of the tag style's leading `-` (brief 033, FP1): `yes` or `no`.
pub const DASH_KEY: &str = "tags.dash";
/// The project property of the tag style's case (brief 033, FP1): `upper` or `typed`.
pub const CASE_KEY: &str = "tags.case";
/// The values of [`DASH_KEY`]; its built-in is data ([`crate::naming::builtins`], brief 066).
pub const DASH_WORDS: &[&str] = &["yes", "no"];
/// The values of [`CASE_KEY`]; its built-in is data.
pub const CASE_WORDS: &[&str] = <Case as crate::naming::words::Word>::WORDS;

crate::word_enum! {
    /// The case tags are written in (brief 033, project property `tags.case`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum Case {
        /// Upper case (the default, and every tag before brief 033).
        #[default]
        Upper,
        /// As typed.
        Typed,
    }
}

/// How a project writes tags (brief 033, design 033 §8): number padding, the leading `-`
/// (`tags.dash`) and the case (`tags.case`). It is a writing rule: [`key`] ignores the `-` and
/// the case, so it never changes which tags are the same. A bare padding is the style of every
/// project before brief 033 (`-`, upper case).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub padding: u8,
    pub dash: bool,
    pub case: Case,
}

impl From<u8> for Style {
    fn from(padding: u8) -> Self {
        Style { padding, dash: true, case: Case::Upper }
    }
}

impl Style {
    /// The project's tag style: its padding, and [`DASH_KEY`] (a yes/no word) and [`CASE_KEY`]
    /// (a [`Case`] word) read as the defaults registry reads them: a value that is not one of the
    /// words is the built-in. `ecad-electrical` registers the same keys and defaults.
    pub fn of(doc: &crate::Document) -> Style {
        let b = crate::naming::builtins::builtins();
        let stored = |key: &str| doc.display.get(key).map(String::as_str);
        let builtin_dash = crate::naming::words::flag(b.tag_dash).unwrap_or(true);
        let builtin_case = <Case as crate::naming::words::Word>::parse(b.tag_case).unwrap_or_default();
        Style {
            padding: doc.settings.tag_padding().min(MAX_PADDING),
            dash: stored(DASH_KEY).and_then(crate::naming::words::flag).unwrap_or(builtin_dash),
            case: stored(CASE_KEY).and_then(<Case as crate::naming::words::Word>::parse).unwrap_or(builtin_case),
        }
    }

    /// The stored text of provisional number `n` (never padded): the dash as set, the letters in
    /// this style's case, no `?` ([`shown`] draws it).
    pub fn provisional(&self, prefix: &str, n: u64) -> String {
        format!("{}{}{n}", self.dash(), self.letters(prefix.trim_start_matches('-')))
    }

    /// `letters` as this style writes them.
    fn letters(&self, letters: &str) -> String {
        match self.case {
            Case::Upper => letters.to_uppercase(),
            Case::Typed => letters.to_string(),
        }
    }

    fn dash(&self) -> &'static str {
        if self.dash { "-" } else { "" }
    }
}

/// The written form of `prefix` and `n`: `-` (unless the style drops it), the prefix in upper case
/// (or as given, for [`Case::Typed`]), the number padded with zeros to the style's padding.
///
/// # Example
/// ```
/// use ecad_model::naming::tag::{Case, Style};
/// assert_eq!(ecad_model::naming::tag::format("K", 1, 2), "-K01");
/// assert_eq!(ecad_model::naming::tag::format("k", 123, 2), "-K123");
/// assert_eq!(ecad_model::naming::tag::format("k", 5, Style { padding: 2, dash: false, case: Case::Typed }), "k05");
/// ```
pub fn format(prefix: &str, n: u64, style: impl Into<Style>) -> String {
    let style = style.into();
    let width = usize::from(style.padding.min(MAX_PADDING));
    format!("{}{}{n:0width$}", style.dash(), style.letters(prefix.trim_start_matches('-')))
}

/// The letters of a tag in the usual form as they were typed (`k05` → `k`), for renumbering a
/// tag under [`Case::Typed`] (design 033 §8.3); `None` for any other tag.
pub fn typed_prefix(tag: &str) -> Option<&str> {
    parse(tag)?;
    let t = tag.trim();
    let t = t.strip_prefix('-').unwrap_or(t);
    let split = t.find(|c: char| !c.is_ascii_alphabetic())?;
    t.get(..split)
}

/// A stored tag rewritten in `style` (design 033 §8.4): the leading `-` added or dropped, and
/// upper-cased under [`Case::Upper`]. The padding and the letters' typed case are not restored.
/// A function or location (`=…`, `+…`) is never touched.
pub fn restyle(tag: &str, style: impl Into<Style>) -> String {
    let style = style.into();
    let t = tag.trim();
    if t.starts_with(['=', '+']) {
        return t.to_string();
    }
    let bare = t.strip_prefix('-').unwrap_or(t);
    let bare = style.letters(bare);
    if style.dash && (bare.starts_with(|c: char| c.is_alphabetic()) || t.starts_with('-')) { format!("-{bare}") } else { bare }
}

/// Refusal of `-K?` where a tag that exists is named.
pub const NEXT_HERE: &str = "-K? asks for the next provisional number; type the tag of a device that exists.";

/// A typed tag that names one (S6, #3): [`enter`], refusing the request for the next provisional
/// number, which names nothing.
///
/// # Errors
/// As [`enter`]; [`NEXT_HERE`] for `-K?`.
pub fn enter_given(text: &str, style: impl Into<Style>) -> Result<Given, crate::ParseError> {
    match enter(text, style)? {
        Entry::Tag(g) => Ok(g),
        Entry::Next(_) => Err(crate::ParseError::invalid(crate::Parsed::Tag, NEXT_HERE)),
    }
}

/// A typed real tag as it is stored ([`normalise_any`], refusing the provisional form: that is
/// [`enter`]'s).
///
/// # Errors
/// As [`normalise_any`], and [`PROVISIONAL_HERE`] for `-K?3`.
pub fn normalise(text: &str, style: impl Into<Style>) -> Result<String, crate::ParseError> {
    let t = normalise_any(text, style)?;
    if provisional(&t).is_some() {
        return Err(crate::ParseError::invalid(crate::Parsed::Tag, PROVISIONAL_HERE));
    }
    Ok(t)
}

/// Refusal of a provisional tag where only a real one is stored.
pub const PROVISIONAL_HERE: &str = "A provisional tag (-K?3) can only be set on a device or cable's tag, not here.";

/// A typed tag as written, provisional spelling included (read by [`enter`]).
///
/// # Semantics
/// Trimmed and in upper case. In the usual form ([`parse`]) it is written by [`format`] with the
/// project's padding (`k5` → `-K05` with padding 2). Anything else is kept as typed, with `-`
/// added when it starts with a letter.
///
/// # Errors
/// [`BAD_TAG`] when it is empty, longer than [`MAX_TAG_CHARS`] characters, holds a control
/// character, or is nothing but `-` (its [`key`] would be empty or `-`).
pub fn normalise_any(text: &str, style: impl Into<Style>) -> Result<String, crate::ParseError> {
    let style = style.into();
    let t = text.trim();
    let n = t.chars().count();
    if !(1..=MAX_TAG_CHARS).contains(&n) || t.chars().any(char::is_control) || t.chars().all(|c| c == '-' || c.is_whitespace()) {
        return Err(crate::ParseError::invalid(crate::Parsed::Tag, BAD_TAG));
    }
    if let Some((_, number)) = parse(t) {
        let prefix = typed_prefix(t).unwrap_or_default();
        return Ok(format(prefix, number, style));
    }
    if let Some((prefix, number)) = provisional(t) {
        let typed = t.strip_prefix('-').unwrap_or(t).split('?').next().unwrap_or(&prefix);
        let letters = style.letters(typed);
        return Ok(if number == 0 { format!("{}{letters}?", style.dash()) } else { format!("{}{letters}?{number}", style.dash()) });
    }
    let shown = style.letters(t);
    let bare = shown.strip_prefix('-').unwrap_or(&shown);
    if !style.dash {
        return Ok(bare.to_string());
    }
    if shown.starts_with(|c: char| c.is_alphabetic()) { Ok(format!("-{shown}")) } else { Ok(shown) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_tag_table() {
        let same = [("-K1", "k01"), ("-K01", "K1"), ("-TB01", "tb1"), ("N01", "-N1"), ("-K 1", "-k 1")];
        for (a, b) in same {
            assert_eq!(key(a), key(b), "{a} and {b}");
        }
        let different = [("-K1", "-K10"), ("-K1", "-Q1"), ("-K1", "-KA1"), ("-K1A", "-K1")];
        for (a, b) in different {
            assert_ne!(key(a), key(b), "{a} and {b}");
        }
    }

    #[test]
    fn the_tag_style_table() {
        let st = |dash: bool, case: Case| Style { padding: 2, dash, case };
        let rows = [
            ("k5", ["-K05", "K05", "k05", "-k05"]),
            ("M02a", ["-M02A", "M02A", "M02a", "-M02a"]),
            ("N01 Slot02", ["-N01 SLOT02", "N01 SLOT02", "N01 Slot02", "-N01 Slot02"]),
            ("-K1", ["-K01", "K01", "K01", "-K01"]),
            ("=PWR", ["=PWR", "=PWR", "=PWR", "=PWR"]),
        ];
        let styles = [st(true, Case::Upper), st(false, Case::Upper), st(false, Case::Typed), st(true, Case::Typed)];
        for (typed, want) in rows {
            for (style, w) in styles.iter().zip(want) {
                assert_eq!(normalise(typed, *style).as_deref(), Ok(w), "{typed} {style:?}");
                assert_eq!(key(typed), key(w), "identity kept: {typed} {w}");
            }
        }
        assert_eq!(restyle("-K01", st(false, Case::Upper)), "K01");
        assert_eq!(restyle("K01", st(true, Case::Upper)), "-K01");
        assert_eq!(restyle("k01", st(true, Case::Upper)), "-K01");
        assert_eq!(restyle("+CP1", st(true, Case::Upper)), "+CP1");
        assert_eq!(typed_prefix("-k05"), Some("k"));
        assert_eq!(format("k", 3, st(false, Case::Typed)), "k03");
    }

    #[test]
    fn normalise_table() {
        let rows = [
            ("k5", 2, "-K05"),
            ("-K1", 1, "-K1"),
            ("-k123", 2, "-K123"),
            ("sr1", 2, "-SR01"),
            ("  tb01 ", 3, "-TB001"),
            ("-K1A", 2, "-K1A"),
            ("=PWR", 2, "=PWR"),
            ("main switch", 2, "-MAIN SWITCH"),
        ];
        for (input, pad, want) in rows {
            assert_eq!(normalise(input, pad).as_deref(), Ok(want), "{input}");
        }
        for bad in ["", "   ", "a\u{7}", &"K".repeat(41), "-", "--", " - - "] {
            assert!(normalise(bad, 2).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn provisional_table() {
        let rows = [
            ("-K?3", Some(("K", 3))),
            ("k?03", Some(("K", 3))),
            ("-W?", Some(("W", 0))),
            ("-K3", None),
            ("-?3", None),
            ("-K?3A", None),
            ("-K??3", None),
        ];
        for (t, want) in rows {
            assert_eq!(provisional(t), want.map(|(p, n)| (p.to_string(), n)), "{t}");
        }
        assert_eq!(enter("k?03", 2), Ok(Entry::Tag(Given::new("-K3", TagState::Provisional))));
        assert_eq!(enter("-k?", 2), Ok(Entry::Next("K".into())));
        assert_eq!(enter("k3", 2), Ok(Entry::Tag(Given::new("-K03", TagState::Fixed))));
        assert!(normalise("-K?3", 2).is_err(), "a real-tag entry refuses the provisional spelling");
        assert_eq!(shown("-K3", TagState::Provisional), "-K?3");
        assert_eq!(shown("K3", TagState::Provisional), "K?3");
        assert_eq!(shown("-K03", TagState::Fixed), "-K03");
        assert_ne!(Key::of("-K3", TagState::Provisional), Key::of("-K3", TagState::Fixed), "a provisional -K3 is not the real -K3");
        assert_eq!(Key::of("-K03", TagState::Fixed), Key::of("k3", TagState::Automatic));
    }

    #[test]
    fn high_reset_overwrites_then_tags_raise() {
        use crate::{ObjectRef, Op};
        let set = |k: &str, v: &str| Op::SetProperty { object: ObjectRef::Project, key: k.into(), before: None, after: Some(v.into()) };
        let tag = |v: &str| Op::SetProperty { object: ObjectRef::Project, key: TAG_KEY.into(), before: None, after: Some(v.into()) };
        let mut high = HighWater::new();
        note_ops(&mut high, &[tag("-K300"), set("numbering.high.K", "40")]);
        assert_eq!(high.get("K"), Some(&40));
        note_ops(&mut high, &[set("numbering.high.K", "x"), set("numbering.high.", "3"), set("numbering.high.K1", "3")]);
        assert_eq!(high.get("K"), Some(&40));
        note_ops(&mut high, &[tag("-K41")]);
        assert_eq!(high.get("K"), Some(&41));
    }

    /// Tester 026, 3-6: a reset's undo restores the mark it replaced and is not a new reset; a
    /// redo resets again; a reset to the same number is still a change.
    #[test]
    fn an_undone_reset_restores_the_mark_it_replaced() {
        use crate::{ObjectRef, Op};
        let k = "numbering.high.K".to_string();
        let op = |b: Option<String>, a: Option<String>| Op::SetProperty { object: ObjectRef::Project, key: k.clone(), before: b, after: a };
        let mut high = HighWater::new();
        note(&mut high, "-K09");
        let first = op(None, Some(reset_value(3, 9, 1)));
        note_ops(&mut high, std::slice::from_ref(&first));
        assert_eq!(high.get("K"), Some(&3));
        note(&mut high, "-K04");
        let second = op(Some(reset_value(3, 9, 1)), Some(reset_value(3, 4, 2)));
        note_ops(&mut high, std::slice::from_ref(&second));
        assert_eq!(high.get("K"), Some(&3), "a reset to the same number still resets");
        note_ops(&mut high, &[second.inverse()]);
        assert_eq!(high.get("K"), Some(&4), "undoing the second reset gives back the -K04 mark");
        note_ops(&mut high, &[first.inverse()]);
        assert_eq!(high.get("K"), Some(&9), "undoing the first gives back the -K09 mark");
        note_ops(&mut high, &[first]);
        assert_eq!(high.get("K"), Some(&3), "a redo resets again");
        assert_eq!(parse_reset("40"), Some((40, 0, 0)));
        assert_eq!(parse_reset("1;2"), None);
        assert_eq!(parse_reset("1;2;3;4"), None);
    }

    #[test]
    fn hostile_numbers_are_text() {
        let long = format!("-K{}", "9".repeat(20));
        assert_eq!(parse(&long), None);
        assert_eq!(key(&long), long.trim_start_matches('-'));
        assert_eq!(format("K", u64::MAX, 255).len(), 2 + u64::MAX.to_string().len());
    }
}
