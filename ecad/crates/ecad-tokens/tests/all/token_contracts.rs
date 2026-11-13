//! Contracts of the token table (brief 037, acceptance criterion 1).

use ecad_tokens::sheet::{self, DARK, Meaning, PAPER, prints};
use ecad_tokens::{Color, GRAPHITE, sheet_type};

/// Every stated text colour reaches 4.5:1 on its background, and the spec's quoted ratios hold.
#[test]
fn every_text_colour_meets_4_5_to_1_on_its_background() {
    let pairs = GRAPHITE.text_pairs();
    assert!(pairs.len() >= 30, "the pairs table is complete ({} pairs)", pairs.len());
    let failing: Vec<String> =
        pairs.iter().filter(|(_, t, _, g)| t.contrast(*g) < 4.5).map(|(tn, t, gn, g)| format!("{tn} on {gn}: {:.2}", t.contrast(*g))).collect();
    assert!(failing.is_empty(), "below 4.5:1: {failing:?}");
    // Spec §4: text-3 on raised 4.6:1, white on accent 4.9:1 (to the spec's rounding).
    let text_3 = GRAPHITE.text_3.contrast(GRAPHITE.raised);
    assert!(text_3 >= 4.5 && (text_3 - 4.6).abs() < 0.05, "text_3 on raised {text_3:.2}");
    let white = ecad_tokens::WHITE.contrast(GRAPHITE.accent);
    assert!((white - 4.9).abs() < 0.05, "white on accent {white:.2}");
    // The pairs left out are left out for a reason: text_3 would fail on hover and active.
    assert!(GRAPHITE.text_3.contrast(GRAPHITE.hover) < 4.5);
    assert!(!pairs.iter().any(|(t, _, g, _)| *t == "text_3" && (*g == "hover" || *g == "active")));
}

/// `prints` is true exactly for ink, ink 2, cable, provisional, terminal link and the
/// missing-data warning.
#[test]
fn only_ink_meanings_print() {
    let printing: Vec<Meaning> = Meaning::ALL.into_iter().filter(|m| prints(*m)).collect();
    assert_eq!(printing, vec![Meaning::Ink, Meaning::Ink2, Meaning::Cable, Meaning::MissingData, Meaning::Provisional, Meaning::TerminalLink]);
    assert!(!prints(Meaning::Warning), "only the missing-data warning prints");
}

/// The cap height of a sheet text style gives its font size through Barlow's 0.70 em cap height.
#[test]
fn cap_height_gives_the_font_size() {
    let rows: &[(sheet_type::SheetText, f32)] = &[
        (sheet_type::TAG, 3.57),
        (sheet_type::LABEL, 2.29),
        (sheet_type::TERMINAL_LABEL, 2.14),
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
        assert_eq!(s.color(Meaning::ConnectionPoint), s.color(Meaning::TerminalLink));
        assert_eq!(s.selection_fill().a, Color::hex(0).at_percent(sheet::SELECTION_FILL_PERCENT).a);
    }
    // Ink reads on its sheet.
    assert!(PAPER.ink.contrast(PAPER.sheet) > 7.0);
    assert!(DARK.ink.contrast(DARK.sheet) > 7.0);
}

/// Colour arithmetic: alpha percentages and the token spelling.
#[test]
fn colours_spell_and_scale() {
    assert_eq!(GRAPHITE.accent_soft.to_token(), "#2F6BE0 at 18 %");
    assert_eq!(GRAPHITE.accent_soft.a, 46);
    assert_eq!(Color::hex(0x123456).at_percent(200).a, 255);
    assert_eq!(ecad_tokens::shadow::COLOR.to_token(), "#000000 at 45 %");
}
