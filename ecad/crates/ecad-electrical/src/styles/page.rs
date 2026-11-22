//! The page type catalogue (`page`, brief 054, design 054 §2, FP1).
//!
//! A page type says what a page is: Front sheet, Schematic, Network, Single-line, Panel layout,
//! Report, or a project's own. It brings the page's sheet, frame style, title block and zones, and
//! says which generated kinds its pages may be (the hidden field `holds`: Cover sheet holds the
//! cover, revision history and contents, Report the report kinds; an own type holds what the type
//! it was copied from holds). A type never decides where a page goes or how it is numbered (owner,
//! Q1 and Q13), and it has no position (owner, fix round: "if I don't know what it does, a user
//! wouldn't").
//!
//! # Semantics
//! - A page's type ([`type_of`]) is its own `page.type` when that names a type that exists, else
//!   from its generated kind, else `schematic` (design 054 §2.3). `ecad_model::sheet` holds the
//!   rule, so the sheet the model resolves and the type shown here always agree (FP3).
//! - Every built-in takes the project's sheet and frame, so an old project looks the same; only
//!   the front and report types (and Panel layout) draw no zones.
//! - A malformed field value reads as absent and falls through (050 §2.4); Checks reports it.
//!
//! This module draws nothing and records nothing.

use ecad_model::sheet::{self as msheet, PROJECT_SHEET, SheetSpec};
use ecad_model::style::PageType;
use ecad_model::style::page::{TypeBlock, TypeFrame, TypeSheet};
use ecad_model::{Document, PageId, StyleRef, Word};

use super::{Builtin, Catalogue, Field};
use crate::CmdError;
use crate::defaults::{Kind, Slot, Value};
use ecad_model::GenKind;

fn is_sheet(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case(PROJECT_SHEET) || SheetSpec::parse(s).is_ok()
}

fn is_frame(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case(PROJECT_SHEET) || StyleRef::parse(s).is_some()
}

/// The value a type's `title_block` holds to draw its frame's own block (brief 055: otherwise a
/// frame style ID whose block the type's pages borrow).
pub const OWN_BLOCK: &str = "frame";

fn is_title_block(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case(OWN_BLOCK) || StyleRef::parse(s).is_some()
}
/// What generated kinds a type holds: none (drawn pages), the front kinds, the report kinds.
pub const HOLDS: &[&str] = &["none", "front", "report"];

static FIELDS: [Field; 6] = [
    Field { key: "sheet", label: "Sheet", kind: Kind::Parsed { max_chars: 64, parse: is_sheet }, slot: Slot::new(), over: None },
    Field { key: "frame", label: "Frame style", kind: Kind::Parsed { max_chars: 40, parse: is_frame }, slot: Slot::new(), over: None },
    Field { key: "title_block", label: "Title block", kind: Kind::Parsed { max_chars: 40, parse: is_title_block }, slot: Slot::new(), over: None },
    Field { key: "zones", label: "Zones", kind: Kind::Flag, slot: Slot::new(), over: None },
    // Design 054 §3.4: off, the type's pages get no electrical checks, cross-references or
    // numbering.
    Field { key: "checks", label: "Checks and references", kind: Kind::Flag, slot: Slot::new(), over: None },
    // Not shown: copied with the type, so an own type holds what its source holds.
    Field { key: "holds", label: "Generated kinds", kind: Kind::Choice(HOLDS), slot: Slot::new(), over: None },
];

static BUILTINS: [Builtin; 7] = [
    Builtin { id: "front", name: "Cover sheet", slot: Slot::new() },
    Builtin { id: "schematic", name: "Schematic", slot: Slot::new() },
    Builtin { id: "network", name: "Network", slot: Slot::new() },
    Builtin { id: "singleline", name: "Single-line", slot: Slot::new() },
    Builtin { id: "panel", name: "Panel layout", slot: Slot::new() },
    Builtin { id: "report", name: "Report", slot: Slot::new() },
    Builtin { id: "plain", name: "Plain", slot: Slot::new() },
];

