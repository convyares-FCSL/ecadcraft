//! The bottom bar (UI rev 3: 32 px, one bar): status at the left, the page well in the centre,
//! the view at the right. Every group sits in its own bordered pill.
//!
//! - Status: the save state and time; the revision, "Rev C D*" (opens History); the checks
//!   counter (opens the inspector's Checks tab).
//! - The page well, 880 px, centred: the page shown (number, "/ 09", title) at its centre between
//!   `« ‹` and `› »`, a divider either side; the groups before and the open group's pages left of
//!   it, the groups after (folded to a folder, a count and a ›) and + right of it. A page button's
//!   right-click opens the page wheel, as a page row in Book does.
//! - The view: standard (its name, brief 050a; opens Settings ▸ Standards) · sheet size · sheet
//!   colour, then Fit, zoom out, zoom %, zoom in.
//!
//! In Project, Library and Publish the well and the view groups are hidden; the status stays.
//!
//! **Update map.** The well's groups are rebuilt only when the Pages copy is
//! ([`crate::project_panel::ProjectPanel::rebuilds`]); the revision and sheet text only when the
//! project's change count, the project or the page shown changes. Nothing else is read per frame.

use std::collections::BTreeSet;

use ecad_model::{FolderId, PageId};
use ecad_ui::tokens::{Icon, size, space, ui_type};
use eframe::egui::{self, Rect, pos2, vec2};

use super::{Workspace, bar};
use crate::Trial;

/// Accessible labels.
pub const SAVE_STATE: &str = "Save state";
/// The standard (brief 042): opens Settings ▸ Standards.
pub const STANDARD: &str = "Standard: IEC (built in)";
pub const SHEET_SIZE: &str = "Sheet size";
pub const SHEET_COLOUR: &str = "Sheet colour";
pub const FIT: &str = "Fit the sheet";
pub const ZOOM_IN: &str = "Zoom in";
pub const ZOOM_OUT: &str = "Zoom out";
pub const ZOOM: &str = "Zoom";
pub const PREV_GROUP: &str = "Previous group";
pub const PREV_PAGE: &str = "Previous page";
pub const NEXT_PAGE: &str = "Next page";
pub const NEXT_GROUP: &str = "Next group";
pub const ADD_PAGE: &str = "Add a page after this one";
pub const PAGE_WELL: &str = "Page well";
/// The sheet colour menu's entries (UI rev 3, "Sheet colour and full dark").
pub const PAPER: &str = "Paper";
pub const FOLLOW: &str = "Follow";
pub const DARK: &str = "Dark";
/// Kept for the Commands button's old callers: the Text panel opens with Ctrl+/.
pub const COMMANDS: &str = "Commands";

/// The pills' height inside the bar.
const PILL_H: f32 = 24.0;
/// A square button in a pill.
const BTN: f32 = 22.0;
/// A page button's width.
const PAGE_BTN: f32 = 32.0;
/// Zoom steps.
const ZOOM_STEP: f64 = 1.25;

/// One group of pages in book order (a top-level run of pages sharing a folder).
#[derive(Debug, Clone, PartialEq)]
pub struct WellGroup {
    pub folder: Option<FolderId>,
    pub name: String,
    pub pages: Vec<(PageId, String)>,
}

/// The well's kept state: its groups (with the Pages copy they were built from) and the groups
/// opened with › in this session.
#[derive(Debug, Clone, Default)]
pub struct Well {
    built_at: Option<u64>,
    pub groups: Vec<WellGroup>,
    pub opened: BTreeSet<Option<FolderId>>,
    /// The revision and sheet text, and what they were read at (serial, change count, page).
    facts_at: Option<(u64, u64, Option<PageId>)>,
    /// `("C", true)`: the last issued revision and whether a pending one has changes.
    pub rev: Option<(String, String, bool)>,
    pub sheet: String,
    /// How many times the groups were rebuilt (tests).
    pub rebuilds: u64,
}

