//! The shell (brief 038, U2; design `ui-architecture.md` §5; reframed by UI rev 3,
//! `docs/discussions/ui-2026-11/rev3-build-spec.md`): ECADCraft's window around CADCraft's
//! drawing. One bar at each edge; the drawing comes first.
//!
//! - top bar: the project pill and menu, the four workspaces (Ctrl+1–4), search (Ctrl+K), the
//!   agent icon, the light/dark switch, help and the account ([`top_bar`]);
//! - the agent banner while an agent is connected (035's banner, styled by 043);
//! - tool strip in Draw, at the left edge: the Book, Devices and History panel buttons, nine tool
//!   groups, and the Snap, Grid and Ortho toggles ([`tool_strip`]);
//! - the panel slot beside it while a panel is open: Book, Devices or History, navigation only
//!   ([`left`]);
//! - main view: the page (CADCraft's drawing through upstream seam US1, `CadApp::ui_embedded`),
//!   or Lists, Library and Publish hosting today's Reports, symbol search and Release / Print Book;
//!   it splits for a companion view ([`layout`]);
//! - the inspector at the right, a 56 px rail of its Device, Layout and Checks tabs when
//!   collapsed ([`right`]), with the Text panel over it (Ctrl+/; [`text_panel`]);
//! - the bottom bar: status, the page well and the view ([`status_bar`]);
//! - brief 048's review drawer under the canvas, only while a symbol review or Compare shows
//!   ([`drawer`]);
//! - Settings, a full-window overlay opened from the project menu ([`crate::settings`], brief
//!   042); its Developer section has the developer switch, which shows CADCraft's own interface
//!   inside the main view (`--cad-ui`, `ui.cadchrome`).
//!
//! Panels keep their contents (design §5.6); only their framing is the shell's. The shell's state
//! and the per-project page views persist in eframe's storage ([`state`]).
//!
//! Excluded: the table component (U3: `lists::show` goes into the Lists slot), the wheel and the
//! action table (U4b), the canvas look (U4a), the inspector and Standards (U4c), the restyled
//! banner (U4d), and the floating windows still to convert (sheets, merge, renumber, preview).

pub mod bar;
pub mod compare;
pub mod connector;
pub mod drawer;
pub mod glyphs;
pub mod intent;
pub mod keys;
pub mod layout;
pub mod left;
pub mod placing;
pub mod right;
pub mod state;
pub mod status_bar;
pub mod text_panel;
pub mod toggles;
pub mod tool_strip;
pub mod top_bar;
pub mod wheel;

use std::collections::BTreeSet;

use cadcraft_ui_egui::Embedding;
use ecad_model::PageId;
use eframe::egui::{self, Rect};

use crate::Trial;
pub use layout::{Slots, layout};
pub use state::{LeftTab, RightTab, ShellState, Views, Workspace};

/// The margin round the sheet when it fits the canvas (UI rev 3), px.
pub const FIT_MARGIN: f32 = 24.0;

/// Accessible labels of the shell's own elements (design §9: tests find them by label).
pub const SETTINGS: &str = "Settings";
pub const DEVELOPER_SWITCH: &str = "Show CADCraft's interface (developer)";
pub const COMPANION: &str = "Open beside";
pub const CLOSE_COMPANION: &str = "Close the companion view";
pub const PUBLISH_RELEASE: &str = "Release";
pub const PUBLISH_PRINT: &str = "Print Book";
/// Publish ▸ Release with no dialog open (after a release): open it for the pending revision.
pub const RELEASE_PENDING: &str = "Release the pending revision…";
/// The Text panel command that flips the developer switch.
pub const CAD_CHROME_COMMAND: &str = "ui.cadchrome";

/// One place visited, for Back and Forward (Alt+Left, Alt+Right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visit {
    pub workspace: Workspace,
    pub page: Option<PageId>,
}

/// Lists' views (spec §2: the managers as tabs). Device list, Terminal plan and Check list are
/// U3's column sets ([`crate::lists::show`]); Reports hosts today's BOM, wires and cables (and its
/// own Terminals tab). While Lists shows a view the left panel or the drawer also hosts, that one
/// defers to Lists (one table is drawn once a frame).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ListsView {
    #[default]
    Devices,
    Terminals,
    /// Brief 051: Lists ▸ PLC (045c's Lists ▸ I/O).
    Plc,
    Checks,
    Reports,
}

