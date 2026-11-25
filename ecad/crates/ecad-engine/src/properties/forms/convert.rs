//! Convert to Device.

use super::{R, Safe, commit, hex_of, other, paper_guard, safely};
use crate::commands::MAX_HANDLES;
use crate::pointer::pick::can_start;
use cadcraft_engine::doc::color::{Color as CColor, Rgb};
use cadcraft_engine::doc::{Entity, Handle};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use serde_json::json;
use std::collections::BTreeSet;

/// `convertcomponent`: the current selection at once, else "Select objects", then one
/// conversion of what was selected.
pub(crate) fn convert(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Convert::default())))
}

#[derive(Default)]
struct Convert {
    picked: Vec<Handle>,
    members: BTreeSet<Handle>,
    removing: bool,
    /// What each pick changed, for Undo: (handles, whether they were removed).
    history: Vec<(Vec<Handle>, bool)>,
}

/// Most picks the selection's Undo remembers.
const MAX_SELECT_HISTORY: usize = 1000;
/// Most picked objects highlighted in the preview.
const MAX_HIGHLIGHT: usize = 2000;

impl Convert {
    fn run(s: &mut Session, hs: &[Handle]) -> R<Step> {
        if hs.len() > MAX_HANDLES {
            s.echo(format!("At most {MAX_HANDLES} objects can be converted at once."));
            return Ok(Step::Done);
        }
        let handles: Vec<String> = hs.iter().map(|h| h.hex()).collect();
        match commit(s, "convertcomponent", &json!({ "handles": handles })) {
            Ok(r) => s.echo(format!("Device {} created from {} objects.", hex_of(&r, "component"), hs.len())),
            // Refused: the selection itself is wrong, so the command ends (no retry loop).
            Err(e) => s.echo(e.to_string()),
        }
        Ok(Step::Done)
    }

    fn set(&mut self, hs: &[Handle], remove: bool) {
        if remove {
            self.picked.retain(|h| !hs.contains(h));
            for h in hs {
                self.members.remove(h);
            }
        } else {
            for h in hs {
                if self.members.insert(*h) {
                    self.picked.push(*h);
                }
            }
        }
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Pick(hs) => {
                let changed: Vec<Handle> = if self.removing {
                    hs.iter().copied().filter(|h| self.members.contains(h)).collect()
                } else {
                    hs.iter().copied().filter(|h| !self.members.contains(h)).collect::<BTreeSet<_>>().into_iter().collect()
                };
                if !self.removing && self.picked.len().saturating_add(changed.len()) > MAX_HANDLES {
                    return Err(other(format!("At most {MAX_HANDLES} objects can be converted at once.")));
                }
                self.set(&changed, self.removing);
                if !changed.is_empty() {
                    if self.history.len() >= MAX_SELECT_HISTORY {
                        self.history.remove(0);
                    }
                    self.history.push((changed, self.removing));
                }
                s.echo(format!("{} found, {} total", hs.len(), self.picked.len()));
                Ok(Step::Continue)
            }
            Input::Text(t) | Input::Keyword(t) => {
                match t.trim().to_ascii_lowercase().as_str() {
                    "r" | "remove" => self.removing = true,
                    "a" | "add" => self.removing = false,
                    "u" | "undo" => match self.history.pop() {
                        Some((hs, removed)) => {
                            self.set(&hs, !removed);
                            s.echo(format!("{} total", self.picked.len()));
                        }
                        None => s.echo("Nothing to undo."),
                    },
                    _ => s.echo("*Invalid selection*"),
                }
                Ok(Step::Continue)
            }
            Input::Enter if self.picked.is_empty() => {
                s.echo("Nothing selected; nothing converted.");
                Ok(Step::Done)
            }
            Input::Enter => {
                let hs = std::mem::take(&mut self.picked);
                Self::run(s, &hs)
            }
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for Convert {
    fn name(&self) -> &'static str {
        "CONVERTCOMPONENT"
    }
    /// With objects selected it converts them at once, as the JSON form converts the selection.
    fn begin(&mut self, s: &mut Session) -> R<Step> {
        safely(s, "convertcomponent", |s| {
            let sel = s.selection();
            if sel.is_empty() { Ok(Step::Continue) } else { Self::run(s, &sel) }
        })
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new(if self.removing { "Remove objects" } else { "Select objects to convert to a component" }, Accept::SELECT)
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "convertcomponent", |s| self.step(s, i))
    }
    /// The objects picked so far, highlighted (the selection itself is left alone, so Esc
    /// leaves it as it was).
    fn preview_objects(&self, s: &Session, _cursor: Vec2) -> Vec<Entity> {
        let Ok(d) = s.doc() else { return Vec::new() };
        self.picked
            .iter()
            .take(MAX_HIGHLIGHT)
            .filter_map(|h| d.entity(*h))
            .map(|e| {
                let mut e = (**e).clone();
                e.common.color = CColor::True(Rgb(0x2f, 0x6b, 0xff));
                e
            })
            .collect()
    }
}
