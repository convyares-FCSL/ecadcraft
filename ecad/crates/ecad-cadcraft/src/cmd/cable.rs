//! Brief 059's commands (design 059 §4): the cable manager. `cablenew`, `cableedit` and
//! `cableremove` (write), and the assign rule `cableslash` and `cableadd` follow for a cable that is
//! not drawn yet.
//!
//! # Semantics
//! - A cable **not drawn** is a `Cable` with no members and no representation (FD1). It is built
//!   in Project ▸ Cables and assigned in the drawing later.
//! - Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//!   [`ecad_electrical::cmd::cable`] decide (their refusals give the words of design 059 §9; every
//!   refusal writes nothing), and the shim commits the ops and answers in JSON.
//! - Every key this module reads or writes is named by [`CableField::key`] (design 059 §2.3): no
//!   other string literal names a cable key here.
//! - `cablenew` is one `CreateCable` (plus the project list entries a new planned code adds);
//!   `cableedit` is `SetProperty` ops in one transaction; `cableremove` is one `DeleteCable` and is
//!   refused for a drawn cable.
//!
//! This module excludes the UI (the Cables tab, the slash picker) and computed glands.
//!
//! Programmatic calls never prompt.

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::cable::{self as rules, CableInput, EndInput};
pub use ecad_electrical::cmd::cable::{BadReason, Drawn, assign_cores, bad_value, cable_named, check_remove, drawn, edit_ops};
use ecad_model::{CableId, CorePosition, Document, Form, Item, ItemId, ItemKind, Op, PageId, Primitive, Representation, RepresentationId, Subject};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

pub use crate::cables::{
    CableField, DrawnEnd, MAX_ECHO, MAX_LENGTH, MAX_TEXT, MAX_WORDS, MIN_LENGTH, OffPlan, Shield, Side, Source, bad_values, clip, cores_shown,
    drawn_ends, drawn_pages, gland, is_drawn, length_of, not_drawn, off_plan, part_value, plan_words, planned, planning, read, shield, type_notation,
    value_checks,
};
use crate::cmd::input::{Command, TextOrNumber, Typed, nullable, typed};
use crate::cmd::on_project;
use crate::designations::{self, Aspect};
use crate::slash::MAX_CABLE_MEMBERS;
use crate::styles::cable;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] =
    &[Typed::of::<CablenewParams>("cablenew"), Typed::of::<CableeditParams>("cableedit"), Typed::of::<CableremoveParams>("cableremove")];

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("cablenew", "New Cable", typed::<CablenewParams>)
            .params(
                "{tag?, cable_type?: id or name, type?, cores?, length?: m, shield?: \"none\"|\"overall\"|\"pairs\"|\"overall+pairs\", maker?, \
part_number?, description?, from?: {location?, function?, gland?}, to?: {…}} (a cable not drawn yet, the next free -W unless `tag`; one transaction)",
            )
            .noundo(),
        CommandSpec::new("cableedit", "Edit Cable", typed::<CableeditParams>)
            .params(
                "{cable: id or tag, set: {tag?, cable_type?, type?, cores?, length?, shield?, maker?, part_number?, description?, \
from?: {location?, function?, gland?}, to?: {…}}} (null clears a field; one transaction; reducing cores below those in use is refused)",
            )
            .noundo(),
        CommandSpec::new("cableremove", "Remove Cable", typed::<CableremoveParams>)
            .params("{cable: id or tag} (deletes a cable that is not drawn; a drawn cable is refused with the pages it is drawn on)")
            .noundo(),
    ]
}

// ---------------------------------------------------------------------------------------------
// Parameters (design 059 §4.1, §4.2)

/// A value as text: the text, or a number as JSON writes it; `None` for null.
fn text(v: Option<Option<TextOrNumber>>) -> Option<Option<String>> {
    v.map(|v| v.map(|t| t.text()))
}

