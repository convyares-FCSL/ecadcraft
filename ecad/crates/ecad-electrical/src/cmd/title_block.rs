//! The rules of brief 055's commands (design 055 §9): which frame `titleblock` names, the record a
//! write or a new frame makes, and the checks a body passes. A title block is a `frame` style
//! record (050a) with its block as the record's body (FP1). The block's layout and drawing are
//! [`crate::title_block`]'s; the engine crate's shims lay out built-in blocks with the machine's
//! fonts, commit the ops and answer in JSON.

use std::collections::BTreeSet;

use ecad_model::sheet::{FRAME_CATALOGUE, FRAME_SHEET_FIELD, SheetSpec};
use ecad_model::style::{StyleBody, StyleLayer, StyleRef, is_name};
use ecad_model::title_block::TitleBlockLayout;
use ecad_model::{Document, IdAllocator, Op};

use crate::cmd::style::{FieldInput, STANDARD_FIRST, STYLE_NEEDS_NAME, fields};
use crate::error::CmdError;
use crate::styles;

/// Refusals, in words.
pub const BUILTIN_READ_ONLY: &str = "Built-in. Duplicate it to make your own (`titleblock {new: {from: \"sheet\", sheet}}`).";
pub const NOT_A_FRAME: &str = "Name a frame: `id` or `name` (`titleblocks` lists them).";
/// A blank block's size (mm).
pub const BLANK_SIZE: (f64, f64) = (180.0, 40.0);

/// The frame catalogue.
fn catalogue() -> &'static styles::Catalogue {
    &styles::frame::CATALOGUE
}

/// The frame `id` names, else the one `name` names (trimmed).
///
/// # Errors
/// Neither given, or no such frame.
pub fn frame(doc: &Document, id: Option<&str>, name: Option<&str>) -> Result<StyleRef, CmdError> {
    let t = id.or(name).map(str::trim).ok_or(CmdError::refused(NOT_A_FRAME))?;
    styles::find(doc, FRAME_CATALOGUE, t)
}

/// The sheet `text` names, else the project's.
///
/// # Errors
/// A sheet that does not read.
pub fn sheet(doc: &Document, text: Option<&str>) -> Result<SheetSpec, CmdError> {
    match text.map(str::trim) {
        Some(t) => Ok(SheetSpec::parse(t)?),
        None => Ok(doc.settings.sheet()),
    }
}

/// The layer that holds own frame `id` (the project's when both do).
pub fn layer_of(doc: &Document, id: &StyleRef) -> Option<StyleLayer> {
    [StyleLayer::Project, StyleLayer::Standard].into_iter().find(|l| styles::record(doc, *l, FRAME_CATALOGUE, id).is_some())
}

/// A body, checked against design 055 §1.3.
///
/// # Errors
/// A bound broken (an unknown item kind does not read: design 072 §5.4).
pub fn check_body(l: TitleBlockLayout) -> Result<TitleBlockLayout, CmdError> {
    l.check().map_err(CmdError::refused)?;
    Ok(l)
}

/// The refusal for a frame name in use.
fn name_taken(n: &str) -> CmdError {
    CmdError::refused(format!("A frame style is already called {n:?}; choose another name."))
}

/// A write to an own frame: a new name, fields and a body.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameEdit {
    /// The new name (trimmed); the same name is no rename.
    pub rename: Option<String>,
    pub fields: Vec<(String, FieldInput)>,
    pub body: Option<TitleBlockLayout>,
}

/// What a `titleblock` write records: one `SetStyle` (none when nothing changes).
#[derive(Debug, Clone, PartialEq)]
pub struct EditPlan {
    pub ops: Vec<Op>,
    pub label: String,
}

/// `titleblock {id | name, name? | rename?, fields?, body?}`: own frame `id` with a new name,
/// fields and body, as one `SetStyle`.
///
/// # Errors
/// A built-in frame, no such own frame, a field or name that does not read, a name in use, or a
/// record the catalogue refuses.
pub fn plan_edit(doc: &Document, id: &StyleRef, edit: FrameEdit) -> Result<EditPlan, CmdError> {
    if id.is_builtin() {
        return Err(CmdError::refused(BUILTIN_READ_ONLY));
    }
    let cat = catalogue();
    let layer = layer_of(doc, id).ok_or(CmdError::refused(NOT_A_FRAME))?;
    let before = styles::record(doc, layer, FRAME_CATALOGUE, id).cloned();
    let fields = fields(doc, cat, edit.fields)?;
    let current = styles::name(doc, FRAME_CATALOGUE, id);
    let rename = edit.rename.as_deref().filter(|n| *n != current);
    if let Some(n) = rename {
        if !is_name(n) {
            return Err(CmdError::refused(STYLE_NEEDS_NAME));
        }
        if styles::rows(doc, cat).iter().any(|r| r.id != *id && r.name.eq_ignore_ascii_case(n)) {
            return Err(name_taken(n));
        }
    }
    let mut after = styles::with_fields(FRAME_CATALOGUE, before.as_ref(), rename, &fields)?;
    if let (Some(b), StyleBody::Frame(f)) = (edit.body, &mut after.body) {
        f.title_block = Some(b);
    }
    styles::check_record(doc, cat, id, layer, &after)?;
    let name = rename.map_or(current, str::to_string);
    let ops = if before.as_ref() == Some(&after) {
        Vec::new()
    } else {
        vec![Op::SetStyle { layer, catalogue: FRAME_CATALOGUE, id: id.clone(), before, after: Some(after) }]
    };
    Ok(EditPlan { ops, label: format!("Title block {name}") })
}

