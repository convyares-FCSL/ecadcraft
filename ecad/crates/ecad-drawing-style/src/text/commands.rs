//! The look's text commands: the text style table (`textstyle`: the project font, and each
//! kind's font, height, weight and Shown setting) and which texts are shown at which level
//! (`showtext`'s rules; the command needs the session and is the engine's). Each function reads
//! the document and returns the ops to commit and what to report.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::look::{Shown, TextKind};
use ecad_model::registry::{Entry, Resolved, Scope, Source};
use ecad_model::{DeviceId, Document, Form, Noun, ObjectRef, Op, PageId, RepresentationId, StyleLayer, StyleRef, Subject, WireId, Word};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use ecad_electrical::CmdError;
use ecad_electrical::commands::input::Said;
use ecad_electrical::commands::input::{Bounded, nullable};
use ecad_electrical::commands::run::{Cx, Generic, Outcome, Pages};
use ecad_electrical::styles::commands::text::{BAD_HEIGHT, NOT_A_SYMBOL};
use ecad_model::objects::styles::text::Weight;

use crate::defaults;
use crate::text::fonts::Fonts;

/// Refusals.
pub(crate) const BAD_SHOW: &str = "Shown is yes, no or first (first: tag only).";
pub const BAD_FONT: &str = "A font name is 1 to 64 characters with no control characters.";
pub const NEEDS_KIND: &str = "height, weight and show need a kind.";

/// Most kinds one `textstyle` call changes (the style table has fewer; a guard on hostile input).
pub(crate) const MAX_KINDS: usize = 64;

/// A field of a call: `None` leaves it, `Some(None)` goes back to the default, `Some(Some(v))`
/// sets it.
pub type Change<T> = Option<Option<T>>;

// ---------------------------------------------------------------- textstyle

/// One kind's row of a `textstyle` call, as given.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KindRow {
    /// The kind (needed when the row changes anything).
    pub kind: Option<TextKind>,
    /// The kind's own font.
    pub font: Change<String>,
    /// The cap height, mm.
    pub height: Change<f64>,
    /// Regular or bold.
    pub weight: Change<Weight>,
    /// Shown, hidden or once per run.
    pub show: Change<Shown>,
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
    weight: Change<Weight>,
    show: Change<Shown>,
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

