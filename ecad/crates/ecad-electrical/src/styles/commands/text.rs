//! The text commands' rules: the text style table
//! (`textstyle`), which texts are shown and at which level (`showtext`), a moved tag or connection
//! point label (`movetext`), and the scope the defaults list is resolved at (`defaults`). Each
//! function reads the document and returns the ops to commit and what to report; the generic
//! commands (`textstyle`, `pagetexts`, `movetext`) are declared at the end, measured with the text metrics
//! the runner gives them. `showtext` and `hiddentext` need the session and stay in the engine.

use crate::defaults::{Entry, Level, Resolved, Source};
use ecad_model::Word;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{
    ConnectionPointId, DeviceId, Document, Form, Noun, ObjectRef, Op, PageId, Pt, RepresentationId, StyleLayer, StyleRef, Subject, TextHAlign,
    TextVAlign, WireId,
};

use crate::commands::input::{Bounded, Point, nullable};
use crate::commands::run::{Access, Cx, Generic, Outcome, Pages};
use crate::defaults::{self, Scope};
use crate::devices::texts::TextOf;
use crate::devices::{self};
use crate::error::CmdError;
use crate::styles::text_style::fonts::{Fonts, TextMetrics};
use crate::styles::text_style::{self, Shown, TextKind, kind_words};

/// Refusals.
pub const BAD_HEIGHT: &str = "A text height is 0.5 to 10 mm.";
pub(crate) const BAD_WEIGHT: &str = "Weight is regular or bold.";
pub const BAD_SHOW: &str = "Shown is yes, no or first (first: tag only).";
pub const BAD_FONT: &str = "A font name is 1 to 64 characters with no control characters.";
pub const NEEDS_KIND: &str = "height, weight and show need a kind.";
pub const BAD_POINT: &str = "A text position must be a finite point.";
pub const BAD_JUSTIFY: &str = "Justify is two letters: L, C or R, then T, M or B.";
pub(crate) const NEEDS_CONNECTION_POINT: &str = "point_label needs the connection point's key.";
const NOT_A_SYMBOL: &str = "That representation is not a symbol.";

/// Most kinds one `textstyle` call changes (the style table has fewer; a guard on hostile input).
pub(crate) const MAX_KINDS: usize = 64;

/// A field of a call: `None` leaves it, `Some(None)` goes back to the default, `Some(Some(v))`
/// sets it.
pub type Change<T> = Option<Option<T>>;

/// `Unknown text kind "x". Kinds: tag, point_label, …`
fn unknown_kind(word: &str) -> CmdError {
    let all: Vec<&str> = TextKind::ALL.iter().map(|k| k.word()).collect();
    CmdError::refused(format!("Unknown text kind \"{word}\". Kinds: {}.", all.join(", ")))
}

/// The kind a call names (any case, spaces around ignored).
///
/// # Errors
/// A word that is no kind.
pub fn kind(word: &str) -> Result<TextKind, CmdError> {
    let w = word.trim().to_ascii_lowercase();
    <TextKind as ecad_model::Word>::parse(&w).ok_or_else(|| unknown_kind(&w))
}

/// The kind a call may name: `None` when it names none.
///
/// # Errors
/// A word that is no kind.
pub(crate) fn opt_kind(word: Option<&str>) -> Result<Option<TextKind>, CmdError> {
    word.map(kind).transpose()
}

// ---------------------------------------------------------------- textstyle

/// One kind's row of a `textstyle` call, as given.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KindRow {
    /// The kind (needed when the row changes anything).
    pub kind: Option<String>,
    /// The kind's own font.
    pub font: Change<String>,
    /// The cap height, mm.
    pub height: Change<f64>,
    /// "regular" or "bold".
    pub weight: Change<String>,
    /// "on", "off" or "first".
    pub show: Change<String>,
}

/// A `textstyle` call that writes, as given.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TextStyleInput {
    /// The project font.
    pub font: Change<String>,
    /// The call's own kind row (`kind`, `height`, `weight`, `show`, and `font` when it names a
    /// kind).
    pub row: KindRow,
    /// More rows, written in the same transaction.
    pub kinds: Vec<KindRow>,
}

/// One kind's changes, checked.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KindChange {
    kind: TextKind,
    /// The kind's own font; `null` goes back to the project font.
    font: Change<String>,
    height: Change<f64>,
    weight: Change<String>,
    show: Change<String>,
}

/// The changes of a `textstyle` call, checked: what [`textstyle_ops`] writes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextStyleChanges {
    font: Change<String>,
    kinds: Vec<KindChange>,
}

