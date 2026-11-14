//! The **History** tab of the Project panel (brief 020a, design 020 §2–§3): the graph of the
//! main line, its versions and issued revisions, and the branches drawn from where they fork;
//! the details of a selected point with Show, Branch from here and Restore; the Steps list; and
//! the branch actions with their dialogs. Every write runs a command's JSON form, so a click
//! records exactly what the command does.
//!
//! Update map (design 020 §8.2), from the project's change count, its unsaved count and the
//! actions run here:
//! - a commit or undo/redo on the branch shown: the Now row's label only, unless it named or
//!   unnamed a version, or was a merge or the undo or redo of one (then the graph is read again:
//!   branch, version and merge rows, one step per row of the page);
//! - Save: the Now row's label; a branch action or switch: the graph is read again;
//! - the details box: computed when a row is selected, kept until another is (history is
//!   immutable), except Now's, refreshed with the Now row;
//! - every other change: nothing.
//!
//! - Merge into the parent and Update from the parent (brief 020b) open the merge dialog
//!   ([`crate::merge_dialog`]); merges are rows of the graph with a join from the source's line.
//! - The graph reads one page of rows; scrolling to its end reads the next page.
//! - F2 (or double-click on the Branch list's name) renames the branch shown; a Now row's
//!   right-click offers switch, merge, update, rename and archive.
//! - Authors (brief 043, screen f): every row carries its author, "You" for the person or the
//!   agent's name in the `agent` colour. In the Steps list, consecutive transactions by one agent
//!   form a **session group** ([`history::sessions`]) that collapses to one row; while that
//!   agent's bridge session is live the group offers Undo session (the banner's plan and
//!   confirmation, design 035 §8); after it ended its steps undo one by one as any others. The
//!   groups are made when the Steps list is read (its update map above), never per frame.

use ecad_cadcraft::history::{self, Graph, NodeKind, Show};
use ecad_file::v2::schema::{BranchId, BranchStatus};
use ecad_model::Op;
use eframe::egui::{self, Color32, RichText, Sense, Stroke, pos2, vec2};
use serde_json::{Value, json};

use crate::agent::BannerAction;
use crate::agent::last_write::LiveSession;
use crate::project_panel::{Action, ROW_H};

/// Accessible names (brief 043): a session group's row (`history.session: claude · 14 changes ·
/// 20:31–20:44`), its Undo session button, and an author chip (`history.author: You`).
pub const SESSION_GROUP: &str = "history.session";
pub const UNDO_SESSION: &str = "history.undo_session";
pub const AUTHOR: &str = "history.author";
/// The person's author chip.
pub const YOU: &str = "You";

/// Accessible label of the tab.
pub const HISTORY_TAB: &str = "History";
/// Width of one lane of the graph.
pub const LANE_W: f32 = 12.0;

/// What the History tab asks the trial to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Do {
    /// Run this command with these parameters; a refusal is shown in the tab.
    Run(&'static str, Value),
    /// Ask `restore {…, preview: true}` and open the Restore dialog with its question.
    AskRestore(Value),
    /// Show the bound file's Save As (branches need a file).
    SaveAs,
    /// Preview this page of the point shown (`RevisionPage`).
    ShowPoint(Value),
    /// Open the merge dialog for `{from, into?}` (brief 020b).
    AskMerge(Value),
    /// UI rev 3 (board 1.04): the agent card's Pause, Resume and disconnect, as the banner's.
    Agent(BannerAction),
}

/// The agent card's facts, copied from the banner when it changes (board 1.04).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCard {
    pub name: String,
    pub steps: usize,
    pub paused: bool,
}

/// Board 1.04's texts.
pub const FOOTER: &str = "Selecting a change marks it on the page. Compare opens the full view with clouds.";
pub const NEW_BRANCH: &str = "New branch";
pub const NAME_THIS: &str = "Name this one";
pub const EARLIER: &str = "Show earlier changes";
pub const OLDER: &str = "Show older versions";
pub const AGENT_DISCONNECT: &str = "Disconnect the agent";
/// The placeholders' reasons.
pub const COMPARE_LATER: &str = "Compare is not available yet";
pub const FILTER_LATER: &str = "Filtering the changes is not available yet";

/// A selected row: a node of the graph, or a step of the Steps list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sel {
    pub branch: BranchId,
    pub seq: u64,
    pub kind: NodeKind,
    pub name: Option<String>,
}

impl Sel {
    /// The point's JSON form (`{branch, version}`, `{branch, revision}` or `{branch, seq}`).
    pub fn params(&self) -> Value {
        match (self.kind, &self.name) {
            (NodeKind::Version, Some(n)) => json!({"branch": self.branch.hex(), "version": n}),
            (NodeKind::Revision, Some(n)) => json!({"branch": self.branch.hex(), "revision": n.trim_start_matches("Rev ")}),
            _ => json!({"branch": self.branch.hex(), "seq": self.seq}),
        }
    }
}

/// A dialog of the tab (design 020 §2.5, §3).
#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    NewVersion {
        name: String,
        description: String,
        error: Option<String>,
    },
    NewBranch {
        name: String,
        from: Value,
        from_text: String,
        switch: bool,
        error: Option<String>,
    },
    Rename {
        branch: String,
        name: String,
        error: Option<String>,
    },
    Archive {
        branch: String,
        name: String,
    },
    Restore {
        params: Value,
        question: String,
    },
    Switch {
        branch: String,
        name: String,
    },
    /// Unsaved changes on a file that cannot be written: Save As or Cancel (design 020 §3.3).
    SwitchReadOnly,
    /// Reopen an archived branch whose name another open branch took, under a new name.
    Reopen {
        branch: String,
        old: String,
        name: String,
        error: Option<String>,
    },
}

/// One step row of the Steps list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRow {
    pub seq: u64,
    pub label: String,
    pub author: String,
    pub time: String,
    /// The agent's name when an agent recorded it.
    pub agent: Option<String>,
}

/// The author chip's text: "You" for the person, the agent's name otherwise (owner, Q1 of brief
/// 043: no name is stored for a person).
pub fn author_chip(agent: Option<&str>) -> &str {
    agent.unwrap_or(YOU)
}

