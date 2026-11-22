//! Cable Join (owner request).

use super::{R, Safe, commit, not_a_point, other, paper_guard, safely};
use crate::interact::{ATTACHED_ACI, CONNECTION_POINT_RING_PX, OUT_OF_RANGE, can_start, in_range, pick, pixel, preview_entity, pt, read, ring_kind};
use crate::project::Mapped;
use cadcraft_engine::doc::{Entity, Handle};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::{CableId, Pt};
use serde_json::{Value, json};

/// `cablefan`: click a fanned cable end, then where its trunk should join the spine; `Middle`
/// puts it back in the middle.
pub(crate) fn cablefan(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(CableFan::default())))
}

/// The cable end picked (cable, end name, its spine) while the join point is asked for.
#[derive(Default)]
struct CableFan {
    picked: Option<(CableId, &'static str, ecad_electrical::cables::layout::Spine)>,
}

const JOIN_NOTHING: &str = "Nothing there. Click a cable shown as a fan.";
const JOIN_NOT_FAN: &str = "That is not a cable shown as a fan.";

impl CableFan {
    /// The fanned end a click names: a computed part of a fan-view cable, the end whose spine is
    /// nearest the click.
    fn end_at(s: &Session, h: Handle, at: Pt) -> R<(CableId, &'static str, ecad_electrical::cables::layout::Spine)> {
        read(s, |p, page| {
            let map = &p.pages.get(&page).ok_or(JOIN_NOT_FAN)?.map;
            let k = match map.get(h) {
                Some(Mapped::Fan(k)) if map.is_fan(k) => k,
                _ => return Err(JOIN_NOT_FAN.into()),
            };
            let ends = ecad_electrical::cables::layout::fan_ends(p.ledger.document(), page, k, p.tolerance, &crate::cables::draw::font_width);
            let d = |f: &ecad_electrical::cables::layout::FanEnd| f.spine.join_for(at).map_or(f64::INFINITY, |j| j.dist(at));
            let fe = ends.iter().min_by(|a, b| d(a).total_cmp(&d(b))).ok_or(JOIN_NOT_FAN)?;
            Ok((k, if fe.end == ecad_electrical::cables::layout::MarkerEnd::A { "A" } else { "B" }, fe.spine))
        })
        .map_err(other)
    }

    fn commit_join(s: &mut Session, cable: CableId, end: &str, at: Value) -> R<Step> {
        let r = commit(s, "cablefan", &json!({"cable": cable.hex(), "end": end, "at": at}))?;
        let title = read(s, |p, _| Ok(ecad_electrical::cables::cable_title(p.ledger.document(), cable))).unwrap_or_default();
        match r.get("at").and_then(Value::as_array) {
            Some(a) => s.echo(format!(
                "Cable {title} now joins end {end} at {}, {}.",
                fmt_num(a.first().and_then(Value::as_f64).unwrap_or(0.0)),
                fmt_num(a.get(1).and_then(Value::as_f64).unwrap_or(0.0))
            )),
            None => s.echo(format!("Cable {title} now joins end {end} in the middle.")),
        }
        Ok(Step::Done)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match (self.picked, i) {
            (None, Input::Point(q)) => {
                if !in_range(q) {
                    return Err(other(OUT_OF_RANGE));
                }
                let h = pick(s, q).ok_or_else(|| other(JOIN_NOTHING))?;
                self.picked = Some(Self::end_at(s, h, pt(q))?);
                Ok(Step::Continue)
            }
            (None, Input::Enter | Input::Cancel) => Ok(Step::Done),
            (None, Input::Text(t)) => Err(not_a_point(&t)),
            (Some((k, end, _)), Input::Point(q)) => {
                if !in_range(q) {
                    return Err(other(OUT_OF_RANGE));
                }
                Self::commit_join(s, k, end, json!([q.x, q.y]))
            }
            (Some((k, end, _)), Input::Keyword(w)) if w == "Middle" => Self::commit_join(s, k, end, Value::Null),
            (Some(_), Input::Enter | Input::Cancel) => Ok(Step::Done),
            (Some(_), Input::Text(t)) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

fn fmt_num(x: f64) -> String {
    let r = (x * 1e4).round() / 1e4;
    if r == r.trunc() { format!("{r:.0}") } else { format!("{r}") }
}

impl Interactive for CableFan {
    fn name(&self) -> &'static str {
        "CABLEFAN"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.picked {
            None => Prompt::new("Select a cable shown as a fan, near the end to change", Accept::POINT),
            Some(_) => {
                let mut p = Prompt::new("Specify where the cable joins", Accept::POINT);
                p.keywords = vec!["Middle".into()];
                p
            }
        }
    }
    /// The join follows the cursor along the spine.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        let (Some((_, _, spine)), Some(px)) = (self.picked, pixel(s)) else { return Vec::new() };
        match spine.join_for(pt(cursor)) {
            Some(j) => vec![preview_entity(s, ring_kind(j, CONNECTION_POINT_RING_PX, px), ATTACHED_ACI)],
            None => Vec::new(),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "cablefan", |s| self.step(s, i))
    }
}