impl Trial {
    /// Keep the well's groups and facts up to date (see the update map above).
    fn sync_well(&mut self) {
        let built = self.panel.rebuilds;
        if self.shell.well.built_at != Some(built) {
            let mut names = std::collections::BTreeMap::new();
            for r in self.panel.tree() {
                if let ecad_cadcraft::TreeRow::Folder { id, name, .. } = r {
                    names.insert(*id, name.clone());
                }
            }
            let mut groups: Vec<WellGroup> = Vec::new();
            for r in self.panel.rows() {
                let folder = r.folder;
                match groups.last_mut() {
                    Some(g) if g.folder == folder => g.pages.push((r.id, r.number.clone())),
                    _ => groups.push(WellGroup {
                        folder,
                        name: folder.and_then(|f| names.get(&f).cloned()).unwrap_or_else(|| "Pages".into()),
                        pages: vec![(r.id, r.number.clone())],
                    }),
                }
            }
            self.shell.well.groups = groups;
            self.shell.well.built_at = Some(built);
            self.shell.well.rebuilds += 1;
        }
        let Some(p) = self.project.clone().filter(|p| p.is_attached(&self.cad.session)) else {
            self.shell.well.facts_at = None;
            self.shell.well.rev = None;
            return;
        };
        let page = self.panel.active;
        let serial = self.project_serial;
        let now = p.read(&mut self.cad.session, |p, _| p.change_count()).ok();
        let Some(now) = now else { return };
        if self.shell.well.facts_at == Some((serial, now, page)) {
            return;
        }
        let facts = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let last = ecad_cadcraft::revisions::issued(doc).last().map(|r| r.rev().to_string()).unwrap_or_default();
            let pending = ecad_cadcraft::revisions::pending_rev(doc);
            let dirty = ecad_cadcraft::revisions::is_pending(&p.ledger);
            let sheet = page.map(|pg| ecad_model::sheet::sheet_of(doc, pg)).unwrap_or_else(|| doc.settings.sheet);
            let sheet = sheet.preset_name().map_or_else(|| format!("{}×{}", sheet.width, sheet.height), str::to_string);
            ((last, pending, dirty), sheet)
        });
        if let Ok((rev, sheet)) = facts {
            self.shell.well.rev = Some(rev);
            self.shell.well.sheet = sheet;
            self.shell.well.facts_at = Some((serial, now, page));
        }
    }

    /// Show `page` as a plain page switch (its stored view comes back).
    fn well_show(&mut self, page: PageId) {
        if self.panel.active != Some(page) {
            self.shell.restore_view = true;
            let _ = self.show_page(page);
        }
    }

    /// The group index of the page shown.
    fn well_group_of_active(&self) -> Option<usize> {
        let a = self.panel.active?;
        self.shell.well.groups.iter().position(|g| g.pages.iter().any(|(p, _)| *p == a))
    }

    /// The first page of the group before (or after) the shown page's.
    pub fn step_group(&mut self, next: bool) {
        let at = self.well_group_of_active();
        let to = match at {
            Some(i) if next => self.shell.well.groups.get(i + 1),
            Some(i) => i.checked_sub(1).and_then(|j| self.shell.well.groups.get(j)),
            None => self.shell.well.groups.first(),
        };
        if let Some(pg) = to.and_then(|g| g.pages.first()).map(|(p, _)| *p) {
            self.well_show(pg);
        }
    }

    /// Zoom the page shown by `factor` about its centre (> 1 zooms out).
    pub fn zoom_by(&mut self, factor: f64) {
        if let Ok(st) = self.cad.session.state_mut() {
            let mut v = st.view();
            let h = v.height * factor;
            if h.is_finite() && h > 0.0 {
                v.height = h;
                let _ = st.set_view(v);
            }
        }
    }
}

