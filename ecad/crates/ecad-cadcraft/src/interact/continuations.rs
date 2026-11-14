//! The interactive form of Caption Continuations (brief 049, design 049 §5.1): select arrows,
//! then type the caption (Enter keeps the session's last one, `R` removes). Like every ECAD form it
//! only collects values; the commit runs the JSON form `contcaption`. Esc writes nothing.

use std::collections::BTreeSet;

use cadcraft_engine::doc::Handle;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::{ComponentId, Subject};
use serde_json::json;

use super::forms::{R, Safe, commit, other, paper_guard, safely};
use super::{OUT_OF_RANGE, can_start, in_range, pick, read};
use crate::project::Mapped;

const NONE_SELECTED: &str = "No continuation arrows selected.";

/// `contcaption`: the selected arrows (if any), or pick them, then the caption.
pub(crate) fn contcaption(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    let last = read(s, |p, _| Ok(p.last_caption.clone())).unwrap_or(None);
    let mut form = Caption { arrows: BTreeSet::new(), others: 0, last, captioning: false };
    form.add(s, &s.selection());
    form.captioning = !form.arrows.is_empty();
    Ok(Box::new(Safe(form)))
}

struct Caption {
    arrows: BTreeSet<ComponentId>,
    /// Picked objects that are not arrows.
    others: usize,
    /// The session's last caption: the prompt's default.
    last: Option<String>,
    /// The caption prompt is shown.
    captioning: bool,
}

impl Caption {
    /// Add the arrows among `hs` (an arrow's symbol, name text or reference parts).
    fn add(&mut self, s: &Session, hs: &[Handle]) {
        let found = read(s, |p, page| {
            let doc = p.ledger.document();
            let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return Ok(Vec::new()) };
            Ok(hs
                .iter()
                .take(crate::commands::MAX_HANDLES)
                .map(|h| {
                    let rep = match map.get(*h) {
                        Some(Mapped::Item(i)) => doc.representation_of_item(i).map(|(_, r)| r),
                        Some(Mapped::Tag(r) | Mapped::Marker(r, _) | Mapped::Xref(r)) => doc.representations.get(&r),
                        _ => None,
                    };
                    match rep.map(|r| r.subject) {
                        Some(Subject::Component(c)) if crate::continuations::is_continuation(doc, c) => Some(c),
                        _ => None,
                    }
                })
                .collect::<Vec<_>>())
        })
        .unwrap_or_default();
        for c in found {
            match c {
                Some(c) => {
                    self.arrows.insert(c);
                }
                None => self.others += 1,
            }
        }
    }

    fn selected_line(&self) -> String {
        let n = self.arrows.len();
        let mut line = format!("{n} arrow{} selected", if n == 1 { "" } else { "s" });
        if self.others > 0 {
            line.push_str(&format!(" ({} other object{} ignored)", self.others, if self.others == 1 { "" } else { "s" }));
        }
        line.push('.');
        line
    }

    fn finish(&mut self, s: &mut Session, caption: Option<String>) -> R<Step> {
        let ids: Vec<String> = self.arrows.iter().map(|c| c.hex()).collect();
        commit(s, "contcaption", &json!({"continuations": ids, "caption": caption}))?;
        Ok(Step::Done)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        if !self.captioning {
            return match i {
                Input::Point(p) if !in_range(p) => Err(other(OUT_OF_RANGE)),
                Input::Point(p) => {
                    if let Some(h) = pick(s, p) {
                        self.add(s, &[h]);
                    }
                    Ok(Step::Continue)
                }
                Input::Pick(hs) => {
                    self.add(s, &hs);
                    Ok(Step::Continue)
                }
                Input::Enter if self.arrows.is_empty() => {
                    self.others = 0;
                    Err(other(NONE_SELECTED))
                }
                Input::Enter => {
                    s.echo(self.selected_line());
                    self.captioning = true;
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            };
        }
        match i {
            Input::Keyword(k) | Input::Text(k) if k.trim().eq_ignore_ascii_case("r") || k.trim().eq_ignore_ascii_case("remove") => {
                self.finish(s, None)
            }
            Input::Keyword(t) | Input::Text(t) => {
                let caption = crate::continuations::caption_text(Some(&t)).map_err(other)?;
                match caption {
                    Some(c) => self.finish(s, Some(c)),
                    None => Err(other(crate::continuations::CAPTION_RULE)),
                }
            }
            Input::Enter => match self.last.clone() {
                Some(c) => self.finish(s, Some(c)),
                None => Err(other("Type a caption, or R to remove it.")),
            },
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for Caption {
    fn name(&self) -> &'static str {
        "CONTCAPTION"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if !self.captioning {
            return Prompt::new("Select continuation arrows", Accept::SELECT);
        }
        let text = match &self.last {
            Some(c) => format!("Caption <{c}> or [Remove]"),
            None => "Caption or [Remove]".to_string(),
        };
        Prompt::new(text, Accept::TEXT).kw(&["Remove"])
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        safely(s, "contcaption", |s| self.step(s, i))
    }
}
