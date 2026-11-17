//! The rules of brief 049's command (design 049 §5.1): `contcaption` sets or removes the caption
//! of continuation arrows, one `SetProperty` per arrow whose caption changes. The engine crate's
//! shim reads the parameters, commits and answers in JSON.

use std::collections::BTreeSet;

use ecad_model::{ComponentId, Document, ObjectRef, Op};

use crate::continuations::{self as k, CAPTION_KEY};
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
pub fn plan_contcaption(doc: &Document, arrows: &[ComponentId], caption: Option<&str>) -> Result<Contcaption, CmdError> {
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
        // Refuses anything that is not a continuation arrow, naming it.
        k::caption_value(doc, *c, caption.as_deref())?;
        let before = doc.components.get(c).and_then(|x| x.properties.get(CAPTION_KEY)).cloned();
        if before != caption {
            ops.push(Op::SetProperty { object: ObjectRef::Component(*c), key: CAPTION_KEY.into(), before, after: caption.clone() });
        }
    }
    let (changed, unchanged) = (ops.len(), seen.len() - ops.len());
    let message = match &caption {
        Some(t) => format!("Captioned {changed} arrow{} \"{t}\".", if changed == 1 { "" } else { "s" }),
        None => format!("Removed the caption of {changed} arrow{}.", if changed == 1 { "" } else { "s" }),
    };
    Ok(Contcaption { ops, label: label(caption.as_deref()), caption, changed, unchanged, message })
}
