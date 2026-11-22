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
//!   `cableedit` is one `ModifyCable`; `cableremove` is one `DeleteCable` and is
//!   refused for a drawn cable.
//!
//! This module excludes the UI (the Cables tab, the slash picker) and computed glands.
//!
//! Programmatic calls never prompt.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::cable::{self as rules, CableInput, EndInput};
pub use ecad_electrical::cmd::cable::{BadReason, Drawn, assign_cores, bad_value, cable_named, check_remove, drawn, edit_ops};
use ecad_electrical::cmd::cable_draw::{crossing_lines, crossing_wires};
use ecad_model::{CableId, Op, PageId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

pub use crate::cables::{
    CableField, DrawnEnd, MAX_ECHO, MAX_LENGTH, MAX_TEXT, MAX_WORDS, MIN_LENGTH, OffPlan, Side, Source, bad_values, clip, cores_shown, drawn_ends,
    drawn_pages, gland, is_drawn, length_of, not_drawn, off_plan, part_value, plan_words, planned, planning, read, shield, type_notation,
    value_checks,
};
use crate::cmd::input::{Command, TextOrNumber, Typed, nullable, typed};
use crate::cmd::on_project;

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

/// `cableslash` onto cable `k` that is not drawn (FD5): commits
/// [`ecad_electrical::cmd::cable_draw::plan_slash_assign`]'s ops as one transaction, `Cable slash`.
///
/// # Errors
/// A wire already in a cable, more wires than free cores, too many members; nothing is written.
pub(crate) fn run_slash_assign(
    p: &mut crate::project::Project, pg: &mut crate::access::Pages<'_>, page: PageId, k: CableId, from: ecad_model::Pt, to: ecad_model::Pt,
    found: &crate::slash::Crossings,
) -> Result<Value, CmdError> {
    let style = crate::cmd::plain_style(p);
    let plan = ecad_electrical::cmd::cable_draw::plan_slash_assign(p.ledger.document(), &mut p.ids, page, style, k, from, to, found)?;
    p.apply_edit(pg, "cableslash", "Cable slash", plan.ops)?;
    Ok(json!({
        "cable": k.hex(),
        "item": plan.item.hex(),
        "representation": plan.rep.hex(),
        "assigned": true,
        "conductors": crossing_wires(&found.members),
        "lines": crossing_lines(&found.members),
        "cores": plan.cores,
        "ignored": found.ignored.iter().map(|(i, _)| i.hex()).collect::<Vec<_>>(),
        "touched": found.touched.iter().map(|c| c.hex()).collect::<Vec<_>>(),
    }))
}
