//! Sheet text styles (spec §4, "Type"; owner answer 9): sizes are **cap heights** on paper, in
//! mm, never enlarged for the screen. The sheet font is Barlow Semi Condensed, whose cap height
//! is 0.70 em, so the font size is the cap height divided by 0.70. Switching the sheet to these
//! styles is U4a; this table is the source it reads.

/// Barlow Semi Condensed's cap height as a fraction of its em.
pub const CAP_HEIGHT_EM: f32 = 0.70;

/// One sheet text style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetText {
    /// Cap height on paper, mm.
    pub cap_mm: f32,
    /// Weight (400 regular, 600 semibold).
    pub weight: u16,
}

impl SheetText {
    /// The font size, mm: the cap height divided by [`CAP_HEIGHT_EM`].
    ///
    /// # Example
    /// ```
    /// assert!((ecad_tokens::sheet_type::TAG.font_size_mm() - 3.57).abs() < 0.005);
    /// ```
    pub fn font_size_mm(self) -> f32 {
        self.cap_mm / CAP_HEIGHT_EM
    }
}

/// Device tag.
pub const TAG: SheetText = SheetText { cap_mm: 2.5, weight: 600 };
/// Pin label, wire label, reference, continuation.
pub const LABEL: SheetText = SheetText { cap_mm: 1.6, weight: 400 };
/// Terminal label, description.
pub const TERMINAL_LABEL: SheetText = SheetText { cap_mm: 1.5, weight: 400 };
/// Title block label.
pub const TITLE_BLOCK_LABEL: SheetText = SheetText { cap_mm: 1.4, weight: 400 };
/// Zone label.
pub const ZONE_LABEL: SheetText = SheetText { cap_mm: 2.0, weight: 400 };
/// Page title.
pub const PAGE_TITLE: SheetText = SheetText { cap_mm: 3.5, weight: 600 };
/// Page number.
pub const PAGE_NUMBER: SheetText = SheetText { cap_mm: 4.2, weight: 600 };

/// Every style as `(name, style)`, names as in `tokens.md` (`sheet_type.tag`).
pub const ALL: [(&str, SheetText); 7] = [
    ("sheet_type.tag", TAG),
    ("sheet_type.label", LABEL),
    ("sheet_type.terminal_label", TERMINAL_LABEL),
    ("sheet_type.title_block_label", TITLE_BLOCK_LABEL),
    ("sheet_type.zone_label", ZONE_LABEL),
    ("sheet_type.page_title", PAGE_TITLE),
    ("sheet_type.page_number", PAGE_NUMBER),
];
