//! A route: a line a person draws on a page (design 072 §4.3, F31, lane D).
//!
//! # Semantics
//! - A [`Route`] is drawing: a polyline on one page, from its first point to its last. Its two
//!   [`RouteEnd`]s say what each end lands on: a connection point, another route (a T), or
//!   nothing.
//! - Routes decide which connection points are on one net, nothing more. The physical wires run
//!   between connection points and are kept apart from the routes that draw them (§4.1).
//! - A route has at least two points, every coordinate finite. An end on another route names a
//!   route other than itself.
//!
//! This module excludes how routes are drawn and edited (ecad-electrical, ecad-engine) and the
//! nets they make (connectivity).

use serde::{Deserialize, Serialize};

use crate::geom::Pt;
use crate::id::{PageId, RouteId, ConnectionPointId};

/// Most points of one route.
pub const MAX_POINTS: usize = 4096;

/// A line drawn on a page (stored intent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    /// The page it is drawn on.
    pub page: PageId,
    /// Its points in order, the first and the last being its ends.
    pub points: Vec<Pt>,
    /// What its first and its last point land on.
    pub ends: [RouteEnd; 2],
}

/// What one end of a route lands on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RouteEnd {
    /// A connection point.
    Point(ConnectionPointId),
    /// The middle of another route, at `at` (a T).
    On { route: RouteId, at: Pt },
    /// Nothing: an open end.
    Free,
}

impl RouteEnd {
    /// The connection point it lands on, if it lands on one.
    pub fn point(self) -> Option<ConnectionPointId> {
        match self {
            RouteEnd::Point(t) => Some(t),
            RouteEnd::On { .. } | RouteEnd::Free => None,
        }
    }

    /// The route it lands on, if it lands on one.
    pub fn route(self) -> Option<RouteId> {
        match self {
            RouteEnd::On { route, .. } => Some(route),
            RouteEnd::Point(_) | RouteEnd::Free => None,
        }
    }
}

/// Why a route is not well formed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    #[error("a route has at least two points")]
    TooFewPoints,
    #[error("a route has at most {MAX_POINTS} points")]
    TooManyPoints,
    #[error("a route's points are finite numbers")]
    NotFinite,
    #[error("a route's end lands on the route itself")]
    OnItself,
}

impl Route {
    /// A route on `page` through `points`, with its ends.
    pub fn new(page: PageId, points: Vec<Pt>, ends: [RouteEnd; 2]) -> Self {
        Route { page, points, ends }
    }

    /// Its first and its last point.
    pub fn end_points(&self) -> Option<[Pt; 2]> {
        Some([*self.points.first()?, *self.points.last()?])
    }

    /// The connection points its ends land on.
    pub fn points_landed(&self) -> impl Iterator<Item = ConnectionPointId> + '_ {
        self.ends.iter().filter_map(|e| e.point())
    }

    /// Whether route `id` holding this is well formed.
    ///
    /// # Errors
    /// The first [`RouteError`] it breaks.
    pub fn check(&self, id: RouteId) -> Result<(), RouteError> {
        if self.points.len() < 2 {
            return Err(RouteError::TooFewPoints);
        }
        if self.points.len() > MAX_POINTS {
            return Err(RouteError::TooManyPoints);
        }
        if self.points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return Err(RouteError::NotFinite);
        }
        if self.ends.iter().any(|e| matches!(e, RouteEnd::On { at, .. } if !at.x.is_finite() || !at.y.is_finite())) {
            return Err(RouteError::NotFinite);
        }
        if self.ends.iter().any(|e| e.route() == Some(id)) {
            return Err(RouteError::OnItself);
        }
        Ok(())
    }

    /// Its values in words, in a fixed order, for change lists, diffs and the inspector.
    /// How far along the route `at` lies, from its first point, and the route's whole length:
    /// `at` is taken at its nearest place on the route. `None` for a route with no length.
    pub fn along(&self, at: Pt) -> Option<(f64, f64)> {
        let mut walked = 0.0;
        let mut best: Option<(f64, f64)> = None;
        for w in self.points.windows(2) {
            let (Some(a), Some(b)) = (w.first().copied(), w.get(1).copied()) else { continue };
            let len = a.dist(b);
            let t = if len > 0.0 { (((at.x - a.x) * (b.x - a.x) + (at.y - a.y) * (b.y - a.y)) / (len * len)).clamp(0.0, 1.0) } else { 0.0 };
            let q = Pt { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
            let d = q.dist(at);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, walked + len * t));
            }
            walked += len;
        }
        best.filter(|_| walked > 0.0).map(|(_, s)| (s, walked))
    }

    pub fn values(&self) -> Vec<(&'static str, String)> {
        let end = |e: &RouteEnd| match e {
            RouteEnd::Point(t) => format!("point {t}"),
            RouteEnd::On { route, at } => format!("on {route} at ({}, {})", at.x, at.y),
            RouteEnd::Free => "free".into(),
        };
        let pts: Vec<String> = self.points.iter().map(|p| format!("({}, {})", p.x, p.y)).collect();
        vec![("page", self.page.to_string()), ("points", pts.join(" ")), ("start", end(&self.ends[0])), ("end", end(&self.ends[1]))]
    }
}