/// `v` checked by `ok` (`None`: refused with `bad`).
fn check<T, U>(v: Change<T>, ok: impl FnOnce(T) -> Option<U>, bad: &'static str) -> Result<Change<U>, CmdError> {
    match v {
        None => Ok(None),
        Some(None) => Ok(Some(None)),
        Some(Some(x)) => ok(x).map(|x| Some(Some(x))).ok_or(CmdError::refused(bad)),
    }
}

/// A word of `allowed`, trimmed and in lower case.
fn word_in(w: String, allowed: &[&str]) -> Option<String> {
    let w = w.trim().to_ascii_lowercase();
    allowed.contains(&w.as_str()).then_some(w)
}

/// The kind changes of `row`; `None` when it names no kind and changes nothing.
fn kind_change(row: KindRow) -> Result<Option<KindChange>, CmdError> {
    let kind = opt_kind(row.kind.as_deref())?;
    let height = check(row.height, |h| (h.is_finite() && (0.5..=10.0).contains(&h)).then_some(h), BAD_HEIGHT)?;
    let weight = check(row.weight, |w| word_in(w, &["regular", "bold"]), BAD_WEIGHT)?;
    let show = check(row.show, |w| Shown::from_input(&w).map(|s| s.word().to_string()), BAD_SHOW)?;
    let changes = height.is_some() || weight.is_some() || show.is_some();
    let Some(kind) = kind else {
        return if changes { Err(CmdError::refused(NEEDS_KIND)) } else { Ok(None) };
    };
    // With a kind, `font` is that kind's own font.
    let font = check(row.font, |f| Some(f.trim().to_string()), BAD_FONT)?;
    if let Some(Some(f)) = &font
        && defaults::parse_value(defaults::Kind::Text { max_chars: 64 }, f).is_none()
    {
        return Err(CmdError::refused(BAD_FONT));
    }
    if let Some(Some(w)) = &show {
        let entry = kind.show_entry().ok_or_else(|| CmdError::refused(format!("{} has no Shown setting.", kind.word())))?;
        if entry.parse(w).is_none() {
            return Err(CmdError::refused(BAD_SHOW));
        }
    }
    Ok(Some(KindChange { kind, font, height, weight, show }))
}

/// The changes of a `textstyle` call, checked before the project is read.
///
/// # Errors
/// A font, height, weight or show that does not read, an unknown kind, a row changing something
/// without a kind, or a kind with no Shown setting given one.
pub(crate) fn textstyle_changes(input: TextStyleInput) -> Result<TextStyleChanges, CmdError> {
    // The project font: `font` with no `kind` (with a kind it is that kind's font).
    let named = input.row.kind.is_some();
    let font = if named { None } else { check(input.font.clone(), |f| Some(f.trim().to_string()), BAD_FONT)? };
    if let Some(Some(f)) = &font
        && defaults::text::FONT.parse(f).is_none()
    {
        return Err(CmdError::refused(BAD_FONT));
    }
    let row = KindRow { font: if named { input.font } else { None }, ..input.row };
    let mut kinds: Vec<KindChange> = kind_change(row)?.into_iter().collect();
    for row in input.kinds {
        match kind_change(row)? {
            Some(c) => kinds.push(c),
            None => return Err(CmdError::refused(NEEDS_KIND)),
        }
    }
    Ok(TextStyleChanges { font, kinds })
}

/// One `text.*` project property change, or nothing when it is already so.
fn set_project(doc: &Document, key: &str, after: Option<String>, ops: &mut Vec<Op>) {
    let before = doc.display().get(key).cloned();
    if before != after {
        ops.push(Op::SetProperty { object: ObjectRef::Project, key: key.to_string(), before, after });
    }
}

/// Field changes per text style, gathered over a `textstyle` call.
type StyleFields = BTreeMap<StyleRef, Vec<(String, Option<String>)>>;

/// The ops of one kind's changes. Height, weight and font are fields of the text style the kind
/// uses at project level, gathered in `style` per style and written
/// as one record each by [`style_ops`]; the wire label height stays in `settings.wiring`.
fn kind_ops(doc: &Document, c: &KindChange, ops: &mut Vec<Op>, style: &mut StyleFields) {
    let k = c.kind;
    let id = text_style::style_of(doc, k, Scope::project());
    let mut put = |field: &str, v: Option<String>| {
        let fields = style.entry(id.clone()).or_default();
        fields.retain(|(f, _)| f != field);
        fields.push((field.to_string(), v));
    };
    if let Some(h) = c.height {
        if k == TextKind::WireLabel && id == k.builtin_style() {
            // One stored value: the wiring setting, never a record. A later row of the
            // same call replaces an earlier one.
            ops.retain(|o| !matches!(o, Op::SetProjectSettings { .. }));
            let before = doc.settings().clone();
            let mut after = before.clone();
            after.wiring.label_height = h;
            if after != before {
                ops.push(Op::SetProjectSettings { before, after });
            }
        } else {
            // The stored text of a number: as short as it reads (`3`, `2.5`).
            put("height", h.map(|x| format!("{x}")));
        }
    }
    if let Some(f) = &c.font {
        put("font", f.clone());
    }
    if let Some(w) = &c.weight {
        put("weight", w.clone());
    }
    if let (Some(entry), Some(w)) = (k.show_entry(), &c.show) {
        set_project(doc, entry.key, w.clone(), ops);
    }
}

