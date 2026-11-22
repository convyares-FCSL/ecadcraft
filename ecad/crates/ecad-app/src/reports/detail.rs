//! Project ▸ Reports: the workspace's right panel for the selected report row.

use ecad_widgets::prim::Tone;

use crate::shell::app::App;
use crate::workspace::detail::*;

pub(crate) fn report(t: &App, k: usize) -> Option<Detail> {
    let rep = crate::reports::list::REPORTS.get(k)?;
    let r = t.reports.as_ref()?;
    let columns: Vec<(String, String)> =
        if rep.tab == Some(r.tab) { r.table().headers.iter().map(|h| ((*h).to_string(), "shown".to_string())).collect() } else { Vec::new() };
    // Counted when the copy is taken, not each frame.
    let flagged = if rep.tab == Some(r.tab) { r.flagged } else { 0 };
    let checks = if flagged > 0 { vec![(format!("{flagged} cell{} marked ?", ecad_widgets::plural(flagged)), Tone::Warning)] } else { Vec::new() };
    Some(Detail {
        title: rep.name.to_string(),
        provisional: false,
        subtitle: "Report \u{b7} generated on demand".into(),
        go: None,
        checks,
        editor: false,
        cable_editor: None,
        core_fixes: Vec::new(),
        sections: vec![
            Section {
                title: "Contents",
                props: vec![read("Reads", rep.reads), read("Group by", rep.grouped), later("Merge", crate::workspace::frame::REPORTS_COME)],
                ..Section::default()
            },
            Section {
                title: "Columns",
                hint: "as the preview shows them".into(),
                lines: columns,
                later: vec![("Choose columns", crate::workspace::frame::REPORTS_COME)],
                ..Section::default()
            },
            Section {
                title: "Output",
                props: vec![
                    later("In the book", crate::workspace::frame::REPORTS_COME),
                    later("Layout", crate::workspace::frame::REPORTS_COME),
                    later("Updates", crate::workspace::frame::REPORTS_COME),
                ],
                note: Some("Export CSV from the preview."),
                ..Section::default()
            },
        ],
    })
}