/// The layer a new frame goes in: `project` (the default) or `standard` (when the project has
/// one).
///
/// # Errors
/// A standard layer without a standard, or another word.
pub fn new_layer(doc: &Document, word: Option<&str>) -> Result<StyleLayer, CmdError> {
    match word.map(str::trim) {
        None | Some("project") => Ok(StyleLayer::Project),
        Some("standard") if doc.standard.is_some() => Ok(StyleLayer::Standard),
        Some("standard") => Err(CmdError::refused(STANDARD_FIRST)),
        Some(other) => Err(CmdError::refused(format!("Unknown layer {other:?}: project or standard."))),
    }
}

/// The frame a new one starts from: `None` for `blank` (any case), else the frame `from` names
/// (default `sheet`).
///
/// # Errors
/// No such frame.
pub fn new_source(doc: &Document, from: Option<&str>) -> Result<Option<StyleRef>, CmdError> {
    let from = from.map(str::trim).unwrap_or("sheet");
    if from.eq_ignore_ascii_case("blank") {
        return Ok(None);
    }
    styles::find(doc, FRAME_CATALOGUE, from).map(Some)
}

/// A new frame's name: the one given, else a unique one from the sheet's size (`A3 title block`,
/// `A3 title block 2`…).
///
/// # Errors
/// A name that does not read, or one in use.
pub fn new_name(doc: &Document, given: Option<&str>, spec: &SheetSpec) -> Result<String, CmdError> {
    let rows = styles::rows(doc, catalogue());
    match given.map(str::trim) {
        Some(n) if is_name(n) => {
            if rows.iter().any(|r| r.name.eq_ignore_ascii_case(n)) {
                return Err(name_taken(n));
            }
            Ok(n.to_string())
        }
        Some(_) => Err(CmdError::refused(STYLE_NEEDS_NAME)),
        None => {
            let size = spec.preset_name().map_or_else(|| format!("{}×{}", spec.width, spec.height), str::to_string);
            let base = format!("{size} title block");
            let taken: BTreeSet<String> = rows.into_iter().map(|r| r.name.to_lowercase()).collect();
            if !taken.contains(&base.to_lowercase()) {
                return Ok(base);
            }
            Ok((2..10_000).map(|n| format!("{base} {n}")).find(|n| !taken.contains(&n.to_lowercase())).unwrap_or(base))
        }
    }
}

/// A new own frame (design 055 §4.6).
#[derive(Debug, Clone, PartialEq)]
pub struct NewFrame<'a> {
    pub name: String,
    pub spec: SheetSpec,
    pub layer: StyleLayer,
    /// Its block: the source's, as made for the sheet, or a blank one.
    pub layout: TitleBlockLayout,
    /// The frame it copies the fields of.
    pub source: Option<&'a StyleRef>,
}

/// `titleblock {new: {from, sheet, name?, layer?}}`: one `SetStyle` creating the frame, and its
/// id.
///
/// # Errors
/// A record the catalogue refuses.
pub fn plan_new(doc: &Document, ids: &mut IdAllocator, new: NewFrame<'_>) -> Result<(Vec<Op>, StyleRef), CmdError> {
    let cat = catalogue();
    let mut set: Vec<(String, Option<String>)> = Vec::new();
    if let Some(src) = new.source {
        for f in cat.fields {
            let v = styles::field(doc, cat, src, f, None).text();
            if !v.is_empty() {
                set.push((f.key.to_string(), Some(v)));
            }
        }
    }
    set.push((FRAME_SHEET_FIELD.into(), Some(new.spec.text())));
    set.push(("block".into(), Some("bottom_right".into())));
    let mut rec = styles::with_fields(FRAME_CATALOGUE, None, Some(&new.name), &set)?;
    if let StyleBody::Frame(f) = &mut rec.body {
        f.title_block = Some(new.layout);
    }
    let id = StyleRef::Own(ids.fresh::<u128>());
    styles::check_record(doc, cat, &id, new.layer, &rec)?;
    let ops = vec![Op::SetStyle { layer: new.layer, catalogue: FRAME_CATALOGUE, id: id.clone(), before: None, after: Some(rec) }];
    Ok((ops, id))
}
