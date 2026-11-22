//! Brief 059 (design 059 §5, §6): the cable manager's actions in the app: the Checks fixes of
//! `cable-not-drawn`, `cable-end-off-plan`, `no-gland` and `cable-bad-value`.
//!
//! The app is thin: every write is a command (`cableedit`, `cableremove`); this module only picks
//! the command and its parameters, and moves the view.

use ecad_electrical::cables::{self, Side};
use ecad_electrical::locations::designations::Aspect;
use serde_json::json;
use {ecad_electrical::checks::Check, ecad_electrical::checks::Fix, ecad_electrical::checks::Kind, ecad_electrical::checks::Object};

/// **Assign in the drawing…** (the `cable-not-drawn` fix's first button).
pub(crate) const ASSIGN_IN_DRAWING: &str = "Assign in the drawing\u{2026}";
/// **Use the drawing** (the `cable-end-off-plan` fix's first button).
pub(crate) const USE_THE_DRAWING: &str = "Use the drawing";
/// **Go to** that end.
pub(crate) const GO_TO_END: &str = "Go to";
/// **Set gland** (the `no-gland` fix).
pub(crate) const SET_GLAND: &str = "Set gland";

/// The two buttons of a 059 fix, when it has two.
pub fn pair(f: Fix) -> Option<[&'static str; 2]> {
    match f {
        Fix::AssignCable(_) => Some([ASSIGN_IN_DRAWING, crate::checks::panel::REMOVE]),
        Fix::UseDrawing(..) => Some([USE_THE_DRAWING, GO_TO_END]),
        _ => None,
    }
}

/// The button name of a 059 fix with one button.
pub fn single(f: Fix) -> &'static str {
    match f {
        Fix::ShowCableField(_, cables::CableField::Gland(_)) => SET_GLAND,
        _ => crate::checks::panel::SHOW,
    }
}

impl crate::shell::app::App {
    /// Button `k` of a 059 fix.
    pub(crate) fn cable_fix(&mut self, f: Fix, k: usize) {
        match (f, k) {
            (Fix::AssignCable(cable), 0) => self.assign_in_drawing(cable),
            (Fix::AssignCable(cable), _) => {
                let _ = self.run_value("cableremove", json!({"cable": cable.hex()}));
            }
            (Fix::UseDrawing(cable, side), 0) => {
                let Some(p) = self.project.clone() else { return };
                // That end's plan becomes its drawn location and function (one `cableedit`).
                let drawn = p.read(&mut self.cad.session, |p, _| {
                    let doc = p.ledger.document();
                    cables::off_plan(doc, cable).into_iter().find(|o| o.side == side).map(|o| o.drawn)
                });
                let Ok(Some(end)) = drawn else { return };
                let mut set = serde_json::Map::new();
                if let Some(l) = end.locations.iter().next() {
                    set.insert("location".into(), json!(l));
                }
                if let Some(f) = end.functions.iter().next() {
                    set.insert("function".into(), json!(f));
                }
                let _ = self.run_value("cableedit", json!({"cable": cable.hex(), "set": {side.word(): set}}));
            }
            (Fix::UseDrawing(cable, _), _) => {
                let Some(p) = self.project.clone() else { return };
                let at = p.read(&mut self.cad.session, |p, _| {
                    ecad_model::naming::numbering::rep_anchor(p.ledger.document(), ecad_model::Subject::Cable(cable))
                });
                let (page, at) = at.ok().flatten().map_or((None, None), |(p, a)| (Some(p), Some(a)));
                let check = Check { kind: Kind::CableEndOffPlan, page, at, object: Object::Cable(cable), which: String::new(), fix: None };
                self.panel_action(crate::pages::project_panel::Action::GotoCheck(Box::new(check)));
            }
            (Fix::ShowCableField(cable, _), _) => self.show_rating(ecad_electrical::circuits::RatedSubject::Cable(cable)),
            _ => {}
        }
    }

    /// **Assign in the drawing…** for `cable` (design 059 §5.6): Draw, on the first page whose
    /// location matches its planned cabinet end, else the active page, with the slash tool started
    /// and the cable picked.
    pub fn assign_in_drawing(&mut self, cable: ecad_model::CableId) {
        let Some(p) = self.project.clone() else { return };
        let page = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let plan = cables::planned(doc, cable, Side::From, Aspect::Location)?;
            doc.page_order.iter().copied().find(|pg| {
                let key = doc.pages.get(pg).and_then(|x| ecad_electrical::locations::designations::page_key(x, Aspect::Location));
                ecad_electrical::locations::designations::shown(doc, Aspect::Location, key.as_deref()).is_some_and(|c| c.eq_ignore_ascii_case(&plan))
            })
        });
        self.set_workspace(crate::shell::Workspace::Draw);
        if let Ok(Some(page)) = page {
            let _ = self.show_page(page);
        }
        // The slash tool with the cable already picked: the same `Cable` keyword the command
        // line takes; the tool says `Slash the wires of -W03 (4 cores).`
        let tag = p.read(&mut self.cad.session, |p, _| ecad_engine::cables::views::cable_title(p.ledger.document(), cable));
        let Ok(tag) = tag else { return };
        self.focus_drawing = true;
        if let Err(e) = self.cad.session.start("cableslash") {
            self.cad.session.echo(e.to_string());
            return;
        }
        let _ = self.cad.session.input(ecad_engine::cad::Input::Keyword("Cable".into()));
        match self.cad.session.input(ecad_engine::cad::Input::Text(tag)) {
            Ok(_) => self.shell.slash_picker = crate::inspector::slash_picker::Picker { want: Some(cable), ..Default::default() },
            Err(e) => self.cad.session.echo(e.to_string()),
        }
    }
}
