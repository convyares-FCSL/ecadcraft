//! Project ▸ Terminal blocks: the workspace's right panel for the selected connection point.

use ecad_widgets::prim::Tone;

use crate::shell::app::App;
use crate::workspace::detail::*;

pub(crate) fn side_text(cells: &[ecad_electrical::terminal_strips::strip_model::SideCell]) -> String {
    let v: Vec<String> = cells
        .iter()
        .map(|c| {
            let target = c.target.clone().unwrap_or_else(|| "open end".into());
            match &c.wire {
                Some(w) => format!("{target} \u{b7} wire {w}"),
                None => target,
            }
        })
        .collect();
    if v.is_empty() { "not wired".into() } else { v.join(", ") }
}

pub(crate) fn connection_point(t: &App, k: crate::terminal_strips::plan::BlockKey) -> Option<Detail> {
    let r = t.reports.as_ref()?;
    let row = r.connection_points.source.row(k)?;
    let crate::terminal_strips::plan::PlanRow::Block { tag, row, place, .. } = row else {
        return None;
    };
    let strip = r.strips.iter().find(|(c, _)| *c == k.0).map_or_else(|| tag.clone(), |(_, t)| t.clone());
    let mut checks = Vec::new();
    if row.inside.iter().chain(&row.outside).any(|c| c.target.is_none()) {
        checks.push(("An end is open.".to_string(), Tone::Error));
    }
    let cable = row.outside.iter().find_map(|c| c.cable.as_ref().map(|(_, t)| t.clone().unwrap_or_default())).unwrap_or_default();
    let core = row.outside.iter().find_map(|c| c.core.clone()).unwrap_or_default();
    let mut outside = side_text(&row.outside);
    if !cable.is_empty() {
        outside = format!("{outside} \u{b7} {cable} {core}");
    }
    Some(Detail {
        title: format!("{tag}:{}", row.shown),
        provisional: false,
        subtitle: format!("{} \u{b7} {strip}", row.ty.name),
        go: row.go.map(|(pg, at)| (place.clone(), Go::Place(pg, at))),
        checks,
        editor: false,
        cable_editor: None,
        core_fixes: Vec::new(),
        sections: vec![
            Section {
                title: "Identity",
                props: vec![
                    read("Strip", strip),
                    set("Position", row.label.clone(), "set in the table"),
                    read("Level", if row.level_name.is_empty() { row.level.to_string() } else { row.level_name.clone() }),
                    read("State", row.state()),
                ],
                ..Section::default()
            },
            Section { title: "Part", hint: "from part set".into(), props: vec![read("Type", row.ty.name)], ..Section::default() },
            Section {
                title: "Connections",
                hint: "read from the drawing".into(),
                props: vec![
                    read("Inside", side_text(&row.inside)),
                    read("Outside", outside),
                    read("Jumper bar", row.bridge_group.map_or_else(|| "none".to_string(), |g| format!("bridge {g}"))),
                ],
                ..Section::default()
            },
            Section { title: "Where it is drawn", lines: vec![("Schematic".into(), place.clone())], ..Section::default() },
        ],
    })
}
