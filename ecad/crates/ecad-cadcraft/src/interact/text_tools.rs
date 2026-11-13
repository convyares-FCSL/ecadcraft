//! Brief 031's text tools (design 031 §5.5, §6.5): Show/Hide Text and Move Text. Like every ECAD form it only collects
//! and resolves values; each click commits the command's JSON form (`showtext`).

use cadcraft_engine::doc::Handle;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::{ComponentId, Form, PageId, RepresentationId, Subject};
use serde_json::json;

use super::forms::{R, Safe, commit, not_a_point, other, paper_guard, safely};
use super::{OUT_OF_RANGE, can_start, in_range, pick, read};
use crate::devices::TextOf;
use crate::project::Mapped;
use crate::text_style::TextKind;

const PICK_A_TEXT: &str = "Pick a tag, a label or a symbol.";
const FREE_TEXT: &str = "That is free text; erase it to remove it.";
const NOTHING_HIDDEN: &str = "Nothing is hidden on this symbol.";
const LEVELS: [&str; 4] = ["Device", "Page", "Project", "Symbol"];

/// The level the next click writes (design 031 §5.5; default Symbol).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Level {
    #[default]
    Symbol,
    Device,
    Page,
    Project,
}

/// `showtext`: pick derived texts to hide (or hidden ones to show), at a chosen level.
pub(crate) fn showtext(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(ShowText::default())))
}

#[derive(Default)]
struct ShowText {
    level: Level,
}

/// What a click named: a derived text (its symbol or box, what it is, whether it is shown) or a
/// symbol.
enum Picked {
    Text { rep: RepresentationId, of: TextOf, component: Option<ComponentId>, symbol: bool, shown: bool, page: PageId },
    Symbol { rep: RepresentationId, component: ComponentId, page: PageId, tag_hidden: bool },
}

impl ShowText {
    fn picked(s: &Session, h: Handle) -> Result<Picked, String> {
        read(s, |p, page| {
            let doc = p.ledger.document();
            let map = &p.pages.get(&page).ok_or(PICK_A_TEXT)?.map;
            let texts = || crate::devices::all_texts_on_page(doc, page);
            if let Some((rep, of)) = map.tag_text_of(h) {
                let r = doc.representations.get(&rep).ok_or(PICK_A_TEXT)?;
                let component = match r.subject {
                    Subject::Component(c) => Some(c),
                    _ => None,
                };
                let shown = texts().iter().find(|t| t.text.rep == rep && t.text.of == of).is_none_or(|t| t.shown);
                return Ok(Picked::Text { rep, of, component, symbol: r.form == Form::Symbol, shown, page });
            }
            match map.get(h) {
                Some(Mapped::Item(i)) => match doc.representation_of_item(i) {
                    Some((rep, r)) => match r.subject {
                        Subject::Component(c) if r.form == Form::Symbol => {
                            let tag_hidden = texts().iter().any(|t| t.text.rep == rep && t.text.of == TextOf::Tag && !t.shown);
                            Ok(Picked::Symbol { rep, component: c, page, tag_hidden })
                        }
                        _ => Err(PICK_A_TEXT.into()),
                    },
                    None => {
                        let free =
                            doc.items.get(&i).is_some_and(|x| matches!(x.kind, ecad_model::ItemKind::Primitive(ecad_model::Primitive::Text { .. })));
                        Err(if free { FREE_TEXT } else { PICK_A_TEXT }.into())
                    }
                },
                _ => Err(PICK_A_TEXT.into()),
            }
        })
    }

    /// The kind a derived text is; a box label (not a symbol's text) is a location label.
    fn kind(of: TextOf, symbol: bool) -> TextKind {
        if symbol { crate::devices::kind_of(of) } else { TextKind::LocationLabel }
    }

