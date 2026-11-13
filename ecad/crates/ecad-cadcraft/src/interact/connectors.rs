//! The interactive form of brief 044's `connector` (design 044 §4.1, §4.2): Place ▸ Connector. Its
//! five questions run as prompts with keywords (041's intent line shows them as chips); like every
//! ECAD form it only collects answers, and each commit runs the JSON form.

use cadcraft_engine::doc::{Entity, EntityKind};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::{ComponentId, Pt};
use serde_json::{Value, json};

use super::forms::{R, Safe, commit, not_a_point, other, paper_guard, safely};
use super::{
    ATTACHED_ACI, OUT_OF_RANGE, REFUSE_ACI, RING_PX, can_start, in_range, line_kind, pixel, preview_entity, preview_text, pt, read, ring_kind, v2,
};
use crate::connectors::{self as cn, Family};
use crate::defaults::{self, Scope};
use crate::symbols::Side;

/// `connector`: which connector, halves, type, pins, pin names, then how to draw it.
pub(crate) fn connector(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    let first = read(s, |p, _| {
        let doc = p.ledger.document();
        let dc = &defaults::connectors::MAKER_HALVES;
        let halves = match defaults::resolve(doc, dc, Scope::project()).word() {
            "socket" => Side::Socket,
            "plug" => Side::Plug,
            _ => Side::Both,
        };
        let family = Family::parse(defaults::resolve(doc, &defaults::connectors::MAKER_FAMILY, Scope::project()).word()).unwrap_or(Family::Plain);
        let n = defaults::resolve(doc, &defaults::connectors::MAKER_PINS, Scope::project()).number();
        let pins = if n.is_finite() && (1.0..=cn::MAX_PINS as f64).contains(&n) { n as usize } else { 4 };
        let names = defaults::resolve(doc, &defaults::connectors::MAKER_NAMES, Scope::project()).word().to_string();
        let pins_layout = defaults::resolve(doc, &defaults::connectors::MAKER_LAYOUT, Scope::project()).word() == "pins";
        let next = p.tag_high.peek_auto(doc, &crate::devices::prefix_for(doc, crate::devices::starter("socket4")?)).unwrap_or_else(|_| "-X?".into());
        Ok((halves, family, pins, names, pins_layout, next))
    })
    .map_err(other)?;
    let (halves, family, pins, names, pins_layout, next) = first;
    Ok(Box::new(Safe(PlaceConnector {
        stage: Stage::Device,
        device: None,
        next,
        halves,
        family,
        pins,
        names,
        list: None,
        pins_layout,
        swap: false,
        rotation: 0.0,
        first: None,
        drawn: 0,
    })))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Device,
    Halves,
    Type,
    Pins,
    Names,
    List,
    Draw,
    Point,
}

const HALF_WORDS: &[&str] = &["Socket", "Plug", "Both"];
const TYPE_WORDS: &[&str] = &["Plain", "Dsub", "M12", "RJ45", "USBA", "USBB"];
const NAME_WORDS: &[&str] = &["Numbers", "Letters", "List"];
const DRAW_WORDS: &[&str] = &["Row", "Pins"];

struct PlaceConnector {
    stage: Stage,
    /// The connector added to (`None`: a new one) and its tag.
    device: Option<(ComponentId, String)>,
    /// The tag a new connector gets.
    next: String,
    halves: Side,
    family: Family,
    pins: usize,
    names: String,
    list: Option<Vec<String>>,
    pins_layout: bool,
    swap: bool,
    rotation: f64,
    first: Option<Pt>,
    /// Pin lines committed so far (the tool repeats).
    drawn: usize,
}

fn word_of(side: Side) -> &'static str {
    match side {
        Side::Socket => "Socket",
        Side::Plug => "Plug",
        Side::Both => "Both",
    }
}

impl PlaceConnector {
    fn bulkhead(&self) -> bool {
        self.family.bulkhead().is_some()
    }

    fn params(&self) -> Value {
        let mut v = json!({
            "halves": self.halves.word(),
            "family": self.family.word(),
            "pins": self.pins,
            "layout": if self.pins_layout && !self.bulkhead() { "pins" } else { "row" },
        });
        v["names"] = match (&self.list, self.names.as_str()) {
            (Some(l), _) => json!(l),
            (None, "letters") => json!("letters"),
            _ => json!("numbers"),
        };
        if let Some((c, _)) = &self.device {
            v["device"] = json!(c.hex());
            if let Some(o) = v.as_object_mut() {
                o.remove("family");
                o.remove("pins");
            }
        }
        v
    }