/// One project-level `SetStyle` per style whose record changes.
fn style_ops(doc: &Document, style: &StyleFields, ops: &mut Vec<Op>) {
    for (id, fields) in style {
        let before = ecad_model::objects::styles::record(doc, StyleLayer::Project, ecad_model::StyleCatalogue::Text, id).cloned();
        let Ok(rec) = crate::styles::with_fields(ecad_model::StyleCatalogue::Text, before.as_ref(), None, fields) else { continue };
        let after = (rec.name.is_some() || !rec.body.is_empty() || !id.is_builtin()).then_some(rec);
        if after != before {
            ops.push(Op::SetStyle { layer: StyleLayer::Project, catalogue: ecad_model::StyleCatalogue::Text, id: id.clone(), before, after });
        }
    }
}

/// The ops of a `textstyle` call: one transaction; empty when everything is
/// already so. A key given twice (two rows of one kind): the last value wins, in one op.
pub(crate) fn textstyle_ops(doc: &Document, c: &TextStyleChanges) -> Vec<Op> {
    let mut ops = Vec::new();
    if let Some(f) = &c.font {
        set_project(doc, defaults::text::FONT.key, f.clone(), &mut ops);
    }
    let mut style = StyleFields::new();
    for k in &c.kinds {
        kind_ops(doc, k, &mut ops, &mut style);
    }
    style_ops(doc, &style, &mut ops);
    let mut seen = BTreeSet::new();
    let mut kept: Vec<Op> = Vec::new();
    for op in ops.into_iter().rev() {
        let key = match &op {
            Op::SetProperty { key, .. } => Some(key.clone()),
            _ => None,
        };
        if key.as_ref().is_none_or(|k| seen.insert(k.clone())) {
            kept.push(op);
        }
    }
    kept.reverse();
    kept
}

// ---------------------------------------------------------------- defaults

/// The scope a `defaults` call resolves at: a symbol, else a device and a page, else the project.
///
/// # Errors
/// A representation, component or page the project does not have.
pub fn scope(doc: &Document, rep: Option<RepresentationId>, component: Option<DeviceId>, page: Option<PageId>) -> Result<Scope, CmdError> {
    if let Some(r) = rep {
        if !doc.representations().contains_key(&r) {
            return Err(CmdError::refused(format!("No representation {}.", r.hex())));
        }
        return Ok(Scope::symbol(doc, r));
    }
    let mut at = Scope::project();
    if let Some(c) = component {
        if !doc.devices().contains_key(&c) {
            return Err(CmdError::refused(format!("No component {}.", c.hex())));
        }
        at.component = Some(c);
    }
    if let Some(pg) = page {
        if !doc.pages().contains_key(&pg) {
            return Err(CmdError::refused(format!("No page {}.", pg.hex())));
        }
        at.page = Some(pg);
    }
    Ok(at)
}

/// Whether `key` names a default: a registry entry or a project symbol's field.
///
/// # Errors
/// No default has that key.
pub fn default_key(doc: &Document, key: &str) -> Result<(), CmdError> {
    if defaults::find(key).is_none() && defaults::symbols::family(doc, key).is_none() {
        return Err(CmdError::refused(format!("No default \"{key}\".")));
    }
    Ok(())
}

// ---------------------------------------------------------------- showtext

/// The level a `showtext` call writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowLevel {
    Symbol(RepresentationId, DeviceId),
    Device(DeviceId),
    Wire(WireId),
    Page(PageId),
    Project,
}

impl ShowLevel {
    /// The word `showtext` returns.
    pub fn word(self) -> &'static str {
        match self {
            ShowLevel::Symbol(..) => "symbol",
            ShowLevel::Device(_) => "device",
            ShowLevel::Wire(_) => "conductor",
            ShowLevel::Page(_) => "page",
            ShowLevel::Project => "project",
        }
    }
}

/// What a `showtext` call names: at most one of them; none is the project.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShowTarget {
    pub representation: Option<RepresentationId>,
    pub component: Option<DeviceId>,
    pub conductor: Option<WireId>,
    pub page: Option<PageId>,
}

