//! The rules of brief 042's command `setdefault`, the one writer for the Standards screen and the
//! inspector's "change at level" and Reset (design `ui-architecture.md` §11, U4c).
//!
//! # Semantics
//! It writes a registry entry's value at one level through the same stored values the registry
//! reads ([`crate::defaults::resolve`]): the project property of the entry's key, or the project
//! setting behind it ([`ProjectSource::Settings`], through [`Op::SetProjectSettings`]); a page's
//! property; a device's, conductor's or symbol's property (`text.<rep>.<rest>` on its component).
//! No new op and no new stored field. A call that changes nothing has no ops. A reset removes the
//! level's value, so the entry resolves one level further out.
//!
//! Brief 050a: the standard level writes the project's standard's value (`SetProperty` on
//! `ObjectRef::Standard`); it is refused while the project uses IEC (built in). A style choice
//! ([`defaults::Kind::Style`]) takes a style's ID or name and stores the ID.
//!
//! A symbol's family entry (`symbol.<key>.<field>`, [`crate::defaults::symbols::family`]) is
//! written at the project level only ([`plan_family`]).
//!
//! Excluded: any entry's levels, which only the registry sets.

use ecad_model::{DeviceId, Document, Noun, ObjectRef, Op, PageId, ProjectSettings, RepresentationId, WireId};

use crate::defaults::{self, Entry, Level, ProjectSource};
use crate::error::CmdError;

/// Refusals, in words.
pub const STANDARD_READ_ONLY: &str = "This project uses IEC (built in). Make a standard first (`standard {new}`).";
pub const NEEDS_VALUE: &str = "Give a `value`, or `reset: true` to remove this level's value.";
pub const NEEDS_OBJECT: &str = "The object level needs a `component`, `conductor` or `representation`.";
/// A call with both a value and a reset.
pub const NOT_BOTH: &str = "Give either `value` or `reset: true`, not both.";

/// The words of a level in `defaults` and here.
pub fn level_word(l: Level) -> &'static str {
    match l {
        Level::Standard => "standard",
        Level::Project => "project",
        Level::Page => "page",
        Level::Object => "object",
    }
}

/// The level a word names (`object`, `symbol` and `device` are the object level).
///
/// # Errors
/// A word that names no level, with the levels.
pub fn level_of(word: &str) -> Result<Level, CmdError> {
    match word.trim().to_ascii_lowercase().as_str() {
        "standard" => Ok(Level::Standard),
        "project" => Ok(Level::Project),
        "page" => Ok(Level::Page),
        "object" | "symbol" | "device" => Ok(Level::Object),
        _ => Err(CmdError::refused(format!("Unknown level \"{word}\". Levels: standard, project, page, object."))),
    }
}

/// `"`text.tag.height` cannot be set at page level. It can be set at: project, object."`
pub fn level_refused(entry: &Entry, level: Level) -> String {
    let allowed: Vec<&str> = entry.levels.iter().filter(|l| **l != Level::Standard).map(|l| level_word(*l)).collect();
    if allowed.is_empty() {
        return format!("`{}` cannot be set at {} level; it has only its built-in value.", entry.key, level_word(level));
    }
    format!("`{}` cannot be set at {} level. It can be set at: {}.", entry.key, level_word(level), allowed.join(", "))
}

/// The text a value is stored as: a number as short as it reads, a word as registered, a prefix
/// as Tag Settings writes it (letters, upper case, no `-`), other text trimmed. `typed` is the value
/// as given (a number as JSON writes it).
///
/// # Errors
/// The value is not valid for the entry, in words.
pub fn stored_text(entry: &Entry, typed: &str) -> Result<String, CmdError> {
    let parsed = entry.parse_input(typed).ok_or_else(|| bad_value(entry))?;
    if is_prefix(entry) {
        return crate::devices::valid_prefix(typed).ok_or_else(|| bad_value(entry));
    }
    Ok(match parsed {
        defaults::Value::Length(x) | defaults::Value::Number(x) => format!("{x}"),
        defaults::Value::Choice(w) => w.to_string(),
        defaults::Value::Flag(b) => ecad_model::words::flag_word(b).to_string(),
        defaults::Value::Size(s) => s.to_string(),
        defaults::Value::Colours(c) => c.to_string(),
        defaults::Value::Colour(c) => c.to_string(),
        defaults::Value::Sheet(s) => s.text(),
        defaults::Value::Margins(m) => ecad_model::sheet::margins_text(&m),
        defaults::Value::CoreColours(c) => c.to_string(),
        defaults::Value::Text(t) => t.trim().to_string(),
        defaults::Value::OwnStyle(id) => ecad_model::StyleRef::Own(id).text(),
    })
}

