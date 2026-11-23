//! Contracts of the drawing style.

use ecad_drawing_style::sheet::{self, DARK, Meaning, PAPER, prints};
use ecad_drawing_style::{Color, sheet_type};

/// `prints` is true exactly for ink, ink 2, cable, provisional, terminal link and the
/// missing-data warning.
#[test]
fn only_ink_meanings_print() {
    let printing: Vec<Meaning> = Meaning::ALL.into_iter().filter(|m| prints(*m)).collect();
    assert_eq!(printing, vec![Meaning::Ink, Meaning::Ink2, Meaning::Cable, Meaning::MissingData, Meaning::Provisional, Meaning::JumperBar]);
    assert!(!prints(Meaning::Warning), "only the missing-data warning prints");
}

/// The cap height of a sheet text style gives its font size through Barlow's 0.70 em cap height.
#[test]
fn cap_height_gives_the_font_size() {
    let rows: &[(sheet_type::SheetText, f32)] = &[
        (sheet_type::TAG, 3.57),
        (sheet_type::LABEL, 2.29),
        (sheet_type::CONNECTION_POINT_LABEL, 2.14),
        (sheet_type::TITLE_BLOCK_LABEL, 2.0),
        (sheet_type::ZONE_LABEL, 2.86),
        (sheet_type::PAGE_TITLE, 5.0),
        (sheet_type::PAGE_NUMBER, 6.0),
    ];
    for (style, size) in rows {
        assert!((style.font_size_mm() - size).abs() < 0.005, "{style:?}: {} mm, expected {size}", style.font_size_mm());
    }
    assert!((sheet_type::TAG.cap_mm - 2.5).abs() < f32::EPSILON);
}

/// Both sheets give every meaning a colour, the missing-data marker shares the warning colour,
/// and the connection point and the terminal link share one hue on purpose (owner answer 5).
#[test]
fn both_sheets_cover_every_meaning() {
    for s in [PAPER, DARK] {
        assert_eq!(s.color(Meaning::MissingData), s.color(Meaning::Warning));
        assert_eq!(s.color(Meaning::ConnectionPoint), s.color(Meaning::JumperBar));
        assert_eq!(s.selection_fill().a, Color::hex(0).at_percent(sheet::SELECTION_FILL_PERCENT).a);
    }
    // Ink reads on its sheet.
    assert!(PAPER.ink.contrast(PAPER.sheet) > 7.0);
    assert!(DARK.ink.contrast(DARK.sheet) > 7.0);
}