    fn choose_device(&mut self, s: &Session, text: &str) -> R<Step> {
        let t = text.trim();
        if t.eq_ignore_ascii_case("new") || t.is_empty() {
            self.device = None;
            self.stage = Stage::Halves;
            return Ok(Step::Continue);
        }
        let found = read(s, |p, _| {
            let doc = p.ledger.document();
            let tag = ecad_model::tag::normalise(t, crate::defaults::print::tag_style(doc))?;
            let c = crate::devices::holder_of(doc, &tag, None)
                .ok_or_else(|| format!("No device {tag}. Type a connector's tag, or Enter for a new one."))?;
            if !cn::is_connector(doc, c) {
                return Err(cn::not_a_connector(doc, c));
            }
            let d = cn::drawn(doc, c);
            Ok((c, crate::devices::tag_words(doc, c), cn::half_family(doc, c, Side::Socket), cn::pin_count(doc, c, &d), d))
        })
        .map_err(other)?;
        let (c, tag, family, pins, d) = found;
        self.device = Some((c, tag));
        self.family = family;
        self.pins = pins;
        // Only the sides it lacks, or Both for pins.
        self.halves = match (d.socket.is_empty(), d.plug.is_empty()) {
            (true, false) => Side::Socket,
            (false, true) => Side::Plug,
            _ => Side::Both,
        };
        self.stage = Stage::Halves;
        Ok(Step::Continue)
    }

    fn choose_halves(&mut self, text: &str) -> R<Step> {
        self.halves = match text.trim().to_ascii_lowercase().as_str() {
            "socket" | "s" => Side::Socket,
            "plug" | "p" => Side::Plug,
            "both" | "b" => Side::Both,
            _ => return Err(other("Type Socket, Plug or Both.")),
        };
        self.stage = if self.device.is_some() { Stage::Draw } else { Stage::Type };
        Ok(Step::Continue)
    }

    fn choose_type(&mut self, text: &str) -> R<Step> {
        self.family = Family::parse(text).ok_or_else(|| other(format!("Type one of {}.", TYPE_WORDS.join(", "))))?;
        if self.bulkhead() {
            self.pins_layout = false;
            self.stage = Stage::Point;
        } else {
            if !self.family.sizes().contains(&self.pins)
                && let Some(n) = self.family.sizes().first()
            {
                self.pins = *n;
            }
            self.stage = Stage::Pins;
        }
        Ok(Step::Continue)
    }

    fn choose_pins(&mut self, text: &str) -> R<Step> {
        let n: usize = text.trim().parse().map_err(|_| other(format!("Type a pin count from 1 to {}.", cn::MAX_PINS)))?;
        if !(1..=cn::MAX_PINS).contains(&n) {
            return Err(other(format!("A connector has 1 to {} pins.", cn::MAX_PINS)));
        }
        self.pins = n;
        self.stage = Stage::Names;
        Ok(Step::Continue)
    }

    fn choose_names(&mut self, text: &str) -> R<Step> {
        let w = text.trim().to_ascii_lowercase();
        match w.as_str() {
            "numbers" | "letters" => {
                if w == "letters" && self.pins > 26 {
                    return Err(other("Letters name at most 26 pins. Use Numbers or a list."));
                }
                self.names = w;
                self.list = None;
                self.stage = Stage::Draw;
            }
            "list" => self.stage = Stage::List,
            _ => return Err(other("Type Numbers, Letters or List.")),
        }
        Ok(Step::Continue)
    }

    fn choose_list(&mut self, text: &str) -> R<Step> {
        let v = cn::parse_list(text).map_err(other)?;
        if v.len() != self.pins {
            return Err(other(format!("Give {} pin names; got {}.", self.pins, v.len())));
        }
        self.list = Some(v);
        self.stage = Stage::Draw;
        Ok(Step::Continue)
    }

    fn choose_draw(&mut self, text: &str) -> R<Step> {
        self.pins_layout = match text.trim().to_ascii_lowercase().as_str() {
            "row" | "r" => false,
            "pins" | "p" => true,
            _ => return Err(other("Type Row or Pins.")),
        };
        if !self.pins_layout && self.device.is_none() && !self.family.sizes().contains(&self.pins) {
            return Err(other(self.family.size_refusal()));
        }
        self.stage = Stage::Point;
        Ok(Step::Continue)
    }