/// The tab's state.
#[derive(Debug, Default)]
pub struct HistoryPanel {
    pub show: Show,
    graph: Graph,
    /// Sections folded (board 1.04: every section folds).
    pub folded: std::collections::BTreeSet<&'static str>,
    /// The agent card while an agent is connected.
    pub agent: Option<AgentCard>,
    /// Project serial, change count, unsaved count and head the copy was taken at.
    seen: Option<(u64, u64, u64, u64)>,
    /// Read the graph again on the next sync (after a branch action).
    dirty: bool,
    pub selected: Option<Sel>,
    details: Option<(Sel, Value)>,
    pub steps_open: bool,
    steps: Vec<StepRow>,
    /// The Steps list's rows grouped by session (brief 043), made with the rows and again when the
    /// live session changes (its steps are a group of their own).
    groups: Vec<history::SessionGroup>,
    /// The Steps list's transactions without their ops, to regroup without reading the project.
    step_txs: Vec<ecad_model::Transaction>,
    /// Session groups shown expanded, by their newest step.
    pub expanded: std::collections::BTreeSet<u64>,
    /// The live agent session, kept from the banner ([`crate::Trial::history_agent_sync`]).
    pub live: Option<LiveSession>,
    /// Undo session was clicked on a group (run on the next frame).
    pub undo_session: bool,
    steps_for: Option<(Sel, u64)>,
    steps_more: u64,
    pub dialog: Option<Dialog>,
    /// A refusal or note under the header.
    pub note: Option<String>,
    /// Defaults read with the graph: next version name, next branch name, unsaved count, the
    /// branch shown's name, whether the project has a version 2 file.
    defaults: (String, String, u64, String, bool),
    /// How many times the graph was read (tests: only when the update map says so).
    pub builds: u64,
    pub rows_drawn: usize,
}

impl HistoryPanel {
    /// Read everything again on the next sync.
    pub fn invalidate(&mut self) {
        self.seen = None;
        self.dirty = true;
        self.details = None;
        self.steps_for = None;
    }

    /// The graph copy (tests).
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// Bring the copy up to date (the update map above). Reads only.
    pub fn sync(&mut self, serial: u64, p: &ecad_cadcraft::Project) {
        let key = (serial, p.change_count(), p.unsaved_transactions(), p.ledger.head());
        let Some(seen) = self.seen else {
            self.rebuild(p, key);
            return;
        };
        if seen != key || self.dirty {
            let versions_moved = self.dirty
                || seen.0 != key.0
                || (seen.3 < key.3
                    && (seen.3 + 1..=key.3).any(|s| {
                        p.ledger.transaction(s).is_some_and(|t| {
                            t.ops.iter().any(|o| matches!(o, Op::NameVersion { .. } | Op::UnnameVersion { .. })) || changes_merges(t)
                        })
                    }));
            if versions_moved {
                self.rebuild(p, key);
            } else {
                self.refresh_now(p, key);
            }
        }
        self.sync_details(p);
        self.sync_steps(p);
    }

    fn rebuild(&mut self, p: &ecad_cadcraft::Project, key: (u64, u64, u64, u64)) {
        match history::graph(p, self.show) {
            Ok(g) => {
                self.graph = g;
                self.note = None;
            }
            Err(e) => self.note = Some(format!("The history could not be read: {e}")),
        }
        self.builds += 1;
        self.dirty = false;
        self.seen = Some(key);
        self.defaults = (
            history::default_version_name(p),
            history::default_branch_name(p),
            p.unsaved_transactions(),
            history::current_name(p),
            p.file.v2().is_some(),
        );
        // A selection that no longer exists is dropped.
        if let Some(s) = &self.selected
            && !self.graph.nodes.iter().any(|n| n.branch == s.branch && n.seq == s.seq && n.kind == s.kind)
            && !matches!(s.kind, NodeKind::Fork)
        {
            self.selected = None;
        }
        if self.details.as_ref().is_some_and(|(s, _)| s.kind == NodeKind::Now) {
            self.details = None;
        }
    }

    /// Only the branch shown's Now row and the defaults change (one integer, a label).
    fn refresh_now(&mut self, p: &ecad_cadcraft::Project, key: (u64, u64, u64, u64)) {
        let cur = self.graph.current;
        let unsaved = p.unsaved_transactions();
        let head = p.ledger.head();
        let name = history::current_name(p);
        if let Some(n) = self.graph.nodes.iter_mut().find(|n| Some(n.branch) == cur && n.kind == NodeKind::Now) {
            n.seq = head;
            n.label = if unsaved > 0 { format!("Now · {name} · {unsaved} unsaved") } else { format!("Now · {name}") };
            if let Some(t) = p.ledger.transaction(head) {
                n.time_ms = t.time_ms;
                n.author = history::author_text(&t.author);
            }
        }
        if let Some(b) = self.graph.branches.iter_mut().find(|b| b.current) {
            b.head = head;
            b.unsaved = unsaved;
        }
        self.defaults.2 = unsaved;
        self.seen = Some(key);
        if self.details.as_ref().is_some_and(|(s, _)| s.kind == NodeKind::Now) {
            self.details = None;
        }
        if let Some(s) = &mut self.selected
            && s.kind == NodeKind::Now
            && Some(s.branch) == cur
        {
            s.seq = head;
        }
    }

    fn sync_details(&mut self, p: &ecad_cadcraft::Project) {
        let Some(sel) = self.selected.clone() else {
            self.details = None;
            return;
        };
        if self.details.as_ref().is_some_and(|(s, _)| *s == sel) {
            return;
        }
        let v = history::point(p, &sel.params()).and_then(|mut pt| {
            pt.kind = sel.kind;
            if let Some(n) = &sel.name {
                pt.title = if sel.kind == NodeKind::Version { format!("Version \"{n}\"") } else { n.clone() };
            }
            history::point_info(p, &pt)
        });
        self.details = Some((sel, v.unwrap_or_else(|e| json!({"error": e}))));
    }

