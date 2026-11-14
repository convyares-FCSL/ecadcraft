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
/// A row's hover or selection ground, and a small button's hover (UI rev 3).
pub const ROW: f32 = 4.0;
/// Check boxes (UI rev 3).
pub const CHECK: f32 = 4.0;
/// The selected segment inside a segmented control: a button's radius less its 1 px inset
/// (UI rev 3).
pub const SEGMENT: f32 = BUTTON - 1.0;
/// A card inside a panel section, such as an inspector part card (UI rev 3).
pub const INSET_CARD: f32 = 6.0;

/// A pill's radius: half its height ("full").
pub fn pill(height: f32) -> f32 {
    (height / 2.0).max(0.0)
}

/// Every fixed token as `(name, px)`, names as in `tokens.md` (`radius.pill` is "full").
pub const ALL: [(&str, f32); 13] = [
    ("radius.chip", CHIP),
    ("radius.keycap", KEYCAP),
    ("radius.badge", BADGE),
    ("radius.field", FIELD),
    ("radius.button", BUTTON),
    ("radius.tool", TOOL),
    ("radius.tile", TILE),
    ("radius.popover", POPOVER),
    ("radius.card", CARD),
    ("radius.row", ROW),
    ("radius.check", CHECK),
    ("radius.segment", SEGMENT),
    ("radius.inset_card", INSET_CARD),
];