/// The kinds `showtext` sets at a level.
fn kinds_at(level: ShowLevel) -> &'static [TextKind] {
    use TextKind::*;
    match level {
        ShowLevel::Symbol(..) | ShowLevel::Device(_) => &[Tag, PointLabel, Description, PartNumber, ContinuationName],
        ShowLevel::Wire(_) => &[WireLabel],
        ShowLevel::Page(_) | ShowLevel::Project => &[Tag, PointLabel, Description, PartNumber, ContinuationName, Reference, LocationLabel, WireLabel],
    }
}

/// The object and key a visibility at `level` is stored under.
pub(crate) fn show_key(level: ShowLevel, kind: TextKind) -> Option<(ObjectRef, String)> {
    let entry = kind.show_entry()?;
    Some(match level {
        ShowLevel::Symbol(rep, c) => (ObjectRef::Device(c), defaults::symbol_key(entry.key, rep)),
        ShowLevel::Device(c) => (ObjectRef::Device(c), entry.key.to_string()),
        ShowLevel::Wire(c) => (ObjectRef::Wire(c), entry.key.to_string()),
        ShowLevel::Page(p) => (ObjectRef::Page(p), entry.key.to_string()),
        ShowLevel::Project => (ObjectRef::Project, entry.key.to_string()),
    })
}

/// The `show` of a `showtext` call as stored (`yes`, `no` or `first`): any word
/// [`Shown::from_input`] reads, or `None` to inherit.
///
/// # Errors
/// Any other word.
pub fn show_word(show: Option<&str>) -> Result<Option<String>, CmdError> {
    show.map(|w| Shown::from_input(w).map(|s| s.word().to_string()).ok_or(CmdError::refused(BAD_SHOW))).transpose()
}

/// The level a `showtext` call names, checked against `doc`.
///
/// # Errors
/// More than one target named, or one the project does not have.
pub fn show_level(doc: &Document, t: &ShowTarget) -> Result<ShowLevel, CmdError> {
    if [t.representation.is_some(), t.component.is_some(), t.conductor.is_some(), t.page.is_some()].iter().filter(|x| **x).count() > 1 {
        return Err(CmdError::input("Name at most one of representation, component, conductor and page."));
    }
    if let Some(r) = t.representation {
        let x = doc.representations().get(&r).ok_or_else(|| CmdError::refused(format!("No representation {}.", r.hex())))?;
        return match x.subject {
            Subject::Device(c) if x.form == Form::Symbol => Ok(ShowLevel::Symbol(r, c)),
            _ => Err(CmdError::refused(NOT_A_SYMBOL)),
        };
    }
    if let Some(c) = t.component {
        return doc.devices().contains_key(&c).then_some(ShowLevel::Device(c)).ok_or_else(|| CmdError::refused(format!("No component {}.", c.hex())));
    }
    if let Some(c) = t.conductor {
        return doc.wires().contains_key(&c).then_some(ShowLevel::Wire(c)).ok_or_else(|| CmdError::refused(format!("No conductor {}.", c.hex())));
    }
    if let Some(p) = t.page {
        return doc.pages().contains_key(&p).then_some(ShowLevel::Page(p)).ok_or_else(|| CmdError::refused(format!("No page {}.", p.hex())));
    }
    Ok(ShowLevel::Project)
}

/// The ops of one `showtext` change: `[]` when it is already so.
///
/// # Errors
/// A kind not set at that level, `first` for a kind other than the tag, or a target gone.
pub fn show_ops(doc: &Document, level: ShowLevel, kind: TextKind, show: Option<&str>) -> Result<Vec<Op>, CmdError> {
    let allowed = kinds_at(level);
    if !allowed.contains(&kind) {
        let words: Vec<&str> = allowed.iter().map(|k| k.word()).collect();
        return Err(CmdError::refused(format!("At {} level the kinds are {}.", level.word(), words.join(", "))));
    }
    if show.and_then(Shown::from_input) == Some(Shown::First) && kind != TextKind::Tag {
        return Err(CmdError::refused(BAD_SHOW));
    }
    let (object, key) = show_key(level, kind).ok_or(CmdError::refused(BAD_SHOW))?;
    let props = match object {
        ObjectRef::Device(c) => doc.devices().get(&c).map(|x| &x.properties),
        ObjectRef::Wire(c) => doc.wires().get(&c).map(|x| &x.display),
        ObjectRef::Page(p) => doc.pages().get(&p).map(|x| &x.display),
        ObjectRef::Project => Some(doc.display()),
        ObjectRef::Cable(_) | ObjectRef::Standard => None,
    }
    .ok_or(CmdError::NotFound(Noun::Object))?;
    let before = props.get(&key).cloned();
    let after = show.map(str::to_string);
    Ok(if before == after { Vec::new() } else { vec![Op::SetProperty { object, key, before, after }] })
}