/// What a value of `entry` must be, in words.
fn bad_value(entry: &Entry) -> CmdError {
    CmdError::refused(match entry.kind {
        defaults::Kind::Length { min, max } => format!("`{}` is a length from {min} to {max} mm.", entry.key),
        defaults::Kind::Number { min, max } => format!("`{}` is a number from {min} to {max}.", entry.key),
        defaults::Kind::Choice(words) => format!("`{}` is one of: {}.", entry.key, words.join(", ")),
        defaults::Kind::Flag => format!("`{}` is yes or no.", entry.key),
        _ if is_prefix(entry) => format!("A prefix is 1 to {} letters, for example K or SR.", crate::devices::MAX_PREFIX_LETTERS),
        defaults::Kind::Text { max_chars } | defaults::Kind::Parsed { max_chars, .. } => {
            format!("`{}` is 1 to {max_chars} characters with no control characters, in its own form.", entry.key)
        }
        defaults::Kind::Style(cat) => format!("`{}` is a style of the {cat} catalogue: its ID or name (see `styles`).", entry.key),
    })
}

/// A tag prefix entry (`symbol.<key>.prefix`, `terminal.<key>.prefix`).
fn is_prefix(entry: &Entry) -> bool {
    starter_key(entry).is_some()
}

/// The starter symbol key of a prefix entry.
fn starter_key(entry: &Entry) -> Option<&'static str> {
    // Brief 045: its new starters' prefixes are in the `plc` area.
    let rest = entry.key.strip_prefix("symbol.").or_else(|| entry.key.strip_prefix("terminal.")).or_else(|| entry.key.strip_prefix("plc."))?;
    let key = rest.strip_suffix(".prefix")?;
    let starter: crate::symbols::StarterKey = ecad_model::Word::parse(key)?;
    defaults::symbols::prefix_entry(starter).filter(|e| e.key == entry.key).map(|_| key)
}

/// Set (or with `None` remove) starter `key`'s tag prefix in `s`, as Tag Settings writes it: the
/// starter's own prefix is stored as none.
pub fn set_tag_prefix(s: &mut ProjectSettings, key: &str, prefix: Option<&str>) {
    crate::devices::set_symbol_prefix(s, ecad_model::prefix::PrefixOf::Starter(key.into()), prefix);
}

/// Write `value` (`None`: remove) into the project setting behind a [`ProjectSource::Settings`]
/// entry, as the setting's own command writes it.
///
/// # Errors
/// The entry has no setting this command knows (a registry entry added without a writer).
pub fn write_setting(entry: &Entry, s: &mut ProjectSettings, value: Option<&str>) -> Result<(), CmdError> {
    // Brief 066 (F2): `None` stores "not chosen", never the built-in's value.
    let number = |v: Option<&str>| v.and_then(|t| t.parse::<f64>().ok());
    // A whole number; the entry's kind has already checked its range.
    let whole =
        |v: Option<&str>| v.map(|t| t.trim().parse::<u32>().map_err(|_| CmdError::refused(format!("`{t}` must be a whole number.")))).transpose();
    let small = |n: u32| u8::try_from(n).unwrap_or(u8::MAX);
    match entry.key {
        k if k == defaults::text::WIRE_LABEL_HEIGHT.key => s.wiring.label_height = number(value),
        k if k == defaults::wires::WIRING_LABEL_RULE.key => s.wiring.label_rule = value.map(str::to_string),
        k if k == defaults::wires::WIRING_LABEL_MIN_LENGTH.key => s.wiring.label_min_length = number(value),
        // Brief 066c (F3): whole numbers and words; the entry's kind has checked the range.
        k if k == defaults::settings::TAG_PADDING.key => s.tag_padding = whole(value)?.map(small),
        k if k == defaults::settings::WIRE_DIGITS.key => s.wiring.digits = whole(value)?.map(small),
        k if k == defaults::settings::PAGE_NUMBER_STEP.key => s.page_number_step = whole(value)?,
        k if k == defaults::settings::COLOUR_STYLE.key => {
            s.wiring.colour_style = value.and_then(<ecad_model::colour::ColourWords as ecad_model::words::Word>::parse)
        }
        k if k == defaults::settings::REFERENCE_STYLE.key => s.reference_style = value.and_then(ecad_model::ReferenceStyle::parse),
        _ => match starter_key(entry) {
            Some(key) => set_tag_prefix(s, key, value),
            None => return Err(CmdError::refused(format!("`{}` has no project setting this command can write.", entry.key))),
        },
    }
    Ok(())
}