    fn sync_steps(&mut self, p: &ecad_cadcraft::Project) {
        // UI rev 3: the Changes list always shows the branch shown's steps back from Now.
        let Some(sel) = self.anchor() else { return };
        let want = (sel.clone(), self.steps_more);
        if self.steps_for.as_ref() == Some(&want) {
            return;
        }
        let limit = history::STEPS_PAGE * (self.steps_more + 1);
        let rows = history::steps(p, sel.branch, Some(sel.seq + 1), limit).unwrap_or_default();
        let today = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64);
        self.steps = rows
            .iter()
            .map(|t| StepRow {
                seq: t.seq,
                label: t.label.clone(),
                author: history::author_text(&t.author),
                time: history::short_time(t.time_ms, today),
                agent: match &t.author {
                    ecad_model::Author::Agent(n) => Some(n.clone()),
                    ecad_model::Author::User => None,
                },
            })
            .collect();
        self.step_txs = rows.into_iter().map(|t| ecad_model::Transaction { ops: Vec::new(), ..t }).collect();
        self.regroup();
        self.steps_for = Some(want);
    }

    /// The panel's contents (UI rev 3, board 1.04), top to bottom: the pending revision, the agent
    /// while one is connected, Branches, Changes and Versions, each folding, then a short footer.
    /// `writable`: a project that can be changed; `read_only`: its file cannot be written.
    pub fn show(&mut self, ui: &mut egui::Ui, writable: bool, read_only: bool, header: &crate::project_panel::Header) -> Vec<Action> {
        let mut out = Vec::new();
        let (next_version, next_branch, unsaved, cur_name, has_file) = self.defaults.clone();
        let s = ecad_ui::SHELL;
        let r = ui.max_rect();
        // F2 renames the branch shown (design §3.2).
        let f2 = ui.input(|i| i.key_pressed(egui::Key::F2)) && self.dialog.is_none();
        if writable
            && f2
            && let Some(b) = self.graph.branches.iter().find(|b| b.current && b.fork.is_some()).cloned()
        {
            self.dialog = Some(Dialog::Rename { branch: b.id.hex(), name: b.name.clone(), error: None });
        }
        if let Some(n) = self.note.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(n).small().color(ecad_ui::color(s.text_2)));
                if !has_file && ui.small_button("Save As…").clicked() {
                    out.push(Action::History(Do::SaveAs));
                }
            });
        }
        let footer_h = 40.0;
        let body_h = (ui.available_height() - footer_h).max(ROW_H * 4.0);
        let mut drawn = 0;
        egui::ScrollArea::vertical().id_salt("ecad.history.body").max_height(body_h).auto_shrink([false, false]).show(ui, |ui| {
            // Rows are as wide as the panel (the content never widens it).
            ui.set_max_width(r.width());
            ui.spacing_mut().item_spacing.y = 0.0;
            self.revision_card(ui, header, &mut out);
            self.agent_card(ui, &cur_name, &mut out);
            drawn += self.branches(ui, writable, unsaved, read_only, has_file, &cur_name, &next_branch, &mut out);
            drawn += self.changes(ui, writable, &cur_name, &mut out);
            drawn += self.versions(ui, writable, &next_version, &mut out);
        });
        self.rows_drawn = drawn;
        // The footer.
        let foot = egui::Rect::from_min_max(pos2(r.left() - 8.0, (r.bottom() - footer_h).max(r.top())), pos2(r.right() + 8.0, r.bottom()));
        ui.painter().hline(foot.x_range(), foot.top() + 0.5, Stroke::new(1.0, ecad_ui::color(s.line)));
        crate::shell::in_rect(ui, foot.shrink2(vec2(10.0, 5.0)), "ecad.history.footer", |ui| {
            ui.add(egui::Label::new(RichText::new(FOOTER).small().color(ecad_ui::color(s.text_3))).wrap());
        });
        self.dialogs(ui, unsaved, &cur_name, &mut out);
        out
    }

    /// The pending revision: `Rev D* pending`, after which revision, the checks' verdict, Release
    /// and Compare (a placeholder until Compare comes).
    fn revision_card(&mut self, ui: &mut egui::Ui, header: &crate::project_panel::Header, out: &mut Vec<Action>) {
        let s = ecad_ui::SHELL;
        let c = ecad_ui::color;
        let last = self.graph.nodes.iter().find(|n| n.kind == NodeKind::Revision).cloned();
        let today = now_ms();
        egui::Frame::NONE.inner_margin(egui::Margin { left: 4, right: 4, top: 6, bottom: 10 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let Some((rev, checks)) = &header.release else {
                ui.label(RichText::new("No revision pending").color(c(s.text_2)));
                return;
            };
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let title = crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::TITLE);
                ui.label(RichText::new("Rev ").font(title.clone()).color(c(s.text)));
                ui.label(RichText::new(format!("{rev}*")).font(title.clone()).color(c(s.provisional)));
                ui.label(RichText::new(" pending").font(title).color(c(s.text)));
                if let Some(l) = &last {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let after = format!("after {}, {}", l.label.trim_start_matches("Rev "), history::short_time(l.time_ms, today));
                        ui.label(RichText::new(after).small().color(c(s.text_3)));
                    });
                }
            });
            let verdict = match checks {
                None => "Checks run at release".to_string(),
                Some((0, 0)) => "No problems: ready to release".to_string(),
                Some((0, w)) => format!("{w} warning{} to look at", if *w == 1 { "" } else { "s" }),
                Some((e, _)) => format!("{e} error{} a full release", if *e == 1 { " stops" } else { "s stop" }),
            };
            ui.label(RichText::new(verdict).small().color(c(s.text_2)));
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let release = format!("Release {rev}\u{2026}");
                if ui.add(ecad_ui::prim::Button::new(&release).kind(ecad_ui::prim::Kind::Primary).small()).clicked() {
                    out.push(Action::Release);
                }
                let compare = last.as_ref().map_or_else(|| "Compare".to_string(), |l| format!("Compare with {}", l.label.trim_start_matches("Rev ")));
                ui.add(ecad_ui::prim::Button::new(&compare).small().disabled(COMPARE_LATER));
            });
        });
        separator(ui);
    }

    /// The agent while one is connected (Carl's ruling: it writes on the branch shown, not a
    /// branch of its own): who, where, how many changes; Pause or Resume, and disconnect.
    fn agent_card(&mut self, ui: &mut egui::Ui, cur_name: &str, out: &mut Vec<Action>) {
        let Some(a) = self.agent.clone() else { return };
        let s = ecad_ui::SHELL;
        let c = ecad_ui::color;
        let lime = c(s.agent);
        let shown = egui::Frame::NONE.fill(lime.gamma_multiply(0.08)).inner_margin(egui::Margin::symmetric(6, 8)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                chip(ui, "AI", lime, Color32::from_rgb(20, 24, 10));
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let state = if a.paused { "is paused" } else { "is writing" };
                    ui.label(RichText::new(format!("{} {state}", a.name)).strong().color(c(s.text)));
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(RichText::new("on branch ").small().color(c(s.text_2)));
                        ui.label(RichText::new(cur_name).small().color(lime));
                        let n = a.steps;
                        ui.label(RichText::new(format!(" \u{b7} {n} change{}", if n == 1 { "" } else { "s" })).small().color(c(s.text_2)));
                    });
                });
            });
            ui.horizontal(|ui| {
                let (label, act) = if a.paused { ("Resume", BannerAction::Resume) } else { ("Pause", BannerAction::Pause) };
                if ui.add(ecad_ui::prim::Button::new(label).small()).clicked() {
                    out.push(Action::History(Do::Agent(act)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(ecad_ui::prim::IconButton::new(ecad_ui::tokens::Icon::Close, AGENT_DISCONNECT)).clicked() {
                        out.push(Action::History(Do::Agent(BannerAction::Disconnect)));
                    }
                });
            });
        });
        let r = shown.response.rect;
        ui.painter().rect_filled(egui::Rect::from_min_size(r.min, vec2(3.0, r.height())), 0.0, lime);
        separator(ui);
    }

    /// A folding section's header: chevron, title, and an action at the right (`None`: none).
    /// Returns (open, action clicked).
    fn section(&mut self, ui: &mut egui::Ui, key: &'static str, title: &str, action: Option<(&str, bool)>) -> (bool, bool) {
        let s = ecad_ui::SHELL;
        let c = ecad_ui::color;
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        let open = !self.folded.contains(key);
        let p = ui.painter_at(rect);
        let y = rect.center().y;
        let chev = egui::Rect::from_center_size(pos2(rect.left() + 8.0, y), vec2(10.0, 10.0));
        ecad_ui::icons::paint(&p, chev, if open { ecad_ui::tokens::Icon::ChevronDown } else { ecad_ui::tokens::Icon::ChevronRight }, c(s.text_3));
        let font = crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::BODY_EMPHASIS);
        p.text(pos2(rect.left() + 20.0, y), egui::Align2::LEFT_CENTER, title, font, c(s.text));
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Section {title}")));
        let mut acted = false;
        if let Some((label, enabled)) = action {
            let w = crate::shell::bar::text_width(ui, label, ecad_ui::tokens::ui_type::BODY) + 12.0;
            let r = egui::Rect::from_min_max(pos2(rect.right() - w, rect.top() + 4.0), pos2(rect.right(), rect.bottom() - 4.0));
            let b = crate::shell::bar::hit(ui, r, label, false, false, enabled);
            ui.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                label,
                crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::BODY),
                crate::shell::bar::ink(false, false, enabled),
            );
            acted = b.clicked();
        }
        if resp.clicked() && !acted && !self.folded.remove(key) {
            self.folded.insert(key);
        }
        (open, acted)
    }

    /// Branches: the branch shown first ("you are here"), the other open branches with how far
    /// ahead they are and Merge, merged and archived ones greyed. Double-click switches; a row's
    /// right-click offers switch, merge, update, rename and archive. Returns the rows drawn.
    #[allow(clippy::too_many_arguments)]
    fn branches(
        &mut self, ui: &mut egui::Ui, writable: bool, unsaved: u64, read_only: bool, has_file: bool, cur_name: &str, next_branch: &str,
        out: &mut Vec<Action>,
    ) -> usize {
        let (open, new) = self.section(ui, "branches", "Branches", writable.then_some((NEW_BRANCH, true)));
        if new {
            if has_file {
                self.dialog = Some(Dialog::NewBranch {
                    name: next_branch.to_string(),
                    from: Value::Null,
                    from_text: format!("{cur_name} · Now"),
                    switch: true,
                    error: None,
                });
            } else {
                self.note = Some("Branches live in the project file. Save it first.".into());
            }
        }
        if !open {
            separator(ui);
            return 0;
        }
        let s = ecad_ui::SHELL;
        let c = ecad_ui::color;
        let mut rows = self.graph.branches.clone();
        rows.sort_by_key(|b| (!b.current, b.status != BranchStatus::Open, b.fork.is_some()));
        let mut menu: Option<Action> = None;
        for b in &rows {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 29.0), Sense::click());
            let p = ui.painter_at(rect);
            let y = rect.center().y;
            let live = b.status == BranchStatus::Open;
            if b.current {
                p.rect_filled(rect, 0.0, c(s.accent_soft));
            } else if resp.hovered() {
                p.rect_filled(rect, 0.0, c(s.hover));
            }
            let lane = if b.fork.is_none() {
                c(s.accent)
            } else if live {
                c(s.agent)
            } else {
                c(s.text_3)
            };
            p.line_segment([pos2(rect.left() + 10.0, y - 7.0), pos2(rect.left() + 10.0, y + 7.0)], Stroke::new(2.0, lane));
            p.circle_filled(pos2(rect.left() + 10.0, y - 7.0), 2.0, lane);
            let font = crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::BODY);
            let small = crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::SMALL);
            let ink = if live { c(s.text) } else { c(s.text_3) };
            let name = p.text(pos2(rect.left() + 24.0, y), egui::Align2::LEFT_CENTER, &b.name, font.clone(), ink);
            let sub = if b.current {
                "you are here".to_string()
            } else if !live {
                format!("{} {}", if b.status == BranchStatus::Merged { "merged" } else { "archived" }, history::short_time(b.created_ms, now_ms()))
            } else {
                let ahead = b.fork.map_or(0, |(_, at)| b.head.saturating_sub(at));
                format!("{ahead} ahead")
            };
            p.text(pos2(name.right() + 8.0, y), egui::Align2::LEFT_CENTER, sub, small, c(s.text_3));
            let label = format!("Branch {}", b.name);
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, b.current, &label));
            if b.current {
                let tick = egui::Rect::from_center_size(pos2(rect.right() - 12.0, y), vec2(10.0, 10.0));
                crate::shell::glyphs::tick(&p, tick, c(s.accent_text));
            } else if live && writable {
                let r = egui::Rect::from_min_max(pos2(rect.right() - 54.0, rect.top() + 3.0), pos2(rect.right() - 2.0, rect.bottom() - 3.0));
                let merge_label = format!("Merge {}", b.name);
                let m = crate::shell::bar::hit(ui, r, &merge_label, false, false, true);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "Merge", font, c(s.text));
                if m.clicked() {
                    out.push(Action::History(Do::AskMerge(json!({"from": b.id.hex()}))));
                }
            }
            if resp.double_clicked() && !b.current && live {
                if unsaved > 0 && read_only {
                    self.dialog = Some(Dialog::SwitchReadOnly);
                } else if unsaved > 0 {
                    self.dialog = Some(Dialog::Switch { branch: b.id.hex(), name: b.name.clone() });
                } else {
                    out.push(Action::History(Do::Run("switchbranch", json!({"branch": b.id.hex()}))));
                }
            }
            resp.context_menu(|ui| {
                if let Some(a) = self.now_menu(ui, Some(b), writable, unsaved, read_only) {
                    menu = Some(a);
                }
            });
        }
        if let Some(a) = menu {
            out.push(a);
        }
        // Merged and archived branches are listed on request.
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            if ui.checkbox(&mut self.show.archived, RichText::new("Show merged and archived").small()).changed() {
                self.invalidate();
            }
        });
        separator(ui);
        rows.len()
    }

    /// Changes: the branch shown's steps, newest first, with their branch line down the left; an
    /// agent's session folds to one row (brief 043). Selecting a change marks it on the page and
    /// offers Show, Branch from here and Restore. Returns the rows drawn.
    fn changes(&mut self, ui: &mut egui::Ui, writable: bool, cur_name: &str, out: &mut Vec<Action>) -> usize {
        let (open, _) = self.section(ui, "changes", "Changes", None);
        if !open {
            separator(ui);
            return 0;
        }
        // The filter: All for now; by branch and This page are placeholders.
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.spacing_mut().item_spacing.x = 2.0;
            for (name, on) in [("All", true), (cur_name, false), ("This page", false)] {
                let w = crate::shell::bar::text_width(ui, name, ecad_ui::tokens::ui_type::SMALL) + 14.0;
                let (r, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
                let label = format!("Changes: {name}");
                let enabled = on;
                let b = crate::shell::bar::hit(ui, r, &label, on, false, enabled);
                let b = if enabled { b } else { b.on_hover_text(FILTER_LATER) };
                let _ = b;
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    name,
                    crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::SMALL),
                    crate::shell::bar::ink(on, false, enabled),
                );
            }
        });
        ui.add_space(4.0);
        let Some(sel) = self.anchor() else {
            separator(ui);
            return 0;
        };
        let groups = self.groups.clone();
        let mut i = 0;
        let mut drawn = 0;
        let top = ui.cursor().top();
        for g in &groups {
            let rows: Vec<StepRow> = self.steps.get(i..i + g.seqs.len()).map(<[StepRow]>::to_vec).unwrap_or_default();
            i += g.seqs.len();
            let key = g.seqs.first().copied().unwrap_or(0);
            if g.is_session() {
                let open = self.expanded.contains(&key);
                let live = self.group_is_live(g);
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    let (toggle, undo) = group_row(ui, g, open, live);
                    if toggle && !self.expanded.remove(&key) {
                        self.expanded.insert(key);
                    }
                    if undo {
                        self.undo_session = true;
                    }
                });
                drawn += 1;
                if !open {
                    continue;
                }
            }
            for r in rows {
                drawn += 1;
                if self.step_row(ui, &sel, &r, cur_name) {
                    let s = Sel { branch: sel.branch, seq: r.seq, kind: NodeKind::Fork, name: None };
                    self.selected = if self.selected.as_ref() == Some(&s) { None } else { Some(s) };
                    self.details = None;
                }
                if self.selected.as_ref().is_some_and(|s| s.kind == NodeKind::Fork && s.seq == r.seq) {
                    self.point_actions(ui, writable, out);
                }
            }
        }
        // The branch line down the left edge.
        let bottom = ui.cursor().top();
        if bottom > top {
            let x = ui.max_rect().left() + 10.0;
            ui.painter().line_segment([pos2(x, top), pos2(x, bottom)], Stroke::new(2.0, ecad_ui::color(ecad_ui::SHELL.accent).gamma_multiply(0.7)));
        }
        if self.steps.len() as u64 >= history::STEPS_PAGE * (self.steps_more + 1) {
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                if ui.add(egui::Label::new(RichText::new(EARLIER).color(ecad_ui::color(ecad_ui::SHELL.accent_text))).sense(Sense::click())).clicked()
                {
                    self.steps_more += 1;
                }
            });
        }
        if self.steps.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new("No changes yet.").small().color(ecad_ui::color(ecad_ui::SHELL.text_3)));
            });
        }
        separator(ui);
        drawn
    }

    /// One change: the line's dot, the author's chip, what it did, and when on which branch.
    /// Returns whether it was clicked.
    fn step_row(&self, ui: &mut egui::Ui, sel: &Sel, r: &StepRow, branch: &str) -> bool {
        let s = ecad_ui::SHELL;
        let c = ecad_ui::color;
        let picked = self.selected.as_ref().is_some_and(|x| x.branch == sel.branch && x.seq == r.seq && x.kind == NodeKind::Fork);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
        let p = ui.painter_at(rect);
        if picked {
            p.rect_filled(rect, 0.0, c(s.accent_soft));
            p.rect_filled(egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, c(s.accent));
        } else if resp.hovered() {
            p.rect_filled(rect, 0.0, c(s.hover));
        }
        let lane = if r.agent.is_some() { c(s.agent) } else { c(s.accent) };
        crate::shell::glyphs::lane_dot(&p, pos2(rect.left() + 10.0, rect.center().y), lane, c(s.panel));
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, picked, format!("Step {}: {}", r.seq, r.label)));
        // The chip: AI for an agent, the person's initials.
        let chip_r = egui::Rect::from_center_size(pos2(rect.left() + 34.0, rect.center().y), vec2(22.0, 22.0));
        let (fill, ink, text) = match &r.agent {
            Some(_) => (c(s.agent), Color32::from_rgb(20, 24, 10), "AI".to_string()),
            None => (c(s.active), c(s.text), crate::shell::top_bar::initials()),
        };
        p.rect_filled(chip_r, 5.0, fill);
        p.text(chip_r.center(), egui::Align2::CENTER_CENTER, text, crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::SMALL), ink);
        let name = author_chip(r.agent.as_deref()).to_string();
        let chip = ui.interact(chip_r, ui.scope_id().with(("ecad.history.author", r.seq)), Sense::hover());
        chip.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{AUTHOR}: {name}")));
        let x = chip_r.right() + 8.0;
        let font = crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::BODY);
        let small = crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::SMALL);
        let room = (rect.right() - x - 4.0).max(0.0);
        let label = crate::project_panel::cut_text(ui, &r.label, &font, room);
        p.text(pos2(x, rect.top() + 14.0), egui::Align2::LEFT_CENTER, label, font, c(s.text));
        let when = format!("{} \u{b7} {branch}", r.time);
        p.text(pos2(x, rect.top() + 32.0), egui::Align2::LEFT_CENTER, when, small, c(s.text_3));
        resp.clicked()
    }

    /// Versions: named versions and issued revisions, newest first, each with Compare (a
    /// placeholder); Name this one makes a version of now. Returns the rows drawn.
    fn versions(&mut self, ui: &mut egui::Ui, writable: bool, next_version: &str, out: &mut Vec<Action>) -> usize {
        let (open, name) = self.section(ui, "versions", "Versions", writable.then_some((NAME_THIS, true)));
        if name {
            self.dialog = Some(Dialog::NewVersion { name: next_version.to_string(), description: String::new(), error: None });
        }
        if !open {
            return 0;
        }
        let s = ecad_ui::SHELL;
        let c = ecad_ui::color;
        let nodes: Vec<history::Node> =
            self.graph.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Version | NodeKind::Revision)).cloned().collect();
        let today = now_ms();
        let mut drawn = 0;
        for n in &nodes {
            drawn += 1;
            let sel = Sel { branch: n.branch, seq: n.seq, kind: n.kind, name: n.name.clone() };
            let picked = self.selected.as_ref() == Some(&sel);
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
            let p = ui.painter_at(rect);
            if picked {
                p.rect_filled(rect, 0.0, c(s.accent_soft));
            } else if resp.hovered() {
                p.rect_filled(rect, 0.0, c(s.hover));
            }
            p.circle_filled(pos2(rect.left() + 10.0, rect.top() + 15.0), 3.5, c(s.accent_text));
            let font = crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::BODY);
            let small = crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::SMALL);
            let title = match n.kind {
                NodeKind::Revision => format!("{}, released", n.label),
                _ => n.label.clone(),
            };
            let cmp = egui::Rect::from_min_max(pos2(rect.right() - 66.0, rect.top() + 6.0), pos2(rect.right() - 2.0, rect.top() + 26.0));
            let room = (cmp.left() - rect.left() - 28.0).max(0.0);
            let title = crate::project_panel::cut_text(ui, &title, &font, room);
            p.text(pos2(rect.left() + 22.0, rect.top() + 15.0), egui::Align2::LEFT_CENTER, title, font.clone(), c(s.text));
            let mut when = history::short_time(n.time_ms, today);
            if n.kind == NodeKind::Revision {
                when.push_str(" \u{b7} locked");
            }
            p.text(pos2(rect.left() + 22.0, rect.top() + 33.0), egui::Align2::LEFT_CENTER, when, small, c(s.text_3));
            let label = self.version_label(n);
            let resp = resp.on_hover_text(history::full_time(n.time_ms));
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, picked, &label));
            let compare = crate::shell::bar::hit(ui, cmp, &format!("Compare {}", n.label), false, false, false).on_hover_text(COMPARE_LATER);
            let _ = compare;
            ui.painter().text(cmp.center(), egui::Align2::CENTER_CENTER, "Compare", font, crate::shell::bar::ink(false, false, false));
            if resp.double_clicked() {
                out.push(Action::History(Do::ShowPoint(sel.params())));
            }
            if resp.clicked() {
                if self.selected.as_ref() != Some(&sel) {
                    self.steps_more = 0;
                }
                self.selected = Some(sel.clone());
            }
            if picked {
                self.point_actions(ui, writable, out);
            }
        }
        // The graph is read a page at a time (design 020 §8.2): the next page on request.
        if self.graph.more {
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                let more = ui.add(egui::Label::new(RichText::new(OLDER).color(c(s.accent_text))).sense(Sense::click()));
                more.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, OLDER));
                if more.clicked() {
                    self.show.limit = self.graph.nodes.len() + history::PAGE;
                    self.invalidate();
                }
            });
        }
        if nodes.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new("Name a version to keep this point.").small().color(c(s.text_3)));
            });
        }
        drawn
    }

    /// A version's or revision's accessible label (`Version Before change on main`).
    fn version_label(&self, n: &history::Node) -> String {
        let b = self.graph.branches.iter().find(|b| b.id == n.branch).map_or("main", |b| b.name.as_str());
        match n.kind {
            NodeKind::Revision => format!("{} on {b}, {}", n.label, history::full_time(n.time_ms)),
            _ => format!("Version {} on {b}", n.label),
        }
    }

    /// The selected point's actions, under its row: Show, Branch from here, Restore, Reopen.
    fn point_actions(&mut self, ui: &mut egui::Ui, writable: bool, out: &mut Vec<Action>) {
        let Some((sel, v)) = self.details.clone() else { return };
        if let Some(e) = v.get("error").and_then(Value::as_str) {
            ui.label(RichText::new(e).small().color(ecad_ui::color(ecad_ui::SHELL.text_2)));
            return;
        }
        let title = v.get("title").and_then(Value::as_str).unwrap_or_default().to_string();
        let restore_here = v.get("restore").and_then(Value::as_bool) == Some(true);
        let branch_row = self.graph.branches.iter().find(|b| b.id == sel.branch).cloned();
        let closed = branch_row.as_ref().is_some_and(|b| b.status != BranchStatus::Open);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(28.0);
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            if ui.add(ecad_ui::prim::Button::new("Show").small()).accessible_name("ecad.history.show").clicked() {
                out.push(Action::History(Do::ShowPoint(sel.params())));
            }
            if writable
                && self.defaults.4
                && ui.add(ecad_ui::prim::Button::new("Branch from here…").small()).accessible_name("ecad.history.branchfrom").clicked()
            {
                let from_text = format!("{} · {}", branch_row.as_ref().map_or("main", |b| b.name.as_str()), title);
                self.dialog = Some(Dialog::NewBranch { name: self.defaults.1.clone(), from: sel.params(), from_text, switch: true, error: None });
            }
            if writable
                && restore_here
                && sel.kind != NodeKind::Now
                && ui.add(ecad_ui::prim::Button::new("Restore…").small()).accessible_name("ecad.history.restore").clicked()
            {
                out.push(Action::History(Do::AskRestore(sel.params())));
            }
            if closed && writable && ui.add(ecad_ui::prim::Button::new("Reopen branch").small()).accessible_name("ecad.history.reopen").clicked() {
                let name = branch_row.as_ref().map(|b| b.name.clone()).unwrap_or_default();
                match self.free_name(sel.branch, &name) {
                    // Another open branch has its name: offer the next free one (never a dead end).
                    Some(free) => self.dialog = Some(Dialog::Reopen { branch: sel.branch.hex(), old: name, name: free, error: None }),
                    None => out.push(Action::History(Do::Run("reopenbranch", json!({"branch": sel.branch.hex()})))),
                }
            }
        });
        ui.add_space(4.0);
    }

    /// The point the Changes list counts back from: the branch shown's Now.
    fn anchor(&self) -> Option<Sel> {
        let n = self.graph.nodes.iter().find(|n| n.kind == NodeKind::Now && Some(n.branch) == self.graph.current)?;
        Some(Sel { branch: n.branch, seq: n.seq, kind: n.kind, name: None })
    }

    /// `None` when no other open branch has `name`; else the next free `name (n)`.
    fn free_name(&self, branch: BranchId, name: &str) -> Option<String> {
        let taken = |n: &str| self.graph.branches.iter().any(|b| b.id != branch && b.status == BranchStatus::Open && b.name.eq_ignore_ascii_case(n));
        if !taken(name) {
            return None;
        }
        (2..).map(|n| format!("{name} ({n})")).find(|n| !taken(n))
    }

    /// The right-click menu of a branch's Now row: switch, merge into the parent, update from
    /// the parent (the branch shown), rename and archive.
    fn now_menu(&mut self, ui: &mut egui::Ui, row: Option<&history::BranchRow>, writable: bool, unsaved: u64, read_only: bool) -> Option<Action> {
        let b = row?;
        let mut out = None;
        if !b.current && b.status == BranchStatus::Open && ui.button(format!("Switch to \"{}\"", b.name)).clicked() {
            if unsaved > 0 && read_only {
                self.dialog = Some(Dialog::SwitchReadOnly);
            } else if unsaved > 0 {
                self.dialog = Some(Dialog::Switch { branch: b.id.hex(), name: b.name.clone() });
            } else {
                out = Some(Action::History(Do::Run("switchbranch", json!({"branch": b.id.hex()}))));
            }
            ui.close();
        }
        if b.fork.is_none() || !writable {
            if b.fork.is_none() {
                ui.label(RichText::new("main is the release line").small());
            }
            return out;
        }
        let parent = self.parent_name(b);
        if b.status == BranchStatus::Open {
            if ui.button(format!("Merge into {parent}…")).clicked() {
                out = Some(Action::History(Do::AskMerge(json!({"from": b.id.hex()}))));
                ui.close();
            }
            if b.current && ui.button(format!("Update from {parent}…")).clicked() {
                out = Some(Action::History(Do::AskMerge(json!({"from": parent, "into": b.id.hex()}))));
                ui.close();
            }
            if ui.button("Rename branch…").clicked() {
                self.dialog = Some(Dialog::Rename { branch: b.id.hex(), name: b.name.clone(), error: None });
                ui.close();
            }
            if ui.button("Archive branch…").clicked() {
                self.dialog = Some(Dialog::Archive { branch: b.id.hex(), name: b.name.clone() });
                ui.close();
            }
        } else if ui.button("Reopen branch").clicked() {
            out = Some(Action::History(Do::Run("reopenbranch", json!({"branch": b.id.hex()}))));
            ui.close();
        }
        out
    }

    fn parent_name(&self, b: &history::BranchRow) -> String {
        b.fork.and_then(|(p, _)| self.graph.branches.iter().find(|x| x.id == p)).map_or_else(|| "main".into(), |x| x.name.clone())
    }

    /// Whether `g` belongs to the live agent session (Undo session is offered on it).
    pub fn group_is_live(&self, g: &history::SessionGroup) -> bool {
        self.live
            .as_ref()
            .is_some_and(|l| matches!(&g.author, ecad_model::Author::Agent(n) if *n == l.agent) && g.ids.iter().any(|id| l.owned.contains(id)))
    }

    /// Group the Steps list's rows by session, keeping the live session's steps apart from other
    /// sessions of the same agent (the ledger names the agent, not the session).
    pub fn regroup(&mut self) {
        let owned = self.live.as_ref().map(|l| &l.owned);
        self.groups = history::sessions_split(&self.step_txs, |id| owned.is_some_and(|o| o.contains(id)));
    }

    /// The Steps list's rows as read, grouped (tests).
    pub fn groups(&self) -> &[history::SessionGroup] {
        &self.groups
    }

    /// The step selected in the Steps list, if one is (its change is marked on the page).
    pub fn selected_step(&self) -> Option<u64> {
        self.selected.as_ref().filter(|s| s.kind == NodeKind::Fork).map(|s| s.seq)
    }

    fn dialogs(&mut self, ui: &mut egui::Ui, unsaved: u64, cur_name: &str, out: &mut Vec<Action>) {
        let Some(d) = self.dialog.clone() else { return };
        let ctx = ui.ctx().clone();
        let mut close = false;
        let mut next = d.clone();
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let (title, id) = match &d {
            Dialog::NewVersion { .. } => ("NEW VERSION", "ecad.newversion"),
            Dialog::NewBranch { .. } => ("NEW BRANCH", "ecad.newbranch"),
            Dialog::Rename { .. } => ("RENAME BRANCH", "ecad.renamebranch"),
            Dialog::Archive { .. } => ("ARCHIVE BRANCH", "ecad.archivebranch"),
            Dialog::Reopen { .. } => ("REOPEN BRANCH", "ecad.reopenbranch"),
            Dialog::Restore { .. } => ("RESTORE", "ecad.restore"),
            Dialog::Switch { .. } | Dialog::SwitchReadOnly => ("SWITCH BRANCH", "ecad.switchbranch"),
        };
        egui::Window::new(title).id(egui::Id::unique(id)).collapsible(false).resizable(false).default_width(380.0).show(&ctx, |ui| {
            let red = Color32::from_rgb(0xe0, 0x60, 0x60);
            let ok_cancel = |ui: &mut egui::Ui, ok: &str| -> (bool, bool) {
                let mut r = (false, false);
                ui.horizontal(|ui| {
                    r.0 = ui.button(ok).accessible_name(format!("{id}.ok")).clicked();
                    r.1 = ui.button("Cancel").accessible_name(format!("{id}.cancel")).clicked();
                });
                r
            };
            match &mut next {
                Dialog::NewVersion { name, description, error } => {
                    egui::Grid::new("ecad.newversion.grid").num_columns(2).show(ui, |ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(name).accessible_name("ecad.newversion.name");
                        ui.end_row();
                        ui.label("Description");
                        ui.text_edit_singleline(description).accessible_name("ecad.newversion.description");
                        ui.end_row();
                    });
                    ui.label(RichText::new(format!("Kept on {cur_name} as the project is now.")).small());
                    if let Some(e) = error {
                        ui.label(RichText::new(e.as_str()).small().color(red));
                    }
                    let (ok, cancel) = ok_cancel(ui, "Create");
                    if ok {
                        let mut v = json!({"name": name.trim()});
                        if !description.trim().is_empty() {
                            v["description"] = json!(description.trim());
                        }
                        out.push(Action::History(Do::Run("version", v)));
                    }
                    close = cancel;
                }
                Dialog::NewBranch { name, from, from_text, switch, error } => {
                    egui::Grid::new("ecad.newbranch.grid").num_columns(2).show(ui, |ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(name).accessible_name("ecad.newbranch.name");
                        ui.end_row();
                        ui.label("From");
                        ui.label(from_text.as_str());
                        ui.end_row();
                    });
                    ui.checkbox(switch, "Switch to the new branch").accessible_name("ecad.newbranch.switch");
                    if unsaved > 0 {
                        ui.label(
                            RichText::new(format!(
                                "{cur_name} has {unsaved} unsaved change{}; they are saved first.",
                                if unsaved == 1 { "" } else { "s" }
                            ))
                            .small(),
                        );
                    }
                    if let Some(e) = error {
                        ui.label(RichText::new(e.as_str()).small().color(red));
                    }
                    let (ok, cancel) = ok_cancel(ui, "Create");
                    if ok {
                        let mut v = json!({"name": name.trim(), "switch": *switch});
                        if !from.is_null() {
                            v["from"] = from.clone();
                        }
                        out.push(Action::History(Do::Run("branch", v)));
                    }
                    close = cancel;
                }
                Dialog::Reopen { branch, old, name, error } => {
                    ui.label(format!("Another open branch is called \"{old}\". Reopen this one as:"));
                    ui.text_edit_singleline(name).accessible_name("ecad.reopenbranch.name");
                    if let Some(e) = error {
                        ui.label(RichText::new(e.as_str()).small().color(red));
                    }
                    let (ok, cancel) = ok_cancel(ui, "Reopen");
                    if ok {
                        out.push(Action::History(Do::Run("reopenbranch", json!({"branch": branch, "name": name.trim()}))));
                    }
                    close = cancel;
                }
                Dialog::Rename { branch, name, error } => {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(name).accessible_name("ecad.renamebranch.name");
                    });
                    if let Some(e) = error {
                        ui.label(RichText::new(e.as_str()).small().color(red));
                    }
                    let (ok, cancel) = ok_cancel(ui, "Rename");
                    if ok {
                        out.push(Action::History(Do::Run("renamebranch", json!({"branch": branch, "name": name.trim()}))));
                    }
                    close = cancel;
                }
                Dialog::Archive { branch, name } => {
                    let mut q = format!(
                        "Archive \"{name}\"? It is hidden from the list; nothing is deleted, and you can reopen it from \"Show merged and archived\"."
                    );
                    if self.graph.branches.iter().any(|b| b.current && b.id.hex() == *branch) {
                        q.push_str(" The project first shows its parent branch (saving first).");
                    }
                    ui.label(q);
                    let (ok, cancel) = ok_cancel(ui, "Archive");
                    if ok {
                        out.push(Action::History(Do::Run("archivebranch", json!({"branch": branch}))));
                    }
                    close = cancel;
                }
                Dialog::Restore { params, question } => {
                    ui.label(question.as_str());
                    let (ok, cancel) = ok_cancel(ui, "Restore");
                    if ok {
                        out.push(Action::History(Do::Run("restore", params.clone())));
                    }
                    close = cancel;
                }
                Dialog::SwitchReadOnly => {
                    ui.label(format!("{cur_name} has {unsaved} unsaved change{} and the file is read-only.", if unsaved == 1 { "" } else { "s" }));
                    let (ok, cancel) = ok_cancel(ui, "Save As…");
                    if ok {
                        out.push(Action::History(Do::SaveAs));
                    }
                    close = ok || cancel;
                }
                Dialog::Switch { branch, name } => {
                    ui.label(format!(
                        "{cur_name} has {unsaved} unsaved change{}. They are saved, then the project shows \"{name}\".",
                        if unsaved == 1 { "" } else { "s" }
                    ));
                    let (ok, cancel) = ok_cancel(ui, "Save and switch");
                    if ok {
                        out.push(Action::History(Do::Run("switchbranch", json!({"branch": branch, "save": true}))));
                    }
                    close = cancel;
                }
            }
        });
        if close || esc {
            self.dialog = None;
        } else if self.dialog.is_some() {
            self.dialog = Some(next);
        }
    }

    /// A command run from the tab succeeded: close its dialog, read the graph again.
    pub fn done(&mut self) {
        self.dialog = None;
        self.note = None;
        self.invalidate();
    }

    /// A command run from the tab was refused: under the dialog's fields, or under the header.
    pub fn refused(&mut self, e: String) {
        match &mut self.dialog {
            Some(
                Dialog::NewVersion { error, .. } | Dialog::NewBranch { error, .. } | Dialog::Rename { error, .. } | Dialog::Reopen { error, .. },
            ) => *error = Some(e),
            _ => {
                self.dialog = None;
                self.note = Some(e);
            }
        }
    }
}

