//! Cable Slash (design 004 §5).

use super::{R, Safe, commit, hex_of, not_a_point, other, paper_guard, safely};
use crate::interact::{
    ATTACHED_ACI, OUT_OF_RANGE, REFUSE_ACI, RING_PX, can_start, in_range, line_kind, pixel, preview_entity, preview_text, pt, read, ring_kind, v2,
};
use cadcraft_engine::doc::{Entity, EntityKind};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_electrical::cables::slash;
use ecad_model::{CableId, Pt, Subject, WireId};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// `cableslash`: two clicks across wires make one cable of the wires crossed.
pub(crate) fn cableslash(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(CableSlash::default())))
}

#[derive(Default)]
struct CableSlash {
    first: Option<Pt>,
    /// Brief 059 (design 059 §5.6): the cable picked (`Cable` keyword, or the options' picker);
    /// `None` is New cable (Q7).
    cable: Option<CableId>,
    /// Asking `Cable tag or [New] <New>`.
    asking: bool,
}

/// Brief 059: the `Cable` keyword's prompt.
pub(crate) const CABLE_PROMPT: &str = "Cable tag or [New] <New>";

/// Rings drawn by the crossing preview at most (the text still counts every crossing).
const MAX_PREVIEW_RINGS: usize = 200;

/// What the crossing preview shows for a slash: ring positions (will join, blocks) and the text
/// line, with whether the slash would be refused.
struct SlashPreview {
    join: Vec<Pt>,
    block: Vec<Pt>,
    text: String,
    refused: bool,
}

impl CableSlash {
    fn tolerance(s: &Session) -> f64 {
        read(s, |p, _| Ok(p.tolerance)).unwrap_or(1e-6)
    }

    /// The preview for `from → to`, from the same crossing function the JSON form uses. `None`
    /// when the project cannot be read now (busy) or the slash has no length.
    fn draw(s: &Session, from: Pt, to: Pt, cable: Option<CableId>) -> Option<SlashPreview> {
        read(s, |p, page| {
            if from.near(to, p.tolerance) {
                return Ok(None);
            }
            let doc = p.ledger.document();
            let found = slash::crossings(doc, page, from, to, p.tolerance);
            let blocked = slash::blocked(doc, &found.members);
            let blocking: BTreeSet<ecad_model::CorePosition> = blocked.iter().map(|(m, _)| m.position).collect();
            let (n, others) = (found.members.len(), found.ignored.len());
            let (text, refused) = if n > slash::MAX_CABLE_MEMBERS {
                (format!("-> refused: more than {} wires", slash::MAX_CABLE_MEMBERS), true)
            } else if !blocked.is_empty() {
                (format!("-> refused: {} already in a cable", slash::wires(blocked.len())), true)
            } else if n == 0 {
                // Refused too (design 004 §5.2: red whenever the slash would be refused).
                ("-> no wires".to_string(), true)
            } else if others > 0 {
                let objects = if others == 1 { "1 other object".to_string() } else { format!("{others} other objects") };
                (format!("-> {}, {objects} ignored", slash::wires(n)), false)
            } else {
                (format!("-> {}", slash::wires(n)), false)
            };
            // Brief 059 (design 059 §5.6): onto a cable not drawn, the cores the wires get, in
            // order, so the order is seen before the commit; more wires than free cores is refused.
            let (text, refused) = match cable {
                Some(k) if !refused && doc.cables.contains_key(&k) && !ecad_electrical::cables::is_drawn(doc, k) => {
                    let tag = ecad_electrical::cables::cable_title(doc, k);
                    match ecad_electrical::cables::commands::cables::assign_cores(doc, k, n) {
                        Ok(cores) => {
                            let labels: Vec<String> = cores.into_iter().flatten().collect();
                            (format!("{text} onto {tag} ({})", labels.join(", ")), false)
                        }
                        Err(e) => (format!("-> refused: {e}"), true),
                    }
                }
                Some(k) if !refused && doc.cables.contains_key(&k) => {
                    (format!("{text}, continuing {}", ecad_electrical::cables::cable_title(doc, k)), false)
                }
                _ => (text, refused),
            };
            let (mut join, mut block) = (Vec::new(), Vec::new());
            for m in found.members.iter().take(MAX_PREVIEW_RINGS) {
                if blocking.contains(&m.position) {
                    block.push(m.at);
                } else {
                    join.push(m.at);
                }
            }
            Ok(Some(SlashPreview { join, block, text, refused }))
        })
        .ok()
        .flatten()
    }

