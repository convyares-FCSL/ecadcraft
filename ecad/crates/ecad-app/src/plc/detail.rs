//! Project ▸ PLC: the workspace's right panel for the selected channel.

use crate::shell::app::App;
use crate::workspace::detail::*;

/// A channel row: its signal, where it is drawn and its test.
pub(crate) fn channel(t: &App, k: crate::plc::list::IoKey) -> Option<Detail> {
    use ecad_electrical::plc::signals;
    let r = t.reports.as_ref()?;
    let row = r.io.source.row(k)?;
    let addr = row.address.clone().unwrap_or_default();
    let mut subtitle = vec![addr.clone(), row.ty.to_string()];
    if let Some(tag) = &row.tag {
        subtitle.push(tag.clone());
    }
    let go = row.page.zip(row.at).map(|(pg, at)| (row.place.clone().unwrap_or_else(|| "Page".into()), Go::Place(pg, at)));
    let checks = Vec::new();
    let safe = match row.safe_shown() {
        Some((s, true)) => set("Safe", signals::safe_words(s), "default"),
        Some((s, false)) => set("Safe", signals::safe_words(s), "set here"),
        None => read("Safe", crate::reports::report::DASH),
    };
    let (drives, through) = signals::drives(&row.targets);
    let mut drawn: Vec<(String, String)> =
        drives.iter().map(|d| (format!("{}:{}{}", d.device, d.point, if through { " (passes through)" } else { "" }), d.place.clone())).collect();
    if drawn.is_empty() {
        drawn.push(("Drives".into(), crate::reports::report::DASH.into()));
    }
    Some(Detail {
        title: format!("{} channel {}", row.card_tag, row.channel),
        provisional: false,
        subtitle: subtitle.join(" \u{b7} "),
        go,
        checks,
        editor: false,
        cable_editor: None,
        core_fixes: Vec::new(),
        sections: vec![
            Section {
                title: "Signal",
                hint: "set in the table".into(),
                props: vec![
                    set("PLC tag", nonempty(row.tag.as_deref().unwrap_or_default()), ""),
                    set("Description", nonempty(row.text.as_deref().unwrap_or_default()), ""),
                    safe,
                    set("Address", nonempty(&ecad_electrical::plc::iolist::address_words(row)), if row.typed { "typed" } else { "automatic" }),
                    set("Pins", nonempty(&ecad_electrical::plc::iolist::pins_words(&row.pins)), ""),
                ],
                ..Section::default()
            },
            Section {
                title: "Drawn",
                hint: "read from the drawing".into(),
                props: vec![
                    read("Place", nonempty(row.place.as_deref().unwrap_or_default())),
                    read("Wire", nonempty(row.wire.as_deref().unwrap_or_default())),
                ],
                lines: drawn,
                ..Section::default()
            },
            Section { title: "Test", lines: vec![("No tests yet.".into(), String::new())], ..Section::default() },
        ],
    })
}
