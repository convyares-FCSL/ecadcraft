//! The trial's main window: size, placement, and remembering them between sessions (brief 023,
//! as CADCraft 0.17.0's desktop app does, #2595 and #1129).
//!
//! eframe saves the window's size, position and maximized state in its storage file (`app.ron`,
//! under [`APP_ID`]) and restores them at the next launch. The first launch opens centred; a later
//! one goes back where the window was (centring would move it). egui's own memory is not kept.
//!
//! This module excludes the wgpu surface and buffer settings, which `main` adds.

use eframe::egui;

/// The id eframe stores the trial's window state under (its own, apart from CADCraft's).
pub const APP_ID: &str = "ecad-trial";

/// The key eframe saves the main window state under in its storage.
pub const WINDOW_KEY: &str = "window";

/// Stencil bits of the window's render pass, as in CADCraft's desktop app (the canvas needs them).
pub const STENCIL_BITS: u8 = 8;

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
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 560.0])
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
    use super::{options, saved_window};

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
