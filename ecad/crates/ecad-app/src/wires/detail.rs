//! Project ▸ Wires: the workspace's right panel for the selected wire.

use ecad_electrical::reports::kept::WireColumn;
use ecad_widgets::prim::Tone;

use crate::shell::app::App;
use crate::workspace::detail::*;

pub(crate) fn wire(t: &App, k: usize) -> Option<Detail> {
    let r = t.reports.as_ref()?;
    let v = &r.wires;
    let rows = &v.source.rows;
    let number = rows.text(k, WireColumn::WireNumber.header());
    let (from, to, page) = (rows.text(k, WireColumn::From.header()), rows.text(k, WireColumn::To.header()), rows.text(k, WireColumn::Page.header()));
    let checks = rows.rows.get(k).map(|r| r.iter().filter_map(|c| c.flag.clone()).map(|f| (f, Tone::Warning)).collect()).unwrap_or_default();
    // The hints: a number is given by the numbering ("auto") unless it is provisional; a
    // colour or size is the category's ("from category") unless the wire sets its own ("set
    // here"), as the wire report records it.
    let number_hint = match (v.source.provisional(k), number.trim().is_empty()) {
        (true, _) => "provisional",
        (false, false) => "auto",
        (false, true) => "",
    };
    let has_category = !rows.text(k, WireColumn::Category.header()).trim().is_empty();
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
                    set("Potential", nonempty(&rows.text(k, WireColumn::Potential.header())), "from the drawing"),
                    read("From", nonempty(&from)),
                    read("To", nonempty(&to)),
                ],
                ..Section::default()
            },
            Section {
                title: "Made as",
                hint: "from the category".into(),
                props: vec![
                    set("Category", nonempty(&rows.text(k, WireColumn::Category.header())), "").choice(),
                    set(
                        "Colour",
                        nonempty(&rows.text(k, WireColumn::Colour.header())),
                        source(colour_own, &rows.text(k, WireColumn::Colour.header())),
                    )
                    .swatch(&rows.text(k, WireColumn::Colour.header())),
                    set(
                        "Size",
                        nonempty(&rows.text(k, WireColumn::CrossSection.header())),
                        source(size_own, &rows.text(k, WireColumn::CrossSection.header())),
                    )
                    .unit(MM2),
                    read("Cable", nonempty(&rows.text(k, WireColumn::Cable.header()))),
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
