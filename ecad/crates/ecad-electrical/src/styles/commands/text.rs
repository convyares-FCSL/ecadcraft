//! The text commands' rules: the texts of a page (`pagetexts`), a moved tag or connection point
//! label (`movetext`), the text kind a call names, and the scope the defaults list is resolved at
//! (`defaults`). Each function reads the document and returns the ops to commit and what to
//! report; the generic commands are declared at the end, measured with the look the runner gives
//! them. The text style table (`textstyle`) and which texts are shown (`showtext`) are the look's:
//! `ecad_drawing_style::text::commands`.

use ecad_model::Word;
use ecad_model::registry::{Entry, Resolved, Source};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use ecad_model::{ConnectionPointId, DeviceId, Document, Form, Noun, ObjectRef, Op, PageId, Pt, RepresentationId, Subject, TextHAlign, TextVAlign};

use super::defaults::level_word;
use crate::commands::input::Point;
use crate::commands::run::{Access, Cx, Generic, Outcome, Pages};
use crate::defaults::{self};
use crate::devices::text_place;
use crate::devices::texts::TextOf;
use crate::devices::{self};
use crate::error::CmdError;
use ecad_model::look::{Look, TextKind};
use ecad_model::registry::Scope;

/// Refusals.
pub const BAD_HEIGHT: &str = "A text height is 0.5 to 10 mm.";
pub const BAD_POINT: &str = "A text position must be a finite point.";
pub const BAD_JUSTIFY: &str = "Justify is two letters: L, C or R, then T, M or B.";
pub(crate) const NEEDS_CONNECTION_POINT: &str = "point_label needs the connection point's key.";
pub const NOT_A_SYMBOL: &str = "That representation is not a symbol.";

/// Whether a text is shown, as a call says it: its word, a yes/no word, or its panel label.
impl crate::commands::input::Words for ecad_model::look::Shown {
    const WORDS: &'static [&'static str] = <ecad_model::look::Shown as ecad_model::Word>::WORDS;

    fn read(text: &str) -> Option<Self> {
        ecad_model::look::Shown::from_input(text)
    }
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
pub fn default_key(doc: &Document, registry: &ecad_model::registry::data::Registry, key: &str) -> Result<(), CmdError> {
    if registry.find(key).is_none() && defaults::symbols::family(doc, key).is_none() {
        return Err(CmdError::refused(format!("No default \"{key}\".")));
    }
    Ok(())
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
    code.map(|c| text_place::parse_justify(c).ok_or(CmdError::refused(BAD_JUSTIFY))).transpose()
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
        TextKind::Tag => (format!("{}tag.at", text_place::symbol_prefix(rep)), TextOf::Tag),
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
            let local = text_place::to_local(&pl, to).ok_or(CmdError::refused(BAD_POINT))?;
            Some(text_place::at_text(local, h, v))
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

/// The JSON form of a registry value.
pub(crate) fn value_json(r: &Resolved<'_>) -> Value {
    match r.value {
        ecad_model::registry::Value::Length(x) | ecad_model::registry::Value::Number(x) => json!(x),
        ecad_model::registry::Value::Choice(w) => json!(w),
        ecad_model::registry::Value::Flag(b) => json!(ecad_model::naming::words::flag_word(b)),
        ecad_model::registry::Value::Text(t) => json!(t),
        ecad_model::registry::Value::OwnStyle(id) => json!(ecad_model::StyleRef::Own(id).text()),
        ecad_model::registry::Value::Size(s) => json!(s.to_string()),
        ecad_model::registry::Value::Colours(c) => json!(c.to_string()),
        ecad_model::registry::Value::Colour(c) => json!(c.to_string()),
        ecad_model::registry::Value::Sheet(s) => json!(s.text()),
        ecad_model::registry::Value::Margins(m) => json!(ecad_model::document::sheet::margins_text(&m)),
        ecad_model::registry::Value::CoreColours(c) => json!(c.to_string()),
    }
}

/// One registry entry as `defaults` lists it, resolved at `at`.
pub(crate) fn entry_json(doc: &Document, e: &Entry, at: Scope) -> Value {
    let r = ecad_model::registry::resolve(doc, e, at);
    let builtin = Resolved { value: e.builtin(), from: Source::Builtin };
    let mut m = Map::new();
    m.insert("key".into(), json!(e.key));
    m.insert("label".into(), json!(e.label));
    m.insert("group".into(), json!(e.group));
    m.insert("kind".into(), json!(e.kind_word()));
    m.insert("builtin".into(), value_json(&builtin));
    m.insert("levels".into(), json!(e.levels.iter().map(|l| level_word(*l)).collect::<Vec<_>>()));
    m.insert("value".into(), value_json(&r));
    m.insert("from".into(), json!(r.from.as_str()));
    Value::Object(m)
}

// ---------------------------------------------------------------- the commands

/// Most rows `pagetexts` returns.
pub(crate) const MAX_TEXT_ROWS: usize = 5_000;

/// Every derived text on a page, hidden ones too. Read only.
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
    const PAGES: Pages = Pages::ReadAnywhere;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let page = text_page(cx.doc, self.page, cx.page)?;
        Ok(Outcome::answer(pagetexts_json(cx.doc, page, cx.look)))
    }
}

