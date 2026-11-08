//! Small geometry value types owned by the model (no CADCraft types, principle P10).

use serde::{Deserialize, Serialize};

/// A point on a page, in drawing units.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Distance to `o`.
    pub fn dist(self, o: Pt) -> f64 {
        (self.x - o.x).hypot(self.y - o.y)
    }

    /// Whether `o` is within `tol` of this point.
    pub fn near(self, o: Pt, tol: f64) -> bool {
        self.dist(o) <= tol
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// Where a symbol instance sits on its page (proposal §4.2).
///
/// # Semantics
/// `page = origin + R(rotation) · M(mirror) · scale · local`, where `M` negates x when mirrored.
/// `rotation` is in radians, counter-clockwise; `scale` is uniform and positive.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub origin: Pt,
    pub rotation: f64,
    pub mirror: bool,
    pub scale: f64,
}

impl Default for Placement {
    fn default() -> Self {
        Self { origin: Pt::default(), rotation: 0.0, mirror: false, scale: 1.0 }
    }
}

impl Placement {
    /// Map a point in the symbol's local frame onto the page.
    pub fn apply(&self, local: Pt) -> Pt {
        let x = if self.mirror { -local.x } else { local.x } * self.scale;
        let y = local.y * self.scale;
        let (s, c) = self.rotation.sin_cos();
        Pt::new(self.origin.x + c * x - s * y, self.origin.y + s * x + c * y)
    }

    /// Whether every value is usable (finite, positive scale).
    pub fn is_valid(&self) -> bool {
        self.origin.is_finite() && self.rotation.is_finite() && self.scale.is_finite() && self.scale > 0.0
    }
}
