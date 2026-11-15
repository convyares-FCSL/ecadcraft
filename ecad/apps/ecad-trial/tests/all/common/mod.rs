//! Helpers shared by this crate's tests (brief 062): the kittest harness around a [`Trial`], a
//! temporary folder, and a pointer click.
//!
//! [`harness`] takes [`Opts`]; the defaults are the window most tests used (1920×1200, CADCraft's
//! developer chrome on, a fresh trial with the hover delay off). Tests that need a different shape
//! say so in the options instead of copying the builder.

#![allow(dead_code)]

use std::path::PathBuf;

use ecad_cadcraft::views::Settings;
use ecad_trial::Trial;
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;

use crate::mouse_tester::{Collect, Deltas};

/// Where a harness's trial keeps remembered cable views.
pub(crate) enum Views {
    /// Leave the trial's own setting (the tests isolate it from the developer's folder anyway).
    Untouched,
    /// Remember nothing.
    Nothing,
    /// Remember them in this folder.
    At(PathBuf),
}

/// What [`harness`] builds.
pub(crate) struct Opts {
    size: egui::Vec2,
    chrome: bool,
    views: Views,
    deltas: Option<Deltas>,
    trial: Option<Trial>,
    panels: bool,
}

impl Opts {
    pub(crate) fn new() -> Self {
        Opts { size: egui::vec2(1920.0, 1200.0), chrome: true, views: Views::Untouched, deltas: None, trial: None, panels: false }
    }

    pub(crate) fn size(mut self, w: f32, h: f32) -> Self {
        self.size = egui::vec2(w, h);
        self
    }

    /// CADCraft's Tool Sets, layout tabs and command line sit behind the shell's developer switch
    /// (brief 038); on by default.
    pub(crate) fn chrome(mut self, on: bool) -> Self {
        self.chrome = on;
        self
    }

    pub(crate) fn views_nothing(mut self) -> Self {
        self.views = Views::Nothing;
        self
    }

    pub(crate) fn views_at(mut self, dir: PathBuf) -> Self {
        self.views = Views::At(dir);
        self
    }

    /// Keep every texture delta, so a frame can be rendered later.
    pub(crate) fn deltas(mut self, deltas: Option<Deltas>) -> Self {
        self.deltas = deltas;
        self
    }

    /// Use this trial instead of a fresh one.
    pub(crate) fn trial(mut self, t: Trial) -> Self {
        self.trial = Some(t);
        self
    }

    /// Open both side panels first (`project_panel::open_panels`).
    pub(crate) fn panels(mut self) -> Self {
        self.panels = true;
        self
    }
}

/// A kittest harness around a trial, run for five frames.
pub(crate) fn harness(opts: Opts) -> Harness<'static, Trial> {
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let mut t = match opts.trial {
        Some(t) => t,
        None => {
            let mut t = Trial::new(None).unwrap();
            t.hover_delay = 0.0;
            t
        }
    };
    if opts.panels {
        crate::project_panel::open_panels(&mut t);
    }
    match opts.views {
        Views::Untouched => {}
        Views::Nothing => {
            let p = t.project().unwrap().clone();
            p.configure(&mut t.cad.session, |p| p.settings = Settings::none()).unwrap();
        }
        Views::At(dir) => {
            let p = t.project().unwrap().clone();
            p.configure(&mut t.cad.session, move |p| p.settings = Settings::at(dir)).unwrap();
        }
    }
    let mut b = Harness::builder().with_size(opts.size);
    if let Some(d) = opts.deltas {
        b = b.renderer(Collect(d));
    }
    let mut h = b.build_ui_state(
        |ui, t: &mut Trial| {
            t.logic(&ui.ctx().clone());
            t.ui(ui);
        },
        t,
    );
    if opts.chrome {
        h.state_mut().set_cad_chrome(true);
    }
    h.run_steps(5);
    h
}

/// A temporary folder, removed when dropped.
pub(crate) struct Tmp(pub(crate) PathBuf);

impl Tmp {
    pub(crate) fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-ui-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Move the pointer to `at` and let two frames pass.
pub(crate) fn pointer_to(h: &mut Harness<'static, Trial>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run_steps(2);
}

/// Press and release the primary button at `at`.
pub(crate) fn click(h: &mut Harness<'static, Trial>, at: Pos2) {
    pointer_to(h, at);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
}