/// Draw the bottom bar in `rect`.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, rect: Rect) {
    let s = ecad_ui::SHELL;
    super::fill(ui, rect, s.panel);
    ui.painter().hline(rect.x_range(), rect.top() + 0.5, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    t.sync_well();
    let y = rect.center().y;
    let pill_at = |x: f32, w: f32| Rect::from_min_size(pos2(x, y - PILL_H / 2.0), vec2(w, PILL_H));
    // UI rev 3: Settings (boards 5.01, 5.02) shows only the status, as Project, Library and Publish.
    let draw = t.shell.state.workspace == Workspace::Draw && !t.shell.settings_open;

    // ---- Status: saved, revision, checks.
    let save = t.save_state();
    let saved_text = match (&save[..], t.shell.saved_at.as_deref()) {
        ("Saved", Some(at)) => format!("Saved {at}"),
        _ => save.clone(),
    };
    let dot = match &save[..] {
        "Saved" => Some(s.ok),
        "No project" => None,
        "Read-only" => Some(s.error),
        _ => Some(s.warning),
    };
    let w = bar::text_width(ui, &saved_text, ui_type::SMALL) + 2.0 * space::M + if dot.is_some() { 12.0 } else { 0.0 };
    let saved = pill_at(rect.left() + space::M, w);
    bar::pill(ui, saved);
    let r = ui.interact(saved, egui::Id::unique(SAVE_STATE), egui::Sense::hover());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{SAVE_STATE}: {save}")));
    let mut x = saved.left() + space::M;
    if let Some(c) = dot {
        bar::dot(ui, pos2(x + 3.0, y), 3.5, ecad_ui::color(c));
        x += 12.0;
    }
    bar::text_at(ui, x, y, &saved_text, ui_type::SMALL, ecad_ui::color(s.text_2));
    let mut x = saved.right() + space::M;

    let mut open_history = false;
    if let Some((last, pending, dirty)) = t.shell.well.rev.clone() {
        let first = if last.is_empty() { "Rev –".to_string() } else { format!("Rev {last}") };
        let star = if dirty { format!(" {pending}*") } else { String::new() };
        let w = size::ICON + space::S + bar::text_width(ui, &first, ui_type::SMALL) + bar::text_width(ui, &star, ui_type::SMALL) + 2.0 * space::M;
        let pill = pill_at(x, w);
        bar::pill(ui, pill);
        let label = format!("{}: {first}{star}", super::top_bar::REVISION_PILL);
        let r = bar::hit(ui, pill.shrink(1.0), &label, false, false, true).on_hover_text("History: revisions and changes");
        open_history = r.clicked();
        let icon = Rect::from_center_size(pos2(pill.left() + space::M + 6.0, y), vec2(12.0, 12.0));
        ecad_ui::icons::paint(ui.painter(), icon, Icon::History, ecad_ui::color(s.text_2));
        let e = bar::text_at(ui, icon.right() + space::S, y, &first, ui_type::SMALL, ecad_ui::color(s.text_2));
        bar::text_at(ui, e, y, &star, ui_type::SMALL, ecad_ui::color(s.provisional));
        x = pill.right() + space::M;
    }

    let (n, errors) = t.checks_count();
    let warnings = n.saturating_sub(errors);
    let label = if n == 0 { "No problems".to_string() } else { "Checks".to_string() };
    let counts = [(errors, s.error), (warnings, s.warning)];
    let mut w = bar::text_width(ui, &label, ui_type::SMALL) + 2.0 * space::M;
    for (c, _) in counts.iter().filter(|(c, _)| *c > 0) {
        w += 10.0 + bar::text_width(ui, &c.to_string(), ui_type::SMALL) + space::S;
    }
    let pill = pill_at(x, w);
    bar::pill(ui, pill);
    let r = bar::hit(ui, pill.shrink(1.0), &format!("{}: Checks {n}", super::top_bar::CHECKS_PILL), false, false, true)
        .on_hover_text("Open the Checks tab");
    let open_checks = r.clicked();
    let mut cx = pill.left() + space::M;
    for (c, tone) in counts.iter().filter(|(c, _)| *c > 0) {
        bar::dot(ui, pos2(cx + 3.0, y), 3.5, ecad_ui::color(*tone));
        cx = bar::text_at(ui, cx + 10.0, y, &c.to_string(), ui_type::SMALL, ecad_ui::color(s.text_2)) + space::S;
    }
    bar::text_at(ui, cx, y, &label, ui_type::SMALL, ecad_ui::color(s.text_2));
    let status_right = pill.right();

    if open_history {
        t.set_workspace(Workspace::Draw);
        t.set_left_tab(super::LeftTab::History);
    }
    if open_checks {
        t.open_inspector(super::state::RightTab::Checks);
    }
    if !draw {
        return;
    }

    // ---- The view, from the right edge.
    let zoom = t.cad.canvas.xf.map(|xf| xf.scale * 25.4 / 96.0 * 100.0);
    let zoom_text = zoom.filter(|z| z.is_finite()).map_or_else(|| "–".to_string(), |z| format!("{z:.0} %"));
    let fit_w = size::ICON + space::XS + bar::text_width(ui, "Fit", ui_type::SMALL) + 2.0 * space::S;
    let zoom_w = bar::text_width(ui, "100 %", ui_type::SMALL) + space::S;
    let zp = pill_at(rect.right() - space::M - (fit_w + 2.0 * BTN + zoom_w + 2.0 * space::XS), fit_w + 2.0 * BTN + zoom_w + 2.0 * space::XS);
    bar::pill(ui, zp);
    let mut x = zp.left() + space::XS;
    let fit_r = Rect::from_min_size(pos2(x, y - BTN / 2.0), vec2(fit_w, BTN));
    let fit = bar::hit(ui, fit_r, FIT, false, false, true).clicked();
    let icon = Rect::from_center_size(pos2(fit_r.left() + space::S + 6.0, y), vec2(12.0, 12.0));
    ecad_ui::icons::paint(ui.painter(), icon, Icon::Fit, ecad_ui::color(s.text_2));
    bar::text_at(ui, icon.right() + space::XS + 2.0, y, "Fit", ui_type::SMALL, ecad_ui::color(s.text_2));
    x = fit_r.right();
    let out = bar::icon_button(ui, Rect::from_min_size(pos2(x, y - BTN / 2.0), vec2(BTN, BTN)), Icon::Minus, ZOOM_OUT, 12.0, true).clicked();
    x += BTN;
    let zr = Rect::from_min_size(pos2(x, y - BTN / 2.0), vec2(zoom_w, BTN));
    let zl = ui.interact(zr, egui::Id::unique(ZOOM), egui::Sense::hover());
    zl.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{ZOOM}: {zoom_text}")));
    ui.painter().text(zr.center(), egui::Align2::CENTER_CENTER, &zoom_text, bar::font(ui, ui_type::SMALL), ecad_ui::color(s.text_2));
    x += zoom_w;
    let zin = bar::icon_button(ui, Rect::from_min_size(pos2(x, y - BTN / 2.0), vec2(BTN, BTN)), Icon::Plus, ZOOM_IN, 12.0, true).clicked();

    let sheet = t.shell.state.sheet;
    let sheet_word = match sheet {
        crate::canvas::Sheet::Paper => PAPER,
        crate::canvas::Sheet::Dark => DARK,
    };
    // Brief 050a: the standard's name (IEC when built in); its label is `Standard: <name>`.
    let standard = t.standard_words();
    let std_text = standard.trim_start_matches("Standard: ").trim_end_matches(" (built in)").to_string();
    let std_w = bar::text_width(ui, &std_text, ui_type::SMALL) + 2.0 * space::S;
    let size_text = if t.project_open() { t.shell.well.sheet.clone() } else { "–".to_string() };
    let size_w = bar::text_width(ui, &size_text, ui_type::SMALL) + 2.0 * space::S;
    let col_w = 12.0 + space::XS + bar::text_width(ui, sheet_word, ui_type::SMALL) + space::XS + 10.0 + 2.0 * space::S;
    let vw = std_w + size_w + col_w + 2.0 * space::XS;
    let vp = pill_at(zp.left() - space::M - vw, vw);
    bar::pill(ui, vp);
    let mut x = vp.left() + space::XS;
    let std_r = Rect::from_min_size(pos2(x, y - BTN / 2.0), vec2(std_w, BTN));
    let standards = bar::hit(ui, std_r, &standard, false, false, true).on_hover_text("Settings ▸ Standards").clicked();
    ui.painter().text(std_r.center(), egui::Align2::CENTER_CENTER, &std_text, bar::font(ui, ui_type::SMALL), ecad_ui::color(s.text_2));
    x = std_r.right();
    let size_r = Rect::from_min_size(pos2(x, y - BTN / 2.0), vec2(size_w, BTN));
    let sr = ui.interact(size_r, egui::Id::unique(SHEET_SIZE), egui::Sense::hover());
    sr.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{SHEET_SIZE}: {size_text}")));
    sr.on_hover_text("The page's sheet (Page properties)");
    ui.painter().text(size_r.center(), egui::Align2::CENTER_CENTER, &size_text, bar::font(ui, ui_type::SMALL), ecad_ui::color(s.text_2));
    x = size_r.right();
    let col_r = Rect::from_min_size(pos2(x, y - BTN / 2.0), vec2(col_w, BTN));
    let col = bar::hit(ui, col_r, SHEET_COLOUR, false, false, true);
    let sw = Rect::from_center_size(pos2(col_r.left() + space::S + 6.0, y), vec2(12.0, 9.0));
    let paper = ecad_ui::color(sheet.tokens().color(ecad_ui::tokens::Meaning::Sheet));
    ui.painter().rect(sw, 1.5, paper, egui::Stroke::new(1.0, ecad_ui::color(s.line_strong)), egui::StrokeKind::Inside);
    let e = bar::text_at(ui, sw.right() + space::XS, y, sheet_word, ui_type::SMALL, ecad_ui::color(s.text_2));
    ecad_ui::icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(e + space::XS + 5.0, y), vec2(10.0, 10.0)),
        Icon::ChevronUp,
        ecad_ui::color(s.text_3),
    );
    let mut pick = None;
    egui::Popup::menu(&col).show(|ui| {
        if ui.selectable_label(sheet == crate::canvas::Sheet::Paper, PAPER).on_hover_text("Always light").clicked() {
            pick = Some(crate::canvas::Sheet::Paper);
        }
        ui.add_enabled(false, egui::Button::selectable(false, FOLLOW))
            .on_disabled_hover_text("Follows the interface theme once the light theme comes");
        if ui.selectable_label(sheet == crate::canvas::Sheet::Dark, DARK).on_hover_text("Always dark").clicked() {
            pick = Some(crate::canvas::Sheet::Dark);
        }
        ui.separator();
        ui.label(egui::RichText::new("The screen only: PDFs and prints use paper colours.").small());
    });

    // ---- The page well, centred, between the status and the view.
    let ww = size::PAGE_WELL.min((vp.left() - space::M) - (status_right + space::M)).max(0.0);
    let cx = rect.center().x.clamp(status_right + space::M + ww / 2.0, (vp.left() - space::M - ww / 2.0).max(status_right + space::M + ww / 2.0));
    let well = Rect::from_center_size(pos2(cx, y), vec2(ww, PILL_H));
    page_well(t, ui, well);

    if standards {
        t.open_standards(None);
    }
    if let Some(sh) = pick {
        t.shell.state.sheet = sh;
    }
    if fit {
        t.fit_sheet();
    }
    if out {
        t.zoom_by(ZOOM_STEP);
    }
    if zin {
        t.zoom_by(1.0 / ZOOM_STEP);
    }
}

