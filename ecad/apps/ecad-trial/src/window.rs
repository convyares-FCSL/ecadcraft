//! The trial's main window: size, placement, and remembering them between sessions (brief 023,
//! as CADCraft 0.17.0's desktop app does, #2595 and #1129).
//!
//! eframe saves the window's size, position and maximized state in its storage file (`app.ron`,
//! under [`APP_ID`]) and restores them at the next launch. The first launch opens centred; a later
//! one goes back where the window was (centring would move it). egui's own memory is not kept.
//!
//! The first launch opens at [`DEFAULT_SIZE`] and then grows to the largest size that fits its
//! monitor ([`FirstFit`]), so the left panel, the main view and the open right panel all show on a
//! 1920 × 1080 screen. The shell's collapse rule still applies to a smaller window.
//!
//! This module excludes the wgpu surface and buffer settings, which `main` adds.

use eframe::egui;

/// The id eframe stores the trial's window state under (its own, apart from CADCraft's).
pub const APP_ID: &str = "ecad-trial";

/// The key eframe saves the main window state under in its storage.
pub const WINDOW_KEY: &str = "window";

/// Stencil bits of the window's render pass, as in CADCraft's desktop app (the canvas needs them).
pub const STENCIL_BITS: u8 = 8;

/// The first launch's inner size before it is fitted to the monitor (and when the monitor's size
/// is not known): wide enough for the shell's default left panel, tool strip, main view and open
/// right panel (264 + 48 + 760 + 340 = 1412, design 038 §5.2), and inside a 1920 × 1080 screen
/// with its title bar and task bar.
pub const DEFAULT_SIZE: [f32; 2] = [1680.0, 940.0];

/// The smallest inner size the window may be given.
pub const MIN_SIZE: [f32; 2] = [900.0, 560.0];

/// Kept free of the monitor when the first launch fits it: a margin left and right, and room for
/// the title bar and a task bar or menu bar (egui knows the monitor's size, not its work area).
pub const MONITOR_MARGIN: [f32; 2] = [64.0, 120.0];

/// The largest inner size that fits a monitor of `monitor` points, never below [`MIN_SIZE`].
/// `None` for a size that is not finite or positive (unknown).
pub fn fit_to_monitor(monitor: egui::Vec2) -> Option<egui::Vec2> {
    if !(monitor.x.is_finite() && monitor.y.is_finite() && monitor.x > 0.0 && monitor.y > 0.0) {
        return None;
    }
    let w = (monitor.x - MONITOR_MARGIN[0]).max(MIN_SIZE[0]);
    let h = (monitor.y - MONITOR_MARGIN[1]).max(MIN_SIZE[1]);
    Some(egui::vec2(w, h))
}

/// Fits the first launch's window to its monitor, once (a restored window keeps its own size).
///
/// # Semantics
/// On the first frames the monitor's size is known, the window gets [`fit_to_monitor`]'s size and
/// stays centred where it was. If the size is not known within [`FirstFit::FRAMES`] frames, the
/// window keeps [`DEFAULT_SIZE`]. Either way it does nothing after that, so the user's own size is
/// what eframe saves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstFit {
    frames_left: u8,
}

impl FirstFit {
    /// How many frames to wait for the monitor's size.
    pub const FRAMES: u8 = 10;

    /// `restoring`: the last session's window is restored, and nothing is changed.
    pub fn new(restoring: bool) -> Self {
        Self { frames_left: if restoring { 0 } else { Self::FRAMES } }
    }

    /// Whether it may still change the window.
    pub fn pending(&self) -> bool {
        self.frames_left > 0
    }

    /// Call once a frame with the main viewport's info; returns the commands to send (empty when
    /// there is nothing to do).
    pub fn commands(&mut self, info: &egui::ViewportInfo) -> Vec<egui::ViewportCommand> {
        if self.frames_left == 0 {
            return Vec::new();
        }
        self.frames_left -= 1;
        let Some(size) = info.monitor_size.and_then(fit_to_monitor) else { return Vec::new() };
        self.frames_left = 0;
        if info.maximized == Some(true) || info.fullscreen == Some(true) {
            return Vec::new();
        }
        let mut out = vec![egui::ViewportCommand::InnerSize(size)];
        // Keep the centre where eframe put it (on the window's own monitor), counting the frame
        // around the inner rect.
        if let (Some(outer), Some(inner)) = (info.outer_rect, info.inner_rect) {
            let frame = outer.size() - inner.size();
            let new_outer = size + frame.max(egui::Vec2::ZERO);
            let min = outer.center() - new_outer / 2.0;
            out.push(egui::ViewportCommand::OuterPosition(min));
        }
        out
    }
}

/// Whether eframe's storage file (`app.ron`, a map of keys to strings) holds a saved main window
/// state. An unreadable or malformed file counts as none, as it does for eframe.
pub fn saved_window(app_ron: &str) -> bool {
    ron::from_str::<std::collections::HashMap<String, String>>(app_ron).is_ok_and(|kv| kv.contains_key(WINDOW_KEY))
}