impl ListsView {
    pub const ALL: [ListsView; 5] = [ListsView::Devices, ListsView::Terminals, ListsView::Plc, ListsView::Checks, ListsView::Reports];

    /// Its name in Lists' switch (apart from the left panel's Devices tab).
    pub fn label(self) -> &'static str {
        match self {
            ListsView::Devices => LISTS_DEVICES,
            ListsView::Terminals => LISTS_TERMINALS,
            ListsView::Plc => LISTS_PLC,
            ListsView::Checks => LISTS_CHECKS,
            ListsView::Reports => LISTS_REPORTS,
        }
    }
}

pub const LISTS_DEVICES: &str = "Device list";
pub const LISTS_TERMINALS: &str = "Terminal plan";
/// Brief 051 (design 051 §3.1): the rack overview and the I/O list (045c's "I/O").
pub const LISTS_PLC: &str = "PLC";
pub const LISTS_CHECKS: &str = "Check list";
pub const LISTS_REPORTS: &str = "Reports";
/// Said in the inspector's Checks tab while Lists shows the Check list.
pub const CHECKS_IN_LISTS: &str = "The checks are shown in Lists ▸ Check list.";
/// Said in the left panel's Devices tab while Lists shows the same view.
pub const DEVICES_IN_LISTS: &str = "The devices are shown in Lists ▸ Device list.";

/// Publish's two hosted bodies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PublishTab {
    #[default]
    Release,
    PrintBook,
}

/// Build counters of the hosted views (design §8.2: a commit changing nothing a view shows leaves
/// its counter unchanged).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counters {
    /// How many times the changed-elsewhere dots were worked out from the change feed.
    pub dots: u64,
}

/// The shell's state in the trial.
pub struct Shell {
    pub state: ShellState,
    pub views: Views,
    /// The texts last written to storage, so `save` writes only when something changed.
    stored: (String, String),
    /// Lines to put in the command history once the window runs (an unreadable stored value).
    pub notes: Vec<String>,
    /// The slots of the last frame.
    pub slots: Option<Slots>,
    /// The project change count and serial the dots were worked out at.
    seen: Option<(u64, u64)>,
    pub counters: Counters,
    /// Back and forward over page and workspace visits.
    pub back: Vec<Visit>,
    pub forward: Vec<Visit>,
    last_visit: Option<Visit>,
    pub tools: tool_strip::ToolStrip,
    pub settings_open: bool,
    pub publish: PublishTab,
    /// The view Lists shows.
    pub lists: ListsView,
    /// The rect the page is shown in this frame (Draw's main view, Lists' companion), or `None`
    /// when it is laid out hidden under a hosted view: ECAD's hover, the zone and the go-to
    /// ring's click react only inside it.
    pub page_rect: Option<Rect>,
    /// A plain page switch (tree click, PgUp/PgDn, Back) asked for the page's stored view; a
    /// go-to keeps the view it set.
    pub restore_view: bool,
    /// Library opened the symbol search: it closes again when another workspace is shown.
    search_from_library: bool,
    /// The project whose views were applied, and the page whose view is shown.
    view_of: Option<(String, Option<PageId>)>,
    /// The shell's own share of the last frame, in milliseconds (design §10).
    pub frame_ms: f64,
    /// The bottom bar's page well (UI rev 3).
    pub well: status_bar::Well,
    /// When the project last became saved in this session (`HH:MM`, local), and the save state
    /// last frame.
    pub saved_at: Option<String>,
    pub saved_was: Option<String>,
}

impl std::fmt::Debug for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell").field("state", &self.state).finish_non_exhaustive()
    }
}

impl Shell {
    /// A shell with the default state. (The right panel reads the trial's `props` copy itself, UI
    /// rev 3; the argument is kept for its callers.)
    pub fn new(_props: crate::sheets_ui::Shared) -> Self {
        Shell {
            state: ShellState::default(),
            views: Views::default(),
            stored: (String::new(), String::new()),
            notes: Vec::new(),
            slots: None,
            seen: None,
            counters: Counters::default(),
            back: Vec::new(),
            forward: Vec::new(),
            last_visit: None,
            tools: tool_strip::ToolStrip::default(),
            settings_open: false,
            publish: PublishTab::default(),
            lists: ListsView::default(),
            page_rect: None,
            restore_view: false,
            search_from_library: false,
            view_of: None,
            frame_ms: 0.0,
            well: status_bar::Well::default(),
            saved_at: None,
            saved_was: None,
        }
    }