    fn change(&self, s: &mut Session, picked: Picked) -> R<Step> {
        let (rep, component, page, kind, show) = match picked {
            Picked::Text { rep, of, component, symbol, shown, page } => {
                (rep, component, page, Self::kind(of, symbol), if shown { "off" } else { "on" })
            }
            Picked::Symbol { tag_hidden: false, .. } => return Err(other(NOTHING_HIDDEN)),
            Picked::Symbol { rep, component, page, .. } => (rep, Some(component), page, TextKind::Tag, "on"),
        };
        let mut params = json!({"kind": kind.word(), "show": show});
        match (self.level, component) {
            (Level::Symbol, Some(_)) => params["representation"] = json!(rep.hex()),
            (Level::Device, Some(c)) => params["component"] = json!(c.hex()),
            (Level::Page, _) | (Level::Symbol | Level::Device, None) => params["page"] = json!(page.hex()),
            (Level::Project, _) => {}
        }
        let v = commit(s, "showtext", &params)?;
        let words = read(s, |p, _| Ok(echo(p.ledger.document(), &v, kind, show, component))).unwrap_or_default();
        s.echo(words);
        Ok(Step::Continue)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Point(p) if !in_range(p) => Err(other(OUT_OF_RANGE)),
            Input::Point(p) => {
                let h = pick(s, p).ok_or_else(|| other(PICK_A_TEXT))?;
                let picked = Self::picked(s, h).map_err(other)?;
                self.change(s, picked)
            }
            Input::Keyword(k) | Input::Text(k) => {
                let w = k.trim().to_ascii_lowercase();
                self.level = match LEVELS.iter().find(|x| !w.is_empty() && x.to_ascii_lowercase().starts_with(&w)).copied() {
                    Some("Device") => Level::Device,
                    Some("Page") => Level::Page,
                    Some("Project") => Level::Project,
                    Some(_) => Level::Symbol,
                    None => return Err(not_a_point(&k)),
                };
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
}

/// The echo after a change (design 031 §5.5): `Tag -X01 hidden on this symbol.`…
fn echo(doc: &ecad_model::Document, v: &serde_json::Value, kind: TextKind, show: &str, component: Option<ComponentId>) -> String {
    let what = crate::text_cmds::kind_words(kind);
    let mut what = what.to_string();
    if let Some(first) = what.get(..1) {
        what = format!("{}{}", first.to_ascii_uppercase(), what.get(1..).unwrap_or_default());
    }
    let tag = component.map(|c| crate::devices::tag_words(doc, c));
    let subject = match (&tag, kind) {
        (Some(t), TextKind::Tag) => format!("{what} {t}"),
        _ => what,
    };
    let done = if show == "off" { "hidden" } else { "shown" };
    let level = v.get("level").and_then(|l| l.as_str()).unwrap_or_default();
    let place = match level {
        "symbol" => "on this symbol".to_string(),
        "device" => format!("for {}", tag.clone().unwrap_or_default()),
        "page" => {
            let page = v.get("target").and_then(|t| t.as_str()).and_then(PageId::parse);
            page.map_or_else(|| "on this page".to_string(), |p| format!("on page {}", crate::references::page_number(doc, p)))
        }
        _ => "in the project".to_string(),
    };
    let mut line = format!("{subject} {done} {place}.");
    let strip = component.is_some_and(|c| crate::strips::is_strip(doc, c));
    if level == "device" && kind == TextKind::Tag && strip {
        line.push_str(" Strip tags show once per run unless you set a symbol.");
    }
    if v.get("changed").and_then(|c| c.as_bool()) == Some(false) {
        line = format!("{subject} is already {done} {place}.");
    }
    line
}

impl Interactive for ShowText {
    fn name(&self) -> &'static str {
        "SHOWTEXT"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        let default = match self.level {
            Level::Symbol => "Symbol",
            Level::Device => "Device",
            Level::Page => "Page",
            Level::Project => "Project",
        };
        let others: Vec<&str> = LEVELS.iter().copied().filter(|l| *l != default).collect();
        Prompt::new(format!("Pick a text to hide, or a symbol to show its hidden texts <{default}>"), Accept::POINT).kw(&others)
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "showtext", |s| self.step(s, i))
    }
}

// ---------------------------------------------------------------- Move Text (design 031 §6.5)

const PICK_TO_MOVE: &str = "Pick a tag or label to move.";
const FREE_TEXT_MOVE: &str = "That is free text; use MOVE.";
const SHOW_FIRST: &str = "Show it first (Show/Hide Text, or Show Hidden Text).";

/// `movetext`: pick a tag or label, then click where it goes (or Reset it).
pub(crate) fn movetext(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(MoveText::default())))
}

#[derive(Default)]
struct MoveText {
    /// Reset (keyword before the pick): the next pick removes the stored place.
    reset: bool,
    picked: Option<(RepresentationId, TextOf, String)>,
    /// A justification typed after **Justify**.
    justify: Option<String>,
    asking_justify: bool,
}

