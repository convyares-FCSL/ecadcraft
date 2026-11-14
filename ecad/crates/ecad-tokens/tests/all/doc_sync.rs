//! `ecad/docs/design/tokens.md` and the constants agree (brief 037, acceptance criterion 1;
//! design §3.1): every row of the "Tokens" tables names a constant and shows its value, and every
//! constant has a row. The `ecad.*` rows after it are retired in U4a and not checked.

use std::collections::BTreeMap;

use ecad_tokens::marker::{self, Shape};
use ecad_tokens::sheet::{self, DARK, Meaning, PAPER, prints};
use ecad_tokens::wire::{self, WireColors};
use ecad_tokens::{Icon, line, radius, shadow, sheet_type, size, space, ui_type, wheel};

const DOC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/design/tokens.md"));

/// The checked part of the doc: from `# Tokens` to the next top-level heading.
fn checked_part(doc: &str) -> &str {
    let start = doc.find("\n# Tokens\n").map_or(0, |i| i + 1);
    let rest = doc.get(start + 1..).unwrap_or_default();
    let end = rest.find("\n# ").map_or(rest.len(), |i| i + 1);
    rest.get(..end).unwrap_or(rest)
}

/// Each table row whose first cell is a quoted token name: the name and the other cells, with
/// backticks removed.
fn doc_rows(text: &str) -> Vec<(String, Vec<String>)> {
    text.lines()
        .filter(|l| l.starts_with("| `"))
        .map(|l| {
            let cells: Vec<String> = l.trim().trim_matches('|').split('|').map(|c| c.trim().trim_matches('`').to_string()).collect();
            let name = cells.first().cloned().unwrap_or_default();
            (name, cells.into_iter().skip(1).collect())
        })
        .collect()
}