    /// Take the stored shell state and views (eframe storage); an unreadable value starts with
    /// the default and leaves one line for the command history.
    pub fn load(&mut self, shell: Option<&str>, views: Option<&str>) {
        let (state, a) = ShellState::from_stored(shell);
        let (views, b) = Views::from_stored(views);
        let cad_ui = self.state.cad_chrome;
        self.state = state;
        self.state.cad_chrome |= cad_ui;
        self.views = views;
        self.notes.extend(a.into_iter().chain(b));
        self.stored = (self.state.to_stored(), self.views.to_stored());
    }

    /// What to store when it changed since the last call: `(shell, views)`, each `None` when
    /// unchanged.
    pub fn to_save(&mut self) -> (Option<String>, Option<String>) {
        let (s, v) = (self.state.to_stored(), self.views.to_stored());
        let shell = (s != self.stored.0).then(|| s.clone());
        let views = (v != self.stored.1).then(|| v.clone());
        self.stored = (s, v);
        (shell, views)
    }
}

/// The ECAD style of a slot's background.
fn fill(ui: &egui::Ui, rect: Rect, c: ecad_ui::tokens::Color) {
    ui.painter().rect_filled(rect, 0.0, ecad_ui::color(c));
}

/// A child `Ui` filling `rect`.
pub(crate) fn in_rect<R>(ui: &mut egui::Ui, rect: Rect, id: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope_builder(egui::UiBuilder::new().id_salt(id).max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.set_clip_rect(rect);
        add(ui)
    })
    .inner
}

/// A floating window of today's (Reports, Release, Print Book, symbol search) hosted in a slot:
/// filling `host`, without a title bar, not movable. `None` leaves the window as it was.
pub fn hosted(window: egui::Window<'_>, host: Option<Rect>) -> egui::Window<'_> {
    match host {
        Some(r) => window.title_bar(false).movable(false).resizable(false).collapsible(false).fixed_rect(r.shrink(ecad_ui::tokens::space::XS)),
        None => window,
    }
}

/// [`hosted`] as a method, so a window's builder chain (and its body's indentation) stays as it
/// was: `egui::Window::new(..).open(&mut open).hosted(self.host).show(ctx, ..)`.
pub trait Hosted {
    fn hosted(self, host: Option<Rect>) -> Self;
}

impl Hosted for egui::Window<'_> {
    fn hosted(self, host: Option<Rect>) -> Self {
        hosted(self, host)
    }
}

impl Trial {
    /// The shell's state (workspace, panels, the developer switch).
    pub fn shell_state(&self) -> &ShellState {
        &self.shell.state
    }

    /// Switch workspace (Ctrl+1–4 or the segmented control), remembering the visit for Back.
    pub fn set_workspace(&mut self, w: Workspace) {
        if self.shell.state.workspace == w {
            return;
        }
        self.shell.state.workspace = w;
        self.enter_workspace();
    }

    /// What the workspace shown needs open: Reports in Lists; Release and Print Book in Publish.
    fn enter_workspace(&mut self) {
        match self.shell.state.workspace {
            Workspace::Lists if self.project_open() => self.open_reports(),
            Workspace::Publish if self.project_open() => {
                if self.print_book.is_none() {
                    self.open_print_book(None);
                }
                if self.release.is_none() {
                    self.open_release(None);
                }
            }
            _ => {}
        }
    }

    /// Turn the developer switch on or off.
    pub fn set_cad_chrome(&mut self, on: bool) {
        self.shell.state.cad_chrome = on;
    }

