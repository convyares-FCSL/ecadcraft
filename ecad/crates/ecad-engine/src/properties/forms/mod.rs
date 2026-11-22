//! The interactive forms (design 008 §5 to §8, design 004 §5). Each one resolves values from
//! clicks, keys and typed points, then commits the command's JSON form with them.

pub(crate) mod add_connection_point;
pub(crate) mod cable_core;
pub(crate) mod cable_fan;
pub(crate) mod cable_slash;
pub(crate) mod cable_view;
pub(crate) mod connect;
pub(crate) mod convert;
pub(crate) mod wire;

use crate::commands::on_project;
use crate::interact::OUT_OF_RANGE;
#[cfg(feature = "fault-injection")]
use crate::interact::read;
use cadcraft_engine::doc::{Entity, EntityKind};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, EngineError, Input, Interactive, Prompt, Session, Step};
use ecad_model::End;
use serde_json::Value;
use std::panic::{AssertUnwindSafe, catch_unwind};

pub(crate) type R<T> = cadcraft_engine::Result<T>;

pub(crate) fn other(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

/// Run one step of a form. A panic (which the code is written never to raise) does not reach the
/// engine, which would put back the drawing from before the command although earlier objects of
/// this run are committed; instead the page is redrawn from the project (PROJ-02: roll forward)
/// and the command ends.
pub(crate) fn safely(s: &mut Session, name: &str, f: impl FnOnce(&mut Session) -> R<Step>) -> R<Step> {
    match catch_unwind(AssertUnwindSafe(|| f(&mut *s))) {
        Ok(r) => r,
        Err(_) => {
            let _ = on_project(s, |p, pg, _| {
                p.refresh_pages(pg, Some(name));
                Ok(())
            });
            s.echo(format!("Internal error in {}; the page was redrawn from the project. Objects already added stay.", name.to_ascii_uppercase()));
            Ok(Step::Done)
        }
    }
}

/// A form whose prompt and previews are guarded too. The engine's own guard would mark a form
/// whose `prompt` or `preview` panicked as broken and end it at the next input by putting back
/// the drawing from before the command, behind objects this run already committed. Here such a
/// panic gives a plain prompt or no preview instead; `input` and `begin` are guarded by
/// [`safely`].
pub(crate) struct Safe<M>(pub(crate) M);

/// The prompt shown when a form's own prompt failed.
fn fallback_prompt() -> Prompt {
    Prompt::new("Internal error in this prompt; press Enter or Esc to end the command", Accept::POINT)
}

/// Fault injection (`fault-injection` feature): while set, prompts and previews of the forms panic.
#[cfg(feature = "fault-injection")]
fn injected_fault(s: &Session) {
    if read(s, |p, _| Ok(p.faults.panic_in_form_preview)).unwrap_or(false) {
        crate::fault_panic();
    }
}

#[cfg(not(feature = "fault-injection"))]
fn injected_fault(_s: &Session) {}

impl<M: Interactive> Interactive for Safe<M> {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn prompt(&self, s: &Session) -> Prompt {
        catch_unwind(AssertUnwindSafe(|| {
            injected_fault(s);
            self.0.prompt(s)
        }))
        .unwrap_or_else(|_| fallback_prompt())
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        self.0.input(s, i)
    }
    fn begin(&mut self, s: &mut Session) -> R<Step> {
        self.0.begin(s)
    }
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        catch_unwind(AssertUnwindSafe(|| {
            injected_fault(s);
            self.0.preview(s, cursor)
        }))
        .unwrap_or_default()
    }
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        catch_unwind(AssertUnwindSafe(|| {
            injected_fault(s);
            self.0.preview_objects(s, cursor)
        }))
        .unwrap_or_default()
    }
}

/// Commit the command's JSON form with resolved values: the same path as `name {json}` typed at
/// `Command:` (start check, transaction, refresh, commit gate).
pub(crate) fn commit(s: &mut Session, id: &str, params: &Value) -> R<Value> {
    s.execute(id, params).map_err(|e| other(e.to_string()))
}

/// Points and picks are refused while a paper-space layout is shown (the user switched layouts
/// while the form ran): ECAD objects are drawn in the page's model space only.
pub(crate) fn paper_guard(s: &Session, i: &Input) -> R<()> {
    if matches!(i, Input::Point(_) | Input::Pick(_)) && !crate::interact::model_space(s) {
        return Err(other(crate::interact::PAPER_SPACE));
    }
    Ok(())
}

/// Text typed at a point prompt that is not a point or keyword.
pub(crate) fn not_a_point(t: &str) -> EngineError {
    if t.contains(',') || t.contains('<') { other(OUT_OF_RANGE) } else { other("Point or option keyword required.") }
}

pub(super) fn hex_of(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn end_name(e: End) -> &'static str {
    match e {
        End::A => "A",
        End::B => "B",
    }
}
