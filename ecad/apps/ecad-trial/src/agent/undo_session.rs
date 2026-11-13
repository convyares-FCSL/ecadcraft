//! Undo session (brief 035, design §8.2): the person takes back an agent session's steps with
//! one click.
//!
//! # Semantics
//! - Recorded as the **person's** action: ordinary undo transactions by `user`, one per step
//!   (011's step mapping: `u`, `undomerge`, `undoissue`, a `redo` for an undo). No new op.
//! - Only the branch shown: the session's steps still on its undo stack, newest first.
//! - When every step from the top down to the session's oldest is the session's, they are taken
//!   back at once. When the person (or anyone else) recorded steps among or after them, a dialog
//!   ([`Confirm`]) names them and offers **Undo all**, **Undo the agent's steps after my last one**
//!   (when there are any) and **Cancel**.
//! - A step that fails stops the run and says where.
//!
//! Excluded: steps on other branches (said, not touched), and a single "undo to point" step
//! (it would need a new op).

use ecad_cadcraft::ProjectHandle;
use ecad_mcp::gate::{self, OwnedSteps};
use eframe::egui;
use serde_json::json;

use crate::Trial;

/// The confirmation's element names.
pub const CONFIRM: &str = "agent.undo.confirm";
pub const ALL: &str = "agent.undo.all";
pub const AFTER: &str = "agent.undo.after";
pub const CANCEL: &str = "agent.undo.cancel";
/// The confirmation's title.
pub const TITLE: &str = "Undo agent session";
/// Most of the person's steps named in the confirmation.
const MAX_NAMED: usize = 8;

/// One step to take back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub seq: u64,
    pub label: String,
    /// The command that takes it back.
    pub command: &'static str,
    /// The agent session recorded it.
    pub agent: bool,
}

/// What Undo session would do on the branch shown.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// From the top of the undo stack down to the session's oldest step still on it, newest first.
    pub steps: Vec<Step>,
    /// The session's steps on top of the person's newest one (the "after my last one" choice).
    pub after: usize,
    /// The session's steps not on this branch's undo stack and not undone (another branch).
    pub elsewhere: usize,
}

impl Plan {
    /// The person's (or anyone else's) steps among the session's.
    pub fn others(&self) -> Vec<&Step> {
        self.steps.iter().filter(|s| !s.agent).collect()
    }
}

/// The open confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub plan: Plan,
}

impl Confirm {
    /// The dialog's text (design §8.2).
    pub fn text(&self) -> String {
        let others = self.plan.others();
        let mut named: Vec<String> = others.iter().take(MAX_NAMED).map(|s| format!("#{} {}", s.seq, s.label)).collect();
        if others.len() > MAX_NAMED {
            named.push(format!("and {} more", others.len() - MAX_NAMED));
        }
        let mine = if others.len() == 1 { "1 of your own steps comes".to_string() } else { format!("{} of your own steps come", others.len()) };
        format!("{mine} after the agent's: {}. Undoing the agent's session also undoes them.", named.join(", "))
    }
}

/// The plan for `owned` on the branch shown.
pub fn plan(s: &mut cadcraft_engine::Session, p: &ProjectHandle, owned: &OwnedSteps) -> Result<Plan, String> {
    p.read(s, |p, _| {
        let stack = p.ledger.undo_stack();
        let log = p.ledger.log();
        let deepest = stack.iter().position(|(_, id)| owned.contains(id));
        let Some(deepest) = deepest else {
            let on_log: usize = log.iter().filter(|t| owned.contains(&t.id)).count();
            return Plan { steps: Vec::new(), after: 0, elsewhere: owned.len().saturating_sub(on_log) };
        };
        let tx = |seq: u64| log.binary_search_by_key(&seq, |t| t.seq).ok().and_then(|i| log.get(i));
        let steps: Vec<Step> = stack
            .get(deepest..)
            .unwrap_or_default()
            .iter()
            .rev()
            .map(|(seq, id)| {
                let t = tx(*seq);
                Step {
                    seq: *seq,
                    label: t.map(|t| t.label.clone()).unwrap_or_default(),
                    command: t.map_or("u", gate::undo_step_for),
                    agent: owned.contains(id),
                }
            })
            .collect();
        let after = steps.iter().take_while(|s| s.agent).count();
        let on_log: usize = log.iter().filter(|t| owned.contains(&t.id)).count();
        Plan { steps, after, elsewhere: owned.len().saturating_sub(on_log) }
    })
}