    /// Lay out the shell and everything in it, for one frame.
    pub(crate) fn shell_ui(&mut self, ui: &mut egui::Ui) {
        let t0 = cadcraft_ui_egui::now_ms();
        for line in std::mem::take(&mut self.shell.notes) {
            self.cad.session.echo(line);
        }
        // The Developer section and the Text panel are one: either opened opens both.
        let open = self.developer.open || self.shell.state.text_panel;
        (self.developer.open, self.shell.state.text_panel) = (open, open);
        // The banner's slot while an agent is shown, unless the agent pill hid it (brief 043).
        let agent = self.agent.banner.as_ref().is_some_and(|b| !crate::agent::banner::hidden(ui.ctx(), &b.session));
        // Brief 048: the review drawer shows while a review or Compare is open.
        self.shell.state.review = self.drawer_review_tab().is_some();
        let slots = layout(ui.max_rect(), &self.shell.state, agent);
        self.shell.slots = Some(slots);
        // Brief 042: Settings, over everything; it takes Esc before the shell's keys.
        crate::settings::show(self, ui);
        keys::handle(self, ui.ctx(), &slots);
        self.refresh_panel();
        self.sync_props();
        self.track_pages();
        self.track_visit();

        // CADCraft's interface stays behind the developer switch (US1); ECAD owns the style.
        let chrome = self.shell.state.cad_chrome;
        self.cad.embedding = Embedding { chrome, idle_typing: chrome, canvas_menus: chrome, style: false };
        // Brief 041: the intent line replaces the command line, at rest and while prompting.
        self.cad.ui.show_command_line = chrome;

        let s = ecad_ui::SHELL;
        top_bar::show(self, ui, slots.top_bar);
        if let Some(r) = slots.agent_banner {
            fill(ui, r, s.panel);
            let banner = self.agent.banner.clone();
            let busy = self.cad.session.running.is_some();
            let since = banner.as_ref().and_then(|b| self.agent.sessions.get(&b.session)).map(|x| crate::agent::last_write::wall_ms(x.since));
            let act = in_rect(ui, r, "ecad.shell.banner", |ui| banner.and_then(|b| crate::agent::banner::show(ui, &b, busy, since)));
            if let Some(a) = act {
                self.agent_action(a);
            }
        }
        // History's session groups and their Undo session (brief 043).
        self.history_agent_sync();
        status_bar::show(self, ui, slots.status);
        left::show(self, ui, &slots);
        right::show(self, ui, &slots);
        drawer::show(self, ui, &slots);
        compare::show(self, ui, &slots);
        if let Some(r) = slots.tool_strip {
            match self.library.builder.as_mut().filter(|_| self.shell.state.workspace == Workspace::Library) {
                Some(b) => crate::library::builder::strip(ui, r, b),
                None => tool_strip::show(self, ui, r),
            }
        }
        self.shell.frame_ms = cadcraft_ui_egui::now_ms() - t0;
        left::close_on_draw(self, ui.ctx(), &slots);
        self.main_view(ui, &slots);
        self.take_props();
    }

    /// The main view (and the companion half): the page through US1, or a hosted body.
    fn main_view(&mut self, ui: &mut egui::Ui, slots: &Slots) {
        let page_rect = match self.shell.state.workspace {
            Workspace::Draw => Some(slots.main),
            Workspace::Lists => slots.companion,
            Workspace::Library | Workspace::Publish => None,
        };
        self.shell.page_rect = page_rect;
        // Read before CADCraft's drawing takes the keys: Esc, or a click on the page shown.
        let ring_input = ui.ctx().input(|i| {
            let esc = i.key_pressed(egui::Key::Escape);
            let click = i.pointer.primary_clicked()
                && self.cad.canvas.hover.is_none()
                && i.pointer.interact_pos().is_some_and(|at| page_rect.is_some_and(|r| r.contains(at)));
            (esc, click)
        });
        // CADCraft's frame runs every frame (input, capture, its dialogs and protective prompts);
        // when no page is shown its drawing is laid out hidden under the hosted view.
        let rect = page_rect.unwrap_or(slots.main);
        in_rect(ui, rect, "ecad.shell.page", |ui| {
            if page_rect.is_none() {
                ui.set_invisible();
            }
            self.cad.ui_embedded(ui);
        });
        crate::canvas::paint(self, ui, page_rect);
        crate::agent::last_write::paint(self, ui, page_rect);
        self.clear_goto_ring(ring_input);
        // Brief 041: right-click opens the wheel (no tool running); then the intent line.
        self.wheel_frame(ui);
        if let Some(h) = slots.handle {
            self.split_handle(ui, h, slots);
        }
        if let Some(c) = slots.companion
            && self.shell.state.workspace == Workspace::Draw
        {
            fill(ui, c, ecad_ui::SHELL.bg);
        }
        if self.shell.state.workspace == Workspace::Lists {
            self.lists_view(ui, slots.main);
        }
        if self.shell.state.workspace == Workspace::Library {
            self.library_view(ui, slots.main);
        }
    }