/// One item of the well's page strip.
enum Item {
    Page(PageId, String),
    /// A folded group: its index, name and page count.
    Folded(usize, String, usize),
    /// The open group's folder mark.
    Folder(String),
}

/// The width of the page shown's number, total and title at the well's centre.
const CENTRE_TEXT: f32 = 250.0;

/// The page well in `well`. Carl's rev 3 round 2: the page shown, `02 / 09 Motor control`, sits at
/// the well's centre between `« ‹` and `› »`, with a divider either side. Left of it, in book
/// order, the groups before the shown one (folded) and the shown group's own pages, packed towards
/// the divider; right of it the groups after (folded, or opened with ›) and +.
fn page_well(t: &mut Trial, ui: &mut egui::Ui, well: Rect) {
    let s = ecad_ui::SHELL;
    ui.painter().rect(
        well,
        ecad_ui::style::corner(ecad_ui::tokens::radius::BUTTON),
        ecad_ui::color(s.bg).gamma_multiply(0.8),
        egui::Stroke::new(1.0, ecad_ui::color(s.line)),
        egui::StrokeKind::Inside,
    );
    let w = ui.interact(well, egui::Id::unique(PAGE_WELL), egui::Sense::hover());
    w.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Group, true, PAGE_WELL));
    let y = well.center().y;
    let btn = |x: f32| Rect::from_min_size(pos2(x, y - BTN / 2.0), vec2(BTN, BTN));
    let has_pages = !t.shell.well.groups.is_empty();
    let mut go: Option<PageId> = None;
    let mut wheel: Option<(PageId, egui::Pos2)> = None;
    // (group, next)
    let mut step: Option<(bool, bool)> = None;
    let mut open_group = None;
    let mut add = false;

    // The centre: « ‹ [page shown] › », the page text centred on the well.
    let text = Rect::from_center_size(well.center(), vec2(CENTRE_TEXT.min(well.width() / 3.0), BTN));
    let mut x = text.left() - space::XS - 2.0 * BTN;
    let left_div = x - space::S;
    if bar::icon_button(ui, btn(x), Icon::ChevronsLeft, PREV_GROUP, 12.0, has_pages).clicked() {
        step = Some((true, false));
    }
    x += BTN;
    if bar::icon_button(ui, btn(x), Icon::ChevronLeft, PREV_PAGE, 12.0, has_pages).clicked() {
        step = Some((false, false));
    }
    let mut x = text.right() + space::XS;
    if bar::icon_button(ui, btn(x), Icon::ChevronRight, NEXT_PAGE, 12.0, has_pages).clicked() {
        step = Some((false, true));
    }
    x += BTN;
    if bar::icon_button(ui, btn(x), Icon::ChevronsRight, NEXT_GROUP, 12.0, has_pages).clicked() {
        step = Some((true, true));
    }
    let right_div = x + BTN + space::S;
    bar::vline(ui, left_div, y - 8.0, y + 8.0);
    bar::vline(ui, right_div, y - 8.0, y + 8.0);

    // The page shown: number, "/ total" and title, as one centred line.
    let total = t.panel.rows().len();
    let active = t.panel.active.and_then(|a| t.panel.rows().iter().find(|r| r.id == a)).map(|r| (r.number.clone(), r.title.clone()));
    match &active {
        Some((number, title)) => {
            let number = padded(number);
            let of = format!("/ {total:02}");
            let fixed = bar::text_width(ui, &number, ui_type::BODY_EMPHASIS) + space::XS + bar::text_width(ui, &of, ui_type::SMALL) + space::M;
            let title = clip(ui, title, (text.width() - fixed).max(0.0));
            let line = fixed + bar::text_width(ui, &title, ui_type::BODY);
            let x0 = text.center().x - line / 2.0;
            let e = bar::text_at(ui, x0, y, &number, ui_type::BODY_EMPHASIS, ecad_ui::color(s.text));
            let e = bar::text_at(ui, e + space::XS, y, &of, ui_type::SMALL, ecad_ui::color(s.text_3));
            bar::text_at(ui, e + space::M, y, &title, ui_type::BODY, ecad_ui::color(s.text_2));
        }
        None => {
            let words = if t.project_open() { "No page shown" } else { "No project" };
            ui.painter().text(text.center(), egui::Align2::CENTER_CENTER, words, bar::font(ui, ui_type::SMALL), ecad_ui::color(s.text_3));
        }
    }

    // The items either side: up to the shown group on the left, after it on the right.
    let at = t.well_group_of_active();
    let several = t.shell.well.groups.len() > 1;
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for (gi, g) in t.shell.well.groups.iter().enumerate() {
        let open = Some(gi) == at || t.shell.well.opened.contains(&g.folder) || !several;
        let side = if at.is_some_and(|a| gi > a) { &mut right } else { &mut left };
        if open {
            if g.folder.is_some() || several {
                side.push(Item::Folder(g.name.clone()));
            }
            side.extend(g.pages.iter().map(|(p, n)| Item::Page(*p, n.clone())));
        } else {
            side.push(Item::Folded(gi, g.name.clone(), g.pages.len()));
        }
    }
    // Left: what fits, kept round the page shown, packed towards the divider.
    let room_l = (left_div - space::S) - (well.left() + space::S);
    let shown_at = left.iter().position(|i| matches!(i, Item::Page(p, _) if Some(*p) == t.panel.active)).unwrap_or(left.len().saturating_sub(1));
    let (lo, hi, used) = window(&left, shown_at, room_l);
    let mut x = left_div - space::S - used;
    for i in left.get(lo..hi).unwrap_or_default() {
        draw_item(t, ui, i, x, y, &mut go, &mut wheel, &mut open_group);
        x += item_w(i);
    }
    // Right: the first items that fit, then +.
    let plus = t.project_open();
    let room_r = (well.right() - space::S) - (right_div + space::S) - if plus { BTN + space::XS } else { 0.0 };
    let (_, hi, _) = window(&right, 0, room_r);
    let mut x = right_div + space::S;
    for i in right.get(..hi).unwrap_or_default() {
        draw_item(t, ui, i, x, y, &mut go, &mut wheel, &mut open_group);
        x += item_w(i);
    }
    if plus && bar::icon_button(ui, btn(x + space::XS), Icon::Plus, ADD_PAGE, 12.0, true).clicked() {
        add = true;
    }

    if let Some(p) = go {
        t.well_show(p);
    }
    if let Some((p, at)) = wheel {
        // The same wheel a page row in Book gets (brief 041: the page row's slots).
        t.open_page_wheel(p, at);
    }
    match step {
        Some((true, next)) => t.step_group(next),
        Some((false, next)) => t.step_page(next),
        None => {}
    }
    if let Some(gi) = open_group
        && let Some(g) = t.shell.well.groups.get(gi)
    {
        let folder = g.folder;
        t.shell.well.opened.insert(folder);
    }
    if add {
        t.panel_action(crate::project_panel::Action::New);
    }
}