fn choices() -> Vec<&'static crate::defaults::Entry> {
    vec![&crate::defaults::pages::PAGE_TYPE]
}

/// The page type catalogue.
pub static CATALOGUE: Catalogue = Catalogue {
    catalogue: ecad_model::StyleCatalogue::Page,
    plural: "Page types",
    one: "Page type",
    fields: &FIELDS,
    builtins: &BUILTINS,
    choices,
    project_field: None,
};

/// The built-in Cover sheet type (ID `front`, kept: old files store it).
pub const FRONT: &str = "front";
/// The built-in Schematic type, a page's type when nothing else says.
pub const SCHEMATIC: &str = "schematic";
/// The built-in Panel layout type (+ Page offers it disabled, design 054 Q3).
pub const PANEL: &str = "panel";
/// The built-in Report type.
pub const REPORT: &str = "report";

pub use ecad_model::style::page::Holds;

/// What a type must hold to take pages of `kind`: a cover, revision history or contents is a
/// Front sheet page, every other kind a Report page.
pub fn holds_of_kind(kind: GenKind) -> Holds {
    match kind {
        GenKind::Cover | GenKind::Revisions | GenKind::Contents => Holds::Front,
        _ => Holds::Report,
    }
}

/// What a page type brings to its pages (design 054 §3).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDraw {
    /// `None`: the project's sheet.
    pub sheet: Option<SheetSpec>,
    /// `None`: the `print.frame.style` choice, as before types.
    pub frame: Option<StyleRef>,

    /// `None`: the frame's own block; else the frame style whose block the pages borrow.
    pub title_block: Option<StyleRef>,
    pub zones: bool,
    pub holds: Holds,
    /// Checks and references (design 054 §3.4).
    pub checks: bool,
}

fn field<'a>(doc: &'a Document, id: &StyleRef, key: &str) -> Option<crate::defaults::Resolved<'a>> {
    super::field_of(doc, ecad_model::StyleCatalogue::Page, id, key, None)
}

/// What type `id` brings (design 054 §2.1, §3). Never fails: a malformed field falls through.
pub fn draw_of(doc: &Document, id: &StyleRef) -> TypeDraw {
    let sheet = msheet::type_sheet(doc, id);
    let frame = field(doc, id, "frame")
        .map(|r| r.text())
        .filter(|t| !t.trim().eq_ignore_ascii_case(PROJECT_SHEET))
        .and_then(|t| StyleRef::parse(&t))
        .filter(|f| super::exists(doc, ecad_model::StyleCatalogue::Frame, f));
    let title_block = field(doc, id, "title_block")
        .map(|r| r.text())
        .filter(|t| !t.trim().eq_ignore_ascii_case(OWN_BLOCK))
        .and_then(|t| StyleRef::parse(&t))
        .filter(|f| super::exists(doc, ecad_model::StyleCatalogue::Frame, f));
    let zones = field(doc, id, "zones").is_none_or(|r| ecad_model::words::flag(r.word()) != Some(false));
    let holds = field(doc, id, "holds").and_then(|r| <Holds as Word>::parse(r.word())).unwrap_or(Holds::None);
    let checks = msheet::type_checks(doc, id);
    TypeDraw { sheet, frame, title_block, zones, holds, checks }
}

/// The type of `page` (design 054 §2.3).
pub fn type_of(doc: &Document, page: PageId) -> StyleRef {
    match doc.pages.get(&page) {
        Some(p) => msheet::page_type_of(doc, p),
        None => StyleRef::Builtin(SCHEMATIC.to_string()),
    }
}

/// The type a page of generated kind `kind` (or a drawn page, `None`) has with no `page.type`.
pub fn derived(kind: Option<GenKind>) -> StyleRef {
    let kind = kind.map_or(ecad_model::PageKind::Schematic, ecad_model::PageKind::generated);
    StyleRef::Builtin(msheet::derived_page_type(kind).to_string())
}

/// The frame style `page` is drawn with: `ecad_model::sheet::frame_style`, the one resolver (the
/// page's own, its page type's, the project's, the standard's, the built-in; brief 055).
pub fn frame_style(doc: &Document, page: PageId) -> StyleRef {
    msheet::frame_style(doc, page)
}

