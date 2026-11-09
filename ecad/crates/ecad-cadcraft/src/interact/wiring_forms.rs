//! Brief 022's click-based forms (design 022 §10): Junction, Fix Wire Number and Renumber Wires.
//! Each resolves its values from clicks and typed text, then commits the command's JSON form.

use cadcraft_engine::doc::{Entity, EntityKind};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::{ConductorId, Pt};
use serde_json::{Value, json};

use super::forms::{R, Safe, commit, not_a_point, other, paper_guard, safely};
use super::{ATTACHED_ACI, OUT_OF_RANGE, can_start, in_range, pick_radius, preview_entity, pt, read};

/// The wire under a click (within the pick radius), and its number.
fn wire_under(s: &Session, p: Vec2) -> R<(ConductorId, Pt, String)> {
    let tol = pick_radius(s).unwrap_or(crate::wires::ON_WIRE).max(crate::wires::ON_WIRE);
    read(s, |proj, page| {
        let doc = proj.ledger.document();
        let on = crate::wires::wire_at(doc, page, pt(p), tol, None).ok_or("Nothing there. Click a wire.")?;
        let conn = doc.connectivity();
        let number = conn
            .net_of_conductor(on.conductor)
            .and_then(|id| conn.net(id))
            .and_then(|n| ecad_model::wiring::net_number(doc, n))
            .unwrap_or_else(|| "?".into());
        Ok((on.conductor, on.at, number))
    })
    .map_err(other)
}

/// The route of `w` on the active page, highlighted.
fn highlight(s: &Session, w: ConductorId) -> Vec<Entity> {
    let route = read(s, |p, page| Ok(ecad_model::derive::route_on_page(p.ledger.document(), w, page).map(|(_, x)| x)));
    match route {
        Ok(Some(prim)) => vec![preview_entity(s, crate::convert::kind_for_primitive(&prim), ATTACHED_ACI)],
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------- Junction

/// `junction`: click wires where junctions go, until Enter.
pub(crate) fn junction(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Junction)))
}

struct Junction;

impl Junction {
    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match i {
            Input::Point(p) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let (_, at, number) = wire_under(s, p)?;
                commit(s, "junction", &json!({"at": [at.x, at.y]}))?;
                s.echo(format!("Junction added on wire {number}; it is split there."));
                Ok(Step::Continue)
            }
            Input::Enter | Input::Cancel => Ok(Step::Done),
            Input::Text(t) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for Junction {
    fn name(&self) -> &'static str {
        "JUNCTION"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Click a wire where the junction goes", Accept::POINT)
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "junction", |s| self.step(s, i))
    }
    fn preview(&self, _s: &Session, _cursor: Vec2) -> Vec<EntityKind> {
        Vec::new()
    }
}

// ---------------------------------------------------------------- Fix Wire Number

/// `wirenumber`: click a wire, type its number; a number another net uses offers the choices.
pub(crate) fn wirenumber(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(WireNumber::default())))
}

#[derive(Default)]
struct WireNumber {
    /// The wire and its net's number.
    picked: Option<(ConductorId, String)>,
    /// A typed number another net uses, waiting for the choice.
    conflict: Option<String>,
}

