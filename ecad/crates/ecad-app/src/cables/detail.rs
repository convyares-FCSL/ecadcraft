//! Project ▸ Cables: the workspace's right panel for the selected cable.

use ecad_electrical::reports::kept::CableColumn;
use ecad_widgets::prim::Tone;

use crate::shell::app::App;
use crate::workspace::detail::*;

pub(crate) fn cable(t: &App, k: usize) -> Option<Detail> {
    let r = t.dialogs.reports.as_ref()?;
    let v = &r.cables;
    let rows = &v.source.rows;
    let tag = rows.text(k, CableColumn::Tag.header());
    let (from, to) = (rows.text(k, CableColumn::From.header()), rows.text(k, CableColumn::To.header()));
    // A cable not drawn has no drawing to flag (no type, no ends yet): its one row is
    // "not in the drawing yet".
    let mut checks: Vec<(String, Tone)> = if v.source.drawn(k) {
        rows.rows.get(k).map(|r| r.iter().filter_map(|c| c.flag.clone()).map(|f| (f, Tone::Error)).collect()).unwrap_or_default()
    } else {
        Vec::new()
    };
    // Each open core, as an error, with its two fixes.
    let mut core_fixes = Vec::new();
    for (i, open) in v.source.open.get(k).into_iter().flatten().enumerate() {
        checks.push((open.message(), Tone::Error));
        if let Some(cable) = v.ids.get(k) {
            core_fixes.push(ecad_electrical::checks::Fix::CoreOpen(*cable, u16::try_from(i).unwrap_or(u16::MAX)));
        }
    }
    if let Some(n) = v.source.spare(k).filter(|n| *n > 0) {
        checks.push((format!("{}.", ecad_electrical::terminal_strips::cores::spare_core_words(n)), Tone::Neutral));
    }
    // A cable not drawn says so (Info), with no core rows.
    let drawn = v.source.drawn(k);
    if !drawn {
        checks.push((format!("{tag} is not in the drawing yet."), Tone::Neutral));
    }
    let subtitle = if drawn {
        format!("{} to {}", nonempty(&rows.text(k, CableColumn::FromLocation.header())), nonempty(&rows.text(k, CableColumn::ToLocation.header())))
    } else {
        "not drawn".to_string()
    };
    Some(Detail {
        title: tag.clone(),
        provisional: false,
        subtitle,
        go: v.gos.get(k).copied().flatten().map(|(pg, at)| ("Page".to_string(), Go::Place(pg, at))),
        checks,
        editor: false,
        cable_editor: v.ids.get(k).map(|c| (*c, [from, to])),
        core_fixes,
        // Identity, Part and Ends are the cable editor's; the Rating section stays its own,
        // and shows a note when the cable has nothing to rate yet.
        sections: vec![match v.source.rating(k) {
            Some(r) => rating_section(t, Some(r), &v.source.rating.methods),
            None => Section { title: "Rating", hint: "not rated".into(), note: Some(RATE_NOTE), ..Section::default() },
        }],
    })
}
