//! Contracts of the canvas palette.

use ecad_model::palette::Color;
use ecad_model::palette::sheet::{self, DARK, Meaning, PAPER, prints};

/// `prints` is true exactly for ink, ink 2, cable, provisional, terminal link and the
/// missing-data warning.
#[test]
fn only_ink_meanings_print() {
    let printing: Vec<Meaning> = Meaning::ALL.into_iter().filter(|m| prints(*m)).collect();
    assert_eq!(printing, vec![Meaning::Ink, Meaning::Ink2, Meaning::Cable, Meaning::MissingData, Meaning::Provisional, Meaning::JumperBar]);
    assert!(!prints(Meaning::Warning), "only the missing-data warning prints");
}

/// Both sheets give every meaning a colour, the missing-data marker shares the warning colour,
/// and the connection point and the terminal link share one hue on purpose.
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
