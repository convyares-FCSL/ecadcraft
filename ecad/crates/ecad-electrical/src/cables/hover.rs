//! What hovering a cable shows: its lens lines, the pages it is drawn on and its references.

use std::collections::BTreeSet;

use ecad_model::{CableId, Document, PageId, Subject, WireId};

use crate::references::cables as refs;

/// A cable slash: its tag, ID, designation, type and reference ends on `page`, and its members in
/// stored order with their cores.
pub fn cable_lines(doc: &Document, page: PageId, k: CableId) -> Vec<String> {
    let cable = doc.cables().get(&k);
    let mut lines = vec![format!("Cable {}", cable.and_then(|x| x.tag_shown()).unwrap_or_else(|| "(no tag)".into())), format!("id {}", k.hex())];
    for (key, label) in [("designation", "Designation"), ("type", "Type")] {
        if let Some(v) = cable.and_then(|x| x.properties.get(key)) {
            lines.push(format!("{label}: {}", super::words::shorten(v, 64)));
        }
    }
    let references: Vec<String> = doc
        .representations()
        .iter()
        .filter(|(_, r)| r.page == page && r.subject == Subject::Cable(k) && r.role == ecad_model::Role::Reference)
        .map(|(id, _)| format!("\"{}\"", super::layout::reference_text(doc, *id)))
        .collect();
    if !references.is_empty() {
        lines.push(format!("References here: {}", references.join(", ")));
    }
    // The core count.
    if let Some(line) = crate::terminal_strips::hover::cable_hover(doc, k) {
        lines.insert(1, line);
    }
    let members = cable.map(|x| x.cores.as_slice()).unwrap_or_default();
    if members.is_empty() {
        lines.push("Wires: none".into());
    } else {
        // The stored member order, nothing more.
        lines.push(format!("Wires: {}", members.len()));
        lines.extend(members.iter().map(|m| match super::member_core(m) {
            Some(c) => format!("  {}  core {c}", super::slash::position_name(doc, m.position)),
            None => format!("  {}", super::slash::position_name(doc, m.position)),
        }));
    }
    lines
}

/// A cable's hover with the pages it is drawn on and its references, after the view line, when
/// it has a reference.
pub fn with_drawings(doc: &Document, k: CableId, mut lines: Vec<String>) -> Vec<String> {
    let references: Vec<_> = refs::all(doc).into_iter().filter(|r| r.cable == k).collect();
    if references.is_empty() {
        return lines;
    }
    let members = doc.cables().get(&k).map(|c| c.cores.as_slice()).unwrap_or_default();
    let member_pages: BTreeSet<PageId> = members.iter().flat_map(|m| super::member_pages(doc, m)).collect();
    let pages: Vec<PageId> = doc
        .page_order()
        .iter()
        .copied()
        .filter(|p| doc.representations_of(Subject::Cable(k)).any(|(_, r)| r.page == *p) || member_pages.contains(p))
        .collect();
    let at = lines.len().min(2);
    let words: Vec<String> = pages.iter().map(|p| refs::page_number_words(doc, *p)).collect();
    let shown = match references.as_slice() {
        [one] => format!("References: {} (orphan)", one.text),
        many => format!("References: {}", many.iter().map(|r| r.locator()).collect::<Vec<_>>().join(" <-> ")),
    };
    lines.insert(at, shown);
    lines.insert(at, format!("Drawn on: {}", words.join(", ")));
    lines
}

/// A cable's hover with its view line (`View: fan`…) after the `id …` line.
pub fn with_view(mut lines: Vec<String>, view: &str) -> Vec<String> {
    lines.insert(lines.len().min(2), view.to_string());
    lines
}

/// The core label of member `w` of cable `k`, when it has one that is not blank, as drawn.
pub fn core_of(doc: &Document, k: CableId, w: WireId) -> Option<String> {
    super::member_core(doc.cables().get(&k)?.cores.iter().find(|m| m.wire() == Some(w))?)
}
