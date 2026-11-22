//! Quantities with their unit in the type (design 072 §9.3), so a length is never added to a
//! current and an angle is never stored in the wrong unit.
//!
//! Each is a transparent newtype over `f64`: stored and written exactly as the bare number. Only
//! arithmetic that means something is defined (`Mm ± Mm`, `Mm × f64`, degrees ⇄ radians); the
//! electrical quantities compare and display (`10 A`) but have no operators, because calculations
//! with them are the methods of the types that own them.
//!
//! This module excludes drawing-unit coordinates ([`crate::Pt`] stays in the project's drawing
//! unit; [`Mm::to_units`] is where the two meet) and parsing of rating texts.
#![deny(clippy::wildcard_enum_match_arm)]

use std::fmt;
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

use crate::doc::LengthUnit;

/// Declares one transparent unit newtype with its `Display` suffix.
macro_rules! unit {
    ($(#[$doc:meta])* $name:ident, $suffix:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize, schemars::JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub f64);

        impl $name {
            /// The unit's symbol, as [`fmt::Display`] writes it after the number.
            pub const SYMBOL: &'static str = $suffix;

            /// The bare number.
            pub fn value(self) -> f64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{} {}", self.0, $suffix)
            }
        }
    };
}

unit!(
    /// A length in millimetres: on paper (symbol sizes, offsets) or in a panel.
    Mm,
    "mm"
);
unit!(
    /// A length in metres: a cable or wire run.
    Metres,
    "m"
);
unit!(
    /// An angle in radians: how stored angles are kept (F7).
    Radians,
    "rad"
);
unit!(
    /// An angle in degrees: how commands and symbol content take angles (F6).
    Degrees,
    "°"
);
unit!(
    /// A current in amperes.
    Amps,
    "A"
);
unit!(
    /// A current in milliamperes.
    MilliAmps,
    "mA"
);
unit!(
    /// A current in kiloamperes (breaking capacity, short-circuit current).
    KiloAmps,
    "kA"
);
unit!(
    /// A voltage in volts.
    Volts,
    "V"
);
unit!(
    /// A power in watts.
    Watts,
    "W"
);
unit!(
    /// An apparent power in volt-amperes: a transformer's rating (VA is not W).
    VoltAmps,
    "VA"
);
unit!(
    /// A conductor cross-section in square millimetres.
    Mm2,
    "mm²"
);
unit!(
    /// A temperature in degrees Celsius.
    Celsius,
    "°C"
);
unit!(
    /// A ratio in percent (volt drop, derating).
    Percent,
    "%"
);
unit!(
    /// A time in seconds (disconnection time).
    Seconds,
    "s"
);

impl Mm {
    /// This length in drawing units of a project whose length unit is `unit`.
    pub fn to_units(self, unit: LengthUnit) -> f64 {
        self.0 * crate::sheet::unit_scale(unit)
    }
}

impl Add for Mm {
    type Output = Mm;
    fn add(self, rhs: Mm) -> Mm {
        Mm(self.0 + rhs.0)
    }
}

impl Sub for Mm {
    type Output = Mm;
    fn sub(self, rhs: Mm) -> Mm {
        Mm(self.0 - rhs.0)
    }
}

impl AddAssign for Mm {
    fn add_assign(&mut self, rhs: Mm) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Mm {
    fn sub_assign(&mut self, rhs: Mm) {
        self.0 -= rhs.0;
    }
}

impl Neg for Mm {
    type Output = Mm;
    fn neg(self) -> Mm {
        Mm(-self.0)
    }
}

impl Mul<f64> for Mm {
    type Output = Mm;
    fn mul(self, rhs: f64) -> Mm {
        Mm(self.0 * rhs)
    }
}

impl Radians {
    /// The sine and cosine of this angle.
    pub fn sin_cos(self) -> (f64, f64) {
        self.0.sin_cos()
    }

    /// Whether the angle is a finite number.
    pub fn is_finite(self) -> bool {
        self.0.is_finite()
    }
}

/// The one degrees-to-radians conversion (G7: no `to_radians` elsewhere).
impl From<Degrees> for Radians {
    fn from(d: Degrees) -> Radians {
        Radians(d.0.to_radians())
    }
}

/// The one radians-to-degrees conversion (G7: no `to_degrees` elsewhere).
impl From<Radians> for Degrees {
    fn from(r: Radians) -> Degrees {
        Degrees(r.0.to_degrees())
    }
}

/// Whether a voltage is alternating or direct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Current {
    Ac,
    Dc,
}

impl Current {
    /// `AC` or `DC`, as written after a voltage.
    pub fn label(self) -> &'static str {
        match self {
            Current::Ac => "AC",
            Current::Dc => "DC",
        }
    }
}

/// A rated voltage with its kind: `24 V DC`, `230 V AC`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RatedVoltage {
    pub volts: Volts,
    pub current: Current,
}

impl fmt::Display for RatedVoltage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.volts, self.current.label())
    }
}
