//! The rules of brief 049's command (design 049 §5.1): `contcaption` sets or removes the caption
//! of continuation arrows, one `ModifyAid` per arrow whose caption changes; and `setprop {aids}`
//! (design 072 §6.2): a continuation's `name` or `caption`, a location box's `function`,
//! `location`, `panel_temperature` or `style`. `contcaption` is a generic command
//! ([`crate::cmd::run`]); its interactive form (select arrows, type the caption) is the engine's.

use std::collections::BTreeSet;

use ecad_model::{Aid, AidId, Document, IdAllocator, LocationBox, Op};

use crate::continuations::{self as k, CAPTION_KEY, NAME_KEY};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::cmd::input::{Bounded, nullable};
use crate::cmd::run::{Cx, Generic, Outcome, Slot, State};
use crate::error::CmdError;

/// Most arrows one `contcaption` names.
pub const MAX_ARROWS: usize = 2000;

/// The label of a `contcaption` transaction.
pub fn label(caption: Option<&str>) -> String {
    match caption {
        Some(t) => format!("Caption {t}"),
        None => "Remove caption".into(),
    }
}

/// A planned `contcaption` call.
#[derive(Debug, Clone)]
pub struct Contcaption {
    /// Empty: every arrow already has the caption.
    pub ops: Vec<Op>,
    /// The caption as stored (`None`: removed).
    pub caption: Option<String>,
    /// How many arrows change, and how many already had it.
    pub changed: usize,
    pub unchanged: usize,
    /// The transaction's label.
    pub label: String,
    pub message: String,
}

/// Plan `contcaption` (design 049 §5.1): `caption` (`None`: remove) on every arrow of `arrows`.
///
/// # Errors
/// No arrow named, a caption the rule refuses, or an object that is not a continuation arrow
/// (named).
pub fn plan_contcaption(doc: &Document, arrows: &[AidId], caption: Option<&str>) -> Result<Contcaption, CmdError> {
    if arrows.is_empty() {
        return Err(CmdError::refused("Name at least one continuation arrow in `continuations`."));
    }
    let caption = k::caption_text(caption)?;
    let mut ops = Vec::new();
    let mut seen = BTreeSet::new();
    for c in arrows {
        if !seen.insert(*c) {
            continue;
        }
        // Refuses anything that is not a continuation arrow.
        k::caption_value(doc, *c, caption.as_deref())?;
        ops.extend(caption_op(doc, *c, caption.as_deref())?);
    }
    let (changed, unchanged) = (ops.len(), seen.len() - ops.len());
    let message = match &caption {
        Some(t) => format!("Captioned {changed} arrow{} \"{t}\".", if changed == 1 { "" } else { "s" }),
        None => format!("Removed the caption of {changed} arrow{}.", if changed == 1 { "" } else { "s" }),
    };
    Ok(Contcaption { ops, label: label(caption.as_deref()), caption, changed, unchanged, message })
}

/// The `ModifyAid` giving continuation `a` the checked caption `caption`; `None` when it has it.
fn caption_op(doc: &Document, a: AidId, caption: Option<&str>) -> Result<Option<Op>, CmdError> {
    let Some(before @ Aid::Continuation { name, .. }) = doc.aids.get(&a) else { return Err(CmdError::refused(k::NO_SUCH_ARROW)) };
    let caption = caption.map(ecad_model::aid::Caption::new).transpose().map_err(|_| CmdError::refused(k::CAPTION_RULE))?;
    let after = Aid::Continuation { name: name.clone(), caption };
    Ok((*before != after).then(|| Op::ModifyAid { id: a, before: before.clone(), after }))
}

/// The `ModifyAid` giving location box `a` the values `edit` leaves; `None` when it is not a box
/// or nothing changes.
pub fn box_op(doc: &Document, a: AidId, edit: impl FnOnce(&mut LocationBox)) -> Option<Op> {
    let before @ Aid::Box(x) = doc.aids.get(&a)? else { return None };
    let mut after = x.clone();
    edit(&mut after);
    (after != *x).then(|| Op::ModifyAid { id: a, before: before.clone(), after: Aid::Box(after) })
}

/// The refusal of a key a location box does not have.
fn box_keys(key: &str) -> CmdError {
    CmdError::refused(format!(
        "A location box has a `function`, a `location`, a `{}` and a `{}`, not `{key}`.",
        crate::locations::PANEL_TEMPERATURE_KEY,
        crate::locations::BOX_STYLE_KEY
    ))
}