/// The rows of `pagetexts` for `page`, measured with `metrics`.
pub(crate) fn pagetexts_json(doc: &Document, page: PageId, look: &dyn Look) -> Value {
    use crate::devices::text_place::{justify_code, text_box};
    use ecad_model::look::TextStyle;
    use ecad_model::objects::styles::text::Weight;
    let all = crate::devices::texts::all_texts_measured(doc, page, None, look);
    let more = all.len().saturating_sub(MAX_TEXT_ROWS);
    let rows: Vec<Value> = all
        .iter()
        .take(MAX_TEXT_ROWS)
        .map(|t| {
            let d = &t.text;
            let kind = crate::devices::texts::kind_of(d.of);
            let style = look.text_style(doc, kind);
            let w = look.width(doc, kind, TextStyle { height: d.height, ..style }, &d.value);
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

/// Moves a symbol's tag or connection point label to a page point (`to`, stored in the symbol's frame), or with
/// `reset: true` back to its computed place.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MovetextParams {
    /// The symbol.
    representation: RepresentationId,
    /// Tag (default) or point_label.
    #[serde(default)]
    kind: Option<TextKind>,
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

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let rep = self.representation;
        let kind = self.kind.unwrap_or(TextKind::Tag);
        let connection_point = self.connection_point;
        let to = match self.to {
            _ if self.reset == Some(true) => None,
            Some(p) => Some(p.0),
            None => return Err(CmdError::refused(BAD_POINT)),
        };
        let justify = justify(self.justify.as_deref())?;
        let look = cx.look;
        let doc = cx.doc;
        let t = move_target(doc, rep, kind, connection_point)?;
        let current = current_text(doc, rep, &t, look).map(|x| (x.text.halign, x.text.valign));
        let plan = plan_move(doc, rep, kind, &t, to, justify, current)?;
        let changed = plan.op.is_some();
        let placed = plan.after.is_some();
        Ok(Outcome::edit_then(plan.label, plan.op.into_iter().collect(), move |doc| {
            let now = move_target(doc, rep, kind, connection_point).ok().and_then(|t| current_text(doc, rep, &t, look));
            json!({
                "representation": rep.hex(),
                "kind": kind.word(),
                "at": if placed { now.as_ref().map(|t| json!([t.text.at.x, t.text.at.y])) } else { None },
                "justify": now.as_ref().map(|t| crate::devices::text_place::justify_code(t.text.halign, t.text.valign)),
                "changed": changed,
            })
        }))
    }
}

/// The text `t` names as drawn now.
fn current_text(doc: &Document, rep: RepresentationId, t: &MoveTarget, look: &dyn Look) -> Option<crate::devices::texts::PageText> {
    crate::devices::texts::all_texts_measured(doc, t.page, None, look).into_iter().find(|x| x.text.rep == rep && x.text.of == t.of)
}
