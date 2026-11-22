//! The words a wire or net is shown in (§7, §9).

use super::*;

/// Most connection points a net line names before `+n`.
pub const MAX_POINTS_SHOWN: usize = 6;

/// A connection point in words: `-K01:A1`; a continuation `→24V`.
pub fn point_words(doc: &Document, t: ConnectionPointId) -> String {
    let Some(term) = doc.connection_points.get(&t) else { return "?".into() };
    if let Some(a) = term.owner.aid() {
        return format!("\u{2192}{}", crate::continuations::display_name(doc, a));
    }
    let Some(c) = term.device() else { return "?".into() };
    let label = term.label.clone().or_else(|| term.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| "?".into());
    match crate::devices::tags::tag_of(doc, c) {
        Some(tag) => format!("{tag}:{label}"),
        None => format!("{}\u{2026}:{label}", c.hex().get(..8).unwrap_or_default()),
    }
}

/// The device connection points of a net (not continuations), in words, sorted.
pub fn net_points(doc: &Document, net: &ecad_model::NetData) -> Vec<String> {
    let mut v: Vec<String> = net
        .connection_points
        .iter()
        .filter(|t| doc.connection_points.get(t).is_none_or(|x| x.owner.aid().is_none()))
        .map(|t| point_words(doc, *t))
        .collect();
    v.sort();
    v
}

/// `a, b, c +n` of at most `max` words.
pub fn first_words(words: &[String], max: usize) -> String {
    let mut s = words.iter().take(max).cloned().collect::<Vec<_>>().join(", ");
    if words.len() > max {
        s.push_str(&format!(" +{}", words.len() - max));
    }
    s
}

/// The net line of a wire's hover lens (design 022 §7): `Net 24V-04 · 3 wires · -S02:14, …`.
pub fn net_line(doc: &Document, net: &ecad_model::NetData) -> String {
    let number = ecad_model::connectivity::wiring::net_number(doc, net).unwrap_or_else(|| "?".into());
    let n = net.wires.len();
    let wires = if n == 1 { "1 wire".to_string() } else { format!("{n} wires") };
    let points = net_points(doc, net);
    if points.is_empty() {
        format!("Net {number} \u{b7} {wires}")
    } else {
        format!("Net {number} \u{b7} {wires} \u{b7} {}", first_words(&points, MAX_POINTS_SHOWN))
    }
}

/// The lines brief 022 adds to a wire's hover lens: number (fixed or not), potential, label
/// values (stored, or the potential's default), and the net.
pub fn hover_lines(doc: &Document, w: WireId) -> Vec<String> {
    let mut lines = Vec::new();
    let conn = doc.connectivity();
    let Some(net) = conn.net_of_conductor(w).and_then(|id| conn.net(id)) else { return lines };
    if !doc.wires.contains_key(&w) {
        return lines;
    }
    let number = ecad_model::connectivity::wiring::net_number(doc, net).unwrap_or_else(|| "? (no number yet)".into());
    let fixed = if ecad_model::connectivity::wiring::net_fixed(net) { " (fixed)" } else { "" };
    lines.push(format!("Wire number {number}{fixed}"));
    let pot = ecad_model::connectivity::wiring::net_potential(doc, net);
    if let Some(p) = &pot.name {
        let from = if pot.from_continuation { " (continuation)" } else { "" };
        lines.push(format!("Potential {p}{from}"));
    }
    // Brief 050b: the wire's own values, else its category's (named).
    let draw = crate::styles::wire::wire_draw(doc, w);
    let w = &doc.settings.wiring;
    let cat = draw.category.as_ref().map(|id| crate::styles::name(doc, ecad_model::StyleCatalogue::Wire, id));
    let shown = |key: &str, v: &Option<(String, crate::defaults::Source)>| {
        let text = |v: &str| if key == ecad_model::connectivity::wiring::COLOUR { w.colour(v) } else { v.to_string() };
        match (v, &cat) {
            (Some((v, crate::defaults::Source::Object)), _) => Some(text(v)),
            (Some((v, _)), Some(n)) => Some(format!("{} ({n})", text(v))),
            (Some((v, _)), None) => Some(text(v)),
            (None, _) => None,
        }
    };
    let mut values = Vec::new();
    if let Some(v) = shown(ecad_model::connectivity::wiring::CROSS_SECTION, &draw.cross_section) {
        values.push(format!("Cross-section {v}"));
    }
    if let Some(v) = shown(ecad_model::connectivity::wiring::COLOUR, &draw.colour_text()) {
        values.push(format!("Colour {v}"));
    }
    if !values.is_empty() {
        lines.push(values.join(" \u{b7} "));
    }
    lines.push(net_line(doc, net));
    lines
}