/// `setprop {aids, key, value}` (design 072 §6.2), one `ModifyAid` per aid that changes: a
/// continuation's `name` (checked, required) or `caption` (checked; `None` removes it); a location
/// box's `function` or `location` (a code, as [`crate::cmd::location::plan_box_set`], without its
/// proposals), `panel_temperature` (−40 to 90 °C; `None` clears it) or `style` (a location style by
/// name or ID; `None` or empty clears it: the project's).
///
/// # Errors
/// No aid named, an id that is no aid, a key the aid's kind does not have, or a value the rules
/// refuse.
pub fn plan_aid_prop(doc: &Document, ids: &mut IdAllocator, aids: &[AidId], key: &str, value: Option<&str>) -> Result<Vec<Op>, CmdError> {
    if aids.is_empty() {
        return Err(CmdError::refused("Name at least one continuation arrow or location box in `aids`."));
    }
    if let Some(a) = crate::designations::Aspect::of_property(key) {
        if aids.iter().any(|a| matches!(doc.aids.get(a), Some(Aid::Continuation { .. }))) {
            return Err(continuation_keys(key));
        }
        return Ok(crate::cmd::location::plan_box_set(doc, ids, aids, a, value)?.ops);
    }
    let mut ops = Vec::new();
    let mut seen = BTreeSet::new();
    for a in aids.iter().filter(|a| seen.insert(**a)) {
        match doc.aids.get(a) {
            None => return Err(CmdError::refused("That is not a continuation arrow or a location box.")),
            Some(Aid::Box(_)) if key == crate::locations::PANEL_TEMPERATURE_KEY => {
                let temp = value.filter(|v| !v.trim().is_empty()).map(crate::cmd::rating::panel_temperature).transpose()?;
                ops.extend(box_op(doc, *a, |b| b.panel_temperature = temp));
                continue;
            }
            Some(Aid::Box(_)) if key == crate::locations::BOX_STYLE_KEY => {
                let style = value.map(str::trim).filter(|v| !v.is_empty());
                let style = style.map(|v| crate::styles::find(doc, ecad_model::StyleCatalogue::Location, v)).transpose()?;
                ops.extend(box_op(doc, *a, |b| b.style = style));
                continue;
            }
            Some(Aid::Box(_)) => return Err(box_keys(key)),
            Some(Aid::Continuation { .. }) => {}
        }
        match key {
            CAPTION_KEY => {
                let caption = k::caption_value(doc, *a, value)?;
                ops.extend(caption_op(doc, *a, caption.as_deref())?);
            }
            NAME_KEY => {
                let v = value.ok_or(CmdError::refused("A continuation needs a name."))?;
                let Some(before @ Aid::Continuation { caption, .. }) = doc.aids.get(a) else { return Err(CmdError::refused(k::NO_SUCH_ARROW)) };
                let Aid::Continuation { name, .. } = crate::wires::continuation_aid(v, None)? else { continue };
                let after = Aid::Continuation { name, caption: caption.clone() };
                if *before != after {
                    ops.push(Op::ModifyAid { id: *a, before: before.clone(), after });
                }
            }
            _ => return Err(continuation_keys(key)),
        }
    }
    Ok(ops)
}

/// The refusal of a key a continuation does not have.
fn continuation_keys(key: &str) -> CmdError {
    CmdError::refused(format!("A continuation has a `{NAME_KEY}` and a `{CAPTION_KEY}`, not `{key}`."))
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::cmd::run)

/// `contcaption {continuations, caption}` (design 049 §5.1): sets (or with null removes) the
/// arrows' caption, one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContcaptionParams {
    /// The continuation arrows (1 to 2000).
    continuations: Bounded<AidId, 0, MAX_ARROWS>,
    /// The caption, or null to remove it.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "crate::cmd::input::OrNull<String>")]
    caption: Option<Option<String>>,
}

impl Generic for ContcaptionParams {
    const ID: &'static str = "contcaption";
    const TITLE: &'static str = "Caption Continuations";
    const HELP: &'static str =
        "{continuations: [id…] (1-2000), caption: \"TO CONTROL SUPPLY\" | null} (sets or removes the arrows' caption, one transaction)";
    const STATE: State = State::LastCaption;

    fn check(&self) -> Result<(), CmdError> {
        match self.caption {
            Some(_) => Ok(()),
            None => Err(CmdError::input("`caption` (text, or null) is required")),
        }
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let caption = self.caption.ok_or(CmdError::input("`caption` (text, or null) is required"))?;
        let plan = plan_contcaption(cx.doc, &self.continuations.0, caption.as_deref())?;
        let answer = json!({"changed": plan.changed, "unchanged": plan.unchanged, "caption": plan.caption, "message": plan.message});
        if let (Some(t), Slot::LastCaption(last)) = (plan.caption, &mut cx.state) {
            **last = Some(t);
        }
        Ok(Outcome::edit_if_any(plan.label, plan.ops, answer).echo(Some(plan.message)))
    }
}
