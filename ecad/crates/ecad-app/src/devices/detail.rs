//! Project ▸ Devices: the workspace's right panel for the selected device.

use ecad_model::DeviceId;
use ecad_widgets::prim::Tone;

use crate::shell::app::App;
use crate::workspace::detail::*;

pub(crate) fn device(t: &App, c: DeviceId) -> Option<Detail> {
    let r = t.panel.devices.source.row(c)?;
    let e = &r.entry;
    let prop = |i: usize| e.properties.get(i).cloned().flatten().unwrap_or_default();
    let tag = e.tag.clone().unwrap_or_default();
    let place = e.symbols.first().map(|s| s.place.clone());
    let symbol = e.symbols.first().map(|s| s.name.clone()).unwrap_or_else(|| crate::reports::report::DASH.into());
    let subtitle = match &place {
        Some(p) => format!("{} \u{b7} drawn at {p}", nonempty(&e.description)),
        None => format!("{} \u{b7} not on a page yet", nonempty(&e.description)),
    };
    let go = (!e.symbols.is_empty()).then(|| ("Page".to_string(), Go::Symbols(e.symbols.iter().map(|s| s.rep).collect())));
    let mut checks = Vec::new();
    if e.tag.as_deref().is_some_and(ecad_model::naming::tag::is_provisional) {
        checks.push(("The tag is provisional.".to_string(), Tone::Provisional));
    }
    if e.symbols.is_empty() {
        checks.push(("Not on a page yet.".to_string(), Tone::Warning));
    }
    if prop(3).is_empty() {
        checks.push(("No part yet: it has no order code.".to_string(), Tone::Warning));
    }
    let maker = prop(2);
    let part = prop(3);
    let main = if part.is_empty() { "No parts yet".to_string() } else { part.clone() };
    let mut lines = vec![(main, if maker.is_empty() { "main part".to_string() } else { format!("{maker} \u{b7} main part") })];
    if r.parts > 1 {
        lines.push((
            format!("{} more line{}", r.parts - 1, if r.parts == 2 { "" } else { "s" }),
            "listed in Project \u{25b8} Reports \u{25b8} Parts list".into(),
        ));
    }
    let drawn = Section {
        title: "Where it is drawn",
        hint: String::new(),
        lines: e.symbols.iter().map(|s| (s.name.clone(), s.place.clone())).collect(),
        ..Section::default()
    };
    Some(Detail {
        title: tag,
        provisional: e.tag.as_deref().is_some_and(ecad_model::naming::tag::is_provisional),
        subtitle,
        go,
        checks,
        editor: true,
        cable_editor: None,
        core_fixes: Vec::new(),
        sections: vec![
            Section {
                title: "Symbol",
                hint: "read from the drawing".into(),
                props: vec![read("Symbol", symbol), read("Location", nonempty(&r.location))],
                ..Section::default()
            },
            Section {
                title: "Parts",
                hint: format!("{} line{}", r.parts, ecad_widgets::plural(r.parts)),
                lines,
                later: vec![("+ Accessory", CATALOGUE_COMES), ("From catalogue", CATALOGUE_COMES), ("Save as a kit", CATALOGUE_COMES)],
                note: Some("The main part is first and gives the device its ratings. A line with no order code raises a check."),
                ..Section::default()
            },
            drawn,
        ],
    })
}