/// What a `setdefault` call writes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Project,
    Page(PageId),
    Device(DeviceId),
    Wire(WireId),
    Symbol(RepresentationId),
}

/// The ops writing `value` (`None`: reset) for `entry` at `level` on `target`; empty when it is
/// already so.
///
/// # Errors
/// A level the entry does not allow, the standard level, or a missing target, in words.
pub fn ops(doc: &Document, entry: &Entry, level: Level, target: Target, value: Option<String>) -> Result<Vec<Op>, CmdError> {
    if level == Level::Standard && doc.standard.is_none() {
        return Err(CmdError::refused(STANDARD_READ_ONLY));
    }
    if !entry.allows(level) {
        return Err(CmdError::refused(level_refused(entry, level)));
    }
    let property = |object: ObjectRef, props: Option<&ecad_model::Properties>, key: String| -> Result<Vec<Op>, CmdError> {
        let props = props.ok_or(CmdError::NotFound(Noun::Object))?;
        let before = props.get(&key).cloned();
        Ok(if before == value { Vec::new() } else { vec![Op::SetProperty { object, key, before, after: value.clone() }] })
    };
    match (level, target) {
        (Level::Project, _) => match entry.project {
            ProjectSource::Property => property(ObjectRef::Project, Some(&doc.display), entry.key.to_string()),
            ProjectSource::Settings(_) => {
                let before = doc.settings.clone();
                let mut after = before.clone();
                write_setting(entry, &mut after, value.as_deref())?;
                Ok(if after == before { Vec::new() } else { vec![Op::SetProjectSettings { before, after }] })
            }
        },
        // Design 072 §5.1: a page's type is its field, changed with `ModifyPage`.
        (Level::Page, Target::Page(pg)) if entry.key == ecad_model::sheet::PAGE_TYPE_KEY => {
            let before = doc.pages.get(&pg).ok_or(CmdError::NotFound(Noun::Object))?;
            let mut after = before.clone();
            after.page_type = match value.as_deref() {
                Some(v) => Some(ecad_model::StyleRef::parse(v).ok_or_else(|| CmdError::refused(format!("`{v}` is not a page type")))?),
                None => None,
            };
            Ok(if after == *before { Vec::new() } else { vec![Op::ModifyPage { id: pg, before: before.clone(), after }] })
        }
        (Level::Page, Target::Page(pg)) => property(ObjectRef::Page(pg), doc.pages.get(&pg).map(|p| &p.display), entry.key.to_string()),
        (Level::Object, Target::Symbol(rep)) => {
            let c = match doc.representations.get(&rep).map(|r| r.subject) {
                Some(ecad_model::Subject::Device(c)) => c,
                _ => return Err(CmdError::refused(format!("No symbol {}.", rep.hex()))),
            };
            property(ObjectRef::Device(c), doc.devices.get(&c).map(|c| &c.properties), defaults::symbol_key(entry.key, rep))
        }
        (Level::Object, Target::Device(c)) => property(ObjectRef::Device(c), doc.devices.get(&c).map(|c| &c.properties), entry.key.to_string()),
        // Design 072 §4.6: a wire's own type is its field, changed with `ModifyWire`.
        (Level::Object, Target::Wire(c)) if entry.key == ecad_model::wiring::WIRE_CATEGORY => {
            let before = doc.wires.get(&c).ok_or(CmdError::NotFound(Noun::Wire))?;
            let mut after = before.clone();
            after.wire_type = match value.as_deref() {
                Some(v) => Some(ecad_model::StyleRef::parse(v).ok_or_else(|| CmdError::refused(format!("`{v}` is not a wire type")))?),
                None => None,
            };
            Ok(if after == *before { Vec::new() } else { vec![Op::ModifyWire { id: c, before: before.clone(), after }] })
        }
        (Level::Object, Target::Wire(c)) => property(ObjectRef::Wire(c), doc.wires.get(&c).map(|c| &c.display), entry.key.to_string()),
        (Level::Object, _) => Err(CmdError::refused(NEEDS_OBJECT)),
        (Level::Page, _) => Err(CmdError::refused("The page level needs a `page`.")),
        (Level::Standard, _) => property(ObjectRef::Standard, doc.standard.as_ref().map(|s| &s.values), entry.key.to_string()),
    }
}

