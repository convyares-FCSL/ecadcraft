//! The rules of brief 049's command (design 049 §5.1): `contcaption` sets or removes the caption
//! of continuation arrows, one `ModifyAid` per arrow whose caption changes; and `setprop {aids}`
//! (design 072 §6.2): a continuation's `name` or `caption`. The engine crate's shim reads the
//! parameters, commits and answers in JSON.

use std::collections::BTreeSet;

use ecad_model::{Aid, AidId, Document, Op};

use crate::continuations::{self as k, CAPTION_KEY, NAME_KEY};
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

/// `setprop {aids, key, value}` (design 072 §6.2): a continuation's `name` (checked, required) or
/// `caption` (checked; `None` removes it), one `ModifyAid` per arrow that changes.
///
/// # Errors
/// No arrow named, an aid that is not a continuation, another key, or a value the rules refuse.
pub fn plan_aid_prop(doc: &Document, aids: &[AidId], key: &str, value: Option<&str>) -> Result<Vec<Op>, CmdError> {
    if aids.is_empty() {
        return Err(CmdError::refused("Name at least one continuation arrow in `aids`."));
    }
    let mut ops = Vec::new();
    let mut seen = BTreeSet::new();
    for a in aids.iter().filter(|a| seen.insert(**a)) {
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
            _ => return Err(CmdError::refused(format!("A continuation has a `{NAME_KEY}` and a `{CAPTION_KEY}`, not `{key}`."))),
        }
    }
    Ok(ops)
}