/// An end's plan and gland.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EndParams {
    /// The planned location (+CAB1); null clears it.
    #[serde(default, deserialize_with = "nullable")]
    location: Option<Option<TextOrNumber>>,
    /// The planned function (=PUMP); null clears it.
    #[serde(default, deserialize_with = "nullable")]
    function: Option<Option<TextOrNumber>>,
    /// The gland (M20); null clears it.
    #[serde(default, deserialize_with = "nullable")]
    gland: Option<Option<TextOrNumber>>,
}

impl From<EndParams> for EndInput {
    fn from(e: EndParams) -> Self {
        EndInput { location: text(e.location), function: text(e.function), gland: text(e.gland) }
    }
}

/// `cablenew {tag?, cable_type?, type?, cores?, length?, shield?, maker?, part_number?,
/// description?, from?, to?}`: a cable not drawn yet, the next free -W unless `tag`; one
/// transaction. The same fields are `cableedit`'s `set`, where null clears one.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CablenewParams {
    /// The tag (-W05).
    #[serde(default, deserialize_with = "nullable")]
    tag: Option<Option<TextOrNumber>>,
    /// The cable type: a style ID or name from the project's standards.
    #[serde(default, deserialize_with = "nullable")]
    cable_type: Option<Option<TextOrNumber>>,
    /// The type notation (H07RN-F 4G1.5).
    #[serde(default, deserialize_with = "nullable", rename = "type")]
    kind: Option<Option<TextOrNumber>>,
    /// The cores offered (4G, 7X, or a list).
    #[serde(default, deserialize_with = "nullable")]
    cores: Option<Option<TextOrNumber>>,
    /// The length in metres.
    #[serde(default, deserialize_with = "nullable")]
    length: Option<Option<TextOrNumber>>,
    /// The shield: none, overall, pairs or overall+pairs.
    #[serde(default, deserialize_with = "nullable")]
    shield: Option<Option<TextOrNumber>>,
    /// The maker.
    #[serde(default, deserialize_with = "nullable")]
    maker: Option<Option<TextOrNumber>>,
    /// The part number.
    #[serde(default, deserialize_with = "nullable")]
    part_number: Option<Option<TextOrNumber>>,
    /// The description.
    #[serde(default, deserialize_with = "nullable")]
    description: Option<Option<TextOrNumber>>,
    /// The from end's plan and gland.
    #[serde(default)]
    from: Option<EndParams>,
    /// The to end's plan and gland.
    #[serde(default)]
    to: Option<EndParams>,
}

impl From<CablenewParams> for CableInput {
    fn from(p: CablenewParams) -> Self {
        CableInput {
            tag: text(p.tag),
            cable_type: text(p.cable_type),
            kind: text(p.kind),
            cores: text(p.cores),
            length: text(p.length),
            shield: text(p.shield),
            maker: text(p.maker),
            part_number: text(p.part_number),
            description: text(p.description),
            from: p.from.map(EndInput::from),
            to: p.to.map(EndInput::from),
        }
    }
}