/// The transaction label of a `showtext` change (`Hide tag -X01`, `Show tag`, `Reset tag`).
pub fn show_label(doc: &Document, level: ShowLevel, kind: TextKind, show: Option<&str>) -> String {
    let verb = match show.map(Shown::from_input) {
        Some(Some(Shown::Off)) => "Hide",
        Some(_) => "Show",
        None => "Reset",
    };
    let of = match level {
        ShowLevel::Symbol(_, c) | ShowLevel::Device(c) => format!(" {}", devices::tags::tag_words(doc, c)),
        _ => String::new(),
    };
    format!("{verb} {}{of}", kind_words(kind))
}

// ---------------------------------------------------------------- pagetexts

/// The page `pagetexts` lists: the one given, else the one shown.
///
/// # Errors
/// Neither given nor shown, or a page the project does not have.
pub(crate) fn text_page(doc: &Document, page: Option<PageId>, shown: Option<PageId>) -> Result<PageId, CmdError> {
    let page = page.or(shown).ok_or(CmdError::input("`page` (hex id) is required: no page is shown"))?;
    if !doc.pages().contains_key(&page) {
        return Err(CmdError::refused(format!("No page {}.", page.hex())));
    }
    Ok(page)
}

// ---------------------------------------------------------------- movetext

/// The justification a `movetext` call gives (`"RT"`…), read.
///
/// # Errors
/// Not two letters L, C or R, then T, M or B.
pub fn justify(code: Option<&str>) -> Result<Option<(TextHAlign, TextVAlign)>, CmdError> {
    code.map(|c| text_style::parse_justify(c).ok_or(CmdError::refused(BAD_JUSTIFY))).transpose()
}

/// A movable derived text: a symbol's tag block, or one of its connection point labels.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MoveTarget {
    /// The symbol's device.
    pub component: DeviceId,
    /// The device property its place is stored under.
    pub key: String,
    /// Which text of the symbol it is.
    pub of: TextOf,
    /// The symbol's page.
    pub page: PageId,
}

/// The text of symbol `rep` that `kind` (and `connection_point`, for a label) names.
///
/// # Errors
/// No such representation, not a symbol, no such connection point on it, or a kind that does
/// not move.
pub(crate) fn move_target(
    doc: &Document, rep: RepresentationId, kind: TextKind, connection_point: Option<ConnectionPointId>,
) -> Result<MoveTarget, CmdError> {
    let r = doc.representations().get(&rep).ok_or_else(|| CmdError::refused(format!("No representation {}.", rep.hex())))?;
    let c = match r.subject {
        Subject::Device(c) if r.form == Form::Symbol => c,
        _ => return Err(CmdError::refused(NOT_A_SYMBOL)),
    };
    let (key, of) = match kind {
        TextKind::Tag => (format!("{}tag.at", text_style::symbol_prefix(rep)), TextOf::Tag),
        TextKind::PointLabel => {
            let t = connection_point.ok_or(CmdError::refused(NEEDS_CONNECTION_POINT))?;
            if doc.connection_points().get(&t).is_none_or(|x| x.device() != Some(c)) {
                return Err(CmdError::refused(format!("No connection point {} on that symbol.", t.hex())));
            }
            (devices::texts::point_label_key(doc, rep, t), TextOf::ConnectionPoint(t))
        }
        _ => return Err(CmdError::refused("Only a tag or a connection point label can be moved.")),
    };
    Ok(MoveTarget { component: c, key, of, page: r.page })
}

/// What a `movetext` call records.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MovePlan {
    /// The op, or `None` when the place is already so.
    pub op: Option<Op>,
    /// "Move tag -K1", "Reset label".
    pub label: String,
    /// The stored place after it (`None`: the computed place).
    pub after: Option<String>,
}

/// Move text `t` of symbol `rep` to page point `to` (`None`: back to its computed place), with
/// `justify`, else the text's current justification `current`, else left top.
///
/// # Errors
/// The symbol cannot be drawn, or `to` has no place in the symbol's frame.
pub(crate) fn plan_move(
    doc: &Document, rep: RepresentationId, kind: TextKind, t: &MoveTarget, to: Option<Pt>, justify: Option<(TextHAlign, TextVAlign)>,
    current: Option<(TextHAlign, TextVAlign)>,
) -> Result<MovePlan, CmdError> {
    let r = doc.representations().get(&rep).ok_or(CmdError::NotFound(Noun::Representation))?;
    let (_, pl) = devices::tags::rep_definition(doc, r).ok_or(CmdError::refused("That symbol cannot be drawn."))?;
    let after = match to {
        None => None,
        Some(to) => {
            let (h, v) = justify.or(current).unwrap_or((TextHAlign::Left, TextVAlign::Top));
            let local = text_style::to_local(&pl, to).ok_or(CmdError::refused(BAD_POINT))?;
            Some(text_style::at_text(local, h, v))
        }
    };
    let c = t.component;
    let before = doc.devices().get(&c).and_then(|x| x.properties.get(&t.key)).cloned();
    let what = match kind {
        TextKind::Tag => format!("tag {}", devices::tags::tag_words(doc, c)),
        _ => "label".to_string(),
    };
    let label = if after.is_some() { format!("Move {what}") } else { format!("Reset {what}") };
    let op = (before != after).then(|| Op::SetProperty { object: ObjectRef::Device(c), key: t.key.clone(), before, after: after.clone() });
    Ok(MovePlan { op, label, after })
}