    /// Lists' main view: its switch, then U3's column set (Device list, Terminal plan) through
    /// [`crate::lists::show`], or Reports hosted under the switch ([`Trial::host_windows`]).
    fn lists_view(&mut self, ui: &mut egui::Ui, main: Rect) {
        let head = Rect::from_min_size(main.min, egui::vec2(main.width(), ecad_ui::tokens::size::PANEL_HEADER));
        let body = Rect::from_min_max(egui::pos2(main.left(), head.bottom()), main.max);
        fill(ui, main, ecad_ui::SHELL.panel);
        let mut i = ListsView::ALL.iter().position(|v| *v == self.shell.lists).unwrap_or(0);
        let labels: Vec<&str> = ListsView::ALL.iter().map(|v| v.label()).collect();
        in_rect(ui, head.shrink(ecad_ui::tokens::space::XS), "ecad.shell.lists.head", |ui| {
            ui.horizontal(|ui| ecad_ui::prim::segmented(ui, &mut i, &labels));
        });
        self.shell.lists = ListsView::ALL.get(i).copied().unwrap_or_default();
        let tab = match self.shell.lists {
            ListsView::Devices => crate::lists::Tab::Devices,
            ListsView::Terminals => crate::lists::Tab::Terminals,
            ListsView::Plc => crate::lists::Tab::Plc,
            ListsView::Checks => crate::lists::Tab::Checks,
            ListsView::Reports => return,
        };
        if !self.project_open() {
            return;
        }
        // The Terminal plan is Reports' Terminals view; Reports keeps it in step.
        self.open_reports();
        let writable = {
            let h = self.header();
            h.name.is_some() && !h.read_only && self.panel.revision.is_none()
        };
        let Some(reports) = self.reports.as_mut() else { return };
        if tab == crate::lists::Tab::Terminals && reports.tab != crate::outputs_ui::Tab::Terminals {
            reports.tab = crate::outputs_ui::Tab::Terminals;
            reports.invalidate();
        }
        reports.io_shown = tab == crate::lists::Tab::Plc;
        let lists = crate::lists::Lists {
            checks: &mut self.panel.checks,
            devices: &mut self.panel.devices,
            terminals: &mut reports.terminals,
            io: &mut reports.io,
        };
        let actions = in_rect(ui, body.shrink(ecad_ui::tokens::space::S), "ecad.shell.lists.body", |ui| crate::lists::show(ui, tab, lists, writable));
        for a in actions {
            match a {
                crate::lists::ListAction::Panel(a) => self.panel_action(a),
                crate::lists::ListAction::Run(line) => {
                    let refusal = self.run_line(&line);
                    if let Some(r) = self.reports.as_mut() {
                        r.ran(refusal);
                    }
                }
                crate::lists::ListAction::GoTo(page, at) => self.goto_place(page, at),
                other => self.plc_action(other),
            }
        }
    }

    /// Brief 051: what Lists ▸ PLC asks the trial for (design 051 §3.4, §3.5).
    pub fn plc_action(&mut self, a: crate::lists::ListAction) {
        use crate::lists::ListAction;
        match a {
            ListAction::Say(t) => self.cad.session.echo(t),
            ListAction::GoToCard(page, card) => {
                // The card's first drawing on that page, else the page.
                let at = match (self.project.clone(), card) {
                    (Some(p), Some(c)) => {
                        p.read(&mut self.cad.session, |p, _| ecad_cadcraft::plc::overview::at_on(p.ledger.document(), c, page)).ok().flatten()
                    }
                    _ => None,
                };
                match at {
                    Some(at) => self.goto_place(page, at),
                    None => {
                        let _ = self.show_page(page);
                    }
                }
            }
            ListAction::ChecksOf(c) => {
                let tag = self
                    .project
                    .clone()
                    .and_then(|p| p.read(&mut self.cad.session, |p, _| ecad_cadcraft::devices::tag_words(p.ledger.document(), c)).ok());
                self.shell.lists = ListsView::Checks;
                if let Some(t) = tag {
                    self.panel.checks.table.query = t;
                }
            }
            ListAction::PlaceCard(slot) => {
                if self.cad.session.running.is_some() {
                    self.cad.session.cancel();
                }
                match self.cad.session.start("place") {
                    Ok(()) => self.cad.session.echo(format!("Search \"card\" and pick a card body: it goes in the next free slot ({slot}).")),
                    Err(e) => self.cad.session.echo(e.to_string()),
                }
                self.focus_drawing = true;
                self.after_logic();
            }
            _ => {}
        }
    }

