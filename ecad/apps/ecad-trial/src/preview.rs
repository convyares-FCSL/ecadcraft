//! The read-only **preview** window (brief 016, design 016 §5): another page of the project,
//! drawn from the model, floating over the drawing area, for checking the other end of a cable
//! reference without leaving the active page.
//!
//! It is drawn when it opens and again only on a resize, a zoom or pan inside it, or a change that
//! could touch its page (the project's change log, [`ecad_cadcraft::Project::changes_since`]); it
//! is never redrawn per frame. The project is held only while the page is projected; the display
//! list is built and rasterised with the project released. It records nothing and never touches
//! the active page's selection, view or history.
//!
//! Excluded: terminal and wire-end marks, selection, grips and any editing.

use std::sync::Arc;

use cadcraft_engine::doc::Drawing;
use cadcraft_ui_egui::icons::{self, Icon};
use cadcraft_ui_egui::theme::Tokens;
use ecad_cadcraft::ProjectHandle;
use ecad_cadcraft::preview::{PREVIEW_SPAN_MM, PreviewView, preview_drawing, rasterise};
use ecad_model::{PageId, Pt};
use eframe::egui::{self, Color32, Rect, RichText, Sense, Stroke, pos2, vec2};

/// The preview window's egui id (and element name, design 016 §8).
pub const WINDOW_ID: &str = "ecad.preview";
/// Accessible label of Open page.
pub const OPEN_PAGE: &str = "Open page";
/// Accessible label of the close button.
pub const CLOSE: &str = "Close preview";
/// Default size of the image (design 016 §5.1).
pub const DEFAULT_SIZE: [f32; 2] = [480.0, 320.0];
/// Ring diameter in pixels (`ecad.preview-ring`).
pub const RING_PX: f32 = 20.0;

/// What the preview asked for this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Make the previewed page active with the preview's view.
    Open(PageId, PreviewView),
    Close,
}

/// The preview of one page.
pub struct Preview {
    pub page: PageId,
    /// `page 2: Field`, kept up to date with renames and moves.
    pub title: String,
    /// Where it looks.
    pub view: PreviewView,
    /// The other end of the reference it was opened on.
    pub ring: Option<Pt>,
    /// The projected page, kept so a zoom or pan does not project it again.
    drawing: Option<Arc<Drawing>>,
    /// The project change count the projection was made at.
    projected_at: Option<u64>,
    /// What the texture shows: image size and view.
    drawn: Option<([u32; 2], PreviewView)>,
    texture: Option<egui::TextureHandle>,
    /// Image size wanted now (the window's image area, in pixels).
    size: [u32; 2],
    /// Where the image was shown on the last frame.
    pub image_rect: Option<Rect>,
    /// Counters for tests: projections and rasterisations so far.
    pub projections: u64,
    pub draws: u64,
    /// The last error (shown in the window), for example a failed projection.
    pub error: Option<String>,
    /// Position the window opens at (the top-right of the drawing area).
    open_at: Option<egui::Pos2>,
    /// Brief 019: a page of an issued revision opened read-only, and its name (`Rev A`): drawn
    /// once from that frozen copy, never from the live project.
    pub revision: Option<(Arc<ecad_model::Document>, String)>,
}

impl Preview {
    /// A preview of `page`, looking at `view` (`None`: the page's extents), with a ring on `ring`.
    pub fn new(page: PageId, view: Option<PreviewView>, ring: Option<Pt>) -> Self {
        Preview {
            page,
            title: String::new(),
            view: view.unwrap_or(PreviewView { center: Pt::new(0.0, 0.0), height: 0.0 }),
            ring,
            drawing: None,
            projected_at: None,
            drawn: None,
            texture: None,
            size: [DEFAULT_SIZE[0] as u32, DEFAULT_SIZE[1] as u32],
            image_rect: None,
            projections: 0,
            draws: 0,
            error: None,
            open_at: None,
            revision: None,
        }
    }

    /// A preview centred on the other end of a reference, `ecad.preview-span` across.
    pub fn at_reference(page: PageId, at: Pt) -> Self {
        let [w, h] = DEFAULT_SIZE;
        let view = PreviewView::around(at, PREVIEW_SPAN_MM, w as u32, h as u32);
        Preview::new(page, Some(view), Some(at))
    }