    fn finish(s: &mut Session, from: Pt, to: Pt, cable: Option<CableId>) -> R<Step> {
        let mut params = json!({"from": [from.x, from.y], "to": [to.x, to.y]});
        if let Some(k) = cable {
            params["cable"] = json!(k.hex());
        }
        let r = commit(s, "cableslash", &params)?;
        let ids = |key: &str| -> Vec<u128> {
            r.get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().and_then(|t| u128::from_str_radix(t, 16).ok()))
                .collect()
        };
        let (members, loose) = (ids("conductors"), ids("lines"));
        let (ignored, touched) = (ids("ignored").len(), ids("touched").len());
        let cable = u128::from_str_radix(&hex_of(&r, "cable"), 16).unwrap_or_default();
        let lines = read(s, |p, _| {
            let doc = p.ledger.document();
            let members: Vec<ecad_model::CorePosition> = members
                .iter()
                .map(|c| ecad_model::CorePosition::Wire(WireId::from(*c)))
                .chain(loose.iter().map(|r| ecad_model::connectivity::wire_order::core_position(doc, (*r).into())))
                .collect();
            Ok(slash::created_lines(doc, cable.into(), &members, ignored, touched))
        })
        .unwrap_or_else(|_| vec![format!("Cable {} created.", hex_of(&r, "cable"))]);
        for l in lines {
            s.echo(l);
        }
        Ok(Step::Done)
    }

    /// Brief 059: the cable `text` names for the picker: a tag (or ID) of a cable that is not
    /// drawn yet, or drawn and not slashed on this page (032).
    fn pick(s: &Session, text: &str) -> R<CableId> {
        read(s, |p, page| {
            let doc = p.ledger.document();
            let k = ecad_electrical::cables::commands::cables::cable_named(doc, text).map_err(|e| e.to_string())?;
            if doc.representations_of(Subject::Cable(k)).any(|(_, r)| r.page == page && r.role == ecad_model::Role::Slash) {
                return Err(format!("{} already has a slash on this page.", ecad_electrical::cables::cable_title(doc, k)));
            }
            Ok(k)
        })
        .map_err(other)
    }

    /// The status line after a pick: `Slash the wires of -W03 (4 cores).`
    fn picked_words(s: &Session, k: CableId) -> String {
        read(s, |p, _| {
            let doc = p.ledger.document();
            let tag = ecad_electrical::cables::cable_title(doc, k);
            Ok(match ecad_electrical::terminal_strips::cores::cable_cores(doc, k).and_then(|c| c.offer) {
                Some(o) if !ecad_electrical::cables::is_drawn(doc, k) => format!("Slash the wires of {tag} ({} cores).", o.len()),
                _ if !ecad_electrical::cables::is_drawn(doc, k) => format!("Slash the wires of {tag}."),
                _ => format!("Slash the wires that continue {tag} here."),
            })
        })
        .unwrap_or_default()
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        // Brief 059 (design 059 §5.6): `Cable` picks a cable by tag, `New` (or Enter) a new one.
        if self.asking {
            return match i {
                Input::Keyword(k) if k.eq_ignore_ascii_case("New") => {
                    self.cable = None;
                    self.asking = false;
                    s.echo("New cable: the next free tag.".to_string());
                    Ok(Step::Continue)
                }
                Input::Enter => {
                    self.cable = None;
                    self.asking = false;
                    Ok(Step::Continue)
                }
                Input::Text(t) | Input::Keyword(t) => {
                    let k = Self::pick(s, &t)?;
                    self.cable = Some(k);
                    self.asking = false;
                    s.echo(Self::picked_words(s, k));
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            };
        }
        match i {
            Input::Keyword(k) if k.eq_ignore_ascii_case("Cable") => {
                self.asking = true;
                Ok(Step::Continue)
            }
            Input::Point(p) => {
                if !in_range(p) {
                    return Err(other(OUT_OF_RANGE));
                }
                let q = pt(p);
                match self.first {
                    None => {
                        self.first = Some(q);
                        Ok(Step::Continue)
                    }
                    // The same point again is ignored (as Wire ignores a repeated point).
                    Some(a) if a.near(q, Self::tolerance(s)) => Ok(Step::Continue),
                    // A refusal keeps the first point: the prompt of step 2 repeats.
                    Some(a) => Self::finish(s, a, q, self.cable),
                }
            }
            Input::Keyword(k) if k == "Undo" => {
                self.first = None;
                Ok(Step::Continue)
            }
            // A slash needs two clicks: Enter never commits.
            Input::Enter => Ok(Step::Done),
            Input::Text(t) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for CableSlash {
    fn name(&self) -> &'static str {
        "CABLESLASH"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if self.asking {
            return Prompt::new(CABLE_PROMPT, Accept::TEXT).kw(&["New"]).default("New");
        }
        match self.first {
            None => Prompt::new("Specify first point of cable slash", Accept::POINT).kw(&["Cable"]),
            Some(a) => Prompt::new("Specify second point of cable slash", Accept::POINT).kw(&["Undo", "Cable"]).base(v2(a)),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "cableslash", |s| self.step(s, i))
    }
    /// The rubber band from the first point to the cursor.
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        match self.first {
            Some(a) if crate::interact::model_space(s) && in_range(cursor) => vec![line_kind(a, pt(cursor))],
            _ => Vec::new(),
        }
    }
    /// The crossing preview (design 004 §5.2): a green ring on each wire that will join, a red one
    /// on each wire that blocks the slash, and one line of text beside the cursor.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let (Some(a), Some(px)) = (self.first, pixel(s)) else { return Vec::new() };
        if !in_range(cursor) {
            return Vec::new();
        }
        let Some(draw) = Self::draw(s, a, pt(cursor), self.cable) else { return Vec::new() };
        let ring = |at: &Pt, aci: u8| [RING_PX - 1.0, RING_PX, RING_PX + 1.0].map(|d| preview_entity(s, ring_kind(*at, d, px), aci));
        let mut out: Vec<Entity> = draw.join.iter().flat_map(|at| ring(at, ATTACHED_ACI)).collect();
        out.extend(draw.block.iter().flat_map(|at| ring(at, REFUSE_ACI)));
        let at = Pt::new(cursor.x + 10.0 * px, cursor.y + 10.0 * px);
        out.push(preview_text(s, at, px, draw.text, if draw.refused { REFUSE_ACI } else { ATTACHED_ACI }));
        out
    }
}
