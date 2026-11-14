//! Brief 049's command (design 049 §5.1): `contcaption` sets or removes the caption of continuation
//! arrows, one transaction of `SetProperty` ops (only for arrows whose caption changes). Its
//! interactive form (select arrows, type the caption) is in [`crate::interact`].
//!
//! Excludes: what a caption draws ([`crate::continuations`]); `setprop` and `place` take the same
//! rules through [`crate::continuations::caption_value`].

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{ComponentId, ObjectRef, Op};
use serde_json::{Value, json};

use crate::commands::{err, id_list, with_project};
use crate::continuations::{self as k, CAPTION_KEY};

/// Most arrows one `contcaption` names.
pub const MAX_ARROWS: usize = 2000;

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("contcaption", "Caption Continuations", run_contcaption)
            .params("{continuations: [id…] (1-2000), caption: \"TO CONTROL SUPPLY\" | null} (sets or removes the arrows' caption, one transaction)")
            .interactive(crate::interact::contcaption)
            .noundo(),
    ]
}

/// The label of a `contcaption` transaction.
pub fn label(caption: Option<&str>) -> String {
    match caption {
        Some(t) => format!("Caption {t}"),
        None => "Remove caption".into(),
    }
}

fn run_contcaption(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let ids: Vec<ComponentId> = id_list(params, "continuations", MAX_ARROWS).map_err(err)?;
    if ids.is_empty() {
        return Err(err("Name at least one continuation arrow in `continuations`."));
    }
    let caption = match params.get(CAPTION_KEY) {
        None => return Err(err("`caption` is required (text, or null to remove it)")),
        Some(Value::Null) => None,
        Some(Value::String(t)) => k::caption_text(Some(t)).map_err(err)?,
        Some(_) => return Err(err("`caption` must be text or null")),
    };
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let mut ops = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for c in &ids {
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
        if !ops.is_empty() {
            p.apply_edit(pg, "contcaption", &label(caption.as_deref()), ops)?;
        }
        if let Some(t) = &caption {
            p.last_caption = Some(t.clone());
        }
        let message = match &caption {
            Some(t) => format!("Captioned {changed} arrow{} \"{t}\".", if changed == 1 { "" } else { "s" }),
            None => format!("Removed the caption of {changed} arrow{}.", if changed == 1 { "" } else { "s" }),
        };
        Ok(json!({"changed": changed, "unchanged": unchanged, "caption": caption, "message": message}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}