    /// Where to open the window: the top-right corner of `area`, 12 px in.
    pub fn place(&mut self, area: Rect) {
        self.open_at = Some(pos2(area.right() - 12.0 - DEFAULT_SIZE[0] - 8.0, area.top() + 12.0));
    }

    /// Bring the projection up to date (only when a change could have touched the page). Returns
    /// `false` when the page no longer exists.
    pub fn update(&mut self, project: &ProjectHandle, s: &mut cadcraft_engine::Session) -> bool {
        let page = self.page;
        if let Some((doc, name)) = self.revision.clone() {
            // A frozen copy: projected once.
            if self.drawing.is_none() && self.error.is_none() {
                let tol = project.read(s, |p, _| p.tolerance).unwrap_or(1e-6);
                self.title = format!(
                    "{name} · page {}: {}",
                    ecad_cadcraft::references::page_number(&doc, page),
                    doc.pages.get(&page).map(|x| x.title.as_str()).unwrap_or_default()
                );
                match ecad_cadcraft::preview::preview_drawing_of(&doc, page, tol) {
                    Ok(d) => {
                        if self.view.height <= 0.0 {
                            self.view = PreviewView::extents(&d, self.size[0], self.size[1]);
                        }
                        self.drawing = Some(Arc::new(d));
                        self.projections += 1;
                        self.drawn = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            return true;
        }
        let since = self.projected_at;
        // One short read: the change count, whether anything since touches this page, and (only
        // then) the projection and the title.
        let read = project.read(s, |p, _| {
            let count = p.change_count();
            let doc = p.ledger.document();
            if !doc.pages.contains_key(&page) {
                return None;
            }
            let title = format!(
                "page {}: {}",
                ecad_cadcraft::references::page_number(doc, page),
                doc.pages.get(&page).map(|x| x.title.as_str()).unwrap_or_default()
            );
            let stale = match since {
                None => true,
                Some(n) => p.changes_since(n).is_none_or(|t| t.contains(&page)),
            };
            let drawing = stale.then(|| preview_drawing(p, page));
            Some((count, title, drawing))
        });
        let Ok(read) = read else { return true }; // busy: try again next frame
        let Some((count, title, drawing)) = read else { return false };
        self.title = title;
        match drawing {
            Some(Ok(d)) => {
                if self.view.height <= 0.0 {
                    self.view = PreviewView::extents(&d, self.size[0], self.size[1]);
                }
                self.drawing = Some(Arc::new(d));
                self.projections += 1;
                self.drawn = None;
                self.error = None;
            }
            Some(Err(e)) => self.error = Some(e),
            None => {}
        }
        self.projected_at = Some(count);
        true
    }

    /// Rasterise again if the size or view changed or the page was projected again.
    fn redraw(&mut self, ctx: &egui::Context) {
        let Some(d) = &self.drawing else { return };
        if self.drawn == Some((self.size, self.view)) && self.texture.is_some() {
            return;
        }
        let t = Tokens::get();
        let bg = [t.canvas.r(), t.canvas.g(), t.canvas.b()];
        match rasterise(d, self.view, self.size[0], self.size[1], bg, 96.0 / 25.4) {
            Ok(img) => {
                let image =
                    egui::ColorImage::from_rgba_premultiplied([img.width as usize, img.height as usize], &img.rgba);
                match &mut self.texture {
                    Some(tex) => tex.set(image, egui::TextureOptions::LINEAR),
                    None => {
                        self.texture = Some(ctx.load_texture(WINDOW_ID, image, egui::TextureOptions::LINEAR));
                    }
                }
                self.draws += 1;
                self.drawn = Some((self.size, self.view));
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Screen position of drawing point `at` in the image at `rect`.
    pub fn to_screen(&self, rect: Rect, at: Pt) -> egui::Pos2 {
        let k = (rect.height() as f64 / self.view.height.max(1e-9)) as f32;
        let dx = (at.x - self.view.center.x) as f32 * k;
        let dy = (at.y - self.view.center.y) as f32 * k;
        pos2(rect.center().x + dx, rect.center().y - dy)
    }

    /// Show the window. Returns what the user asked for.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Action> {
        let t = Tokens::get();
        let mut action = None;
        let mut window = egui::Window::new("Preview")
            .id(egui::Id::unique(WINDOW_ID))
            .title_bar(false)
            .resizable(true)
            .collapsible(false)
            .default_size(vec2(DEFAULT_SIZE[0], DEFAULT_SIZE[1] + 22.0))
            .min_size(vec2(240.0, 160.0))
            .frame(egui::Frame::NONE.fill(t.chrome_dark).stroke(Stroke::new(1.0, t.border)).corner_radius(0.0));
        if let Some(at) = self.open_at.take() {
            window = window.current_pos(at);
        }
        let max = ctx.content_rect().size() * 0.8;
        window = window.max_size(max);
        window.show(ctx, |ui| {
            // Title bar: lock, PREVIEW, the page, read-only, Open page, close.
            ui.horizontal(|ui| {
                ui.set_height(22.0);
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                icons::paint(ui.painter(), r, Icon::Lock, true);
                ui.label(RichText::new("PREVIEW").small().strong().color(t.text_dim));
                ui.label(RichText::new(&self.title).small().color(t.text));
                ui.label(RichText::new("read-only").small().color(t.text_dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let close = ui.small_button("×").on_hover_text(CLOSE);
                    close.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, CLOSE));
                    if close.clicked() {
                        action = Some(Action::Close);
                    }
                    // A revision's page is read-only: it cannot become the active page.
                    if self.revision.is_none() {
                        let open = ui.small_button(OPEN_PAGE).on_hover_text("Make this page the active page");
                        if open.clicked() {
                            action = Some(Action::Open(self.page, self.view));
                        }
                    }
                });
            });
            // The image fills the rest.
            let avail = ui.available_size().max(vec2(16.0, 16.0));
            let ppp = ctx.pixels_per_point();
            self.size =
                [((avail.x * ppp).round() as u32).clamp(16, 4096), ((avail.y * ppp).round() as u32).clamp(16, 4096)];
            let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
            self.redraw(ctx);
            if let Some(tex) = &self.texture {
                ui.painter().image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            } else {
                ui.painter().rect_filled(rect, 0.0, t.canvas);
            }
            if let Some(e) = &self.error {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    e,
                    egui::FontId::proportional(12.0),
                    t.warn,
                );
            }
            if let Some(at) = self.ring {
                let c = self.to_screen(rect, at);
                if rect.contains(c) {
                    ui.painter().circle_stroke(c, RING_PX / 2.0, Stroke::new(2.0, REFERENCE_MAGENTA));
                }
            }
            self.image_rect = Some(rect);
            let resp = resp.on_hover_cursor(egui::CursorIcon::Default);
            // Pan by dragging, zoom about the pointer with the wheel; Esc over it closes it.
            let k = self.view.height / f64::from(rect.height().max(1.0));
            if resp.dragged() {
                let d = resp.drag_delta();
                self.view.center =
                    Pt::new(self.view.center.x - f64::from(d.x) * k, self.view.center.y + f64::from(d.y) * k);
            }
            if resp.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0
                    && let Some(p) = resp.hover_pos()
                {
                    let factor = f64::from((scroll / 200.0).exp());
                    let about = Pt::new(
                        self.view.center.x + f64::from(p.x - rect.center().x) * k,
                        self.view.center.y - f64::from(p.y - rect.center().y) * k,
                    );
                    let h = (self.view.height / factor).clamp(1e-3, 1e9);
                    let s = h / self.view.height;
                    self.view = PreviewView {
                        center: Pt::new(
                            about.x + (self.view.center.x - about.x) * s,
                            about.y + (self.view.center.y - about.y) * s,
                        ),
                        height: h,
                    };
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    action = Some(Action::Close);
                }
            }
            if resp.double_clicked() {
                action = Some(Action::Open(self.page, self.view));
            }
        });
        action
    }
}

/// `ecad.reference` magenta.
const REFERENCE_MAGENTA: Color32 = Color32::from_rgb(0xff, 0x00, 0xff);
