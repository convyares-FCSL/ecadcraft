//! The interactive forms of brief 028 (design 028 §3.2, §4): Place Terminals and Draw Bridge.
//! Brief 036 (design 036 §3.2) adds Place Terminals' type, level and position prompts.
//! Like every ECAD form they only collect and resolve values; each commit runs the command's JSON
//! form (`terminals`, `bridge`).

use cadcraft_engine::doc::{Entity, EntityKind};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::{ComponentId, Pt, TerminalId};
use serde_json::{Value, json};

use super::forms::{R, Safe, commit, not_a_point, other, paper_guard, safely};
use super::{
    ATTACHED_ACI, OUT_OF_RANGE, REFUSE_ACI, RING_PX, can_start, in_range, line_kind, pixel, preview_entity, preview_text, pt, read, ring_kind,
    terminal_for, v2,
};
use crate::strips;
use crate::terminal_cmds::{self, MAX_FENCE};
use crate::terminal_types as tt;

// ---------------------------------------------------------------- Place Terminals (§3.2)

/// `terminals`: which strip (Enter: the last one used, or a new one), the terminal type and level
/// (brief 036), then a line across the wires.
pub(crate) fn terminals(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    let (last, last_type) = read(s, |p, _| {
        let doc = p.ledger.document();
        let strip = p.last_placed.as_ref().map(|(_, c)| *c).filter(|c| strips::is_strip(doc, *c)).map(|c| (c, crate::devices::tag_words(doc, c)));
        let ty = p.last_placed.as_ref().and_then(|(k, _)| tt::of_starter(k)).map(|(t, _)| t);
        Ok((strip, ty))
    })
    .unwrap_or((None, None));
    let ty = last_type.unwrap_or_else(tt::feedthrough);
    Ok(Box::new(Safe(PlaceTerminals { stage: Stage::Strip, last, strip: None, pe: false, ty, level: Some(1), position: None, first: None })))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Strip,
    /// Brief 036: the terminal type.
    Type,
    /// Brief 036: the level of a multi-level type.
    Level,
    /// Brief 036: the position of the first crossing (Enter: the next free one).
    Position,
    Line,
}

/// The type keywords of the Type prompt.
const TYPE_WORDS: &[&str] = &["Feedthrough", "PE", "x2", "x2B", "x3", "LNPE", "SEN", "DIS", "FUSE"];

struct PlaceTerminals {
    stage: Stage,
    /// The strip Enter takes: the last one used.
    last: Option<(ComponentId, String)>,
    /// The chosen strip (`None`: a new one).
    strip: Option<ComponentId>,
    pe: bool,
    /// Brief 036: the type, its level (`None`: whole terminals), and a typed position.
    ty: &'static tt::TermType,
    level: Option<u32>,
    position: Option<String>,
    first: Option<Pt>,
}

impl PlaceTerminals {
    fn choose(&mut self, s: &Session, text: &str) -> R<Step> {
        let t = text.trim();
        if t.eq_ignore_ascii_case("new") || t.eq_ignore_ascii_case("n") {
            self.strip = None;
        } else if t.eq_ignore_ascii_case("pe") {
            self.pe = true;
            self.strip = self.last.as_ref().map(|(c, _)| *c);
        } else {
            let found = read(s, |p, _| {
                let doc = p.ledger.document();
                Ok(crate::devices::holder_of(doc, t, None).filter(|c| strips::is_strip(doc, *c)))
            })
            .ok()
            .flatten();
            match found {
                Some(c) => self.strip = Some(c),
                None => {
                    return Err(other(format!("No terminal strip \"{t}\". Press Enter for {}, type New, or a strip's tag.", self.default_words())));
                }
            }
        }
        // `PE` at the strip prompt is 028's shortcut: a PE terminal, no type prompt.
        if self.pe {
            self.ty = tt::by_code("pe").unwrap_or_else(tt::feedthrough);
            self.stage = Stage::Line;
        } else {
            self.stage = Stage::Type;
        }
        Ok(Step::Continue)
    }

    /// The level names of the chosen type on the chosen strip.
    fn level_names(&self, s: &Session) -> Vec<String> {
        match self.strip {
            Some(c) => read(s, |p, _| Ok(tt::level_names(p.ledger.document(), c, self.ty))).unwrap_or_default(),
            None => self.ty.levels.iter().map(|x| x.to_string()).collect(),
        }
    }

    fn choose_type(&mut self, text: &str) -> R<Step> {
        let t = tt::by_word(text).ok_or_else(|| other(format!("No terminal type \"{}\". Type one of {}.", text.trim(), TYPE_WORDS.join(", "))))?;
        self.ty = t;
        self.level = Some(1);
        self.stage = if t.multi() {
            Stage::Level
        } else if t.legacy() {
            Stage::Line
        } else {
            Stage::Position
        };
        Ok(Step::Continue)
    }