/// An item's width in the well.
fn item_w(i: &Item) -> f32 {
    match i {
        Item::Page(..) => PAGE_BTN + space::XXS,
        Item::Folder(_) => 24.0,
        // A folded group's chip, then a gap (the board's).
        Item::Folded(_, _, n) => 44.0 + 7.0 * n.to_string().len() as f32 + space::M,
    }
}

/// The run of `items` that fits in `room`, grown from `around` both ways: `(lo, hi, width)`.
fn window(items: &[Item], around: usize, room: f32) -> (usize, usize, f32) {
    let (mut lo, mut hi) = (around.min(items.len()), around.min(items.len()));
    let mut used = 0.0;
    loop {
        let mut grew = false;
        if let Some(i) = items.get(hi)
            && used + item_w(i) <= room
        {
            used += item_w(i);
            hi += 1;
            grew = true;
        }
        if lo > 0
            && let Some(i) = items.get(lo - 1)
            && used + item_w(i) <= room
        {
            used += item_w(i);
            lo -= 1;
            grew = true;
        }
        if !grew {
            return (lo, hi, used);
        }
    }
}

/// Draw one well item at `x`; a click on a page shows it, a right-click opens its wheel, a click on
/// a folded group opens it.
#[allow(clippy::too_many_arguments)]
fn draw_item(
    t: &Trial, ui: &mut egui::Ui, i: &Item, x: f32, y: f32, go: &mut Option<PageId>, wheel: &mut Option<(PageId, egui::Pos2)>,
    open_group: &mut Option<usize>,
) {
    let s = ecad_ui::SHELL;
    match i {
        Item::Folder(name) => {
            let r = Rect::from_center_size(pos2(x + 10.0, y), vec2(14.0, 14.0));
            ecad_ui::icons::paint(ui.painter(), r, Icon::Folder, ecad_ui::color(s.text_3));
            ui.interact(r, egui::Id::unique(("ecad.well.folder", name)), egui::Sense::hover()).on_hover_text(name);
        }
        Item::Page(p, n) => {
            let r = Rect::from_min_size(pos2(x, y - BTN / 2.0), vec2(PAGE_BTN, BTN));
            let on = Some(*p) == t.panel.active;
            let resp = bar::text_button_small(ui, r, &padded(n), &format!("Page {n}"), on);
            if resp.clicked() {
                *go = Some(*p);
            }
            if resp.secondary_clicked() {
                *wheel = Some((*p, resp.interact_pointer_pos().unwrap_or(r.center())));
            }
        }
        Item::Folded(gi, name, count) => {
            let r = Rect::from_min_size(pos2(x + space::XS, y - BTN / 2.0), vec2(item_w(i) - space::M, BTN));
            ui.painter().rect_stroke(r, ecad_ui::tokens::radius::FIELD, egui::Stroke::new(1.0, ecad_ui::color(s.line)), egui::StrokeKind::Inside);
            let resp = bar::hit(ui, r, &format!("Group {name}"), false, false, true).on_hover_text(name);
            let icon = Rect::from_center_size(pos2(r.left() + 12.0, y), vec2(12.0, 12.0));
            ecad_ui::icons::paint(ui.painter(), icon, Icon::Folder, ecad_ui::color(s.text_3));
            let e = bar::text_at(ui, icon.right() + space::XS, y, &count.to_string(), ui_type::SMALL, ecad_ui::color(s.text_2));
            ecad_ui::icons::paint(
                ui.painter(),
                Rect::from_center_size(pos2(e + 6.0, y), vec2(8.0, 8.0)),
                Icon::ChevronRight,
                ecad_ui::color(s.text_3),
            );
            if resp.clicked() {
                *open_group = Some(*gi);
            }
        }
    }
}

