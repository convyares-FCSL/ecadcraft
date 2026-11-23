//! Contracts of the token table.

use {ecad_widgets::tokens::Color, ecad_widgets::tokens::GRAPHITE};

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
    let white = ecad_widgets::tokens::WHITE.contrast(GRAPHITE.accent);
    assert!((white - 4.9).abs() < 0.05, "white on accent {white:.2}");
    // The pairs left out are left out for a reason: text_3 would fail on hover and active.
    assert!(GRAPHITE.text_3.contrast(GRAPHITE.hover) < 4.5);
    assert!(!pairs.iter().any(|(t, _, g, _)| *t == "text_3" && (*g == "hover" || *g == "active")));
}

/// UI rev 3 (board 1.17): the light shell's text colours reach 4.5:1 on their backgrounds too,
/// and every token is a colour of its own set (none left at Graphite's dark value), except the
/// accent and the white ink on it, which are the same in both.
#[test]
fn the_light_shell_meets_4_5_to_1_on_its_backgrounds() {
    let light = ecad_widgets::tokens::LIGHT;
    let failing: Vec<String> = light
        .text_pairs()
        .iter()
        .filter(|(_, t, _, g)| t.contrast(*g) < 4.5)
        .map(|(tn, t, gn, g)| format!("{tn} on {gn}: {:.2}", t.contrast(*g)))
        .collect();
    assert!(failing.is_empty(), "below 4.5:1: {failing:?}");
    assert!(light.text.contrast(light.panel) > 12.0, "dark text on a light panel");
    for ((name, d), (_, l)) in GRAPHITE.rows().into_iter().zip(light.rows()) {
        assert!(name == "shell.accent" || name == "shell.on_accent" || d != l, "{name} has a light value of its own");
    }
}

/// Colour arithmetic: alpha percentages and the token spelling.
#[test]
fn colours_spell_and_scale() {
    assert_eq!(GRAPHITE.accent_soft.to_token(), "#2F6BE0 at 18 %");
    assert_eq!(GRAPHITE.accent_soft.a, 46);
    assert_eq!(Color::hex(0x123456).at_percent(200).a, 255);
    assert_eq!(ecad_widgets::tokens::shadow::COLOR.to_token(), "#000000 at 45 %");
}
