//! Settings ▸ Project: the Project info and Project settings popouts (design 019 §7.1, split by
//! the owner), contents unchanged, as two tabs (brief 042). Their copy is the trial's
//! `project_props`, filled from `projectprops {}` when the tab is shown; OK writes the changed
//! fields as before (one transaction per line) and closes Settings, Cancel writes nothing.

use eframe::egui;

use crate::Trial;
use crate::sheets_ui::{Out, Part, Popout};

/// The tabs' accessible names.
pub const INFO_TAB: &str = "settings.project.info";
pub const SETTINGS_TAB: &str = "settings.project.settings";
/// Said where a project is needed.
pub const NO_PROJECT: &str = "No project is open.";

/// Draw the section.
pub fn show(t: &mut Trial, ui: &mut egui::Ui) {
    let current = t.project_props.as_ref().map_or(Popout::Info, |d| d.popout);
    let mut choose = None;
    ui.horizontal(|ui| {
        for (popout, label, name) in [(Popout::Info, "Info", INFO_TAB), (Popout::Settings, "Settings", SETTINGS_TAB)] {
            let on = current == popout;
            let r = ui.selectable_label(on, label);
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, name));
            if r.clicked() && !on {
                choose = Some(popout);
            }
        }
    });
    ui.separator();
    if let Some(p) = choose {
        t.fill_project_props(p);
    }
    let part = if current == Popout::Settings { Part::Settings } else { Part::Info };
    body(t, ui, current, part);
}

/// The copy of `popout` drawn as `part`, filled first if there is none of that popout.
pub fn body(t: &mut Trial, ui: &mut egui::Ui, popout: Popout, part: Part) {
    if !t.project_open() {
        ecad_ui::prim::read_value(ui, NO_PROJECT);
        return;
    }
    if t.project_props.as_ref().is_none_or(|d| d.popout != popout) {
        t.fill_project_props(popout);
    }
    let Some(d) = &mut t.project_props else { return };
    if let Some(out) = d.show_in(ui, part) {
        t.project_props_out(out);
    }
}

impl Trial {
    /// Fill the Project info or Project settings copy from `projectprops {}` (reads only).
    pub fn fill_project_props(&mut self, popout: Popout) {
        let read_only = self.header().read_only;
        match self.run_value("projectprops", serde_json::json!({})) {
            Ok(v) => {
                let mut d = crate::sheets_ui::ProjectProps::from_json(&v, read_only, popout);
                if popout == Popout::Settings {
                    // Brief 031: the Text section reads `textstyle {}`; brief 032 §4.4: the wire
                    // label rows read `wiresettings {}` (reads only).
                    d.text = self.run_value("textstyle", serde_json::json!({})).ok().map(|t| crate::text_ui::TextSection::from_json(&t));
                    d.labels = self.run_value("wiresettings", serde_json::json!({})).ok().map(|w| crate::sheets_ui::WireLabels::from_json(&w));
                }
                self.project_props = Some(d);
            }
            Err(e) => self.cad.session.echo(e),
        }
    }

    /// What the Project or Text styles section asked for: OK runs its lines (`projectprops`,
    /// then `textstyle` and `wiresettings`; the first refusal stops and is shown) and closes
    /// Settings; Cancel closes it.
    pub fn project_props_out(&mut self, out: Out) {
        match out {
            Out::Run(lines) => match lines.lines().find_map(|l| self.run_line(l)) {
                None => self.close_settings(),
                Some(e) => {
                    if let Some(d) = &mut self.project_props {
                        d.error = Some(e);
                    }
                }
            },
            Out::Close => self.close_settings(),
            Out::OpenRevisions => self.open_revisions(),
            Out::Open(_) | Out::SwitchToMain | Out::MergeIntoMain | Out::Release(_) | Out::OpenFolder(_) => {}
        }
    }
}