    fn row(&mut self, s: &mut Session, at: Pt) -> R<Step> {
        let mut params = self.params();
        params["at"] = json!([at.x, at.y]);
        params["rotation"] = json!(self.rotation);
        params["swap"] = json!(self.swap);
        commit(s, "connector", &params)?;
        Ok(Step::Done)
    }

    fn line(&mut self, s: &mut Session, a: Pt, b: Pt) -> R<Step> {
        let mut params = self.params();
        if a.near(b, 1e-9) {
            let w = read(s, |p, page| Ok(crate::wires::wire_at(p.ledger.document(), page, a, crate::wires::ON_WIRE, None).map(|o| o.conductor)))
                .map_err(other)?
                .ok_or_else(|| other("Click on a wire, or draw a line across the wires."))?;
            params["wire"] = json!(w.hex());
            params["line"] = json!([[a.x, a.y], [a.x, a.y]]);
        } else {
            params["line"] = json!([[a.x, a.y], [b.x, b.y]]);
        }
        let v = commit(s, "connector", &params)?;
        // The tool repeats on the connector it made (one transaction per line).
        if self.device.is_none()
            && let (Some(c), Some(t)) = (v.get("component").and_then(Value::as_str), v.get("tag").and_then(Value::as_str))
            && let Ok(n) = u128::from_str_radix(c, 16)
        {
            self.device = Some((ComponentId::from(n), t.to_string()));
        }
        self.drawn += 1;
        self.first = None;
        Ok(Step::Continue)
    }