    fn choose_level(&mut self, s: &Session, text: &str) -> R<Step> {
        let level = read(s, |p, _| terminal_cmds::level_word(p.ledger.document(), self.strip, self.ty, text)).map_err(other)?;
        self.level = level;
        self.stage = if level.is_some() { Stage::Position } else { Stage::Line };
        Ok(Step::Continue)
    }

    fn default_words(&self) -> String {
        self.last.as_ref().map_or_else(|| "a new strip".to_string(), |(_, t)| t.clone())
    }

    /// The crossings of `from → to` the JSON form would land on, and a line of text.
    fn look(&self, s: &Session, from: Pt, to: Pt) -> Option<(Vec<(Pt, String)>, String, bool)> {
        read(s, |p, page| {
            if from.near(to, p.tolerance) {
                return Ok(None);
            }
            let doc = p.ledger.document();
            let found = crate::slash::crossings(doc, page, from, to, p.tolerance);
            let n = found.members.len();
            if n > MAX_FENCE {
                return Ok(Some((Vec::new(), format!("-> refused: {n} wires, at most {MAX_FENCE}"), true)));
            }
            if n == 0 {
                return Ok(Some((Vec::new(), "-> no wires".into(), true)));
            }
            let rings: Vec<(Pt, String)> = if self.ty.legacy() {
                let next = self.strip.map_or(1, |c| tt::highest_number(doc, c) + 1);
                found.members.iter().take(MAX_FENCE).zip(next..).map(|(m, n)| (m.at, n.to_string())).collect()
            } else {
                // Brief 036: the label each crossing would get, by §3.3's rule.
                let mut chooser = tt::Chooser::new(doc, self.strip);
                let mut named = self.position.clone();
                let mut out = Vec::new();
                for m in found.members.iter().take(MAX_FENCE) {
                    let Ok(pos) = chooser.choose(doc, self.strip, self.ty, self.level, named.take().as_deref()) else { break };
                    let label = match (self.strip, self.level) {
                        (Some(c), Some(n)) => tt::derived_label(doc, c, self.ty, &pos, n),
                        (None, Some(n)) if self.ty.multi() => format!(
                            ".{}",
                            self.ty.levels.get(n.saturating_sub(1) as usize).copied().unwrap_or("?")
                        )
                        .replacen('.', &format!("{pos}."), 1),
                        _ => pos,
                    };
                    out.push((m.at, label));
                }
                out
            };
            let words = if n == 1 { "1 wire".to_string() } else { format!("{n} wires") };
            Ok(Some((rings, format!("-> {words}"), false)))
        })
        .ok()
        .flatten()
    }

    fn finish(&mut self, s: &mut Session, from: Pt, to: Pt) -> R<Step> {
        let mut params = json!({"from": [from.x, from.y], "to": [to.x, to.y], "pe": self.pe});
        if let Some(c) = self.strip {
            params["strip"] = Value::String(c.hex());
        }
        // Brief 036: the type, level and position (a legacy type keeps 028's call).
        if !self.ty.legacy() {
            params["type"] = json!(self.ty.code);
            params["level"] = json!(self.level.map_or_else(|| "All".to_string(), |n| n.to_string()));
            if let Some(p) = &self.position {
                params["position"] = json!(p);
            }
        }
        commit(s, "terminals", &params)?;
        Ok(Step::Done)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match (self.stage, i) {
            (Stage::Strip, Input::Enter) => {
                self.strip = self.last.as_ref().map(|(c, _)| *c);
                self.stage = Stage::Type;
                Ok(Step::Continue)
            }
            (Stage::Strip, Input::Keyword(k) | Input::Text(k)) => self.choose(s, &k),
            (Stage::Type, Input::Enter) => self.choose_type(self.ty.code),
            (Stage::Type, Input::Keyword(k) | Input::Text(k)) => self.choose_type(&k),
            (Stage::Level, Input::Enter) => {
                self.stage = Stage::Position;
                Ok(Step::Continue)
            }
            (Stage::Level, Input::Keyword(k) | Input::Text(k)) => self.choose_level(s, &k),
            (Stage::Position, Input::Enter) => {
                self.position = None;
                self.stage = Stage::Line;
                Ok(Step::Continue)
            }
            (Stage::Position, Input::Keyword(k) | Input::Text(k)) => {
                let t = k.trim().to_string();
                if t.is_empty() || t.chars().count() > tt::MAX_POSITION_CHARS || t.chars().any(char::is_whitespace) {
                    return Err(other(format!(
                        "A position label is 1 to {} characters, without spaces. Enter for the next free one.",
                        tt::MAX_POSITION_CHARS
                    )));
                }
                self.position = Some(t);
                self.stage = Stage::Line;
                Ok(Step::Continue)
            }
            (Stage::Line, Input::Point(p)) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let q = pt(p);
                match self.first {
                    None => {
                        self.first = Some(q);
                        Ok(Step::Continue)
                    }
                    Some(a) if a.near(q, 1e-9) => Ok(Step::Continue),
                    Some(a) => self.finish(s, a, q),
                }
            }
            (Stage::Line, Input::Enter) => Ok(Step::Done),
            (Stage::Line, Input::Text(t)) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for PlaceTerminals {
    fn name(&self) -> &'static str {
        "TERMINALS"
    }
    fn prompt(&self, s: &Session) -> Prompt {
        let text = Accept { point: false, number: false, text: true, select: false, enter: true };
        match (self.stage, self.first) {
            (Stage::Type, _) => {
                let mut p = Prompt::new("Type", text).kw(TYPE_WORDS);
                p.default = Some(if self.ty.legacy() && self.ty.code == "ft" { "Feedthrough".to_string() } else { self.ty.offer.to_string() });
                p
            }
            (Stage::Level, _) => {
                let mut words = self.level_names(s);
                words.push("All".into());
                let refs: Vec<&str> = words.iter().map(String::as_str).collect();
                let mut p = Prompt::new("Level", text).kw(&refs);
                p.default = words.first().cloned();
                p
            }
            (Stage::Position, _) => {
                let mut p = Prompt::new("Position", text);
                p.default = Some("next free".into());
                p
            }
            (Stage::Strip, _) => {
                let mut p =
                    Prompt::new("Terminal strip", Accept { point: false, number: false, text: true, select: false, enter: true }).kw(&["New", "PE"]);
                p.default = Some(self.default_words());
                p
            }
            (Stage::Line, None) => Prompt::new("First point of the line across the wires", Accept::POINT),
            (Stage::Line, Some(a)) => Prompt::new("Second point", Accept::POINT).base(v2(a)),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "terminals", |s| self.step(s, i))
    }
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        match self.first {
            Some(a) if super::model_space(s) && in_range(cursor) => vec![line_kind(a, pt(cursor))],
            _ => Vec::new(),
        }
    }
    /// A ring and its label at each crossing (design 028 §3.2).
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let (Some(a), Some(px)) = (self.first, pixel(s)) else { return Vec::new() };
        if !in_range(cursor) {
            return Vec::new();
        }
        let Some((rings, text, refused)) = self.look(s, a, pt(cursor)) else { return Vec::new() };
        let mut out = Vec::new();
        for (at, label) in rings {
            out.push(preview_entity(s, ring_kind(at, RING_PX, px), ATTACHED_ACI));
            out.push(preview_text(s, Pt::new(at.x + 6.0 * px, at.y + 6.0 * px), px, label, ATTACHED_ACI));
        }
        let at = Pt::new(cursor.x + 10.0 * px, cursor.y + 10.0 * px);
        out.push(preview_text(s, at, px, text, if refused { REFUSE_ACI } else { ATTACHED_ACI }));
        out
    }
}