/// Whether `t` adds or removes a merge row (brief 020b): a merge, or the undo or redo of one (their
/// labels are the merge's own).
fn changes_merges(t: &ecad_model::Transaction) -> bool {
    match t.kind {
        ecad_model::TxKind::Merge { .. } => true,
        ecad_model::TxKind::Undo { .. } | ecad_model::TxKind::Redo { .. } => t.label.starts_with("Merge \"") || t.label.starts_with("Update from "),
        ecad_model::TxKind::Edit => false,
    }
}

/// An author chip: "You", or the agent's name in the `agent` tone. Its accessible name is
/// `history.author: <name>`.
fn author_pill(ui: &mut egui::Ui, agent: Option<&str>) {
    let name = author_chip(agent);
    let tone = if agent.is_some() { ecad_ui::prim::Tone::Agent } else { ecad_ui::prim::Tone::Neutral };
    let r = ecad_ui::prim::pill(ui, name, tone);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{AUTHOR}: {name}")));
}

/// A session group's row: chevron, the agent's chip, `14 changes · 20:31–20:44`, and Undo session
/// while `live`. Returns (expand or collapse clicked, Undo session clicked).
fn group_row(ui: &mut egui::Ui, g: &history::SessionGroup, open: bool, live: bool) -> (bool, bool) {
    let shell = ecad_ui::SHELL;
    let text = g.text();
    let agent = match &g.author {
        ecad_model::Author::Agent(n) => n.clone(),
        ecad_model::Author::User => String::new(),
    };
    let rest = text.strip_prefix(agent.as_str()).map_or(text.as_str(), |r| r.trim_start_matches(" \u{b7} ")).to_string();
    let mut undo = false;
    let row = ui.horizontal(|ui| {
        let chevron = if open { ecad_ui::Icon::ChevronDown } else { ecad_ui::Icon::ChevronRight };
        let (icon, _) = ui.allocate_exact_size(vec2(ecad_ui::tokens::size::ICON, ecad_ui::tokens::size::ICON), Sense::hover());
        ecad_ui::icons::paint(ui.painter(), icon, chevron, ecad_ui::color(shell.text_2));
        author_pill(ui, Some(&agent));
        ui.label(RichText::new(rest).small().color(ecad_ui::color(shell.text_2)));
        let end = ui.min_rect().right();
        if live {
            let b = ui.add(ecad_ui::prim::Button::new("Undo session").kind(ecad_ui::prim::Kind::Danger).small());
            b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, UNDO_SESSION));
            undo = b.clicked();
        }
        end
    });
    // The row up to the button expands or collapses the group.
    let r = row.response.rect;
    let hit = egui::Rect::from_min_max(r.min, egui::pos2(row.inner.min(r.right()), r.bottom()));
    let click = ui.interact(hit, egui::Id::unique(("ecad.history.session", g.seqs.first().copied().unwrap_or(0))), Sense::click());
    click.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{SESSION_GROUP}: {text}")));
    (click.clicked() && !undo, undo)
}

