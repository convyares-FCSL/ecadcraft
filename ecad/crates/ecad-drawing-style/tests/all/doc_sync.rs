//! The design tokens document and the drawing style agree: every row of the
//! "Drawing style" tables names a constant and shows its value, and every constant has a row.

use std::collections::BTreeMap;

use ecad_drawing_style::marker::{self, Shape};
use ecad_drawing_style::sheet::{self, DARK, Meaning, PAPER, prints};
use ecad_drawing_style::wire::{self, WireColors};
use ecad_drawing_style::{line, sheet_type};

const DOC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/design/tokens.md"));

/// The checked part of the doc: from `# Drawing style` to the next top-level heading.
fn checked_part(doc: &str) -> &str {
    let start = doc.find("\n# Drawing style\n").map_or(0, |i| i + 1);
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

/// Every constant as `name → the cells its row must start with`.
fn constants() -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut put = |name: String, cells: Vec<String>| {
        assert!(m.insert(name.clone(), cells).is_none(), "constant {name} listed twice");
    };
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
    for (name, s) in sheet_type::ALL {
        put(name.into(), vec![mm(s.cap_mm), format!("{:.2} mm", s.font_size_mm()), s.weight.to_string()]);
    }
    for (name, v) in line::MM {
        put(name.into(), vec![mm(v)]);
    }
    m
}

#[test]
fn every_drawing_style_row_equals_its_constant() {
    let rows = doc_rows(checked_part(DOC));
    assert!(rows.len() > 40, "the Drawing style tables were found ({} rows)", rows.len());
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
    assert!(problems.is_empty(), "tokens.md and ecad-drawing-style differ:\n  {}", problems.join("\n  "));
}