    /// Whether Lists shows the Device list now (the left panel's Devices tab then defers to it).
    pub fn lists_shows_devices(&self) -> bool {
        self.shell.state.workspace == Workspace::Lists && self.shell.lists == ListsView::Devices
    }

    /// Whether Lists shows the Check list now (the drawer then defers to it).
    pub fn lists_shows_checks(&self) -> bool {
        self.shell.state.workspace == Workspace::Lists && self.shell.lists == ListsView::Checks
    }

    /// Whether `at` is over the page shown, with no window or popup above it there.
    pub fn over_shown_page(&self, ctx: &egui::Context, at: egui::Pos2) -> bool {
        self.shell.page_rect.is_some_and(|r| r.contains(at)) && ctx.layer_id_at(at).is_none_or(|l| l.order == egui::Order::Background)
    }

    /// Fit the page shown with its frame and title block visible and a 24 px margin (UI rev 3,
    /// "Canvas"); a page without a frame fits its drawing.
    pub fn fit_sheet(&mut self) -> bool {
        let Some(rect) = self.shell.slots.map(|s| s.main) else { return false };
        self.fit_sheet_in(rect)
    }

    /// [`Trial::fit_sheet`] in `rect`.
    pub fn fit_sheet_in(&mut self, rect: Rect) -> bool {
        let Ok(d) = self.cad.session.doc() else { return false };
        let mut b = cadcraft_engine::geom::Bounds2::default();
        for e in d.model.iter().filter(|e| e.common.layer == ecad_cadcraft::frame::LAYER) {
            b = b.union(&cadcraft_engine::doc::entity_bounds(d, e, 0));
        }
        if b.is_empty() {
            return self.zoom_to_drawing();
        }
        self.fit_bounds(b, rect)
    }

