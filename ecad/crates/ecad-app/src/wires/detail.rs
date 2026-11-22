//! Project ▸ Wires: the workspace's right panel for the selected wire.

use ecad_widgets::prim::Tone;

use crate::shell::app::App;
use crate::workspace::detail::*;

pub(crate) fn wire(t: &App, k: usize) -> Option<Detail> {
    let r = t.reports.as_ref()?;
    let v = &r.wires;
    let rows = &v.source.rows;
    let number = rows.text(k, "Wire number");
    let (from, to, page) = (rows.text(k, "From"), rows.text(k, "To"), rows.text(k, "Page"));
    let checks = rows.rows.get(k).map(|r| r.iter().filter_map(|c| c.flag.clone()).map(|f| (f, Tone::Warning)).collect()).unwrap_or_default();
    // Board 2.05's hints: a number is given by the numbering ("auto") unless it is provisional; a
    // colour or size is the category's ("from category") unless the wire sets its own ("set
    // here"), as the wire report records it.
    let number_hint = match (v.source.provisional(k), number.trim().is_empty()) {
        (true, _) => "provisional",
        (false, false) => "auto",
        (false, true) => "",
    };
    let has_category = !rows.text(k, "Category").trim().is_empty();
    let (colour_own, size_own) = v.own.get(k).copied().unwrap_or_default();
    let source = |own: bool, value: &str| match (own, has_category && !value.trim().is_empty()) {
        (true, _) => "set here",
        (false, true) => "from category",
        (false, false) => "",
    };
    Some(Detail {
        title: number.clone(),
        provisional: v.source.provisional(k),
        subtitle: format!("Wire \u{b7} {from} to {to} \u{b7} {page}"),
        go: v.gos.get(k).copied().flatten().map(|(pg, at)| (page.clone(), Go::Place(pg, at))),
        checks,
        editor: false,
        cable_editor: None,
        core_fixes: Vec::new(),
        sections: vec![
            Section {
                title: "Identity",
                props: vec![
                    set("Number", nonempty(&number), number_hint).bold(),
                    set("Potential", nonempty(&rows.text(k, "Potential")), "from the drawing"),
                    read("From", nonempty(&from)),
                    read("To", nonempty(&to)),
                ],
                ..Section::default()
            },
            Section {
                title: "Made as",
                hint: "from the category".into(),
                props: vec![
                    set("Category", nonempty(&rows.text(k, "Category")), "").choice(),
                    set("Colour", nonempty(&rows.text(k, "Colour")), source(colour_own, &rows.text(k, "Colour"))).swatch(&rows.text(k, "Colour")),
                    set("Size", nonempty(&rows.text(k, "Cross-section")), source(size_own, &rows.text(k, "Cross-section"))).unit(MM2),
                    read("Cable", nonempty(&rows.text(k, "Cable"))),
                ],
                ..Section::default()
            },
            rating_section(t, v.source.rating(k), &v.source.rating.methods),
            Section {
                title: "Ends",
                props: vec![later("Ferrules", CABLE_MANAGER_COMES), later("Marker", CABLE_MANAGER_COMES), later("Length", CABLE_MANAGER_COMES)],
                ..Section::default()
            },
        ],
    })
}