/// The kind changes of `row`; `None` when it names no kind and changes nothing.
fn kind_change(row: KindRow) -> Result<Option<KindChange>, CmdError> {
    let kind = row.kind;
    let height = check(row.height, |h| (h.is_finite() && (0.5..=10.0).contains(&h)).then_some(h), BAD_HEIGHT)?;
    let (weight, show) = (row.weight, row.show);
    let changes = height.is_some() || weight.is_some() || show.is_some();
    let Some(kind) = kind else {
        return if changes { Err(CmdError::refused(NEEDS_KIND)) } else { Ok(None) };
    };
    // With a kind, `font` is that kind's own font.
    let font = check(row.font, |f| Some(f.trim().to_string()), BAD_FONT)?;
    if let Some(Some(f)) = &font
        && ecad_model::registry::parse_value(ecad_model::registry::Kind::Text { max_chars: 64 }, f).is_none()
    {
        return Err(CmdError::refused(BAD_FONT));
    }
    if let Some(Some(w)) = &show {
        let entry = crate::text::show_entry(kind).ok_or_else(|| CmdError::refused(format!("{} has no Shown setting.", kind.word())))?;
        if entry.parse(w.word()).is_none() {
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
    let id = crate::text::style_of(doc, k, Scope::project());
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
    if let Some(w) = c.weight {
        put("weight", w.map(|w| w.word().to_string()));
    }
    if let (Some(entry), Some(w)) = (crate::text::show_entry(k), c.show) {
        set_project(doc, entry.key, w.map(|w| w.word().to_string()), ops);
    }
}

/// One project-level `SetStyle` per style whose record changes.
fn style_ops(doc: &Document, style: &StyleFields, ops: &mut Vec<Op>) {
    for (id, fields) in style {
        let before = ecad_model::objects::styles::record(doc, StyleLayer::Project, ecad_model::StyleCatalogue::Text, id).cloned();
        let Ok(rec) = ecad_electrical::styles::with_fields(&crate::catalogues::text::CATALOGUE, before.as_ref(), None, fields) else { continue };
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
    let entry = crate::text::show_entry(kind)?;
    Some(match level {
        ShowLevel::Symbol(rep, c) => (ObjectRef::Device(c), ecad_model::registry::symbol_key(entry.key, rep)),
        ShowLevel::Device(c) => (ObjectRef::Device(c), entry.key.to_string()),
        ShowLevel::Wire(c) => (ObjectRef::Wire(c), entry.key.to_string()),
        ShowLevel::Page(p) => (ObjectRef::Page(p), entry.key.to_string()),
        ShowLevel::Project => (ObjectRef::Project, entry.key.to_string()),
    })
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
        ShowLevel::Symbol(_, c) | ShowLevel::Device(c) => format!(" {}", ecad_electrical::devices::tags::tag_words(doc, c)),
        _ => String::new(),
    };
    format!("{verb} {}{of}", kind.words())
}

/// The style table as `textstyle` returns it: `{font, drawn_with, status, height_is, kinds: [...],
/// changed}`. Heights are cap heights: CADCraft draws a text's height
/// as the height of its capitals, so `height` 2.5 is 2.5 mm from baseline to the top of a `K`.
/// `fonts` is the project's font resolved on this machine.
pub fn textstyle_json(doc: &Document, fonts: &crate::text::fonts::Fonts, changed: bool) -> Value {
    let kinds: Vec<Value> = TextKind::ALL
        .iter()
        .map(|k| {
            let id = crate::text::style_of(doc, *k, Scope::project());
            let f = |key: &str| ecad_model::registry::styles::field_of(doc, &crate::catalogues::text::CATALOGUE, &id, key, None);
            let h = f("height").unwrap_or(Resolved { value: ecad_model::registry::Value::Length(2.5), from: Source::Builtin });
            let w = f("weight").unwrap_or(Resolved { value: ecad_model::registry::Value::Choice("regular"), from: Source::Builtin });
            let colour = f("colour").map(|r| r.text());
            let sh = crate::text::show_entry(*k).map(|e| ecad_model::registry::resolve(doc, e, Scope::project()));
            let default = h.from == Source::Builtin && w.from == Source::Builtin && sh.is_none_or(|r| r.from == Source::Builtin);
            let builtin = |e: &Entry| Resolved { value: e.builtin(), from: Source::Builtin };
            let cat = &crate::catalogues::text::CATALOGUE;
            let b = |key: &str| cat.builtin(&k.builtin_style()).and_then(|s| s.value(key));
            json!({
                "kind": k.word(),
                "style": id.text(),
                "height": h.number(),
                "weight": w.word(),
                "colour": colour,
                "font": crate::text::kind_font(doc, *k),
                "show": sh.map(|r| r.word()),
                "default": default,
                "builtin": {
                    "height": b("height").map_or(0.0, |v| Resolved { value: v, from: Source::Builtin }.number()),
                    "weight": b("weight").map_or("regular", |v| Resolved { value: v, from: Source::Builtin }.word()),
                    "show": crate::text::show_entry(*k).map(|e| builtin(e).word()),
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

/// One kind's row of `kinds`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct KindRowParams {
    /// The kind.
    #[serde(default)]
    kind: Option<TextKind>,
    /// The kind's own font, or null for the project font.
    #[serde(default, deserialize_with = "nullable")]
    font: Change<String>,
    /// The cap height, mm (0.5 to 10), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    height: Change<f64>,
    /// Regular or bold, or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    weight: Change<Weight>,
    /// Shown (yes), hidden (no) or once per run (first), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    show: Change<Said<Shown>>,
}

/// A change of a [`Said`] value: the value.
fn said<T>(c: Change<Said<T>>) -> Change<T> {
    c.map(|v| v.map(|s| s.0))
}

impl From<KindRowParams> for KindRow {
    fn from(r: KindRowParams) -> KindRow {
        KindRow { kind: r.kind, font: r.font, height: r.height, weight: r.weight, show: said(r.show) }
    }
}

/// Reads or sets the project's text styles: the font, and per kind of text its height, weight and whether it
/// shows. No fields reads the table; all given fields are one transaction; null goes back to the default. `kinds`
/// sets several kinds at once.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextstyleParams {
    /// The project font (with `kind`: that kind's own font), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    font: Change<String>,
    /// The kind `height`, `weight` and `show` change.
    #[serde(default, deserialize_with = "nullable")]
    kind: Change<TextKind>,
    /// The kind's cap height, mm (0.5 to 10), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    height: Change<f64>,
    /// Regular or bold, or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    weight: Change<Weight>,
    /// Shown (yes), hidden (no) or once per run (first, tag only), or null for the default.
    #[serde(default, deserialize_with = "nullable")]
    show: Change<Said<Shown>>,
    /// More kinds' rows, in the same transaction.
    #[serde(default, deserialize_with = "nullable")]
    kinds: Option<Option<Bounded<KindRowParams, 0, { MAX_KINDS }>>>,
}

impl Generic for TextstyleParams {
    const ID: &'static str = "textstyle";
    const TITLE: &'static str = "Text Styles";
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
            row: KindRow { kind: self.kind.flatten(), font: None, height: self.height, weight: self.weight, show: said(self.show) },
            kinds: self.kinds.flatten().map(|b| b.0).unwrap_or_default().into_iter().map(KindRow::from).collect(),
        };
        let changes = textstyle_changes(input)?;
        let ops = textstyle_ops(cx.doc, &changes);
        let changed = !ops.is_empty();
        Ok(Outcome::edit_then("Text styles", ops, move |doc| textstyle_json(doc, &Fonts::resolve_with(doc, metrics), changed)))
    }
}
