//! The wheel in the window (brief 041 step 3): a right-click on the page while no tool runs
//! classifies what is under the cursor (`ecad_cadcraft::interact::kind`) and opens that kind's
//! wheel there; a right-click on a page row in the Pages tab opens the page row's wheel. Choosing
//! a slot runs its action ([`Trial::run_action`]), the same path the tool strip, the keys and
//! Ctrl+K use.
//!
//! While a tool runs, a right-click is Enter (it finishes the step, CADCraft's default) and opens
//! no wheel (U0 Q4). Over Lists, Library and Publish the page is hidden and does not answer.
//!
//! Update map: the wheel reads the object's kind and each slot's `enabled` once, when it opens;
//! nothing while it is closed.

use cadcraft_engine::Input;
use cadcraft_engine::geom::Vec2;
use ecad_cadcraft::actions::{self, Action, App, Choices, Context, Enabled, IdOf, ObjectKind, Params};
use ecad_cadcraft::interact::kind::{self, Target};
use ecad_model::{PageId, Subject};
use ecad_ui::wheel::{Event, SlotSpec, Wheel};
use eframe::egui;
use serde_json::{Value, json};

use super::{LeftTab, ListsView, Workspace};
use crate::Trial;

/// The open wheel: its widget state, the object and its eight slots.
#[derive(Debug, Clone)]
pub struct Open {
    pub widget: Wheel,
    pub target: Target,
    pub slots: [Option<&'static Action>; 8],
    pub specs: [Option<SlotSpec>; 8],
    /// Each slot's list values (what a row of its short list passes).
    pub values: [Vec<Value>; 8],
    /// How many times the project was read to open it (once; the update map).
    pub reads: u64,
}

/// A refusal (spec §5): what was refused, why, and where (for **Show it**).
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub what: String,
    pub why: String,
    pub page: Option<PageId>,
    pub at: Option<[f64; 2]>,
    /// Refused by the window or a command run from it; a running tool's echoed line is not marked.
    pub refused: bool,
}

impl Trial {
    /// The project takes writes now (open, not read only, no revision shown).
    /// A drawing with no ECAD project is CADCraft's and always takes writes.
    pub fn writable(&mut self) -> bool {
        if self.project.is_none() {
            return true;
        }
        let h = self.header();
        h.name.is_some() && !h.read_only && self.panel.revision.is_none()
    }

    /// Whether the wheel is open, and on what (tests).
    pub fn wheel_target(&self) -> Option<&Target> {
        self.shell.tools.wheel.as_ref().map(|w| &w.target)
    }