/// A page number as the well shows it: a plain number padded to two digits (`2` → `02`), any
/// other number as it is.
fn padded(number: &str) -> String {
    if !number.is_empty() && number.len() < 2 && number.chars().all(|c| c.is_ascii_digit()) { format!("{number:0>2}") } else { number.to_string() }
}

/// `text` cut to `room` px with an ellipsis.
fn clip(ui: &egui::Ui, text: &str, room: f32) -> String {
    if bar::text_width(ui, text, ui_type::BODY) <= room {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if bar::text_width(ui, &format!("{out}…"), ui_type::BODY) > room {
            out.pop();
            break;
        }
    }
    format!("{out}…")
}

impl Trial {
    /// `Standard: <name>` (brief 050a), or [`STANDARD`] with IEC (built in) or no project. One
    /// field read, no copy of the project.
    pub fn standard_words(&mut self) -> String {
        let name = self
            .project
            .clone()
            .and_then(|p| p.read(&mut self.cad.session, |p, _| p.ledger.document().standard.as_ref().map(|s| s.header.name.clone())).ok().flatten());
        name.map_or_else(|| STANDARD.to_string(), |n| format!("Standard: {n}"))
    }

    /// "Saved", "Unsaved changes", "Read-only", or "No project".
    pub fn save_state(&mut self) -> String {
        let state = match self.status() {
            None => "No project".to_string(),
            Some(s) if s.file.read_only.is_some() => "Read-only".into(),
            Some(s) if s.unsaved > 0 => format!("Unsaved changes ({})", s.unsaved),
            Some(s) if s.file.path.is_none() => "Not saved yet".into(),
            Some(_) => "Saved".into(),
        };
        // The time it became saved (this session; local time through chrono).
        if state == "Saved" && self.shell.saved_was.as_deref() != Some("Saved") {
            self.shell.saved_at = Some(chrono::Local::now().format("%H:%M").to_string());
        }
        self.shell.saved_was = Some(state.clone());
        state
    }

    /// The sheet zone (column and row labels) under the pointer on the page shown, if any.
    pub fn zone_under_cursor(&mut self, ctx: &egui::Context) -> Option<String> {
        let at = ctx.pointer_hover_pos().filter(|at| self.over_shown_page(ctx, *at))?;
        let xf = self.cad.canvas.xf?;
        let w = xf.to_world(at);
        let p = self.project.clone()?;
        let page = self.panel.active?;
        p.read(&mut self.cad.session, |p, _| {
            let grid = ecad_model::sheet::Grid::of(p.ledger.document(), page);
            let (c, r) = (grid.column_of(w.x)?, grid.row_of(w.y)?);
            Some(format!("{}-{}", grid.column_label(c), grid.row_label(r)))
        })
        .ok()
        .flatten()
    }

    /// Open or close the Text panel (Ctrl+/), its field taking the keyboard.
    pub fn toggle_text_panel(&mut self) {
        self.shell.state.text_panel = !self.shell.state.text_panel;
        self.developer.open = self.shell.state.text_panel;
        if self.shell.state.text_panel {
            self.focus_command = true;
        }
    }
}