/// A built-in's fixed subtitle on the Settings table (design 054 §2.2); own types have none.
pub fn subtitle(id: &StyleRef) -> &'static str {
    match id {
        StyleRef::Builtin(w) => match w.as_str() {
            "front" => "Cover, revisions, contents",
            "network" => "From ports and cables",
            "singleline" => "From the load tree",
            "panel" => "Scale 1:4",
            "report" => "Parts, cables, terminals, PLC",
            "plain" => "Imported drawings and notes",
            _ => "",
        },
        StyleRef::Own(_) => "",
    }
}

/// The generated kinds a type that `holds` them takes, in the picker's order (design 054 §4.1).
pub fn kinds(holds: Holds) -> &'static [GenKind] {
    match holds {
        Holds::Front => &[GenKind::Cover, GenKind::Revisions, GenKind::Contents],
        Holds::Report => &[GenKind::BomLocation, GenKind::Terminals, GenKind::Cables, GenKind::PlcRack, GenKind::IoList],
        Holds::None => &[],
    }
}

/// Whether type `id` holds generated pages only (the built-in Front sheet and Report, design 054
/// §4.2): `newpage {type}` with no `generated` is refused for them.
pub fn generated_only(id: &StyleRef) -> bool {
    matches!(id, StyleRef::Builtin(w) if w == FRONT || w == REPORT)
}

/// The types in the picker's and the table's order: the catalogue's (built-ins in their order,
/// then own types by name).
pub fn ordered(doc: &Document) -> Vec<super::Row> {
    super::rows(doc, &CATALOGUE)
}

/// How many pages have each type, in one pass over the pages (asked on demand: the Settings
/// table and the picker, never per op).
pub fn counts(doc: &Document) -> std::collections::BTreeMap<StyleRef, usize> {
    let mut out = std::collections::BTreeMap::new();
    for p in doc.pages.values() {
        *out.entry(msheet::page_type_of(doc, p)).or_insert(0) += 1;
    }
    out
}

/// The pages of type `id`, in page order.
pub fn pages_of(doc: &Document, id: &StyleRef) -> Vec<PageId> {
    doc.page_order.iter().copied().filter(|p| doc.pages.get(p).is_some_and(|x| msheet::page_type_of(doc, x) == *id)).collect()
}

/// The pages a `SetStyle` of catalogue `page` reaches (design 054 §10.2), worked out on the
/// document before it: those whose `page.type` names the type, and those whose resolved type it
/// is. With whether it changes their sheet (019's per-page sheet path: their grids and the
/// references to items on them). `None` for any other op. One pass over the pages.
pub fn touched_by(doc: &Document, op: &ecad_model::Op) -> Option<(Vec<PageId>, bool)> {
    let ecad_model::Op::SetStyle { catalogue, id, before, after, .. } = op else { return None };
    reach(doc, *catalogue, id, before.as_ref(), after.as_ref())
}

/// [`touched_by`] for a style effect: the pages a record of catalogue `page` set from `before` to
/// `after` reaches, and whether their sheet changes. `None` for any other catalogue.
pub fn touched_by_effect(doc: &Document, effect: &ecad_model::effect::Effect<'_>) -> Option<(Vec<PageId>, bool)> {
    let ecad_model::effect::Effect::Style { catalogue, id, before, after, .. } = effect else { return None };
    reach(doc, *catalogue, id, *before, *after)
}

fn reach(
    doc: &Document, catalogue: ecad_model::StyleCatalogue, id: &ecad_model::StyleRef, before: Option<&ecad_model::StyleRecord>,
    after: Option<&ecad_model::StyleRecord>,
) -> Option<(Vec<PageId>, bool)> {
    if catalogue != ecad_model::StyleCatalogue::Page {
        return None;
    }
    let text = id.text();
    let pages = doc
        .page_order
        .iter()
        .copied()
        .filter(|p| doc.pages.get(p).is_some_and(|x| x.page_type.as_ref().is_some_and(|v| v.text() == text) || msheet::page_type_of(doc, x) == *id))
        .collect();
    let sheet = |r: Option<&ecad_model::StyleRecord>| r.and_then(|r| r.body.page()).and_then(|p| p.sheet);
    // A new or removed record can also make a `page.type` start or stop naming a type.
    let sheet_changed = sheet(before) != sheet(after) || before.is_none() != after.is_none();
    Some((pages, sheet_changed))
}