impl crate::Trial {
    /// History's side of the agent (brief 043), once per frame: run an Undo session clicked on a
    /// group (the banner's own plan and confirmation), and keep the live session for the groups
    /// (copied only when the banner's session, steps or last change moved).
    pub(crate) fn history_agent_sync(&mut self) {
        if std::mem::take(&mut self.panel.history.undo_session) {
            self.agent_action(crate::agent::BannerAction::UndoSession);
        }
        let card = self
            .agent
            .banner
            .as_ref()
            .filter(|b| !matches!(b.state, crate::agent::BannerState::Finished | crate::agent::BannerState::Error))
            .map(|b| AgentCard { name: b.agent.clone(), steps: b.steps, paused: b.state == crate::agent::BannerState::Paused });
        if self.panel.history.agent != card {
            self.panel.history.agent = card;
        }
        let kept = self.panel.history.live.take();
        let was = kept.as_ref().map(|l| l.key.clone());
        self.panel.history.live = crate::agent::last_write::live_session(self, kept);
        // The live session's steps split the groups: regroup when it changed.
        if self.panel.history.live.as_ref().map(|l| l.key.clone()) != was {
            self.panel.history.regroup();
        }
    }
}

/// A thin line between sections.
fn separator(ui: &mut egui::Ui) {
    let r = ui.max_rect();
    let y = ui.cursor().top() + 0.5;
    ui.painter().hline(r.x_range(), y, Stroke::new(1.0, ecad_ui::color(ecad_ui::SHELL.line)));
    ui.add_space(1.0);
}

/// A small square chip with `text` (the agent card's AI).
fn chip(ui: &mut egui::Ui, text: &str, fill: Color32, ink: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
    ui.painter().rect_filled(r, 5.0, fill);
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, text, crate::shell::bar::font(ui, ecad_ui::tokens::ui_type::SMALL), ink);
}

/// Now, in milliseconds since 1970 (for the short times).
fn now_ms() -> Option<u64> {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}