/// A value as a call gives it.
#[derive(Debug, Clone, PartialEq)]
pub enum Given {
    /// Text.
    Text(String),
    /// A number, as JSON writes it.
    Number(String),
}

/// What a call writes, before the project is read.
#[derive(Debug, Clone, PartialEq)]
pub enum Chosen {
    /// The stored text (`None`: reset).
    Stored(Option<String>),
    /// A style of catalogue `.0`, by ID or name, resolved in the project ([`ops_for`]).
    Style(ecad_model::StyleCatalogue, String),
}

/// What `entry` is given: `value`, or `reset`.
///
/// # Errors
/// Neither or both given, or a value the entry refuses.
pub fn chosen(entry: &Entry, value: Option<Given>, reset: bool) -> Result<Chosen, CmdError> {
    // A style choice names a style by ID or name, resolved in the project (brief 050a).
    if let (Some(Given::Text(v)), false, defaults::Kind::Style(cat)) = (&value, reset, entry.kind) {
        return Ok(Chosen::Style(cat, v.clone()));
    }
    match (value, reset) {
        // Brief 054 (design 054 §6.1): an empty cover heading is allowed, stored as `none`.
        (Some(Given::Text(v)), false) if entry.key == defaults::pages::HEADING.key && v.trim().is_empty() => {
            Ok(Chosen::Stored(Some(defaults::pages::NO_HEADING.to_string())))
        }
        (Some(Given::Text(v) | Given::Number(v)), false) => Ok(Chosen::Stored(Some(stored_text(entry, &v)?))),
        (None, true) => Ok(Chosen::Stored(None)),
        (Some(_), true) => Err(CmdError::input(NOT_BOTH)),
        (None, false) => Err(CmdError::input(NEEDS_VALUE)),
    }
}

/// The object a call names for the object level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Objects {
    pub representation: Option<RepresentationId>,
    pub component: Option<DeviceId>,
    pub conductor: Option<WireId>,
}

/// The target of `level`: the page `page` (else `shown`), or the object named.
///
/// # Errors
/// The object level with no object.
pub fn target(level: Level, page: Option<PageId>, shown: PageId, o: Objects) -> Result<Target, CmdError> {
    Ok(match level {
        Level::Project | Level::Standard => Target::Project,
        Level::Page => Target::Page(page.unwrap_or(shown)),
        Level::Object => match (o.representation, o.component, o.conductor) {
            (Some(r), _, _) => Target::Symbol(r),
            (None, Some(c), _) => Target::Device(c),
            (None, None, Some(c)) => Target::Wire(c),
            _ => return Err(CmdError::refused(NEEDS_OBJECT)),
        },
    })
}

