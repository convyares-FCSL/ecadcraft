//! Hover lines (§6.4).

use super::*;

/// A symbol's short name in the lens: `coil`, `NO contact`…
fn short_name(s: &Starter) -> &'static str {
    match s.key {
        K::Coil => "coil",
        _ => s.name,
    }
}

/// `NO contact 13/14` for a representation.
fn rep_words(doc: &Document, c: DeviceId, r: &Representation) -> String {
    let labels: Vec<String> = shown_connection_points(doc, c, r).iter().map(|(t, _)| label_of(doc, *t)).collect();
    let name = rep_starter(doc, r).map_or("symbol", short_name);
    if labels.is_empty() { name.to_string() } else { format!("{name} {}", labels.join("/")) }
}

/// Most other symbols the lens's `Also` line lists before `+N more`.
pub(crate) const MAX_ALSO: usize = 4;

/// The lens of device `c` hovered through its representation `here` (design 018 §6.4).
///
/// `m` measures the page's texts (the C wrapper `hover_lines` passes CADCraft's fonts).
pub fn hover_lines_measured(doc: &Document, c: DeviceId, here: RepresentationId, m: &dyn TextMetrics) -> Vec<String> {
    let device = doc.devices.get(&c).cloned().unwrap_or_default();
    let get = |k: &str| device.value(k).unwrap_or_else(|| "?".into());
    let head = match device.tag() {
        // Brief 026 (design §9): `(fixed)` or `(provisional)` after the tag.
        Some(t) if tag::is_provisional(t) => format!("Device {t} (provisional)   {}", description(doc, c)),
        Some(t) if device.is_fixed() => format!("Device {t} (fixed)   {}", description(doc, c)),
        Some(t) => format!("Device {t}   {}", description(doc, c)),
        None => format!("Device (no tag)   {}", description(doc, c)),
    };
    let mut lines = vec![head, format!("Maker: {}   Part number: {}", get("maker"), get("part_number"))];
    // Brief 044 (design 044 §7, tester D3): a connector half says where its mate is.
    if let Some(side) = doc.representations.get(&here).and_then(|r| crate::connectors::rep_side(doc, r)) {
        let sides: &[crate::symbols::Side] = match side {
            crate::symbols::Side::Both => &[crate::symbols::Side::Socket, crate::symbols::Side::Plug],
            crate::symbols::Side::Socket => &[crate::symbols::Side::Socket],
            crate::symbols::Side::Plug => &[crate::symbols::Side::Plug],
        };
        lines.extend(sides.iter().filter_map(|s| crate::connectors::hover_words(doc, c, *s)));
    }
    // Brief 028 (design 028 §7.1): a terminal's sides and its strip's positions.
    if crate::terminal_strips::strip_of(doc, c).is_some() {
        lines.extend(crate::terminal_strips::hover::connection_point_hover(doc, c, here));
    }
    // Brief 024 (design 024 §3.3): the location shown, where it comes from, and this symbol's own
    // context when it differs.
    let mut boxes = crate::locations::BoxCache::default();
    let loc = crate::locations::resolve_with(doc, c, &mut boxes);
    let mut words = loc.shown(doc);
    if let Some(r) = doc.representations.get(&here) {
        let ctx = crate::locations::rep_context(doc, r, &mut boxes);
        if ctx.value.is_some() && ctx.value != loc.value {
            words.push_str(&format!("; this symbol is inside {}", ctx.text()));
        }
    }
    if loc.key.is_some() || device.function.is_some() {
        let function = crate::locations::resolve_function(doc, c);
        let function = if function.key.is_some() { function.text() } else { String::new() };
        lines.push(format!("Location: {words}   Function: {function}"));
    }
    if let Some(r) = doc.representations.get(&here) {
        let drawable = rep_definition(doc, r).is_some_and(|(_, pl)| pl.is_valid());
        let cannot = if drawable { "" } else { " (cannot be drawn)" };
        lines.push(format!("This symbol: {}, {}{cannot}", rep_words(doc, c, r), crate::references::cables::page_words(doc, r.page)));
        // Brief 031 (design §5.4): a tag hidden here is still data; the lens names it. (Descriptions
        // and part numbers are hidden by default, so naming them would add a line to every lens.)
        let hidden: Vec<String> = all_texts_measured(doc, r.page, None, m)
            .into_iter()
            .filter(|t| t.text.rep == here && !t.shown && t.text.of == TextOf::Tag)
            .map(|t| format!("Tag {} (hidden here)", t.text.value))
            .collect();
        if !hidden.is_empty() {
            lines.push(hidden.join("; "));
        }
    }
    let order: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    let mut others: Vec<(usize, RepresentationId, String)> = doc
        .representations_of(Subject::Device(c))
        .filter(|(id, _)| *id != here)
        .map(|(id, r)| {
            let n = order.get(&r.page).copied().unwrap_or(usize::MAX);
            (n, id, format!("{} {}", rep_words(doc, c, r), crate::references::cables::page_words(doc, r.page)))
        })
        .collect();
    others.sort_by_key(|(n, id, _)| (*n, *id));
    if !others.is_empty() {
        let mut words: Vec<String> = others.iter().take(MAX_ALSO).map(|(_, _, w)| w.clone()).collect();
        if others.len() > MAX_ALSO {
            words.push(format!("+{} more", others.len() - MAX_ALSO));
        }
        lines.push(format!("Also: {}", words.join("; ")));
    }
    if let Some(r) = doc.representations.get(&here) {
        let ends: Vec<String> = shown_connection_points(doc, c, r)
            .iter()
            .map(|(t, _)| {
                let label = label_of(doc, *t);
                match doc.ends_at(*t).first() {
                    Some((w, _)) => format!("{label} -> {}", crate::cables::slash::wire_name(doc, *w)),
                    None => format!("{label} (not connected)"),
                }
            })
            .collect();
        if !ends.is_empty() {
            lines.push(format!("  {}", ends.join("        ")));
        }
    }
    lines.push(format!("id {}", c.hex()));
    lines
}

/// The kind a derived text is; a box label (not a symbol's text) is a location label.
pub fn derived_text_kind(of: TextOf, symbol: bool) -> crate::styles::text_style::TextKind {
    if symbol { kind_of(of) } else { crate::styles::text_style::TextKind::LocationLabel }
}
