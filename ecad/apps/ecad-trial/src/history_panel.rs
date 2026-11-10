//! The **History** tab of the Project panel (brief 020a, design 020 §2–§3): the graph of the
//! main line, its versions and issued revisions, and the branches drawn from where they fork;
//! the details of a selected point with Show, Branch from here and Restore; the Steps list; and
//! the branch actions with their dialogs. Every write runs a command's JSON form, so a click
//! records exactly what the command does.
//!
//! Update map (design 020 §8.2), from the project's change count, its unsaved count and the
//! actions run here:
//! - a commit or undo/redo on the branch shown: the Now row's label only, unless it named or
//!   unnamed a version (then the graph is read again: branch and version rows, one step each);
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

use cadcraft_ui_egui::theme::Tokens;
use ecad_cadcraft::history::{self, Graph, Layout, NodeKind, Show};
use ecad_file::v2::schema::{BranchId, BranchStatus};
use ecad_model::Op;
use eframe::egui::{self, Color32, RichText, Sense, Stroke, pos2, vec2};
use serde_json::{Value, json};

use crate::project_panel::{Action, ROW_H};

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
}

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
}

/// The tab's state.
#[derive(Debug, Default)]
pub struct HistoryPanel {
    pub show: Show,
    graph: Graph,
    layout: Layout,
    /// Project serial, change count, unsaved count and head the copy was taken at.
    seen: Option<(u64, u64, u64, u64)>,
    /// Read the graph again on the next sync (after a branch action).
    dirty: bool,
    pub selected: Option<Sel>,
    details: Option<(Sel, Value)>,
    pub steps_open: bool,
    steps: Vec<StepRow>,
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
                        p.ledger.transaction(s).is_some_and(|t| t.ops.iter().any(|o| matches!(o, Op::NameVersion { .. } | Op::UnnameVersion { .. })))
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
                self.layout = history::layout(&g);
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
        if !self.steps_open {
            return;
        }
        let Some(sel) = self.selected.clone() else { return };
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
            })
            .collect();
        self.steps_for = Some(want);
    }

    /// The tab's contents; `writable`: a project that can be changed; `file`: it has a version 2
    /// file.
    pub fn show(&mut self, ui: &mut egui::Ui, writable: bool, read_only: bool) -> Vec<Action> {
        let mut out = Vec::new();
        let t = Tokens::get();
        let (next_version, next_branch, unsaved, cur_name, has_file) = self.defaults.clone();
        // The header: Branch [▾], New version, New branch, ⋯.
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(RichText::new("Branch").small());
            let combo = egui::ComboBox::from_id_salt("ecad.history.branch").selected_text(RichText::new(&cur_name).small());
            let shown = combo.show_ui(ui, |ui| {
                for b in self.graph.branches.iter().filter(|b| b.status == BranchStatus::Open) {
                    if ui.selectable_label(b.current, &b.name).clicked() && !b.current {
                        if unsaved > 0 && read_only {
                            self.dialog = Some(Dialog::SwitchReadOnly);
                        } else if unsaved > 0 {
                            self.dialog = Some(Dialog::Switch { branch: b.id.hex(), name: b.name.clone() });
                        } else {
                            out.push(Action::History(Do::Run("switchbranch", json!({"branch": b.id.hex(), "save": false}))));
                        }
                    }
                }
                ui.separator();
                if ui.checkbox(&mut self.show.archived, "Show merged and archived").changed() {
                    self.invalidate();
                }
            });
            // Double-click on the Branch list's name, or F2, renames the branch shown (design §3.2).
            let cur_row = self.graph.branches.iter().find(|b| b.current).cloned();
            let f2 = ui.input(|i| i.key_pressed(egui::Key::F2)) && self.dialog.is_none();
            if writable
                && (shown.response.double_clicked() || f2)
                && let Some(b) = cur_row.filter(|b| b.fork.is_some())
            {
                self.dialog = Some(Dialog::Rename { branch: b.id.hex(), name: b.name.clone(), error: None });
            }
            if writable {
                if ui
                    .small_button("New version")
                    .on_hover_text("Keep the project as it is now under a name")
                    .accessible_name("ecad.history.newversion")
                    .clicked()
                {
                    self.dialog = Some(Dialog::NewVersion { name: next_version.clone(), description: String::new(), error: None });
                }
                if ui
                    .small_button("New branch")
                    .on_hover_text("Try something on a branch from Now")
                    .accessible_name("ecad.history.newbranch")
                    .clicked()
                {
                    if has_file {
                        self.dialog = Some(Dialog::NewBranch {
                            name: next_branch.clone(),
                            from: Value::Null,
                            from_text: format!("{cur_name} · Now"),
                            switch: true,
                            error: None,
                        });
                    } else {
                        self.note = Some("Branches live in the project file. Save it first.".into());
                    }
                }
            }
            ui.menu_button(RichText::new("\u{22ef}").small(), |ui| {
                if ui.checkbox(&mut self.show.autos, "Show automatic snapshots").changed() {
                    self.invalidate();
                }
                if ui.checkbox(&mut self.show.archived, "Show merged and archived branches").changed() {
                    self.invalidate();
                }
                let on_main = self.graph.branches.iter().find(|b| b.current).is_none_or(|b| b.fork.is_none());
                if writable && !on_main {
                    ui.separator();
                    let cur = self.graph.branches.iter().find(|b| b.current).cloned();
                    if let Some(b) = cur {
                        if ui.button("Rename branch…").clicked() {
                            self.dialog = Some(Dialog::Rename { branch: b.id.hex(), name: b.name.clone(), error: None });
                            ui.close();
                        }
                        if ui.button("Archive branch…").clicked() {
                            self.dialog = Some(Dialog::Archive { branch: b.id.hex(), name: b.name.clone() });
                            ui.close();
                        }
                        let parent = self.parent_name(&b);
                        if ui.button(format!("Merge into {parent}…")).clicked() {
                            out.push(Action::History(Do::AskMerge(json!({"from": b.id.hex()}))));
                            ui.close();
                        }
                        if ui.button(format!("Update from {parent}…")).clicked() {
                            out.push(Action::History(Do::AskMerge(json!({"from": parent, "into": b.id.hex()}))));
                            ui.close();
                        }
                    }
                }
            })
            .response
            .accessible_name("ecad.history.more");
        });
        if let Some(n) = self.note.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(n).small().color(t.text_dim));
                if !has_file && ui.small_button("Save As…").clicked() {
                    out.push(Action::History(Do::SaveAs));
                }
            });
        }
        // The graph, newest first, virtualised.
        let n = self.graph.nodes.len();
        let details_h = if self.selected.is_some() { 120.0 + if self.steps_open { 160.0 } else { 0.0 } } else { 24.0 };
        let list_h = (ui.available_height() - details_h).max(ROW_H * 4.0);
        let mut drawn = 0;
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut clicked: Option<(usize, bool)> = None;
        let mut at_end = false;
        let mut menu: Option<Action> = None;
        egui::ScrollArea::vertical().id_salt("ecad.history.graph").max_height(list_h).auto_shrink([false, true]).show_rows(
            ui,
            ROW_H,
            n,
            |ui, range| {
                at_end = range.end >= n;
                for i in range {
                    drawn += 1;
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                    self.draw_row(ui, rect, i);
                    let label = self.row_label(i);
                    let resp = resp.on_hover_text(self.hover(i));
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
                    if resp.double_clicked() {
                        clicked = Some((i, true));
                    } else if resp.clicked() {
                        clicked = Some((i, false));
                    }
                    // A branch's Now row: its actions (design §4.1).
                    if let Some(node) = self.graph.nodes.get(i).cloned()
                        && node.kind == NodeKind::Now
                    {
                        let row = self.graph.branches.iter().find(|b| b.id == node.branch).cloned();
                        resp.context_menu(|ui| {
                            if let Some(a) = self.now_menu(ui, row.as_ref(), writable, unsaved, read_only) {
                                menu = Some(a);
                            }
                        });
                    }
                }
            },
        );
        if let Some(a) = menu {
            out.push(a);
        }
        // The next page of rows, read when the list is scrolled to its end (design §8.2).
        if at_end && self.graph.more {
            self.show.limit = self.graph.nodes.len() + history::PAGE;
            self.invalidate();
        }
        self.rows_drawn = drawn;
        if n <= 2 && self.graph.nodes.iter().all(|x| matches!(x.kind, NodeKind::Now | NodeKind::Root)) {
            ui.label(RichText::new("Create a version to keep this point, or a branch to try something.").small().color(t.text_dim));
        }
        if let Some((i, double)) = clicked
            && let Some(node) = self.graph.nodes.get(i).cloned()
        {
            let sel = Sel { branch: node.branch, seq: node.seq, kind: node.kind, name: node.name.clone() };
            if double {
                if node.kind == NodeKind::Now && Some(node.branch) != self.graph.current {
                    let name = self.graph.branches.iter().find(|b| b.id == node.branch).map(|b| b.name.clone()).unwrap_or_default();
                    if unsaved > 0 && read_only {
                        self.dialog = Some(Dialog::SwitchReadOnly);
                    } else if unsaved > 0 {
                        self.dialog = Some(Dialog::Switch { branch: node.branch.hex(), name });
                    } else {
                        out.push(Action::History(Do::Run("switchbranch", json!({"branch": node.branch.hex()}))));
                    }
                } else {
                    out.push(Action::History(Do::ShowPoint(sel.params())));
                }
            }
            if self.selected.as_ref() != Some(&sel) {
                self.steps_more = 0;
            }
            self.selected = Some(sel);
        }
        ui.separator();
        self.details_box(ui, writable, &mut out);
        self.dialogs(ui, unsaved, &cur_name, &mut out);
        out
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

    fn row_label(&self, i: usize) -> String {
        let Some(n) = self.graph.nodes.get(i) else { return String::new() };
        let b = self.graph.branches.iter().find(|b| b.id == n.branch).map_or("main", |b| b.name.as_str());
        match n.kind {
            NodeKind::Now => format!("Now on {b}{}", n.label.split(" · ").nth(2).map(|u| format!(", {u}")).unwrap_or_default()),
            NodeKind::Revision => format!("{} on {b}, {}", n.label, history::full_time(n.time_ms)),
            NodeKind::Version => format!("Version {} on {b}", n.label),
            _ => format!("{} on {b}", n.label),
        }
    }

    fn hover(&self, i: usize) -> String {
        let Some(n) = self.graph.nodes.get(i) else { return String::new() };
        let mut s = history::full_time(n.time_ms);
        if !n.author.is_empty() {
            s.push_str(&format!(" · {}", n.author));
        }
        if self.layout.spans.iter().any(|sp| sp.branch == n.branch && sp.overflow) {
            let b = self.graph.branches.iter().find(|b| b.id == n.branch).map_or("", |b| b.name.as_str());
            s.push_str(&format!("\nBranch \"{b}\": more branches than lanes; pick it in the Branch list to follow it"));
        }
        s
    }

    /// One row: lane lines, the node's glyph, its label and its date.
    fn draw_row(&self, ui: &egui::Ui, rect: egui::Rect, i: usize) {
        let t = Tokens::get();
        let Some(n) = self.graph.nodes.get(i) else { return };
        let painter = ui.painter_at(rect);
        let selected = self.selected.as_ref().is_some_and(|s| s.branch == n.branch && s.seq == n.seq && s.kind == n.kind);
        if selected {
            painter.rect_filled(rect, 0.0, t.select_bg);
        }
        let lane_x = |l: usize| rect.left() + 6.0 + l as f32 * LANE_W;
        let cy = rect.center().y;
        let dim = Stroke::new(1.0, t.text_dim);
        let cur = Stroke::new(2.0, t.accent);
        for s in &self.layout.spans {
            if i < s.top || i > s.bottom {
                continue;
            }
            let x = lane_x(s.lane);
            // The segment between rows `upper` and `upper + 1` is on the current line when its upper
            // end is at or below where the current line starts on this lane.
            let stroke = |upper: usize| if s.current_from.is_some_and(|f| upper >= f) { cur } else { dim };
            let bends = i == s.bottom && s.into.is_some();
            // Up from this row's centre (unless it is the top, or the fork row, where the line
            // bends into the parent's lane instead).
            if i > s.top && !bends {
                painter.line_segment([pos2(x, rect.top()), pos2(x, cy)], stroke(i - 1));
            }
            // Down to the next row (unless this is the bottom).
            if i < s.bottom {
                painter.line_segment([pos2(x, cy), pos2(x, rect.bottom())], stroke(i));
            }
            if bends && let Some(into) = s.into {
                painter.line_segment([pos2(x, rect.top()), pos2(lane_x(into), cy)], stroke(i.saturating_sub(1)));
            }
        }
        // Merge joins (brief 020b): from the source's row up its lane, then into the merge row.
        for j in &self.layout.joins {
            if i < j.row || i > j.from_row {
                continue;
            }
            let x = lane_x(j.from_lane);
            if i == j.row {
                painter.line_segment([pos2(x, rect.bottom()), pos2(lane_x(j.lane), cy)], dim);
            } else if i == j.from_row {
                painter.line_segment([pos2(x, rect.top()), pos2(x, cy)], dim);
            } else {
                painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], dim);
            }
        }
        let lane = self.layout.lanes.get(i).copied().unwrap_or(0);
        let c = pos2(lane_x(lane), cy);
        let is_current = Some(n.branch) == self.graph.current;
        match n.kind {
            NodeKind::Now => {
                if is_current {
                    painter.circle_stroke(c, 4.5, Stroke::new(1.5, t.accent));
                    painter.circle_filled(c, 2.0, t.accent);
                } else {
                    painter.circle_filled(c, 4.0, t.text);
                }
            }
            NodeKind::Revision => {
                let r = 4.5;
                painter.add(egui::Shape::convex_polygon(
                    vec![pos2(c.x, c.y - r), pos2(c.x + r, c.y), pos2(c.x, c.y + r), pos2(c.x - r, c.y)],
                    t.accent,
                    Stroke::NONE,
                ));
            }
            NodeKind::Version => {
                painter.circle_filled(c, 4.0, t.panel);
                painter.circle_stroke(c, 4.0, Stroke::new(1.0, t.text));
            }
            NodeKind::Merge => {
                painter.circle_filled(c, 4.0, t.text);
            }
            NodeKind::Fork => {
                painter.circle_filled(c, 2.0, t.text_dim);
            }
            NodeKind::Auto | NodeKind::Root => {
                painter.circle_filled(c, 1.5, t.text_faint);
            }
        }
        let lanes_used = self.layout.spans.iter().map(|s| s.lane + 1).max().unwrap_or(1).min(history::MAX_LANES);
        let text_x = rect.left() + 6.0 + lanes_used as f32 * LANE_W + 4.0;
        let font = egui::FontId::monospace(11.0);
        let color = match n.kind {
            NodeKind::Auto => t.text_faint,
            NodeKind::Fork | NodeKind::Root => t.text_dim,
            _ => t.text,
        };
        let date =
            history::short_time(n.time_ms, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64));
        let date_w = painter.layout_no_wrap(date.clone(), font.clone(), color).size().x;
        let room = (rect.right() - 4.0 - date_w - 8.0 - text_x).max(10.0);
        let label = fit(&painter, &n.label, &font, room);
        let galley = if n.kind == NodeKind::Revision {
            painter.layout_no_wrap(label, egui::FontId::monospace(11.0), t.accent)
        } else {
            painter.layout_no_wrap(label, font.clone(), color)
        };
        painter.galley(pos2(text_x, cy - galley.size().y / 2.0), galley, color);
        painter.text(pos2(rect.right() - 4.0, cy), egui::Align2::RIGHT_CENTER, date, font, t.text_dim);
        if self.layout.spans.iter().any(|s| s.branch == n.branch && s.overflow) {
            painter.text(pos2(lane_x(lane) + 6.0, cy), egui::Align2::LEFT_CENTER, "\u{bb}", egui::FontId::monospace(9.0), t.text_dim);
        }
    }

    fn details_box(&mut self, ui: &mut egui::Ui, writable: bool, out: &mut Vec<Action>) {
        let t = Tokens::get();
        let Some((sel, v)) = self.details.clone() else {
            ui.label(RichText::new("Select a row to see it.").small().color(t.text_dim));
            return;
        };
        let frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(2, 2));
        frame.show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            if let Some(e) = v.get("error").and_then(Value::as_str) {
                ui.label(RichText::new(e).small().color(t.text_dim));
                return;
            }
            let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            ui.label(RichText::new(format!("── {} ", s("title"))).small().strong())
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, "ecad.history.details"));
            ui.label(RichText::new(s("line1")).small());
            if !s("line2").is_empty() {
                ui.label(RichText::new(s("line2")).small().color(t.text_dim));
            }
            if !s("description").is_empty() {
                ui.label(RichText::new(format!("\"{}\"", s("description"))).small());
            }
            ui.label(RichText::new(s("summary")).small().color(t.text_dim));
            let restore_here = v.get("restore").and_then(Value::as_bool) == Some(true);
            let branch_row = self.graph.branches.iter().find(|b| b.id == sel.branch).cloned();
            let closed = branch_row.as_ref().is_some_and(|b| b.status != BranchStatus::Open);
            if !restore_here && !closed && writable && sel.kind != NodeKind::Now {
                let b = branch_row.as_ref().map_or("this branch", |b| b.name.as_str());
                ui.label(RichText::new(format!("On \"{b}\": branch from here, or merge that branch.")).small().color(t.text_dim));
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui.small_button("Show").accessible_name("ecad.history.show").clicked() {
                    out.push(Action::History(Do::ShowPoint(sel.params())));
                }
                if writable && self.defaults.4 && ui.small_button("Branch from here…").accessible_name("ecad.history.branchfrom").clicked() {
                    let from_text = format!("{} · {}", branch_row.as_ref().map_or("main", |b| b.name.as_str()), s("title"));
                    self.dialog = Some(Dialog::NewBranch { name: self.defaults.1.clone(), from: sel.params(), from_text, switch: true, error: None });
                }
                if writable
                    && restore_here
                    && sel.kind != NodeKind::Now
                    && ui.small_button("Restore…").accessible_name("ecad.history.restore").clicked()
                {
                    out.push(Action::History(Do::AskRestore(sel.params())));
                }
                if closed && writable && ui.small_button("Reopen branch").accessible_name("ecad.history.reopen").clicked() {
                    let name = branch_row.as_ref().map(|b| b.name.clone()).unwrap_or_default();
                    match self.free_name(sel.branch, &name) {
                        // Another open branch has its name: offer the next free one (never a dead end).
                        Some(free) => self.dialog = Some(Dialog::Reopen { branch: sel.branch.hex(), old: name, name: free, error: None }),
                        None => out.push(Action::History(Do::Run("reopenbranch", json!({"branch": sel.branch.hex()})))),
                    }
                }
                let steps = if self.steps_open { "Steps \u{25b4}" } else { "Steps \u{25be}" };
                if ui.small_button(steps).accessible_name("ecad.history.steps").clicked() {
                    self.steps_open = !self.steps_open;
                    self.steps_for = None;
                }
            });
            if self.steps_open {
                egui::ScrollArea::vertical().id_salt("ecad.history.stepslist").max_height(150.0).show(ui, |ui| {
                    for r in self.steps.clone() {
                        let text = format!("#{:<5} {}  {}  {}", r.seq, r.label, r.author, r.time);
                        let picked = self.selected.as_ref().is_some_and(|s| s.branch == sel.branch && s.seq == r.seq && s.kind == NodeKind::Fork);
                        let resp = ui.selectable_label(picked, RichText::new(text).small().monospace());
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Step {}: {}", r.seq, r.label)));
                        if resp.clicked() {
                            self.selected = Some(Sel { branch: sel.branch, seq: r.seq, kind: NodeKind::Fork, name: None });
                            self.details = None;
                        }
                    }
                    if self.steps.len() as u64 >= history::STEPS_PAGE * (self.steps_more + 1) && ui.small_button("Older steps…").clicked() {
                        self.steps_more += 1;
                    }
                });
            }
        });
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

/// `text` cut with an ellipsis to fit `width` points.
fn fit(p: &egui::Painter, text: &str, font: &egui::FontId, width: f32) -> String {
    let w = |s: &str| p.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE).size().x;
    if w(text) <= width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 0 {
        n -= 1;
        let cut: String = chars.iter().take(n).collect::<String>() + "\u{2026}";
        if w(&cut) <= width {
            return cut;
        }
    }
    String::new()
}