/// The style table as `textstyle` returns it: `{font, drawn_with, status, height_is, kinds: [...],
/// changed}`. Heights are cap heights: CADCraft draws a text's height
/// as the height of its capitals, so `height` 2.5 is 2.5 mm from baseline to the top of a `K`.
/// `fonts` is the project's font resolved on this machine.
pub fn textstyle_json(doc: &Document, fonts: &crate::styles::text_style::fonts::Fonts, changed: bool) -> Value {
    let kinds: Vec<Value> = TextKind::ALL
        .iter()
        .map(|k| {
            let id = crate::styles::text_style::style_of(doc, *k, Scope::project());
            let f = |key: &str| crate::styles::field_of(doc, ecad_model::StyleCatalogue::Text, &id, key, None);
            let h = f("height").unwrap_or(Resolved { value: defaults::Value::Length(2.5), from: Source::Builtin });
            let w = f("weight").unwrap_or(Resolved { value: defaults::Value::Choice("regular"), from: Source::Builtin });
            let colour = f("colour").map(|r| r.text());
            let sh = k.show_entry().map(|e| defaults::resolve(doc, e, Scope::project()));
            let default = h.from == Source::Builtin && w.from == Source::Builtin && sh.is_none_or(|r| r.from == Source::Builtin);
            let builtin = |e: &Entry| Resolved { value: e.builtin(), from: Source::Builtin };
            let cat = crate::styles::catalogue(ecad_model::StyleCatalogue::Text);
            let b = |key: &str| cat.builtin(&k.builtin_style()).and_then(|s| s.value(key));
            json!({
                "kind": k.word(),
                "style": id.text(),
                "height": h.number(),
                "weight": w.word(),
                "colour": colour,
                "font": crate::styles::text_style::kind_font(doc, *k),
                "show": sh.map(|r| r.word()),
                "default": default,
                "builtin": {
                    "height": b("height").map_or(0.0, |v| Resolved { value: v, from: Source::Builtin }.number()),
                    "weight": b("weight").map_or("regular", |v| Resolved { value: v, from: Source::Builtin }.word()),
                    "show": k.show_entry().map(|e| builtin(e).word()),
                },
            })
        })
        .collect();
    json!({
        "font": fonts.wanted,
        "drawn_with": fonts.drawn_with,
        "status": fonts.status(),
        "height_is": "cap height",
        "kinds": kinds,
        "changed": changed,
    })
}

/// The JSON form of a registry value.
pub(crate) fn value_json(r: &Resolved<'_>) -> Value {
    match r.value {
        defaults::Value::Length(x) | defaults::Value::Number(x) => json!(x),
        defaults::Value::Choice(w) => json!(w),
        defaults::Value::Flag(b) => json!(ecad_model::naming::words::flag_word(b)),
        defaults::Value::Text(t) => json!(t),
        defaults::Value::OwnStyle(id) => json!(ecad_model::StyleRef::Own(id).text()),
        defaults::Value::Size(s) => json!(s.to_string()),
        defaults::Value::Colours(c) => json!(c.to_string()),
        defaults::Value::Colour(c) => json!(c.to_string()),
        defaults::Value::Sheet(s) => json!(s.text()),
        defaults::Value::Margins(m) => json!(ecad_model::document::sheet::margins_text(&m)),
        defaults::Value::CoreColours(c) => json!(c.to_string()),
    }
}

pub fn level_word(l: Level) -> &'static str {
    match l {
        Level::Standard => "standard",
        Level::Project => "project",
        Level::Page => "page",
        Level::Object => "object",
    }
}

