//! The interactive forms of brief 030 (design 030 §2.3, §5.4): Strip Accessory and Jumper. Like
//! every ECAD form they only collect and resolve values; each commit runs the command's JSON form
//! (`accessory`, `jumper`).

use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::{ComponentId, Pt, TerminalId};
use serde_json::{Value, json};

use super::forms::{R, Safe, commit, other, paper_guard, safely};
use super::{OUT_OF_RANGE, can_start, in_range, line_kind, pt, read, terminal_for, v2};
use crate::strips;
use crate::terminal_cmds;

/// A picked terminal: its strip, its label and where it is drawn.
fn picked(s: &Session, p: Vec2) -> R<(TerminalId, ComponentId, String, Pt)> {
    if !in_range(p) {
        return Err(other(OUT_OF_RANGE));
    }
    let Some((t, at, _)) = terminal_for(s, p) else {
        return Err(other("No terminal there. Click a terminal of a strip (its ring)."));
    };
    let found = read(s, |p, _| {
        let doc = p.ledger.document();
        Ok(strips::strip_of_terminal(doc, t).map(|c| (c, crate::interact::terminal_name(doc, t))))
    })
    .ok()
    .flatten();
    let Some((c, label)) = found else {
        return Err(other("That connection point is not on a terminal strip. Click a terminal of a strip."));
    };
    Ok((t, c, label, at))
}

// ---------------------------------------------------------------- Strip Accessory (§2.3)

/// `accessory`: a terminal (or Start/End), then the item; repeats until Enter or Esc.
pub(crate) fn accessory(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(StripAccessory { at: None, last_strip: None })))
}

struct StripAccessory {
    /// The strip, the place (`start`, `end` or a label), its words and where to put the base.
    at: Option<(ComponentId, String, String, Option<Pt>)>,
    last_strip: Option<(ComponentId, String)>,
}

const ITEMS: [&str; 6] = ["Partition", "Endplate", "endStop", "Marker", "Label", "Remove"];