/// The ops of a `setdefault` call: the chosen value (a style resolved in `doc`) for `entry` at
/// `level` on `target`.
///
/// # Errors
/// A style that does not exist, a page type a generated page cannot take, or [`ops`]' refusals.
pub fn ops_for(doc: &Document, entry: &Entry, level: Level, target: Target, chosen: Chosen) -> Result<Vec<Op>, CmdError> {
    let value = match chosen {
        Chosen::Style(cat, v) => Some(crate::styles::find(doc, cat, &v)?.text()),
        Chosen::Stored(v) => v,
    };
    // Brief 054 (design 054 §2.3): a generated page's type keeps its kind's position.
    if let (Target::Page(pg), Some(v)) = (target, value.as_deref())
        && entry.key == defaults::pages::PAGE_TYPE.key
    {
        crate::page_types::newpage_type(doc, Some(v), crate::generated::kind_of(doc, pg))?;
    }
    ops(doc, entry, level, target, value)
}

// ---------------------------------------------------------------- symbol family entries (brief 048)

/// A symbol family entry's change ([`plan_family`]).
#[derive(Debug, Clone, PartialEq)]
pub struct FamilySet {
    /// The symbol's key.
    pub symbol: String,
    pub field: crate::defaults::symbols::Field,
    /// The ops; none when nothing changes.
    pub ops: Vec<Op>,
}

/// `setdefault {key: symbol.<key>.<field>, level?: project, value | reset: true}` for a symbol's
/// family entry (design 048 §7): the project level only. A prefix is written to the project's
/// `tag_prefixes`, a description or rating to the project property of the entry's key. `None` when
/// `key` names no family entry.
///
/// # Errors
/// Neither or both of a text value and `reset`; another level; a field the symbol keeps as its
/// own; a value the field does not take.
pub fn plan_family(doc: &Document, key: &str, level: Option<&str>, value: Option<Given>, reset: bool) -> Result<Option<FamilySet>, CmdError> {
    use crate::defaults::symbols::{self as family, Field};
    // Only `symbol.<key>.<field>` keys: any other key is "No default" for the caller to say.
    if !key.trim().starts_with("symbol.") {
        return Ok(None);
    }
    let Some((symbol, field)) = family::family(doc, key) else { return Ok(None) };
    let value = match (value, reset) {
        (Some(Given::Text(v)), false) => Some(v.trim().to_string()),
        (None, true) => None,
        (Some(_), true) => return Err(CmdError::refused(NOT_BOTH)),
        _ => return Err(CmdError::refused(NEEDS_VALUE)),
    };
    if !level.unwrap_or("project").trim().eq_ignore_ascii_case("project") {
        return Err(CmdError::refused(format!("{key} is set for the project; a symbol's own value is edited in the symbol.")));
    }
    if !field.overridable() {
        return Err(CmdError::refused(format!("{key} is the symbol's own: edit the symbol to change it.")));
    }
    if let Some(v) = &value
        && !field.accepts(v)
    {
        return Err(CmdError::refused(format!("{v} is not a {} ({} characters at most).", field.label().to_lowercase(), field.max_chars())));
    }
    let ops = match field {
        Field::Prefix => {
            let before = doc.settings.clone();
            let mut after = before.clone();
            let of = ecad_model::prefix::PrefixOf::Project(symbol.as_str().into());
            crate::devices::set_symbol_prefix(&mut after, of, value.as_deref());
            if after == before { Vec::new() } else { vec![Op::SetProjectSettings { before, after }] }
        }
        _ => {
            let k = family::family_property(&symbol, field).unwrap_or_default();
            let before = doc.display.get(&k).cloned();
            if before == value { Vec::new() } else { vec![Op::SetProperty { object: ObjectRef::Project, key: k, before, after: value }] }
        }
    };
    Ok(Some(FamilySet { symbol, field, ops }))
}

/// The text a value given as JSON is stored as ([`stored_text`]), for the Standards
/// screen's field.
///
/// # Errors
/// Not text or a number, or not valid for the entry, in words.
pub fn json_stored_text(entry: &Entry, value: &serde_json::Value) -> Result<String, CmdError> {
    let typed = match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return Err(CmdError::input(format!("`value` for `{}` is a string or a number.", entry.key))),
    };
    stored_text(entry, &typed)
}