    fn intent(&self, s: &Session, cursor: Pt) -> Option<(String, bool)> {
        let (tag, new) = match &self.device {
            Some((_, t)) => (t.clone(), false),
            None => (self.next.clone(), true),
        };
        if !self.pins_layout || self.bulkhead() {
            let what = match (self.bulkhead(), self.halves) {
                (true, _) => format!("places {tag} ({} bulkhead)", self.family.name()),
                (false, Side::Both) => format!("places {tag}.S:1\u{2013}{n} and {tag}.P:1\u{2013}{n}", n = self.pins),
                (false, side) => format!("places the {} half of {tag}", side.word()),
            };
            return Some((what, false));
        }
        let a = self.first?;
        read(s, |p, page| {
            let doc = p.ledger.document();
            let found = crate::slash::crossings(doc, page, a, cursor, p.tolerance);
            let n = found.members.len();
            if n == 0 {
                return Ok(Some(("-> no wires".to_string(), true)));
            }
            if n > cn::MAX_PINS {
                return Ok(Some((format!("-> refused: {n} wires, at most {}", cn::MAX_PINS), true)));
            }
            let names: Vec<String> = found.members.iter().take(6).map(|m| crate::slash::wire_name(doc, m.conductor)).collect();
            let who = if new { tag.clone() } else { format!("{tag} (next free pins)") };
            Ok(Some((
                format!(
                    "cuts {n} wire{}: {}; {who}; socket side keeps the number, plug side is the same net",
                    if n == 1 { "" } else { "s" },
                    names.join(", ")
                ),
                false,
            )))
        })
        .ok()
        .flatten()
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match (self.stage, i) {
            (Stage::Device, Input::Enter) => {
                self.stage = Stage::Halves;
                Ok(Step::Continue)
            }
            (Stage::Device, Input::Keyword(k) | Input::Text(k)) => self.choose_device(s, &k),
            (Stage::Halves, Input::Enter) => self.choose_halves(word_of(self.halves)),
            (Stage::Halves, Input::Keyword(k) | Input::Text(k)) => self.choose_halves(&k),
            (Stage::Type, Input::Enter) => self.choose_type(self.family.word()),
            (Stage::Type, Input::Keyword(k) | Input::Text(k)) => self.choose_type(&k),
            (Stage::Pins, Input::Enter) => self.choose_pins(&self.pins.to_string()),
            (Stage::Pins, Input::Keyword(k) | Input::Text(k)) => self.choose_pins(&k),
            (Stage::Names, Input::Enter) => self.choose_names(&self.names.clone()),
            (Stage::Names, Input::Keyword(k) | Input::Text(k)) => self.choose_names(&k),
            (Stage::List, Input::Keyword(k) | Input::Text(k)) => self.choose_list(&k),
            (Stage::List, Input::Enter) => Err(other(format!("Type {} pin names, comma-separated (L,N,PE).", self.pins))),
            (Stage::Draw, Input::Enter) => self.choose_draw(if self.pins_layout { "pins" } else { "row" }),
            (Stage::Draw, Input::Keyword(k) | Input::Text(k)) => self.choose_draw(&k),
            (Stage::Point, Input::Keyword(k) | Input::Text(k)) if k.trim().eq_ignore_ascii_case("r") && !self.pins_layout => {
                self.rotation = (self.rotation + 90.0) % 360.0;
                Ok(Step::Continue)
            }
            (Stage::Point, Input::Keyword(k) | Input::Text(k)) if k.trim().eq_ignore_ascii_case("x") && !self.pins_layout => {
                self.swap = !self.swap;
                Ok(Step::Continue)
            }
            (Stage::Point, Input::Point(p)) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let q = pt(p);
                if !self.pins_layout || self.bulkhead() {
                    return self.row(s, q);
                }
                match self.first {
                    None => {
                        self.first = Some(q);
                        Ok(Step::Continue)
                    }
                    Some(a) => self.line(s, a, q),
                }
            }
            (Stage::Point, Input::Enter) => {
                // A single click on a wire places one pin.
                match self.first {
                    Some(a) => self.line(s, a, a),
                    None => Ok(Step::Done),
                }
            }
            (Stage::Point, Input::Text(t)) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for PlaceConnector {
    fn name(&self) -> &'static str {
        "CONNECTOR"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        let text = Accept { point: false, number: false, text: true, select: false, enter: true };
        let with = |label: &str, words: &[&str], default: String| {
            let mut p = Prompt::new(label, text).kw(words);
            p.default = Some(default);
            p
        };
        match self.stage {
            Stage::Device => with("Connector", &["New"], self.next.clone()),
            Stage::Halves => with("Halves", HALF_WORDS, word_of(self.halves).to_string()),
            Stage::Type => with("Type", TYPE_WORDS, self.family.keyword().to_string()),
            Stage::Pins => {
                let sizes: Vec<String> = self.family.sizes().iter().map(ToString::to_string).collect();
                let refs: Vec<&str> = sizes.iter().map(String::as_str).collect();
                with("Pins", &refs, self.pins.to_string())
            }
            Stage::Names => with("Pin names", NAME_WORDS, {
                let mut w = self.names.clone();
                if let Some(c) = w.get_mut(..1) {
                    c.make_ascii_uppercase();
                }
                w
            }),
            Stage::List => Prompt::new("Pin names, comma-separated", text),
            Stage::Draw => with("Draw", DRAW_WORDS, if self.pins_layout { "Pins".into() } else { "Row".into() }),
            Stage::Point if !self.pins_layout || self.bulkhead() => {
                Prompt::new("Insertion point (R rotates, X swaps the halves)", Accept::POINT).kw(&["R", "X"])
            }
            Stage::Point => match self.first {
                None if self.drawn > 0 => Prompt::new("Next line across the wires, first point (Enter ends)", Accept::POINT),
                None => Prompt::new("Line across the wires, first point (or click one wire)", Accept::POINT),
                Some(a) => Prompt::new("Second point (Enter: just this wire)", Accept::POINT).base(v2(a)),
            },
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "connector", |s| self.step(s, i))
    }
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        match self.first {
            Some(a) if super::model_space(s) && in_range(cursor) => vec![line_kind(a, pt(cursor))],
            _ => Vec::new(),
        }
    }
    /// The intent line at the cursor, and a ring at each crossing (design 044 §4.2).
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        if self.stage != Stage::Point || !in_range(cursor) {
            return Vec::new();
        }
        let Some(px) = pixel(s) else { return Vec::new() };
        let mut out = Vec::new();
        if let Some(a) = self.first
            && let Ok(found) = read(s, |p, page| Ok(crate::slash::crossings(p.ledger.document(), page, a, pt(cursor), p.tolerance)))
        {
            for m in found.members.iter().take(cn::MAX_PINS) {
                out.push(preview_entity(s, ring_kind(m.at, RING_PX, px), ATTACHED_ACI));
            }
        }
        if let Some((text, refused)) = self.intent(s, pt(cursor)) {
            let at = Pt::new(cursor.x + 10.0 * px, cursor.y + 10.0 * px);
            out.push(preview_text(s, at, px, text, if refused { REFUSE_ACI } else { ATTACHED_ACI }));
        }
        out
    }
}