    /// Lane U4b's part of the main view, each frame after the page: open the wheel on a right
    /// press over the page while no tool runs, draw it and act on it; then the intent line.
    pub(crate) fn wheel_frame(&mut self, ui: &mut egui::Ui) {
        // CADCraft reports an idle right-click too; the press below is what opens the wheel.
        let _ = self.cad.take_secondary_click();
        let ctx = ui.ctx().clone();
        self.search_ran();
        let idle = self.cad.session.running.is_none() && !self.shell.tools.was_running;
        let press = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary).then(|| i.pointer.interact_pos()).flatten());
        if let Some(at) = press
            && idle
            && self.shell.tools.wheel.is_none()
            && !self.shell.state.cad_chrome
            && self.over_shown_page(&ctx, at)
        {
            let cursor = self.cad.canvas.xf.map_or(self.cad.session.cursor, |xf| xf.to_world(at));
            match kind::under_cursor(&self.cad.session, cursor) {
                Ok(target) => self.open_wheel(at, target, true),
                Err(why) => self.refuse("Wheel", why, None),
            }
        }
        self.show_wheel(&ctx);
        self.intent_frame(ui);
        self.shell.tools.was_running = self.cad.session.running.is_some();
    }

    /// A page or action Ctrl+K picked: show the page, or run the action on the selection (or
    /// start its tool).
    fn search_ran(&mut self) {
        let Some(pick) = self.search.ran.take() else { return };
        self.set_workspace(Workspace::Draw);
        match pick {
            crate::search::Pick::Page(pg) => {
                self.shell.restore_view = true;
                let _ = self.show_page(pg);
            }
            crate::search::Pick::Action(id) => {
                let Some(a) = actions::find(id) else { return };
                let target = self.target_from_selection();
                if let Err(why) = self.run_action(a, target.as_ref(), None) {
                    self.refuse(a.label, why, target.as_ref());
                }
            }
            crate::search::Pick::Symbol(_) | crate::search::Pick::Device(_) => {}
        }
    }

    /// Open the wheel at screen point `at` for `target` (`held`: a right press opened it).
    pub fn open_wheel(&mut self, at: egui::Pos2, target: Target, held: bool) {
        let slots = actions::wheel(target.kind);
        let writable = self.writable();
        let cx = Context { target: Some(&target), writable };
        let mut specs: [Option<SlotSpec>; 8] = Default::default();
        let mut values: [Vec<Value>; 8] = Default::default();
        for (i, a) in slots.iter().enumerate() {
            let Some(a) = a else { continue };
            let mut spec = SlotSpec::new(a.label, a.icon);
            if let Enabled::Greyed { why, next } = (a.enabled)(&cx) {
                spec.greyed = Some(why);
                spec.next = next.and_then(actions::find).map(|n| n.label.to_string());
            } else if let Params::Choose(c) = a.params {
                let (labels, vals) = self.choices(c);
                spec.list = Some(labels);
                if let Some(v) = values.get_mut(i) {
                    *v = vals;
                }
            }
            if let Some(s) = specs.get_mut(i) {
                *s = Some(spec);
            }
        }
        self.shell.tools.flyout = None;
        // Kept inside the window, so every slot can be reached (a page row's wheel opens near the
        // window's left edge).
        let at = match self.shell.slots {
            Some(sl) => {
                let m = ecad_ui::tokens::wheel::OUTER_RADIUS + ecad_ui::tokens::space::XXL;
                let (x0, x1) = (sl.top_bar.left() + m, (sl.top_bar.right() - m).max(sl.top_bar.left() + m));
                let (y0, y1) = (
                    sl.top_bar.bottom() + m + ecad_ui::tokens::space::XXL,
                    (sl.status.top() - m - 2.0 * ecad_ui::tokens::space::XXL).max(sl.top_bar.bottom() + m),
                );
                egui::pos2(at.x.clamp(x0, x1), at.y.clamp(y0.min(y1), y1))
            }
            None => at,
        };
        self.shell.tools.wheel = Some(Open { widget: Wheel::new(at, held), target, slots, specs, values, reads: 1 });
    }

    /// The page row's wheel (the Pages tab's right-click).
    pub fn open_page_wheel(&mut self, page: PageId, at: egui::Pos2) {
        let name = self.panel.rows().iter().find(|r| r.id == page).map_or_else(|| "page".to_string(), |r| format!("Page {}", r.number));
        let mut t = Target::new(ObjectKind::PageRow, [0.0, 0.0], name);
        t.page = Some(page);
        self.open_wheel(at, t, false);
    }

    /// A slot's short list: its labels and the values its rows pass.
    fn choices(&mut self, c: Choices) -> (Vec<String>, Vec<Value>) {
        match c {
            Choices::Potentials => {
                let w = self.run_value("wiresettings", json!({})).unwrap_or(Value::Null);
                let mut names: Vec<String> = w
                    .get("potentials")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|p| p.get("name").and_then(Value::as_str).map(str::to_string)).collect())
                    .unwrap_or_default();
                names.truncate(ecad_ui::wheel::MAX_LIST - 1);
                let mut vals: Vec<Value> = names.iter().map(|n| Value::String(n.clone())).collect();
                names.push("None".into());
                vals.push(Value::Null);
                (names, vals)
            }
        }
    }

    fn show_wheel(&mut self, ctx: &egui::Context) {
        let Some(open) = self.shell.tools.wheel.as_mut() else { return };
        let ev = ecad_ui::wheel::show(ctx, &mut open.widget, &open.target.name, &open.specs);
        let (slots, target, values) = (open.slots, open.target.clone(), open.values.clone());
        let run = |t: &mut Trial, a: Option<&'static Action>, choice: Option<Value>| {
            t.shell.tools.wheel = None;
            if let Some(a) = a
                && let Err(why) = t.run_action(a, Some(&target), choice)
            {
                t.refuse(a.label, why, Some(&target));
            }
        };
        match ev {
            Event::None => {}
            Event::Cancel => self.shell.tools.wheel = None,
            Event::Run(i) => run(self, slots.get(i).copied().flatten(), None),
            Event::Choose(i, j) => {
                let v = values.get(i).and_then(|v| v.get(j)).cloned();
                run(self, slots.get(i).copied().flatten(), v);
            }
            Event::Next(i) => {
                let cx = Context { target: Some(&target), writable: true };
                let next = slots.get(i).copied().flatten().and_then(|a| match (a.enabled)(&cx) {
                    Enabled::Greyed { next, .. } => next.and_then(actions::find),
                    Enabled::Ok => None,
                });
                run(self, next, None);
            }
        }
    }

    /// Note a refusal for the intent line (and the command history).
    pub fn refuse(&mut self, what: &str, why: String, target: Option<&Target>) {
        self.cad.session.echo(format!("{what}: {why}"));
        self.shell.tools.intent.refusal =
            Some(Refusal { what: what.to_string(), why, page: target.and_then(|t| t.page), at: target.map(|t| t.at), refused: true });
    }

    /// The object the selection names, for an action run from a key or Ctrl+K: classified at
    /// its first handle (as if the wheel opened there).
    pub fn target_from_selection(&mut self) -> Option<Target> {
        let sel = self.cad.session.selection();
        let first = *sel.first()?;
        let uid = self.cad.session.state().ok()?.uid;
        let cursor = self.cad.session.cursor;
        let on = self.cad.session.doc().ok().and_then(|d| d.entity(first).and_then(|e| e.kind.grips().first().copied())).unwrap_or(cursor);
        let p = self.project.clone()?;
        let mut t = p.read(&mut self.cad.session, |p, _| kind::classify(p, uid, cursor, 0.0, Some(first), &sel, false)).ok()?;
        // A point on the object, for a form that asks for a click on it.
        t.at = [on.x, on.y];
        Some(t)
    }

    /// Run action `a` on `target` (the wheel's object; `None` for a tool, or the selection for a
    /// key or Ctrl+K). `choice` is a short list's value.
    ///
    /// # Errors
    /// The action is greyed (its reason), it needs an object and has none, or its command refused.
    pub fn run_action(&mut self, a: &'static Action, target: Option<&Target>, choice: Option<Value>) -> Result<(), String> {
        let writable = self.writable();
        if let Enabled::Greyed { why, .. } = (a.enabled)(&Context { target, writable }) {
            return Err(why);
        }
        self.shell.tools.intent.refusal = None;
        if self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        let need = || format!("{} needs an object: select one or right-click it.", a.label);
        let r = match a.params {
            Params::Start => self.start_command(a.command),
            Params::StartAt => {
                let t = target.ok_or_else(need)?;
                self.start_command(a.command)?;
                self.give_object(t).ok_or_else(need)?
            }
            Params::Selected => {
                if let Some(t) = target.filter(|t| !t.handles.is_empty()) {
                    self.cad.session.set_selection(t.handles.clone());
                }
                self.start_command(a.command)
            }
            Params::Id(of) => {
                let t = target.ok_or_else(need)?;
                let params = id_params(of, t).ok_or_else(need)?;
                self.cad.run(a.command, params).map(|_| ())
            }
            Params::At => {
                let t = target.ok_or_else(need)?;
                self.cad.run(a.command, json!({"at": t.at})).map(|_| ())
            }
            Params::Choose(Choices::Potentials) => {
                let t = target.ok_or_else(need)?;
                let w = t.conductor.ok_or_else(need)?;
                self.cad.run(a.command, json!({"conductor": w.hex(), "value": choice.unwrap_or(Value::Null)})).map(|_| ())
            }
            Params::App(app) => self.app_action(a, app, target),
        };
        self.after_logic();
        r
    }

    /// Answer the running form's first prompt with the object: the point the person right-clicked
    /// (as the click the form asks for), or its handle when it came from the selection. `None`
    /// when there is neither.
    fn give_object(&mut self, t: &Target) -> Option<Result<(), String>> {
        let at = Vec2::new(t.at[0], t.at[1]);
        self.cad.session.cursor = at;
        let prompt = self.cad.session.current_prompt();
        let r = if t.pointed {
            self.cad.session.input(Input::Point(at))
        } else if prompt.as_ref().is_some_and(|p| p.accept.select) {
            self.cad.session.input(Input::Pick(t.handles.clone()))
        } else if prompt.as_ref().is_some_and(|p| p.picks_objects()) {
            self.cad.session.input_object(*t.handles.first()?, Some(at))
        } else {
            // A click prompt: a point on the object (its first grip).
            self.cad.session.input(Input::Point(at))
        };
        Some(r.map_err(|e| e.to_string()))
    }

    fn start_command(&mut self, command: &str) -> Result<(), String> {
        self.focus_drawing = true;
        let r = self.cad.session.start(command).map_err(|e| e.to_string());
        if let Err(e) = &r {
            self.cad.session.echo(e.clone());
        }
        r
    }

    /// What the window does itself for an [`Params::App`] action.
    fn app_action(&mut self, a: &'static Action, app: App, target: Option<&Target>) -> Result<(), String> {
        let page = target.and_then(|t| t.page).or(self.panel.active);
        let select = |t: &mut Trial| {
            if let Some(h) = target.map(|t| t.handles.clone()).filter(|h| !h.is_empty()) {
                t.cad.session.set_selection(h);
            }
        };
        match app {
            App::Select => {}
            App::Properties => {
                select(self);
                self.shell.state.right_collapsed = false;
            }
            App::GotoOther => {
                let t = target.ok_or("no device")?;
                let c = t.component.ok_or("no device")?;
                let here = t.page;
                let p = self.project.clone().ok_or("no ECAD project is open")?;
                // The device's symbols on the first other page it is drawn on, all selected there.
                let there = p.read(&mut self.cad.session, |p, _| {
                    let doc = p.ledger.document();
                    let page = doc.representations_of(Subject::Component(c)).find(|(_, r)| Some(r.page) != here).map(|(_, r)| r.page);
                    page.map(|pg| doc.representations_of(Subject::Component(c)).filter(|(_, r)| r.page == pg).map(|(id, _)| id).collect::<Vec<_>>())
                })?;
                self.goto_symbols(&there.ok_or_else(|| format!("{} has no symbol on another page.", t.name))?)?;
            }
            App::DeviceRow | App::TerminalRow | App::CableRow => {
                self.set_workspace(Workspace::Lists);
                self.shell.lists = match app {
                    App::DeviceRow => ListsView::Devices,
                    App::TerminalRow => ListsView::Terminals,
                    _ => ListsView::Reports,
                };
            }
            App::ShowNet => {
                let t = target.ok_or("no wire")?;
                let w = t.conductor.ok_or("no wire")?;
                let v = self.cad.run("net", json!({"conductor": w.hex()}))?;
                let wires: Vec<ecad_model::ConductorId> = v
                    .get("conductors")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .filter_map(|h| u128::from_str_radix(h, 16).ok())
                            .map(ecad_model::ConductorId::from)
                            .collect()
                    })
                    .unwrap_or_default();
                if let (Some(pg), Some(p)) = (page, self.project.clone()) {
                    let handles = p.read(&mut self.cad.session, |p, _| {
                        wires
                            .iter()
                            .flat_map(|w| ecad_cadcraft::checks::handles_of(p, pg, ecad_cadcraft::checks::Object::Conductor(*w)))
                            .collect::<Vec<_>>()
                    })?;
                    self.cad.session.set_selection(handles);
                }
                let number = v.get("number").and_then(Value::as_str).unwrap_or("unnumbered");
                self.shell.tools.intent.note = Some(format!("Net {number}: {} wire{}", wires.len(), if wires.len() == 1 { "" } else { "s" }));
            }
            App::CableReference => {
                let t = target.ok_or("no cable")?;
                let k = t.cable.ok_or("no cable")?;
                let p = self.project.clone().ok_or("no ECAD project is open")?;
                let here = t.page;
                let refs = p.read(&mut self.cad.session, |p, _| {
                    p.ledger
                        .document()
                        .representations_of(Subject::Cable(k))
                        .filter(|(_, r)| ecad_cadcraft::references::is_reference(r))
                        .map(|(id, r)| (id, r.page))
                        .collect::<Vec<_>>()
                })?;
                match refs.iter().find(|(_, pg)| Some(*pg) == here) {
                    Some((r, _)) => {
                        self.cad.run("cablegoto", json!({"reference": r.hex()}))?;
                    }
                    None => {
                        let (_, pg) = refs.first().ok_or_else(|| format!("{} has no reference to another page.", t.name))?;
                        self.show_page(*pg)?;
                    }
                }
            }
            App::PreviewReference => {
                select(self);
                self.preview_reference()?;
            }
            App::EditText => {
                let t = target.ok_or("no text")?;
                let command = if t.component.is_some() {
                    "deviceprops"
                } else if t.conductor.is_some() {
                    "wirenumber"
                } else {
                    "textedit"
                };
                if command == "textedit" {
                    select(self);
                    self.start_command(command)?;
                } else {
                    self.start_command(command)?;
                    self.give_object(t).ok_or("no text")??;
                }
            }
            App::PageProperties => {
                if let Some(pg) = page
                    && self.panel.active != Some(pg)
                {
                    self.show_page(pg)?;
                }
                self.cad.session.set_selection(Vec::new());
                self.shell.state.right_collapsed = false;
            }
            App::PageRow => {
                self.shell.state.left.tab = LeftTab::Pages;
                self.shell.state.left.collapsed = false;
                if let Some(pg) = page {
                    self.panel.reveal(pg);
                }
            }
            App::OpenPage => self.show_page(page.ok_or("no page")?)?,
            App::NewPage => {
                // From a page row in a folder: a new page in that folder (as the row's menu did).
                let folder = target
                    .filter(|t| t.kind == ObjectKind::PageRow)
                    .and_then(|t| t.page)
                    .and_then(|pg| self.panel.rows().iter().find(|r| r.id == pg).and_then(|r| r.folder));
                self.panel_action(match folder {
                    Some(f) => crate::project_panel::Action::NewPageIn(Some(f)),
                    None => crate::project_panel::Action::New,
                });
            }
            App::RenamePage => self.panel_action(crate::project_panel::Action::RenameInBox(page.ok_or("no page")?)),
            App::PreviewPage => self.panel_action(crate::project_panel::Action::Preview(page.ok_or("no page")?)),
            App::NewFolder => {
                let folder = page.and_then(|pg| self.panel.rows().iter().find(|r| r.id == pg).and_then(|r| r.folder));
                self.panel_action(crate::project_panel::Action::NewFolder(folder));
            }
            App::FindSymbol => {
                self.search.open = true;
                self.search.focus = true;
            }
            App::Renumber => self.open_renumber(false, false, None),
            App::RenumberWires => self.open_renumber(false, true, None),
            App::Finalise => self.open_renumber(true, false, None),
            App::WireSettings => self.panel_action(crate::project_panel::Action::ProjectSettings),
            App::Template => {
                let line = template(a, target, &self.picked());
                self.command = line;
                self.command_hint = Some("Fill in the <placeholders>, then press Enter.");
                self.focus_command = true;
                self.developer.open = true;
            }
        }
        Ok(())
    }
}