/// The window state from the last session is saved and will be restored.
pub fn restores_window() -> bool {
    eframe::storage_dir(APP_ID).and_then(|dir| std::fs::read_to_string(dir.join("app.ron")).ok()).is_some_and(|ron| saved_window(&ron))
}

/// The main window's options. `restoring`: a window state saved by the last session is restored
/// instead of opening centred.
pub fn options(restoring: bool) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(crate::TITLE)
            .with_inner_size(DEFAULT_SIZE)
            .with_min_inner_size(MIN_SIZE)
            .with_app_id(APP_ID),
        // Size, position and maximized state are kept between sessions; eframe keeps a restored
        // window on the monitors connected now.
        persist_window: true,
        // The first launch opens centred on its screen; later launches go back where the window
        // was. Wayland compositors place windows themselves.
        centered: !restoring,
        stencil_buffer: STENCIL_BITS,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_SIZE, FirstFit, MIN_SIZE, fit_to_monitor, options, saved_window};
    use eframe::egui::{Rect, ViewportCommand, ViewportInfo, pos2, vec2};

    /// Left panel, tool strip, main view's minimum and right panel at their defaults (design 038).
    const THREE_REGIONS: f32 = 264.0 + 48.0 + 760.0 + 340.0;

    #[test]
    fn the_default_size_shows_the_right_panel_and_fits_1920_by_1080() {
        const { assert!(DEFAULT_SIZE[0] >= THREE_REGIONS) };
        const { assert!(DEFAULT_SIZE[0] <= 1920.0 && DEFAULT_SIZE[1] + 120.0 <= 1080.0) };
        let fit = fit_to_monitor(vec2(1920.0, 1080.0)).unwrap();
        assert!(fit.x >= THREE_REGIONS && fit.x <= 1920.0 && fit.y <= 1080.0 - 100.0, "{fit:?}");
        assert!(fit.x >= DEFAULT_SIZE[0] && fit.y >= DEFAULT_SIZE[1], "never smaller than the default on 1920 × 1080");
    }

    #[test]
    fn a_small_or_unknown_monitor() {
        assert_eq!(fit_to_monitor(vec2(800.0, 500.0)), Some(vec2(MIN_SIZE[0], MIN_SIZE[1])));
        for bad in [vec2(0.0, 1080.0), vec2(f32::NAN, 1080.0), vec2(1920.0, f32::INFINITY), vec2(-5.0, -5.0)] {
            assert_eq!(fit_to_monitor(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_first_launch_is_fitted_once_and_stays_centred() {
        let mut f = FirstFit::new(false);
        let mut info = ViewportInfo::default();
        // Before the monitor is known: nothing yet.
        assert!(f.commands(&info).is_empty() && f.pending());
        info.monitor_size = Some(vec2(1920.0, 1080.0));
        info.inner_rect = Some(Rect::from_min_size(pos2(121.0, 101.0), vec2(1680.0, 940.0)));
        info.outer_rect = Some(Rect::from_min_size(pos2(120.0, 70.0), vec2(1682.0, 972.0)));
        let c = f.commands(&info);
        let size = fit_to_monitor(vec2(1920.0, 1080.0)).unwrap();
        assert_eq!(c.first(), Some(&ViewportCommand::InnerSize(size)));
        let Some(ViewportCommand::OuterPosition(p)) = c.get(1) else { panic!("{c:?}") };
        let centre = *p + (size + vec2(2.0, 32.0)) / 2.0;
        assert!((centre - pos2(961.0, 556.0)).length() < 0.01, "{centre:?}");
        assert!(!f.pending() && f.commands(&info).is_empty(), "once: the user's own size is kept");
    }

    #[test]
    fn a_restored_or_maximized_window_is_left_alone() {
        let mut info = ViewportInfo { monitor_size: Some(vec2(1920.0, 1080.0)), ..Default::default() };
        assert!(FirstFit::new(true).commands(&info).is_empty());
        info.maximized = Some(true);
        let mut f = FirstFit::new(false);
        assert!(f.commands(&info).is_empty() && !f.pending());
    }

    #[test]
    fn an_unknown_monitor_keeps_the_default_size() {
        let mut f = FirstFit::new(false);
        for _ in 0..FirstFit::FRAMES {
            assert!(f.commands(&ViewportInfo::default()).is_empty());
        }
        assert!(!f.pending());
    }

    #[test]
    fn the_first_launch_opens_centred_and_saves_the_window() {
        let o = options(false);
        assert!(o.centered);
        assert!(o.persist_window);
    }

    #[test]
    fn a_saved_window_reopens_where_it_was() {
        let o = options(true);
        assert!(!o.centered, "centring would move a restored window");
        assert!(o.persist_window);
    }

    #[test]
    fn a_saved_window_is_read_from_app_ron() {
        assert!(saved_window(r#"{"other": "{}", "window": "(maximized: true)"}"#));
        assert!(!saved_window(r#"{"other": "{\"window\":1}"}"#), "a value mentioning it is not the key");
        assert!(!saved_window(""));
        assert!(!saved_window("{\"window\": "), "truncated");
        assert!(!saved_window("not ron at all"));
    }
}