impl WireNumber {
    fn fix(&mut self, s: &mut Session, w: ConductorId, value: Value, conflict: Option<&str>) -> R<Step> {
        let mut params = json!({"conductor": w.hex(), "value": value});
        if let Some(c) = conflict {
            params["conflict"] = json!(c);
        }
        match commit(s, "wirenumber", &params) {
            Ok(r) => {
                let n = r.get("number").and_then(Value::as_str).unwrap_or("?").to_string();
                let fixed = r.get("fixed").and_then(Value::as_bool).unwrap_or(false);
                if let Some(other_n) = r.get("swapped").and_then(Value::as_str) {
                    s.echo(format!("Swapped: this net is now {n} (fixed), the other {other_n}."));
                } else if r.get("merged").is_some() {
                    s.echo(format!("The nets are joined by continuations {n}; the net is {n} (fixed)."));
                } else if fixed {
                    s.echo(format!("Wire number {n} fixed on the whole net."));
                } else {
                    s.echo(format!("Wire number {n} released; it stays as an automatic number."));
                }
                Ok(Step::Done)
            }
            Err(e) => {
                let text = e.to_string();
                if conflict.is_none() && text.contains("is used by another net") {
                    // Never a dead end (owner answer Q14): ask which way.
                    s.echo(text);
                    self.conflict = value.as_str().map(str::to_string);
                    self.picked = Some((w, String::new()));
                    return Ok(Step::Continue);
                }
                Err(e)
            }
        }
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        if let (Some((w, _)), Some(v)) = (self.picked.clone(), self.conflict.clone()) {
            let choice = match &i {
                Input::Keyword(k) | Input::Text(k) => k.trim().to_ascii_lowercase(),
                Input::Enter => "another".into(),
                Input::Cancel => return Ok(Step::Done),
                _ => return Ok(Step::Continue),
            };
            let c = match choice.chars().next() {
                Some('s') => "swap",
                Some('a') | Some('c') => "another",
                Some('m') => "merge",
                _ => return Err(other("Type S (swap), A (another number) or M (merge).")),
            };
            self.conflict = None;
            return self.fix(s, w, json!(v), Some(c)).inspect_err(|_| {
                self.conflict = Some(v.clone());
            });
        }
        match (self.picked.clone(), i) {
            (None, Input::Point(p)) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let (w, _, number) = wire_under(s, p)?;
                self.picked = Some((w, number));
                Ok(Step::Continue)
            }
            (None, Input::Enter | Input::Cancel) => Ok(Step::Done),
            (None, Input::Text(t)) => Err(not_a_point(&t)),
            (None, _) => Ok(Step::Continue),
            (Some((_, n)), Input::Enter) => {
                s.echo(format!("Wire number {n} unchanged."));
                Ok(Step::Done)
            }
            (Some(_), Input::Cancel) => Ok(Step::Done),
            (Some((w, _)), Input::Text(t)) => {
                // `.` releases a fixed number.
                let value = if t.trim() == "." { Value::Null } else { json!(t.trim()) };
                self.fix(s, w, value, None)
            }
            (Some(_), _) => Ok(Step::Continue),
        }
    }
}

impl Interactive for WireNumber {
    fn name(&self) -> &'static str {
        "WIRENUMBER"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match (&self.picked, &self.conflict) {
            (Some(_), Some(v)) => {
                let mut p = Prompt::new(format!("{v} is used by another net. [Swap/Another/Merge]"), Accept::TEXT)
                    .kw(&["Swap", "Another", "Merge"]);
                p.default = Some("Another".into());
                p
            }
            (Some((_, n)), None) => {
                let mut p = Prompt::new("Wire number (. releases a fixed one)", Accept::TEXT);
                p.default = Some(n.clone());
                p
            }
            (None, _) => Prompt::new("Click a wire", Accept::POINT),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "wirenumber", |s| self.step(s, i))
    }
    fn preview_objects(&self, s: &Session, _cursor: Vec2) -> Vec<Entity> {
        self.picked.as_ref().map(|(w, _)| highlight(s, *w)).unwrap_or_default()
    }
}

// ---------------------------------------------------------------- Renumber Wires

/// `renumberwires`: asks which nets, then renumbers them in one transaction.
pub(crate) fn renumberwires(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Renumber)))
}

struct Renumber;

impl Renumber {
    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        let scope = match &i {
            Input::Enter => "unnumbered".to_string(),
            Input::Keyword(k) | Input::Text(k) => match k.trim().to_ascii_lowercase().chars().next() {
                Some('a') => "all".into(),
                Some('u') => "unnumbered".into(),
                _ => return Err(other("Type U (unnumbered nets) or A (all automatic numbers).")),
            },
            Input::Cancel => return Ok(Step::Done),
            _ => return Ok(Step::Continue),
        };
        let r = commit(s, "renumberwires", &json!({"scope": scope}))?;
        let n = r.get("renumbered").and_then(Value::as_u64).unwrap_or(0);
        s.echo(match n {
            0 => "No net needed a new number.".to_string(),
            1 => "1 net renumbered; fixed numbers are unchanged.".to_string(),
            n => format!("{n} nets renumbered; fixed numbers are unchanged."),
        });
        Ok(Step::Done)
    }
}

impl Interactive for Renumber {
    fn name(&self) -> &'static str {
        "RENUMBERWIRES"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        let mut p = Prompt::new("Renumber [Unnumbered/All]", Accept::TEXT).kw(&["Unnumbered", "All"]);
        p.default = Some("Unnumbered".into());
        p
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        safely(s, "renumberwires", |s| self.step(s, i))
    }
}