/// The JSON form's parameters for an [`Params::Id`] action on `t`.
fn id_params(of: IdOf, t: &Target) -> Option<Value> {
    Some(match of {
        IdOf::Component => json!({"component": t.component?.hex()}),
        IdOf::Conductor => json!({"conductor": t.conductor?.hex()}),
        IdOf::Cable => json!({"cable": t.cable?.hex()}),
        IdOf::Source => json!({"source": t.representation?.hex()}),
        IdOf::Conductors => json!({"conductors": [t.conductor?.hex()]}),
        IdOf::End => json!({"conductor": t.conductor?.hex(), "end": match t.end? { ecad_model::End::A => "A", ecad_model::End::B => "B" }}),
        IdOf::Electrical => match (t.cable, t.conductor, t.component) {
            (Some(k), _, _) if t.kind == ObjectKind::Cable => json!({"cable": k.hex()}),
            (_, Some(w), _) => json!({"conductor": w.hex()}),
            (_, _, Some(c)) => json!({"component": c.hex()}),
            (Some(k), _, _) => json!({"cable": k.hex()}),
            _ => return None,
        },
    })
}

/// The Text panel's line for a template action (no form exists yet): the object's IDs filled in,
/// the rest as `<placeholders>`.
fn template(a: &Action, target: Option<&Target>, picked: &crate::tools::Picked) -> String {
    let tool = match a.command {
        "setprop" => "ecad.setprop",
        "cableadd" => "ecad.cableadd",
        "ecaddelete" => "ecad.delete",
        _ => "",
    };
    if let Some((line, _)) = crate::tools::template(tool, picked) {
        return line;
    }
    match target.and_then(|t| t.terminal) {
        Some(term) => format!(r#"{} {{"terminal":"{}","position":"<new label>"}}"#, a.command, term.hex()),
        None => format!(r#"{} {{"terminal":"<terminal id>","position":"<new label>"}}"#, a.command),
    }
}
