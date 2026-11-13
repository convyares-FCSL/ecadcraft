//! Corner radii (spec §4, "Spacing, sizes, radii"), px at 100 % UI scale. Pills are fully
//! rounded: their radius is half their height ([`pill`]).

/// Chips.
pub const CHIP: f32 = 4.0;
/// Key caps.
pub const KEYCAP: f32 = 4.0;
/// Badges (design §3.3).
pub const BADGE: f32 = 4.0;
/// Fields.
pub const FIELD: f32 = 5.0;
/// Buttons.
pub const BUTTON: f32 = 6.0;
/// Tools.
pub const TOOL: f32 = 8.0;
/// Tiles.
pub const TILE: f32 = 8.0;
/// Popovers.
pub const POPOVER: f32 = 10.0;
/// Cards.
pub const CARD: f32 = 10.0;

/// A pill's radius: half its height ("full").
pub fn pill(height: f32) -> f32 {
    (height / 2.0).max(0.0)
}

/// Every fixed token as `(name, px)`, names as in `tokens.md` (`radius.pill` is "full").
pub const ALL: [(&str, f32); 9] = [
    ("radius.chip", CHIP),
    ("radius.keycap", KEYCAP),
    ("radius.badge", BADGE),
    ("radius.field", FIELD),
    ("radius.button", BUTTON),
    ("radius.tool", TOOL),
    ("radius.tile", TILE),
    ("radius.popover", POPOVER),
    ("radius.card", CARD),
];