fn px(v: f32) -> String {
    format!("{v} px")
}
fn mm(v: f32) -> String {
    format!("{v} mm")
}
fn wire_colors(w: WireColors) -> String {
    match w.dashes {
        Some(d) => format!("{} with {} dashes", w.solid.to_hex(), d.to_hex()),
        None => w.solid.to_hex(),
    }
}
fn marker_text(m: Meaning) -> String {
    let mk = marker::marker(m);
    let shape = match mk.shape {
        Shape::None => "none",
        Shape::Ring => "ring",
        Shape::Dot => "dot",
        Shape::Square => "square",
        Shape::Slash => "slash",
        Shape::Disc => "disc",
        Shape::Triangle => "triangle",
        Shape::DottedUnderline => "dotted underline",
        Shape::DashedBar => "dashed bar",
        Shape::Outline => "outline",
        Shape::DashedOutline => "dashed outline",
    };
    match mk.size_px {
        Some(s) => format!("{shape}, {}", px(s)),
        None => shape.to_string(),
    }
}
/// `WorkspaceDraw` → `workspace_draw`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Every constant as `name → the cells its row must start with`.
fn constants() -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut put = |name: String, cells: Vec<String>| {
        assert!(m.insert(name.clone(), cells).is_none(), "constant {name} listed twice");
    };
    // UI rev 3: each shell token's Graphite value, then its light value (board 1.17).
    for ((name, c), (_, l)) in ecad_tokens::GRAPHITE.rows().into_iter().zip(ecad_tokens::LIGHT.rows()) {
        put(name.into(), vec![c.to_token(), l.to_token()]);
    }
    put("shell.soft".into(), vec![format!("{} %", ecad_tokens::shell::SOFT_PERCENT)]);
    put("shell.light_soft".into(), vec![format!("{} %", ecad_tokens::shell::LIGHT_SOFT_PERCENT)]);
    for meaning in Meaning::ALL {
        put(
            format!("sheet.{}", meaning.name()),
            vec![
                PAPER.color(meaning).to_token(),
                DARK.color(meaning).to_token(),
                if prints(meaning) { "yes" } else { "no" }.into(),
                marker_text(meaning),
            ],
        );
    }
    put("sheet.selection_fill".into(), vec![format!("{} %", sheet::SELECTION_FILL_PERCENT)]);
    put("sheet.preview_fill".into(), vec![format!("{} %", sheet::PREVIEW_FILL_PERCENT)]);
    put("sheet.thumbnail_paper".into(), vec![sheet::THUMBNAIL_PAPER.to_token()]);
    put("sheet.preview_paper".into(), vec![sheet::PREVIEW_PAPER.to_token()]);
    put("sheet.thumbnail_ink".into(), vec![sheet::THUMBNAIL_INK.to_token()]);
    for (name, cat) in [("wire.power", wire::POWER), ("wire.dc_24v", wire::DC_24V), ("wire.pe", wire::PE)] {
        put(name.into(), vec![wire_colors(cat.paper), wire_colors(cat.dark)]);
    }
    put("wire.dash_on".into(), vec![mm(wire::DASH_ON_MM)]);
    put("wire.dash_off".into(), vec![mm(wire::DASH_OFF_MM)]);
    for (name, s) in ui_type::ALL {
        put(name.into(), vec![format!("{} / {} px", s.size, s.line), s.weight.to_string()]);
    }
    for (name, s) in sheet_type::ALL {
        put(name.into(), vec![mm(s.cap_mm), format!("{:.2} mm", s.font_size_mm()), s.weight.to_string()]);
    }
    for (name, v) in space::ALL.iter().chain(size::ALL.iter()).chain(radius::ALL.iter()).chain(line::PX.iter()) {
        put((*name).into(), vec![px(*v)]);
    }
    put("radius.pill".into(), vec!["full".into()]);
    for (name, v) in line::MM {
        put(name.into(), vec![mm(v)]);
    }
    put("wheel.outer_radius".into(), vec![px(wheel::OUTER_RADIUS)]);
    put("wheel.inner_radius".into(), vec![px(wheel::INNER_RADIUS)]);
    put("wheel.slots".into(), vec![wheel::SLOTS.to_string()]);
    put("wheel.slot".into(), vec![format!("{} deg", wheel::SLOT_DEGREES)]);
    put("wheel.gap".into(), vec![format!("{} deg", wheel::GAP_DEGREES)]);
    put("shadow.offset".into(), vec![format!("{} {}", shadow::OFFSET_X, px(shadow::OFFSET_Y))]);
    put("shadow.blur".into(), vec![px(shadow::BLUR)]);
    put("shadow.color".into(), vec![shadow::COLOR.to_token()]);
    for icon in Icon::ALL {
        put(format!("icon.{}", snake(&format!("{icon:?}"))), vec![icon.name().into()]);
    }
    m
}

#[test]
fn every_token_row_equals_its_constant() {
    let rows = doc_rows(checked_part(DOC));
    assert!(rows.len() > 100, "the Tokens tables were found ({} rows)", rows.len());
    let expected = constants();
    let mut seen = BTreeMap::new();
    let mut problems = Vec::new();
    for (name, cells) in &rows {
        if seen.insert(name.clone(), ()).is_some() {
            problems.push(format!("{name}: two rows"));
            continue;
        }
        match expected.get(name) {
            None => problems.push(format!("{name}: a row with no constant")),
            Some(want) => {
                let got: Vec<String> = cells.iter().take(want.len()).cloned().collect();
                if &got != want {
                    problems.push(format!("{name}: doc {got:?}, code {want:?}"));
                }
            }
        }
    }
    for name in expected.keys() {
        if !seen.contains_key(name) {
            problems.push(format!("{name}: a constant with no row"));
        }
    }
    assert!(problems.is_empty(), "tokens.md and ecad-tokens differ:\n  {}", problems.join("\n  "));
}

/// The legacy `ecad.*` rows are outside the checked part, and stay in the file (design §12 O6).
#[test]
fn the_legacy_rows_are_kept_outside_the_checked_tables() {
    assert!(!checked_part(DOC).contains("`ecad."), "no ecad.* token row in the checked tables");
    for kept in ["`ecad.terminal`", "`ecad.provisional`", "`ecad.text-tag`", "`ecad.history-lane`", "`ecad.flagged`"] {
        assert!(DOC.contains(kept), "{kept} kept until U4a");
    }
}