impl StripAccessory {
    fn place(&mut self, s: &mut Session, word: &str) -> R<Step> {
        let Some((c, at, _, _)) = self.at.clone() else { return Ok(Step::Continue) };
        let w = word.trim().to_ascii_lowercase();
        let gap = at != "start" && at != "end";
        let params: Value = match w.as_str() {
            "" | "p" | "partition" => json!({"strip": c.hex(), "at": at, "add": ["partition"]}),
            "e" | "endplate" | "end_plate" => json!({"strip": c.hex(), "at": at, "add": ["end_plate"]}),
            "s" | "endstop" | "end_stop" | "stop" => json!({"strip": c.hex(), "at": at, "add": ["end_stop"]}),
            "l" | "label" => json!({"strip": c.hex(), "at": at, "add": ["strip_label"]}),
            "m" | "marker" if gap => json!({"strip": c.hex(), "at": at, "on": true, "items": ["marker"]}),
            "m" | "marker" => return Err(other("A marker goes on a terminal; pick a terminal, not Start or End.")),
            "r" | "remove" => {
                let doc_items = read(s, |p, _| {
                    let doc = p.ledger.document();
                    let key = if gap { format!("{}{at}", crate::accessories::AFTER_PREFIX) } else { format!("acc.{at}") };
                    Ok(doc.components.get(&c).and_then(|x| x.properties.get(&key)).cloned())
                })
                .ok()
                .flatten();
                match doc_items.and_then(|v| v.rsplit(',').next().map(str::to_string)) {
                    Some(last) => json!({"strip": c.hex(), "at": at, "remove": [last]}),
                    None => json!({"strip": c.hex(), "at": at, "on": true, "items": null}),
                }
            }
            _ => {
                return Err(other(format!("\"{}\" is not an accessory. Choose Partition, Endplate, endStop, Marker, Label or Remove.", word.trim())));
            }
        };
        commit(s, "accessory", &params)?;
        self.last_strip = Some((c, String::new()));
        self.at = None;
        Ok(Step::Continue)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        if self.at.is_some() {
            return match i {
                Input::Keyword(k) | Input::Text(k) => self.place(s, &k),
                Input::Enter => self.place(s, ""),
                _ => Ok(Step::Continue),
            };
        }
        match i {
            Input::Point(p) => {
                let (t, c, label, at) = picked(s, p)?;
                let words = read(s, |p, _| Ok(terminal_cmds::terminal_words(p.ledger.document(), t))).unwrap_or_default();
                self.at = Some((c, label, words, Some(at)));
                Ok(Step::Continue)
            }
            Input::Keyword(k) if k.eq_ignore_ascii_case("start") || k.eq_ignore_ascii_case("end") => {
                let Some((c, _)) = self.last_strip.clone() else {
                    return Err(other("Pick a terminal first, so the tool knows the strip; then Start or End act on its ends."));
                };
                let tag = read(s, |p, _| Ok(crate::devices::tag_words(p.ledger.document(), c))).unwrap_or_default();
                let at = k.to_ascii_lowercase();
                self.at = Some((c, at.clone(), format!("{at} of {tag}"), None));
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Done),
            Input::Text(t) => Err(super::forms::not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for StripAccessory {
    fn name(&self) -> &'static str {
        "ACCESSORY"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match &self.at {
            None => Prompt::new("Select terminal", Accept::POINT).kw(&["Start", "End"]),
            Some((_, _, w, base)) => {
                let p = Prompt::new(format!("Accessory at {w} <Partition>"), Accept::TEXT).kw(&ITEMS);
                match base {
                    Some(b) => p.base(v2(*b)),
                    None => p,
                }
            }
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "accessory", |s| self.step(s, i))
    }
    /// The mark beside the terminal: a short bar across it (§7.3).
    fn preview(&self, _s: &Session, _cursor: Vec2) -> Vec<EntityKind> {
        let Some((_, _, _, Some(b))) = &self.at else { return Vec::new() };
        vec![line_kind(Pt::new(b.x - 3.0, b.y - 2.5), Pt::new(b.x + 3.0, b.y - 2.5))]
    }
}

// ---------------------------------------------------------------- Jumper (§5.4)

/// `jumper`: a terminal (one already in a link extends it), then the next terminals; ranges
/// between picks unless Skip; Enter writes one transaction, Esc nothing.
pub(crate) fn jumper(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Jumper { strip: None, extend: None, picks: Vec::new(), typed: Vec::new(), skip: false })))
}

struct Jumper {
    strip: Option<(ComponentId, String)>,
    /// The link the first pick extends.
    extend: Option<u32>,
    picks: Vec<(TerminalId, String, Pt)>,
    /// Labels typed at the prompt (positions on other pages).
    typed: Vec<String>,
    skip: bool,
}