impl MoveText {
    fn pick_text(s: &Session, h: Handle) -> Result<(RepresentationId, TextOf, String), String> {
        read(s, |p, page| {
            let doc = p.ledger.document();
            let map = &p.pages.get(&page).ok_or(PICK_TO_MOVE)?.map;
            match map.tag_text_of(h) {
                Some((rep, of)) => {
                    let r = doc.representations.get(&rep).ok_or(PICK_TO_MOVE)?;
                    if r.form != Form::Symbol {
                        return Err(PICK_TO_MOVE.into());
                    }
                    let t = crate::devices::all_texts_on_page(doc, page).into_iter().find(|t| t.text.rep == rep && t.text.of == of);
                    if t.as_ref().is_some_and(|t| !t.shown) {
                        return Err(SHOW_FIRST.into());
                    }
                    let what = match of {
                        TextOf::Terminal(_) => "Label".to_string(),
                        _ => match r.subject {
                            Subject::Component(c) => format!("Tag {}", crate::devices::tag_words(doc, c)),
                            _ => "Tag".to_string(),
                        },
                    };
                    Ok((rep, of, what))
                }
                None => match map.get(h) {
                    Some(Mapped::Item(i)) if doc.representation_of_item(i).is_none() => Err(FREE_TEXT_MOVE.into()),
                    _ => Err(PICK_TO_MOVE.into()),
                },
            }
        })
    }

    fn params(rep: RepresentationId, of: TextOf) -> serde_json::Value {
        match of {
            TextOf::Terminal(t) => json!({"representation": rep.hex(), "kind": "point_label", "terminal": t.hex()}),
            _ => json!({"representation": rep.hex(), "kind": "tag"}),
        }
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match (i, self.picked.clone()) {
            (Input::Point(p), _) if !in_range(p) => Err(other(OUT_OF_RANGE)),
            (Input::Point(p), None) => {
                let h = pick(s, p).ok_or_else(|| other(PICK_TO_MOVE))?;
                let (rep, of, what) = Self::pick_text(s, h).map_err(other)?;
                if self.reset {
                    let mut q = Self::params(rep, of);
                    q["reset"] = json!(true);
                    commit(s, "movetext", &q)?;
                    s.echo(format!("{what} is placed automatically again."));
                    return Ok(Step::Done);
                }
                self.picked = Some((rep, of, what));
                Ok(Step::Continue)
            }
            (Input::Point(p), Some((rep, of, what))) => {
                let mut q = Self::params(rep, of);
                q["to"] = json!([p.x, p.y]);
                if let Some(j) = &self.justify {
                    q["justify"] = json!(j);
                }
                commit(s, "movetext", &q)?;
                s.echo(format!("{what} moved. Reset puts it back."));
                Ok(Step::Done)
            }
            (Input::Keyword(k) | Input::Text(k), picked) => {
                let w = k.trim().to_ascii_lowercase();
                if self.asking_justify {
                    if crate::text_style::parse_justify(&w).is_none() {
                        return Err(other(crate::text_cmds::BAD_JUSTIFY));
                    }
                    self.justify = Some(w.to_ascii_uppercase());
                    self.asking_justify = false;
                    return Ok(Step::Continue);
                }
                match (w.as_str(), picked.is_some()) {
                    ("r" | "reset", false) => {
                        self.reset = true;
                        Ok(Step::Continue)
                    }
                    ("j" | "justify", true) => {
                        self.asking_justify = true;
                        Ok(Step::Continue)
                    }
                    _ => Err(not_a_point(&k)),
                }
            }
            (Input::Enter, _) => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for MoveText {
    fn name(&self) -> &'static str {
        "MOVETEXT"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match (&self.picked, self.asking_justify) {
            (_, true) => Prompt::new("Justify (two letters: L, C or R, then T, M or B)", Accept::TEXT),
            (None, _) if self.reset => Prompt::new("Pick a tag or label to place automatically again", Accept::POINT),
            (None, _) => Prompt::new("Pick a tag or label to move", Accept::POINT).kw(&["Reset"]),
            (Some(_), _) => Prompt::new("Place the text", Accept::POINT).kw(&["Justify"]),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "movetext", |s| self.step(s, i))
    }
}