    /// Fit `b` (drawing units) in `rect` with [`FIT_MARGIN`] px all round.
    pub fn fit_bounds(&mut self, b: cadcraft_engine::geom::Bounds2, rect: Rect) -> bool {
        let (bw, bh) = (b.max.x - b.min.x, b.max.y - b.min.y);
        let (w, h) = (f64::from(rect.width() - 2.0 * FIT_MARGIN), f64::from(rect.height() - 2.0 * FIT_MARGIN));
        if !(bw > 0.0 && bh > 0.0 && w > 0.0 && h > 0.0) {
            return false;
        }
        let scale = (w / bw).min(h / bh);
        let Ok(st) = self.cad.session.state_mut() else { return false };
        let mut view = st.view();
        view.center = cadcraft_engine::geom::Vec2::new((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0);
        view.height = f64::from(rect.height()) / scale;
        view.height.is_finite() && st.set_view(view)
    }

    /// The companion split's handle: drag to resize, double-click for the default.
    fn split_handle(&mut self, ui: &mut egui::Ui, h: Rect, slots: &Slots) {
        let r = ui.interact(h, egui::Id::unique("ecad.shell.split"), egui::Sense::click_and_drag());
        fill(ui, h, if r.hovered() || r.dragged() { ecad_ui::SHELL.line_strong } else { ecad_ui::SHELL.line });
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        let Some(c) = &mut self.shell.state.companion else { return };
        if r.double_clicked() {
            c.ratio = state::COMPANION_RATIO;
        } else if r.dragged()
            && let Some(p) = r.interact_pointer_pos()
            && let Some(comp) = slots.companion
        {
            let room = (comp.right() - slots.main.left() - layout::SPLIT_HANDLE).max(1.0);
            c.ratio = ((p.x - slots.main.left()) / room).clamp(0.0, 1.0);
        }
    }

    /// Where the hosted windows go this frame, and which are drawn at all: Reports in Lists (or
    /// as Draw's companion), symbol search in Library (floating elsewhere), Release or Print Book
    /// in Publish.
    pub(crate) fn host_windows(&mut self) {
        let Some(slots) = self.shell.slots else { return };
        let w = self.shell.state.workspace;
        // Reports and Print Book closed (Cancel) are opened again while their workspace shows;
        // Release only when Publish is entered or asked for (it closes after releasing).
        match w {
            Workspace::Lists if self.project_open() => self.open_reports(),
            Workspace::Publish if self.project_open() && self.print_book.is_none() => self.open_print_book(None),
            _ => {}
        }
        let under_head = Rect::from_min_max(slots.main.min + egui::vec2(0.0, ecad_ui::tokens::size::PANEL_HEADER), slots.main.max);
        let reports = match w {
            Workspace::Lists if self.shell.lists == ListsView::Reports => Some(under_head),
            Workspace::Draw => slots.companion,
            _ => None,
        };
        if let Some(r) = &mut self.reports {
            r.host = reports;
            r.hidden = reports.is_none();
        }
        // Brief 048: the Library shows its own browser; the search (Ctrl+K) floats everywhere.
        self.search.host = None;
        if w != Workspace::Library && std::mem::take(&mut self.shell.search_from_library) {
            self.search.open = false;
        }
        let body = Rect::from_min_max(slots.main.min + egui::vec2(0.0, ecad_ui::tokens::size::PANEL_HEADER), slots.main.max);
        if let Some(d) = &mut self.release {
            d.host = (w == Workspace::Publish && self.shell.publish == PublishTab::Release).then_some(body);
            d.hidden = d.host.is_none();
        }
        if let Some(d) = &mut self.print_book {
            d.host = (w == Workspace::Publish && self.shell.publish == PublishTab::PrintBook).then_some(body);
            d.hidden = d.host.is_none();
        }
    }

    /// Publish's header row (Release | Print Book), drawn after the hosted window so it sits
    /// above the body.
    pub(crate) fn publish_header(&mut self, ui: &mut egui::Ui) {
        let Some(slots) = self.shell.slots else { return };
        if self.shell.state.workspace != Workspace::Publish {
            return;
        }
        let head = Rect::from_min_size(slots.main.min, egui::vec2(slots.main.width(), ecad_ui::tokens::size::PANEL_HEADER));
        fill(ui, head, ecad_ui::SHELL.panel);
        let mut i = match self.shell.publish {
            PublishTab::Release => 0,
            PublishTab::PrintBook => 1,
        };
        let project = self.project_open();
        let no_release = project && i == 0 && self.release.is_none();
        let mut open_release = false;
        in_rect(ui, head.shrink(ecad_ui::tokens::space::XS), "ecad.shell.publish", |ui| {
            ui.horizontal(|ui| {
                ecad_ui::prim::segmented(ui, &mut i, &[PUBLISH_RELEASE, PUBLISH_PRINT]);
                if !project {
                    ecad_ui::prim::read_value(ui, "No project is open.");
                } else if no_release {
                    open_release = ui.add(ecad_ui::prim::Button::new(RELEASE_PENDING).small()).clicked();
                }
            });
        });
        if open_release {
            self.open_release(None);
        }
        self.shell.publish = if i == 0 { PublishTab::Release } else { PublishTab::PrintBook };
    }

    /// The dots on pages changed while another page was shown (design §8.2, Pages): from the
    /// change feed, only when its count moved; the Pages rows are not rebuilt for it.
    fn track_pages(&mut self) {
        let Some(p) = self.project.clone().filter(|p| p.is_attached(&self.cad.session)) else {
            self.panel.changed.clear();
            self.shell.seen = None;
            return;
        };
        let serial = self.project_serial;
        let seen = self.shell.seen;
        let r = p.read(&mut self.cad.session, |p, _| {
            let now = p.change_count();
            match seen {
                Some((s, n)) if s == serial && n == now => None,
                Some((s, n)) if s == serial => Some((now, p.changes_since(n), p.active_page())),
                _ => Some((now, Some(BTreeSet::new()), p.active_page())),
            }
        });
        let Ok(Some((now, touched, active))) = r else { return };
        if seen.is_some_and(|(s, _)| s != serial) || seen.is_none() {
            self.panel.changed.clear();
        }
        // A rolled-over feed marks nothing: what changed is no longer known page by page.
        for page in touched.unwrap_or_default() {
            if Some(page) != active {
                self.panel.changed.insert(page);
            }
        }
        self.shell.counters.dots += 1;
        self.shell.seen = Some((serial, now));
    }

    /// Note the page and workspace shown for Back, clear the shown page's dot and keep each
    /// page's view (zoom and centre) for the project.
    fn track_visit(&mut self) {
        let page = self.panel.active;
        if let Some(pg) = page {
            self.panel.changed.remove(&pg);
        }
        let now = Visit { workspace: self.shell.state.workspace, page };
        if self.shell.last_visit != Some(now) {
            if let Some(prev) = self.shell.last_visit
                && self.shell.back.last() != Some(&prev)
            {
                self.shell.back.push(prev);
                if self.shell.back.len() > 64 {
                    self.shell.back.remove(0);
                }
            }
            self.shell.last_visit = Some(now);
        }
        self.sync_page_view(page);
    }

    /// Go back (or forward) one visit.
    pub fn go_back(&mut self, forward: bool) {
        let (from, to) = if forward { (&mut self.shell.forward, &mut self.shell.back) } else { (&mut self.shell.back, &mut self.shell.forward) };
        let Some(v) = from.pop() else { return };
        if let Some(cur) = self.shell.last_visit {
            to.push(cur);
        }
        // The visit we go to is not pushed again.
        self.shell.last_visit = Some(v);
        self.shell.state.workspace = v.workspace;
        self.enter_workspace();
        if let Some(pg) = v.page
            && self.panel.active != Some(pg)
        {
            self.shell.restore_view = true;
            let _ = self.show_page(pg);
        }
    }

    /// Show the page before or after the shown one in page order (PgUp, PgDn).
    pub fn step_page(&mut self, next: bool) {
        let rows = self.panel.rows();
        let at = self.panel.active.and_then(|a| rows.iter().position(|r| r.id == a));
        let to = match at {
            Some(i) if next => rows.get(i + 1),
            Some(i) => i.checked_sub(1).and_then(|j| rows.get(j)),
            None => rows.first(),
        };
        if let Some(pg) = to.map(|r| r.id) {
            self.shell.restore_view = true;
            let _ = self.show_page(pg);
        }
    }

    /// Keep the page views (design §5.4): the shown page's view is recorded every frame it
    /// changes; a page shown again (or a project opened again) gets its view back.
    fn sync_page_view(&mut self, page: Option<PageId>) {
        let Some(pid) = self.project_key() else {
            self.shell.view_of = None;
            return;
        };
        let changed = self.shell.view_of.as_ref() != Some(&(pid.clone(), page));
        let pv = self.shell.views.projects.get(&pid);
        // Taken every frame: it asks for the page change it came with (or none).
        let restore = std::mem::take(&mut self.shell.restore_view);
        if changed {
            let opened = self.shell.view_of.as_ref().is_none_or(|(p, _)| *p != pid);
            if opened {
                let last_ws = pv.and_then(|v| v.last_workspace);
                let entry = self.shell.views.open(&pid);
                let _ = entry;
                if let Some(w) = last_ws {
                    self.shell.state.workspace = w;
                }
            }
            // Only a plain page switch, or the project opened again, restores the stored view: a
            // go-to (a check, a device, a Terminals row's place, a reference) keeps the view it set.
            if (restore || opened)
                && let Some(v) = page.and_then(|pg| self.shell.views.projects.get(&pid).and_then(|p| p.pages.get(&pg.hex())).copied())
                && let Ok(st) = self.cad.session.state_mut()
            {
                let mut view = st.view();
                view.center = cadcraft_engine::geom::Vec2::new(v.cx, v.cy);
                view.height = v.height;
                let _ = st.set_view(view);
            }
            self.shell.view_of = Some((pid.clone(), page));
        }
        let Some(pg) = page else { return };
        let Ok(st) = self.cad.session.state() else { return };
        let v = st.view();
        let now = state::PageView { cx: v.center.x, cy: v.center.y, height: v.height };
        let ws = self.shell.state.workspace;
        let entry = self.shell.views.projects.entry(pid).or_default();
        if now.usable() && entry.pages.get(&pg.hex()) != Some(&now) {
            entry.pages.insert(pg.hex(), now);
        }
        entry.last_page = Some(pg.hex());
        entry.last_workspace = Some(ws);
    }

    /// The open project's stable ID, as the views key.
    fn project_key(&mut self) -> Option<String> {
        let p = self.project.clone().filter(|p| p.is_attached(&self.cad.session))?;
        p.read(&mut self.cad.session, |p, _| p.id.hex()).ok()
    }
}
