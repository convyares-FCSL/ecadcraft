//! Default wire category colours on each sheet, and the two-colour dash (spec §4). All are
//! defaults the user changes in Standards (U4c); these are the built-in values.

use crate::Color;

/// A wire's look: the solid colour, and for a two-colour wire the colour laid over it as dashes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WireColors {
    pub solid: Color,
    pub dashes: Option<Color>,
}

/// A wire category's default look on each sheet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Category {
    pub paper: WireColors,
    pub dark: WireColors,
}

/// Power: ink at 0.6 mm ([`crate::line::POWER_WIRE_MM`]).
pub const POWER: Category =
    Category { paper: WireColors { solid: crate::sheet::PAPER.ink, dashes: None }, dark: WireColors { solid: crate::sheet::DARK.ink, dashes: None } };

/// 24 V DC and 0 V.
pub const DC_24V: Category =
    Category { paper: WireColors { solid: Color::hex(0x23408F), dashes: None }, dark: WireColors { solid: Color::hex(0x8FB0FF), dashes: None } };

/// PE: green with yellow dashes.
pub const PE: Category = Category {
    paper: WireColors { solid: Color::hex(0x2F8F3A), dashes: Some(Color::hex(0xE3B800)) },
    dark: WireColors { solid: Color::hex(0x6FD67A), dashes: Some(Color::hex(0xF2D34B)) },
};

/// A two-colour wire or core: the second colour's dashes, on, in mm.
pub const DASH_ON_MM: f32 = 2.0;
/// A two-colour wire or core: the gap between the second colour's dashes, in mm.
pub const DASH_OFF_MM: f32 = 2.0;
