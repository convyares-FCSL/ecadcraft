//! The hover tooltip naming the ECAD object under the pointer (design 008 §4.3).

use crate::*;

impl App {
    /// The terminal within the capture aperture (APERTURE pixels) of screen point `at` on the
    /// drawing area, with its hover description; none while a paper-space layout is shown. Only
    /// reads.
    /// The cable reference end under the pointer (its point within the pick box, or its text),
    /// with its hover lines; model space only, as for terminals.
    fn reference_under(&mut self, at: egui::Pos2) -> Option<(ecad_model::RepresentationId, Vec<String>)> {
        if self.cad.session.layout_space() != ecad_engine::cad::Space::Model {
            return None;
        }
        let (rect, xf) = (self.cad.canvas.rect?, self.cad.canvas.xf?);
        if !rect.contains(at) {
            return None;
        }
        let world = xf.to_world(at);
        let radius = self.cad.session.pixel_size() * self.cad.session.settings.pickbox.max(1.0) * 1.5;
        let uid = self.cad.session.state().ok()?.uid;
        let p = self.project.as_ref()?;
        p.read(&mut self.cad.session, |p, _| ecad_engine::interact::hover_reference(p, uid, world, radius)).ok().flatten()
    }

    fn connection_point_under(&mut self, at: egui::Pos2) -> Option<(ecad_model::ConnectionPointId, Vec<String>)> {
        // Terminals are at model positions: in a paper-space layout (or one of its viewports) the
        // canvas shows the sheet, and nothing on it is a terminal.
        if self.cad.session.layout_space() != ecad_engine::cad::Space::Model {
            return None;
        }
        let (rect, xf) = (self.cad.canvas.rect?, self.cad.canvas.xf?);
        if !rect.contains(at) {
            return None;
        }
        let world = xf.to_world(at);
        let aperture = self.cad.session.pixel_size() * self.cad.session.settings.aperture.max(1.0);
        let uid = self.cad.session.state().ok()?.uid;
        let p = self.project.as_ref()?;
        p.read(&mut self.cad.session, |p, _| ecad_engine::interact::hover_connection_point(p, uid, world, aperture)).ok().flatten()
    }

    /// The hover description shown now, if any (design 008 §4.3).
    pub fn hover_lines(&self) -> Option<&[String]> {
        self.hover.as_deref()
    }

    /// With no command running, the ECAD object CADCraft highlights under the pointer gets a
    /// tooltip after [`App::hover_delay`]: its name or ID and its connections. It goes when the
    /// pointer leaves the object, a button is pressed or a command starts. Only reads.
    pub(crate) fn hover_tooltip(&mut self, ctx: &egui::Context) {
        let (now, pointer, pressed) = ctx.input(|i| (i.time, i.pointer.hover_pos(), i.pointer.any_down()));
        // Brief 038: only over the page shown, not over a hosted view laid over the hidden page.
        let pointer = pointer.filter(|at| self.over_shown_page(ctx, *at));
        let idle = self.cad.session.running.is_none() && !pressed;
        // A terminal under the pointer wins over the wire ending on it or the symbol around it.
        let connection_point = match pointer.filter(|_| idle) {
            Some(at) => self.connection_point_under(at),
            None => None,
        };
        // Then a cable reference end, which wins over the wire starting on its point (brief 010).
        let reference = match (pointer.filter(|_| idle), &connection_point) {
            (Some(at), None) => self.reference_under(at),
            _ => None,
        };
        let target = match (connection_point, reference) {
            (Some((t, lines)), _) => Some((Hovered::ConnectionPoint(t), Some(lines))),
            (None, Some((r, lines))) => Some((Hovered::Reference(r), Some(lines))),
            (None, None) => self.cad.canvas.hover.filter(|_| idle).map(|h| (Hovered::Object(h), None)),
        };
        let (Some((h, connection_point_lines)), Some(pointer)) = (target, pointer) else {
            self.hovered = None;
            self.hover = None;
            return;
        };
        let since = match self.hovered {
            Some((was, since)) if was == h => since,
            _ => {
                self.hover = None;
                now
            }
        };
        self.hovered = Some((h, since));
        if now - since < self.hover_delay {
            // A hostile delay (NaN, infinite, huge) never panics: it falls back to a short wait.
            let wait = std::time::Duration::try_from_secs_f64((self.hover_delay - (now - since)).clamp(0.01, 3600.0))
                .unwrap_or(std::time::Duration::from_millis(10));
            ctx.request_repaint_after(wait);
            return;
        }
        if self.hover.is_none() {
            self.hover = match h {
                Hovered::ConnectionPoint(_) | Hovered::Reference(_) => connection_point_lines,
                Hovered::Object(h) => {
                    let uid = self.cad.session.state().map(|st| st.uid).ok();
                    match (self.project.as_ref(), uid) {
                        (Some(p), Some(uid)) => p.read(&mut self.cad.session, |p, _| ecad_engine::interact::hover_lines(p, uid, h)).ok().flatten(),
                        _ => None,
                    }
                }
            };
        }
        let Some(lines) = &self.hover else { return };
        // Brief 038: the shell's popover style and text colours.
        let t = shell::left::colours();
        egui::Area::new(egui::Id::unique(HOVER_ID))
            .order(egui::Order::Tooltip)
            .fixed_pos(pointer + egui::Vec2::splat(ecad_widgets::tokens::space::XL))
            .constrain(true)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    for (i, line) in lines.iter().enumerate() {
                        let dim = i > 0 && (line.starts_with("id ") || line.contains("not connected"));
                        let text = RichText::new(line).monospace().small();
                        ui.label(text.color(if dim { t.text_dim } else { t.text }));
                    }
                });
            });
    }
}
