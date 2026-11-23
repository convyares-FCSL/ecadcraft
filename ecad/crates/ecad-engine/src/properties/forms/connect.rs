//! Connect and Disconnect.

use super::{R, Safe, commit, end_name, not_a_point, other, paper_guard, safely};
use crate::interact::{
    ATTACHED_ACI, LOOSE_ACI, RING_PX, can_start, capture_at, capture_preview, connection_point_for, in_range, line_kind, pick, pixel, preview_entity,
    pt, read, ring_kind, v2,
};
use crate::project::Mapped;
use cadcraft_engine::doc::{Entity, EntityKind};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_electrical::cables::words::OUT_OF_RANGE;
use ecad_model::{End, Pt, RouteId, Subject};
use serde_json::json;

/// `connect`: pick a wire near the end, then a connection point; repeat until Enter.
pub(crate) fn connect(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(ConnectForm { connect: true, end: None })))
}

/// `disconnect`: pick a wire near an attached end; repeat until Enter.
pub(crate) fn disconnect(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(ConnectForm { connect: false, end: None })))
}

const NOT_WIRE: &str = "That is not a wire.";
const NO_CONNECTION_POINT: &str = "No connection point there. Move onto a connection point until the green ring shows.";

struct ConnectForm {
    connect: bool,
    /// Connect: the route end chosen, waiting for its connection point.
    end: Option<(RouteId, End, Pt)>,
}

/// A picked route's end nearer to `p` (A on a tie), its position and whether it is on a
/// connection point.
fn wire_end(s: &Session, p: Vec2) -> Result<(RouteId, End, Pt, bool), String> {
    let h = pick(s, p).ok_or(NOT_WIRE)?;
    read(s, |proj, page| {
        let doc = proj.ledger.document();
        let map = proj.pages.get(&page).map(|v| &v.map);
        let i = match map.and_then(|m| m.get(h)) {
            Some(Mapped::Item(i)) => i,
            Some(Mapped::Fan(k)) => {
                return Err(map.map_or(crate::cables::views::FAN_REFUSED, |m| crate::cables::views::refusal(m, k)).into());
            }
            _ => return Err(NOT_WIRE.into()),
        };
        let Some((_, rep)) = doc.representation_of_item(i) else { return Err(NOT_WIRE.into()) };
        let Subject::Route(r) = rep.subject else { return Err(NOT_WIRE.into()) };
        let route = doc.routes().get(&r).ok_or(NOT_WIRE)?;
        let [a, b] = route.end_points().ok_or(NOT_WIRE)?;
        let q = pt(p);
        let (end, at, landed) = if q.dist(a) <= q.dist(b) { (End::A, a, route.ends[0]) } else { (End::B, b, route.ends[1]) };
        Ok((r, end, at, landed.point().is_some()))
    })
}

impl ConnectForm {
    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match (i, self.end) {
            (Input::Point(p), _) if !in_range(p) => Err(other(OUT_OF_RANGE)),
            (Input::Point(p), None) => {
                let (w, end, at, _) = wire_end(s, p).map_err(other)?;
                if self.connect {
                    self.end = Some((w, end, at));
                    return Ok(Step::Continue);
                }
                commit(s, "disconnect", &json!({"route": w.hex(), "end": end_name(end)}))?;
                s.echo(format!("End {} of wire {} disconnected.", end_name(end), w.hex()));
                Ok(Step::Continue)
            }
            (Input::Point(p), Some((w, end, _))) => {
                let (t, _, name) = connection_point_for(s, p).ok_or_else(|| other(NO_CONNECTION_POINT))?;
                commit(s, "connect", &json!({"route": w.hex(), "end": end_name(end), "connection_point": t.hex()}))?;
                s.echo(format!("End {} of wire {} connected to {name}.", end_name(end), w.hex()));
                self.end = None;
                Ok(Step::Continue)
            }
            (Input::Enter, Some(_)) => {
                self.end = None;
                Ok(Step::Continue)
            }
            (Input::Enter, None) => Ok(Step::Done),
            (Input::Text(t), _) => Err(not_a_point(&t)),
            _ => Ok(Step::Continue),
        }
    }
}

impl Interactive for ConnectForm {
    fn name(&self) -> &'static str {
        if self.connect { "CONNECT" } else { "DISCONNECT" }
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match (self.end, self.connect) {
            (None, true) => Prompt::new("Select wire near the end to connect", Accept::POINT),
            (None, false) => Prompt::new("Select wire near the end to disconnect", Accept::POINT),
            (Some((_, end, at)), _) => Prompt::new(format!("Select connection point for end {}", end_name(end)), Accept::POINT).base(v2(at)),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        let name = if self.connect { "connect" } else { "disconnect" };
        safely(s, name, |s| self.step(s, i))
    }
    /// Connect: the rubber band from the chosen end to the cursor or the captured connection point.
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        if !crate::interact::model_space(s) {
            return Vec::new();
        }
        match self.end {
            Some((_, _, at)) if in_range(cursor) => {
                vec![line_kind(at, capture_at(s, cursor).map_or(pt(cursor), |c| c.at))]
            }
            _ => Vec::new(),
        }
    }
    /// The end a click would choose, or the connection point a click would connect to.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        if self.end.is_some() {
            return capture_at(s, cursor).map(|c| capture_preview(s, &c, ATTACHED_ACI)).unwrap_or_default();
        }
        let (Some(px), Ok((_, _, at, attached))) = (pixel(s), wire_end(s, cursor)) else { return Vec::new() };
        let aci = match (self.connect, attached) {
            (true, _) => ATTACHED_ACI,
            (false, true) => LOOSE_ACI,
            (false, false) => return Vec::new(),
        };
        [RING_PX - 1.0, RING_PX, RING_PX + 1.0].into_iter().map(|d| preview_entity(s, ring_kind(at, d, px), aci)).collect()
    }
}