impl Trial {
    /// **Undo session** on the banner.
    pub(crate) fn agent_undo_session(&mut self) {
        let Some(session) = self.agent.banner.as_ref().map(|b| b.session.clone()) else { return };
        let Some(p) = self.project.clone() else { return };
        if self.cad.session.running.is_some() {
            return;
        }
        let Some(owned) = self.agent.sessions.get(&session).map(|s| s.owned.clone()) else { return };
        let plan = match plan(&mut self.cad.session, &p, &owned) {
            Ok(pl) => pl,
            Err(e) => {
                self.cad.session.echo(format!("Undo session: the project could not be read ({e})."));
                return;
            }
        };
        if plan.elsewhere > 0 {
            self.cad.session.echo(format!("{} agent steps are on another branch; switch to it to undo them.", plan.elsewhere));
        }
        if plan.steps.is_empty() {
            return;
        }
        if plan.others().is_empty() {
            let n = plan.steps.len();
            self.agent_undo_steps(&plan, n);
        } else {
            self.agent.undo_confirm = Some(Confirm { plan });
        }
    }

    /// Take back the first `n` steps of `plan` (newest first), as the person.
    pub(crate) fn agent_undo_steps(&mut self, plan: &Plan, n: usize) {
        let Some(p) = self.project.clone() else { return };
        // The person clicked it: the author is the person (checked, as at every frame).
        let _ = gate::restore_user(&mut self.cad.session, &p);
        let steps: Vec<&Step> = plan.steps.iter().take(n).collect();
        for (done, st) in steps.iter().enumerate() {
            if let Err(e) = self.cad.session.execute(st.command, &json!({})) {
                let after = steps.get(done).map_or(0, |s| s.seq.saturating_sub(1));
                self.cad.session.echo(format!("Stopped after {done} of {}: {e}. The project is as after #{after}.", steps.len()));
                self.agent_after_undo();
                return;
            }
        }
        let agent_steps = steps.iter().filter(|s| s.agent).count();
        let (lo, hi) = (steps.last().map_or(0, |s| s.seq), steps.first().map_or(0, |s| s.seq));
        let range = if lo == hi { format!("#{lo}") } else { format!("#{lo}–#{hi}") };
        let what = if agent_steps == steps.len() {
            format!("Undid {agent_steps} agent steps ({range}).")
        } else {
            format!("Undid {} steps ({range}): {agent_steps} by the agent, {} of your own.", steps.len(), steps.len() - agent_steps)
        };
        self.cad.session.echo(what);
        self.agent_after_undo();
    }

    fn agent_after_undo(&mut self) {
        if let Some(session) = self.agent.banner.as_ref().map(|b| b.session.clone()) {
            let n = self.agent_steps(&session);
            if let Some(b) = &mut self.agent.banner {
                b.steps = n;
            }
        }
    }

    /// The Undo agent session dialog, while open.
    pub(crate) fn agent_undo_window(&mut self, ctx: &egui::Context) {
        let Some(c) = self.agent.undo_confirm.clone() else { return };
        let mut choice: Option<Option<usize>> = None;
        egui::Modal::new(egui::Id::unique(CONFIRM)).show(ctx, |ui| {
            ui.set_width(480.0);
            let t = ui.label(egui::RichText::new(TITLE).strong());
            t.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, CONFIRM));
            ui.label(c.text());
            ui.horizontal(|ui| {
                let all = ui.button(format!("Undo all {}", c.plan.steps.len()));
                all.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, ALL));
                if all.clicked() {
                    choice = Some(Some(c.plan.steps.len()));
                }
                if c.plan.after > 0 {
                    let after = ui.button(format!("Undo the agent's steps after my last one ({})", c.plan.after));
                    after.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, AFTER));
                    if after.clicked() {
                        choice = Some(Some(c.plan.after));
                    }
                }
                let cancel = ui.button("Cancel");
                cancel.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, CANCEL));
                if cancel.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    choice = Some(None);
                }
            });
        });
        if let Some(n) = choice {
            self.agent.undo_confirm = None;
            if let Some(n) = n {
                self.agent_undo_steps(&c.plan, n);
            }
        }
    }

    /// Answer the open confirmation as its buttons do: `Some(n)` undoes the first `n` steps,
    /// `None` cancels (tests and the UI shell).
    pub fn agent_undo_answer(&mut self, n: Option<usize>) {
        let Some(c) = self.agent.undo_confirm.take() else { return };
        if let Some(n) = n {
            self.agent_undo_steps(&c.plan, n.min(c.plan.steps.len()));
        }
    }
}