impl Jumper {
    fn finish(&mut self, s: &mut Session) -> R<Step> {
        let Some((c, _)) = self.strip.clone() else { return Ok(Step::Done) };
        let picks: Vec<String> = self.picks.iter().map(|(t, _, _)| t.hex()).collect();
        let r = match self.extend {
            Some(n) => {
                // The new positions: ranges between consecutive picks, as a make would take them.
                let mut add: Vec<String> = Vec::new();
                let order = read(s, |p, _| Ok(crate::accessories::positions(p.ledger.document(), c))).unwrap_or_default();
                let mut prev: Option<&String> = None;
                for (_, l, _) in &self.picks {
                    match (prev, self.skip) {
                        (Some(a), false) => {
                            let (i, j) = (order.iter().position(|o| o == a), order.iter().position(|o| o == l));
                            if let (Some(i), Some(j)) = (i, j) {
                                add.extend(order.get(i.min(j)..=i.max(j)).unwrap_or_default().iter().cloned());
                            }
                        }
                        _ => add.push(l.clone()),
                    }
                    prev = Some(l);
                }
                add.extend(self.typed.iter().cloned());
                commit(s, "jumper", &json!({"link": {"strip": c.hex(), "n": n}, "add": add.join(",")}))
            }
            None => {
                let mut params = json!({"strip": c.hex(), "terminals": picks, "skip": self.skip});
                if !self.typed.is_empty() {
                    params["positions"] = json!(self.typed.join(","));
                }
                commit(s, "jumper", &params)
            }
        };
        r.map(|_| Step::Done)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Point(p) => {
                let (t, c, label, at) = picked(s, p)?;
                match &self.strip {
                    None => {
                        let found = read(s, |p, _| {
                            let doc = p.ledger.document();
                            let tag = crate::devices::tag_words(doc, c);
                            let order = crate::accessories::positions(doc, c);
                            let n = doc.components.get(&c).and_then(|x| {
                                ecad_model::links::of(&x.properties).into_iter().find(|(_, p)| p.select(&order).contains(&&label)).map(|(n, _)| n)
                            });
                            Ok((tag, n))
                        })
                        .unwrap_or_default();
                        self.strip = Some((c, found.0));
                        self.extend = found.1;
                    }
                    Some((sc, tag)) if *sc != c => {
                        let w = read(s, |p, _| Ok(terminal_cmds::terminal_words(p.ledger.document(), t))).unwrap_or_default();
                        return Err(other(format!(
                            "{w} is on another strip; a link stays on one strip. Pick a terminal of {tag}, or Enter to finish."
                        )));
                    }
                    Some(_) => {}
                }
                if !self.picks.iter().any(|(x, _, _)| *x == t) {
                    self.picks.push((t, label, at));
                }
                Ok(Step::Continue)
            }
            Input::Keyword(k) if k.eq_ignore_ascii_case("skip") => {
                self.skip = !self.skip;
                Ok(Step::Continue)
            }
            Input::Keyword(k) if k.eq_ignore_ascii_case("undo") => {
                if self.typed.pop().is_none() {
                    self.picks.pop();
                }
                if self.picks.is_empty() && self.typed.is_empty() {
                    self.strip = None;
                    self.extend = None;
                }
                Ok(Step::Continue)
            }
            Input::Text(t) if self.strip.is_some() => {
                let p = ecad_model::links::parse(&t).map_err(other)?;
                for item in p.0 {
                    match item {
                        ecad_model::links::Item::One(l) => self.typed.push(l),
                        ecad_model::links::Item::Range(a, b) => self.typed.push(format!("{a}-{b}")),
                    }
                }
                Ok(Step::Continue)
            }
            Input::Text(t) => Err(super::forms::not_a_point(&t)),
            Input::Enter => self.finish(s),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for Jumper {
    fn name(&self) -> &'static str {
        "JUMPER"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match (&self.strip, self.picks.last()) {
            (Some((_, tag)), Some((_, _, at))) => {
                let mode = if self.skip { " (Skip on)" } else { "" };
                Prompt::new(format!("Next terminal of {tag}{mode} (Enter finishes)"), Accept { text: true, ..Accept::POINT })
                    .kw(&["Skip", "Undo"])
                    .base(v2(*at))
            }
            _ => Prompt::new("Select first terminal", Accept::POINT).kw(&["Skip"]),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "jumper", |s| self.step(s, i))
    }
    /// The link as picked so far, and a rubber band to the terminal under the cursor (else the
    /// cursor).
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        let Some((_, _, last)) = self.picks.last() else { return Vec::new() };
        if !(super::model_space(s) && in_range(cursor)) {
            return Vec::new();
        }
        let mut out: Vec<EntityKind> = self.picks.windows(2).filter_map(|w| Some(line_kind(w.first()?.2, w.get(1)?.2))).collect();
        let b = terminal_for(s, cursor).map_or(pt(cursor), |(_, at, _)| at);
        out.push(line_kind(*last, b));
        out
    }
}