impl Command for CablenewParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let fields = CableInput::from(self);
        let v = on_project(s, |p, pg, _| {
            let plan = rules::plan_new(&p.ledger, &mut p.ids, &mut p.tag_high, &fields)?;
            p.apply_edit(pg, "cablenew", "Create cable", plan.ops)?;
            let tag = plan.tag;
            let mut message = format!("Created {tag} (not drawn). Fill it in on the right, then assign it in the drawing.");
            if let Some(w) = plan.added {
                message = format!("{message} {w}");
            }
            Ok(json!({"cable": plan.id.hex(), "tag": tag, "message": message}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

/// `cableedit {cable, set}`: change a cable's fields in one transaction (null clears a field;
/// reducing cores below those in use is refused).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CableeditParams {
    /// The cable: an ID or a tag.
    cable: String,
    /// The fields to change, as `cablenew` takes them; null clears one.
    set: CablenewParams,
}

impl Command for CableeditParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let set = CableInput::from(self.set);
        let v = on_project(s, |p, pg, _| {
            p.tag_high.catch_up(&p.ledger);
            let doc = p.ledger.document();
            let k = cable_named(doc, &self.cable)?;
            let (ops, changed, words) = edit_ops(doc, &mut p.ids, &mut p.tag_high, k, &set)?;
            let names: Vec<&str> = changed.iter().map(|f| f.key()).collect();
            if ops.is_empty() {
                return Ok(json!({"cable": k.hex(), "changed": names, "written": false}));
            }
            p.apply_edit(pg, "cableedit", "Edit cable", ops)?;
            Ok(json!({"cable": k.hex(), "changed": names, "written": true, "message": words}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

/// `cableremove {cable}`: delete a cable that is not drawn; a drawn cable is refused with the
/// pages it is drawn on.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CableremoveParams {
    /// The cable: an ID or a tag.
    cable: String,
}

impl Command for CableremoveParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let v = on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let k = cable_named(doc, &self.cable)?;
            check_remove(doc, k)?;
            let cable = doc.cables.get(&k).cloned().ok_or(CmdError::NotFound(ecad_model::Noun::Cable))?;
            let tag = crate::cables::cable_title(doc, k);
            p.apply_edit(pg, "cableremove", "Remove cable", vec![Op::DeleteCable { id: k, cable }])?;
            Ok(json!({"cable": k.hex(), "removed": true, "message": format!("Removed {tag}. Undo with Ctrl Z.")}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

// ---------------------------------------------------------------------------------------------
// The assign rule (design 059 §4.3, FD5)

/// The hex of the wires a slash crossed, in crossing order (the `conductors` of a result).
pub(crate) fn crossing_wires(members: &[crate::slash::Crossing]) -> Vec<String> {
    members.iter().filter_map(|m| m.wire()).map(|w| w.hex()).collect()
}

/// The hex of the loose lines (no wire yet) a slash crossed, in crossing order (the `lines` of a
/// result).
pub(crate) fn crossing_lines(members: &[crate::slash::Crossing]) -> Vec<String> {
    members.iter().filter_map(|m| m.position.route()).map(|r| r.hex()).collect()
}

/// `cableslash` onto cable `k` that is not drawn (FD5): the crossed wires become its members in
/// crossing order with its free cores' labels, plus the slash. One transaction, `Cable slash`.
///
/// # Errors
/// A wire already in a cable, more wires than free cores, too many members; nothing is written.
pub(crate) fn run_slash_assign(
    p: &mut crate::project::Project, pg: &mut crate::access::Pages<'_>, page: PageId, k: CableId, from: ecad_model::Pt, to: ecad_model::Pt,
    found: &crate::slash::Crossings,
) -> Result<Value, CmdError> {
    let doc = p.ledger.document();
    let blocked: Vec<(CorePosition, CableId)> = crate::slash::blocked(doc, &found.members).into_iter().map(|(m, c)| (m.position, c)).collect();
    if !blocked.is_empty() {
        return Err(rules::already_in_cable(crate::slash::already_in_cable(doc, &blocked, "Nothing was changed.")));
    }
    if found.members.len() > MAX_CABLE_MEMBERS {
        return Err(rules::too_many());
    }
    let cores = assign_cores(doc, k, found.members.len())?;
    let (item, rep): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
    let mut ops = Vec::new();
    // Design 072 §4.5: each crossed line takes its core, the stored unplaced one of that label.
    if let Some(before) = doc.cables.get(&k) {
        let mut after = before.clone();
        for (m, core) in found.members.iter().zip(cores.iter()) {
            ecad_electrical::styles::cable::place_core(&mut after, core.as_deref().and_then(|c| ecad_model::CoreLabel::new(c).ok()), m.position);
        }
        ops.extend(Op::modify_cable(k, before, after));
    }
    let style = crate::cmd::plain_style(p);
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Line { a: from, b: to }) } });
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Cable(k),
            page,
            form: Form::Drawn,
            role: "slash".into(),
            items: vec![item],
            connection_points: BTreeMap::new(),
        },
    });
    p.apply_edit(pg, "cableslash", "Cable slash", ops)?;
    Ok(json!({
        "cable": k.hex(),
        "item": item.hex(),
        "representation": rep.hex(),
        "assigned": true,
        "conductors": crossing_wires(&found.members),
        "lines": crossing_lines(&found.members),
        "cores": cores,
        "ignored": found.ignored.iter().map(|(i, _)| i.hex()).collect::<Vec<_>>(),
        "touched": found.touched.iter().map(|c| c.hex()).collect::<Vec<_>>(),
    }))
}

/// Board 2.06's **Add a core** (design 059 §3): cable `k`'s offer plus one core (`4G` → `5G`,
/// `7X` → `8X`; a list gains the next number), or its member count plus one with no offer.
pub fn next_offer(doc: &Document, k: CableId) -> String {
    let members = doc.cables.get(&k).map_or(0, |c| c.cores.len());
    let Some((cores, _, text)) = cable::offer(doc, k) else { return (members + 1).to_string() };
    let t = text.trim();
    let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
    let rest = &t[digits.len()..];
    if let Ok(n) = digits.parse::<usize>()
        && (rest.is_empty() || rest.eq_ignore_ascii_case("G") || rest.eq_ignore_ascii_case("X"))
    {
        return format!("{}{}", n.saturating_add(1), rest.to_ascii_uppercase());
    }
    let high = cores.iter().filter_map(|c| c.parse::<usize>().ok()).max().unwrap_or(0);
    let mut v = cores;
    v.push((high + 1).to_string());
    v.join(",")
}

/// One item of the slash and Add to cable pickers (design 059 §5.6).
#[derive(Debug, Clone, PartialEq)]
pub struct PickItem {
    pub cable: CableId,
    pub tag: String,
    /// `-W03  H07RN-F 4G1.5 · +CAB1 → +FIELD`.
    pub line: String,
    pub drawn: bool,
    /// A cable not drawn whose planned end location is this page's (or one of its location boxes').
    pub matches: bool,
}

/// The picker's cables on `page` (Q7): not drawn first (those whose planned end location matches
/// the page's location, or one of its location boxes', before the rest, marked), then drawn cables
/// not slashed on this page (032). Each group by tag. O(cables), built once when a picker opens.
pub fn picker(doc: &Document, page: PageId) -> Vec<PickItem> {
    let mut here: BTreeSet<String> = crate::locations::boxes_on(doc, page).into_iter().filter_map(|b| b.location).collect();
    let page_loc = doc.pages.get(&page).and_then(|x| designations::page_key(x, Aspect::Location));
    if let Some(code) = designations::shown(doc, Aspect::Location, page_loc.as_deref()) {
        here.insert(code);
    }
    let mut out: Vec<PickItem> = Vec::new();
    for k in doc.cables.keys() {
        let drawn = is_drawn(doc, *k);
        if drawn && doc.representations_of(Subject::Cable(*k)).any(|(_, r)| r.page == page && r.role == "slash") {
            continue;
        }
        let tag = crate::cables::cable_title(doc, *k);
        let matches = !drawn
            && Side::BOTH.into_iter().any(|s| planned(doc, *k, s, Aspect::Location).is_some_and(|c| here.iter().any(|h| h.eq_ignore_ascii_case(&c))));
        let dash = "\u{2014}";
        let ends = format!(
            "{} \u{2192} {}",
            plan_words(doc, *k, Side::From).unwrap_or_else(|| dash.into()),
            plan_words(doc, *k, Side::To).unwrap_or_else(|| dash.into())
        );
        let line = match read(doc, *k, CableField::Type) {
            Some(t) => format!("{tag}  {t} \u{b7} {ends}"),
            None => format!("{tag}  {ends}"),
        };
        out.push(PickItem { cable: *k, tag, line, drawn, matches });
    }
    out.sort_by(|a, b| (a.drawn, !a.matches).cmp(&(b.drawn, !b.matches)).then_with(|| crate::locations::natural_cmp(&a.tag, &b.tag)));
    out
}