/// The refusal for a generated `kind` given a type of the wrong position (design 054 §2.3).
pub fn kind_refusal(kind: GenKind) -> String {
    let (one, ty) = match holds_of_kind(kind) {
        Holds::Front => ("Cover sheet", "Cover sheet"),
        _ => ("Report", "Report"),
    };
    format!("A {} page is a {one} page; choose a {ty} type.", kind.title().to_lowercase())
}

/// The refusal for a generated-only type with no `generated` (design 054 §4.2).
pub fn generated_refusal(doc: &Document, id: &StyleRef) -> String {
    let pos = draw_of(doc, id).holds;
    let list: Vec<String> = kinds(pos).iter().map(|k| format!("\"{}\"", ecad_model::Word::word(*k))).collect();
    let words = match list.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    };
    format!("A {} page is generated: give generated: {words}.", super::name(doc, ecad_model::StyleCatalogue::Page, id))
}

/// The value of field `key` that `t` sets, if it sets one.
pub fn get<'a>(t: &'a PageType, key: &str) -> Option<Value<'a>> {
    match key {
        "sheet" => t.sheet.map(|v| match v {
            TypeSheet::Project => Value::Text(PROJECT_SHEET),
            TypeSheet::Sheet(s) => Value::Sheet(s),
        }),
        "frame" => t.frame.as_ref().map(|v| match v {
            TypeFrame::Project => Value::Text(PROJECT_SHEET),
            TypeFrame::Style(id) => Value::of_style(id),
        }),
        "title_block" => t.title_block.as_ref().map(|v| match v {
            TypeBlock::Frame => Value::Text(OWN_BLOCK),
            TypeBlock::Style(id) => Value::of_style(id),
        }),
        "zones" => t.zones.map(Value::Flag),
        "checks" => t.checks.map(Value::Flag),
        "holds" => t.holds.map(|v| Value::Choice(v.word())),
        _ => None,
    }
}

/// Set (`Some`) or clear (`None`) field `key` of `t` from its text, as the `style` command
/// checked it.
///
/// # Errors
/// An unknown field, or text the field does not take, in words.
pub fn set(t: &mut PageType, key: &str, value: Option<&str>) -> Result<(), CmdError> {
    let bad = |what: &str| CmdError::refused(format!("{key} takes {what}, not {:?}.", value.unwrap_or_default()));
    let flag = |v: &str| ecad_model::words::flag(v).ok_or_else(|| bad("yes or no"));
    match (key, value) {
        ("sheet", None) => t.sheet = None,
        ("frame", None) => t.frame = None,
        ("title_block", None) => t.title_block = None,
        ("zones", None) => t.zones = None,
        ("checks", None) => t.checks = None,
        ("holds", None) => t.holds = None,
        ("sheet", Some(v)) => t.sheet = Some(TypeSheet::parse(v).ok_or_else(|| bad("project or a sheet"))?),
        ("frame", Some(v)) => t.frame = Some(TypeFrame::parse(v).ok_or_else(|| bad("project or a frame style"))?),
        ("title_block", Some(v)) => t.title_block = Some(TypeBlock::parse(v).ok_or_else(|| bad("frame or a frame style"))?),
        ("zones", Some(v)) => t.zones = Some(flag(v)?),
        ("checks", Some(v)) => t.checks = Some(flag(v)?),
        ("holds", Some(v)) => t.holds = Some(Word::parse(v).ok_or_else(|| bad(&HOLDS.join(", ")))?),
        _ => return Err(CmdError::refused(format!("Page types have no field {key:?}."))),
    }
    Ok(())
}