// ---------------------------------------------------------------- Draw Bridge (§4)

/// `bridge`: click a terminal, then another; the chain then continues from the second (Enter
/// ends, New starts a fresh pair). The engine passes no Shift state to forms, so continuing is
/// the default rather than Shift-click (design 028 §4).
pub(crate) fn bridge(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(DrawBridge { first: None })))
}

struct DrawBridge {
    first: Option<(TerminalId, Pt, String)>,
}

impl DrawBridge {
    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Point(p) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let Some((t, at, _)) = terminal_for(s, p) else {
                    return Err(other("No terminal there. Click a terminal of a strip (its ring)."));
                };
                let words = read(s, |p, _| Ok(terminal_cmds::terminal_words(p.ledger.document(), t))).unwrap_or_default();
                match self.first.take() {
                    None => {
                        self.first = Some((t, at, words));
                        Ok(Step::Continue)
                    }
                    Some((a, at_a, wa)) => {
                        let r = commit(s, "bridge", &json!({"from": a.hex(), "to": t.hex()}));
                        match r {
                            Ok(_) => {
                                self.first = Some((t, at, words));
                                Ok(Step::Continue)
                            }
                            Err(e) => {
                                self.first = Some((a, at_a, wa));
                                Err(e)
                            }
                        }
                    }
                }
            }
            Input::Keyword(k) if k.eq_ignore_ascii_case("new") => {
                self.first = None;
                Ok(Step::Continue)
            }
            Input::Enter => Ok(Step::Done),
            Input::Text(t) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for DrawBridge {
    fn name(&self) -> &'static str {
        "BRIDGE"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match &self.first {
            None => Prompt::new("Select first terminal", Accept::POINT),
            Some((_, at, w)) => {
                let strip = w.split(':').next().unwrap_or(w);
                Prompt::new(format!("Select second terminal of {strip} (from {w}; Enter ends)"), Accept::POINT).kw(&["New"]).base(v2(*at))
            }
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "bridge", |s| self.step(s, i))
    }
    /// The bridge as it will be drawn, to the terminal under the cursor (else to the cursor).
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        let Some((_, a, _)) = &self.first else { return Vec::new() };
        if !(super::model_space(s) && in_range(cursor)) {
            return Vec::new();
        }
        let b = terminal_for(s, cursor).map_or(pt(cursor), |(_, at, _)| at);
        let pts = terminal_cmds::staple(*a, b);
        pts.windows(2).filter_map(|w| Some(line_kind(*w.first()?, *w.get(1)?))).collect()
    }
}