/// One registry entry as `defaults` lists it, resolved at `at`.
pub(crate) fn entry_json(doc: &Document, e: &Entry, at: Scope) -> Value {
    let r = defaults::resolve(doc, e, at);
    let builtin = Resolved { value: e.builtin(), from: Source::Builtin };
    let mut m = Map::new();
    m.insert("key".into(), json!(e.key));
    m.insert("label".into(), json!(e.label));
    m.insert("group".into(), json!(e.group));
    m.insert("brief".into(), json!(e.brief));
    m.insert("kind".into(), json!(e.kind_word()));
    m.insert("builtin".into(), value_json(&builtin));
    m.insert("levels".into(), json!(e.levels.iter().map(|l| level_word(*l)).collect::<Vec<_>>()));
    m.insert("value".into(), value_json(&r));
    m.insert("from".into(), json!(r.from.as_str()));
    Value::Object(m)
}

// ---------------------------------------------------------------- the commands

/// One kind's row of `kinds`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct KindRowParams {
    /// The kind: tag, point_label, …
    #[serde(default)]
    kind: Option<String>,
    /// The kind's own font, or null for the project font.
    #[serde(default, deserialize_with = "nullable")]
    font: Change<String>,
    /// The cap height, mm (0.5 to 10), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    height: Change<f64>,
    /// "regular" or "bold", or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    weight: Change<String>,
    /// "on", "off" or "first", or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    show: Change<String>,
}

impl From<KindRowParams> for KindRow {
    fn from(r: KindRowParams) -> KindRow {
        KindRow { kind: r.kind, font: r.font, height: r.height, weight: r.weight, show: r.show }
    }
}

/// `textstyle {font?, kind?, height?, weight?, show?, kinds?: [{kind, height?, weight?, show?}]}`
/// (`kinds` lets the Text section write several kinds as one transaction). No
/// field reads the table.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextstyleParams {
    /// The project font (with `kind`: that kind's own font), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    font: Change<String>,
    /// The kind `height`, `weight` and `show` change: tag, point_label, …
    #[serde(default, deserialize_with = "nullable")]
    kind: Change<String>,
    /// The kind's cap height, mm (0.5 to 10), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    height: Change<f64>,
    /// "regular" or "bold", or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    weight: Change<String>,
    /// "on", "off" or "first" (tag only), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    show: Change<String>,
    /// More kinds' rows, in the same transaction.
    #[serde(default, deserialize_with = "nullable")]
    kinds: Option<Option<Bounded<KindRowParams, 0, { MAX_KINDS }>>>,
}

impl Generic for TextstyleParams {
    const ID: &'static str = "textstyle";
    const TITLE: &'static str = "Text Styles";
    const HELP: &'static str = "{font?: string|null, kind?: word, height?: number|null (cap height, mm, 0.5-10), weight?: \"regular\"|\"bold\"|null, \
show?: \"on\"|\"off\"|\"first\"|null} (no fields: read only; all given fields are one transaction; null: back to the default)";
    const PAGES: Pages = Pages::ReadAnywhere;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let given =
            [self.font.is_some(), self.kind.is_some(), self.height.is_some(), self.weight.is_some(), self.show.is_some(), self.kinds.is_some()];
        let metrics = cx.metrics;
        if !given.contains(&true) {
            let doc = cx.doc;
            return Ok(Outcome::answer(textstyle_json(doc, &Fonts::resolve_with(doc, metrics), false)));
        }
        let input = TextStyleInput {
            font: self.font,
            row: KindRow { kind: self.kind.flatten(), font: None, height: self.height, weight: self.weight, show: self.show },
            kinds: self.kinds.flatten().map(|b| b.0).unwrap_or_default().into_iter().map(KindRow::from).collect(),
        };
        let changes = textstyle_changes(input)?;
        let ops = textstyle_ops(cx.doc, &changes);
        let changed = !ops.is_empty();
        Ok(Outcome::edit_then("Text styles", ops, move |doc| textstyle_json(doc, &Fonts::resolve_with(doc, metrics), changed)))
    }
}

/// Most rows `pagetexts` returns.
pub(crate) const MAX_TEXT_ROWS: usize = 5_000;

/// `pagetexts {page?}` (named so because `texts` is CADCraft's): every derived
/// text on a page, hidden ones too.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PagetextsParams {
    /// The page (default: the page shown).
    #[serde(default)]
    page: Option<PageId>,
}

impl Generic for PagetextsParams {
    const ID: &'static str = "pagetexts";
    const TITLE: &'static str = "Page Texts";
    const HELP: &'static str = "{page?: id} (every derived text on the page, hidden ones too; read only)";
    const PAGES: Pages = Pages::ReadAnywhere;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let page = text_page(cx.doc, self.page, cx.page)?;
        Ok(Outcome::answer(pagetexts_json(cx.doc, page, cx.metrics)))
    }
}

