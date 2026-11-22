//! What hovering a connection point or a cable says (design 028 §7.1).

use std::collections::BTreeSet;

use ecad_model::{CableId, ConnectionPointId, DeviceId, Document, End, RepresentationId, WireId};

use super::strip_model::geometric_inside;
use super::*;

/// The far end of `w` from end `mine`, in words (`-K01:A1`, `24V >`, `loose`).
fn far_words(doc: &Document, w: WireId, mine: End) -> (String, Option<DeviceId>) {
    let far = match mine {
        End::A => End::B,
        End::B => End::A,
    };
    let Some(t) = doc.wires.get(&w).map(|c| c.end(far)) else { return ("loose".into(), None) };
    if let Some(a) = doc.connection_points.get(&t).and_then(|x| x.owner.aid()) {
        return (format!("{} >", crate::continuations::display_name(doc, a)), None);
    }
    let Some(c) = doc.connection_points.get(&t).and_then(|x| x.device()) else { return ("loose".into(), None) };
    (crate::terminal_strips::connection_point_words(doc, t), Some(c))
}

/// The hover lines of a terminal symbol `here` of strip `c` (design 028 §7.1), from the document
/// alone: `-X01:3  feed-through  +CP1`, `inside  -K01:A1  24V-07`, `outside  -W01 core 3 → -M01:U1`,
/// `bridged to 4`, then the strip's positions.
pub fn connection_point_hover(doc: &Document, c: DeviceId, here: RepresentationId) -> Vec<String> {
    let mut lines = Vec::new();
    let Some(r) = doc.representations.get(&here) else { return lines };
    let pe = rep_is_pe(doc, r);
    let location = crate::locations::resolve(doc, c).value;
    let number = |w: WireId| ecad_model::connectivity::wiring::wire_number(doc, w).unwrap_or_else(|| "?".into());
    let mine: BTreeSet<ConnectionPointId> = doc.connection_points_of(c).map(|(t, _)| t).collect();
    for (t, _) in crate::devices::shown_connection_points(doc, c, r) {
        // Brief 036: the type, and the level of a multi-level position.
        let ty = crate::terminal_strips::terminal_blocks::type_of_connection_point(doc, t);
        let kind = if ty.multi() {
            format!(
                "{} level {}",
                ty.word,
                crate::terminal_strips::terminal_blocks::level_name(doc, c, ty, crate::terminal_strips::terminal_blocks::level_of(doc, t).0)
            )
        } else if pe {
            "PE".to_string()
        } else {
            ty.word.to_string()
        };
        lines.push(format!("{}  {kind}  {}", crate::terminal_strips::connection_point_words(doc, t), location.clone().unwrap_or_else(|| "?".into())));
        let mut bridged = Vec::new();
        for (w, end) in doc.ends_at(t) {
            let far_t = doc.wires.get(&w).map(|x| {
                x.end(match end {
                    End::A => End::B,
                    End::B => End::A,
                })
            });
            if let Some(o) = far_t.filter(|o| *o != t && mine.contains(o)) {
                bridged.push(crate::terminal_strips::terminal_blocks::connection_point_name(doc, o));
                continue;
            }
            let (target, far) = far_words(doc, w, end);
            let cable = doc.cables_of(w).into_iter().next();
            let inside = if cable.is_some() {
                false
            } else {
                let far_loc = far.filter(|f| crate::locations::is_device(doc, *f)).and_then(|f| crate::locations::resolve(doc, f).value);
                match (&location, far_loc) {
                    (Some(a), Some(b)) => *a == b,
                    _ => geometric_inside(doc, w, end, r.page).unwrap_or(true),
                }
            };
            match cable {
                Some(k) => {
                    let core = doc
                        .cables
                        .get(&k)
                        .and_then(|x| x.cores.iter().find(|m| m.wire() == Some(w)))
                        .and_then(|m| m.label_str().map(str::to_string))
                        .unwrap_or_else(|| "?".into());
                    lines.push(format!("outside  {} core {core} \u{2192} {target}", crate::cables::slash::cable_name(doc, k)));
                }
                None => lines.push(format!("{}  {target}  {}", if inside { "inside" } else { "outside" }, number(w))),
            }
        }
        if !bridged.is_empty() {
            lines.push(format!("bridged to {}", bridged.join(", ")));
        }
    }
    // Brief 036: counted in positions (a three-level terminal is one).
    let labels: BTreeSet<String> = doc.connection_points_of(c).filter_map(|(_, t)| t.label.clone()).collect();
    let drawn = labels.len();
    let tag = crate::devices::tag_words(doc, c);
    lines.push(match offer_of(doc, c) {
        Some(o) => {
            let spare = (1..=o.positions).filter(|n| !labels.contains(&n.to_string())).count();
            format!("{tag}: {} positions, {drawn} drawn, {spare} spare", o.positions)
        }
        None => format!("{tag}: {drawn} drawn"),
    });
    lines
}

/// A cable's hover line: `-W01  7G1.5: 5 used, 2 spare (6, GNYE)`; `None` with no offer.
pub fn cable_hover(doc: &Document, k: CableId) -> Option<String> {
    let cab = doc.cables.get(&k)?;
    let count = count_cores(crate::styles::cable::offer(doc, k), &cab.cores);
    let named = cab.properties.get("type").cloned().or_else(|| count.offer_text.clone())?;
    count.offer.as_ref()?;
    let spare = if count.spare.is_empty() { "0 spare".to_string() } else { format!("{} spare ({})", count.spare.len(), count.spare.join(", ")) };
    let over = if count.over.is_empty() { String::new() } else { format!(", {} over", count.over.len()) };
    Some(format!("{}  {named}: {} used, {spare}{over}", crate::cables::slash::cable_name(doc, k), count.used()))
}