/// The rows of `pagetexts` for `page`, measured with `metrics`.
pub(crate) fn pagetexts_json(doc: &Document, page: PageId, metrics: &dyn TextMetrics) -> Value {
    use crate::styles::text_style::fonts::{DrawStyles, width_in};
    use crate::styles::text_style::{TextStyle, Weight, justify_code, text_box};
    let draw = DrawStyles::with(doc, metrics);
    let all = crate::devices::texts::all_texts_measured(doc, page, None, metrics);
    let more = all.len().saturating_sub(MAX_TEXT_ROWS);
    let rows: Vec<Value> = all
        .iter()
        .take(MAX_TEXT_ROWS)
        .map(|t| {
            let d = &t.text;
            let kind = crate::devices::texts::kind_of(d.of);
            let style = draw.style(kind);
            let w = width_in(draw.fonts_for(kind), TextStyle { height: d.height, ..style }, &d.value, metrics);
            let (lo, hi) = text_box(w, d.height, d.at, d.halign, d.valign);
            let component = doc.representations().get(&d.rep).and_then(|r| match r.subject {
                Subject::Device(c) => Some(c.hex()),
                _ => None,
            });
            let weight = match style.weight {
                Weight::Bold => "bold",
                Weight::Regular => "regular",
            };
            let mut row = json!({
                "representation": d.rep.hex(),
                "component": component,
                "kind": kind.word(),
                "value": d.value,
                "at": [d.at.x, d.at.y],
                "justify": justify_code(d.halign, d.valign),
                "height": d.height,
                "weight": weight,
                "shown": t.shown,
                "moved": t.moved,
                "overlaps": t.overlaps,
                "box": [[lo.x, lo.y], [hi.x, hi.y]],
            });
            if let TextOf::ConnectionPoint(id) = d.of {
                row["connection_point"] = json!(id.hex());
            }
            row
        })
        .collect();
    let mut v = json!({ "texts": rows });
    if more > 0 {
        v["more"] = json!(more);
    }
    v
}

/// `movetext {representation, kind?, connection_point?, to, justify?}` or `{…, reset: true}`:
/// a symbol's tag or connection point label moved to a page point, or back to its
/// computed place.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MovetextParams {
    /// The symbol.
    representation: RepresentationId,
    /// "tag" (default) or "point_label".
    #[serde(default)]
    kind: Option<String>,
    /// The connection point whose label moves (point_label).
    #[serde(default)]
    connection_point: Option<ConnectionPointId>,
    /// The page point the text moves to.
    #[serde(default)]
    to: Option<Point>,
    /// The text's justification: "RT", "LB", … (default: as it is).
    #[serde(default)]
    justify: Option<String>,
    /// Back to the computed place (`to` is not read).
    #[serde(default)]
    reset: Option<bool>,
}

impl Generic for MovetextParams {
    const ID: &'static str = "movetext";
    const TITLE: &'static str = "Move Text";
    const HELP: &'static str = "{representation: id, kind?: \"tag\"|\"point_label\", connection_point?: id, to: [x,y], justify?: \"RT\"…} or {representation, kind?, connection_point?, reset: true} \
(to is a page point; stored in the symbol's frame)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let rep = self.representation;
        let kind = opt_kind(self.kind.as_deref())?.unwrap_or(TextKind::Tag);
        let connection_point = self.connection_point;
        let to = match self.to {
            _ if self.reset == Some(true) => None,
            Some(p) => Some(p.0),
            None => return Err(CmdError::refused(BAD_POINT)),
        };
        let justify = justify(self.justify.as_deref())?;
        let metrics = cx.metrics;
        let doc = cx.doc;
        let t = move_target(doc, rep, kind, connection_point)?;
        let current = current_text(doc, rep, &t, metrics).map(|x| (x.text.halign, x.text.valign));
        let plan = plan_move(doc, rep, kind, &t, to, justify, current)?;
        let changed = plan.op.is_some();
        let placed = plan.after.is_some();
        Ok(Outcome::edit_then(plan.label, plan.op.into_iter().collect(), move |doc| {
            let now = move_target(doc, rep, kind, connection_point).ok().and_then(|t| current_text(doc, rep, &t, metrics));
            json!({
                "representation": rep.hex(),
                "kind": kind.word(),
                "at": if placed { now.as_ref().map(|t| json!([t.text.at.x, t.text.at.y])) } else { None },
                "justify": now.as_ref().map(|t| crate::styles::text_style::justify_code(t.text.halign, t.text.valign)),
                "changed": changed,
            })
        }))
    }
}

/// The text `t` names as drawn now.
fn current_text(doc: &Document, rep: RepresentationId, t: &MoveTarget, metrics: &dyn TextMetrics) -> Option<crate::devices::texts::PageText> {
    crate::devices::texts::all_texts_measured(doc, t.page, None, metrics).into_iter().find(|x| x.text.rep == rep && x.text.of == t.of)
}
