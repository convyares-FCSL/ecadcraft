//! `ecad-trial`: the ECAD desktop trial harness (experiment).
//!
//! Usage: `ecad-trial [--snapshots <dir>] [<project.ecad>]`
//!
//! With a project file, the trial opens it; otherwise it starts a new, unsaved project.
//! `--snapshots <dir>` enables exporting inspector snapshots (a banner button, before a project is
//! discarded, and at exit). Snapshots are diagnostic files, not saved projects.
//! See `ecad/docs/quick-start.md` for building, running and the manual trial.
//!
//! `ECAD_STARTUP_TIMING=1` prints start-up timing marks on stderr and writes them to
//! `ecad-startup-<time>.json` in the snapshots folder (or the temp folder); see [`ecad_trial::startup`].
//!
//! Release builds on Windows open no console window beside the trial, as CADCraft's desktop app;
//! their `--help`, `--version` and usage errors are then not shown (use a debug build).
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use cadcraft_ui_egui::switch::Switch;
use ecad_trial::startup::{Adapter, StartupTiming, Step, preferred_folder};
use ecad_trial::window::{self, STENCIL_BITS};
use ecad_trial::{Invocation, TITLE, Trial, USAGE, parse_args};
use eframe::egui;

struct App(Trial, Option<Timing>);

/// Start-up timing still running (only with `ECAD_STARTUP_TIMING` on).
struct Timing {
    marks: StartupTiming,
    /// Where the JSON should go (the snapshots folder); the temp folder otherwise.
    dir: Option<std::path::PathBuf>,
    /// Whether the first full `ui` pass has been marked.
    built: bool,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // Called at the start of every frame, logic-only ones (minimised or occluded) included. A
        // full `ui` pass is painted and presented in the same frame, so the first call after it
        // follows the first present.
        if let Some(t) = &mut self.1
            && t.built
        {
            t.marks.mark(Step::FirstPresent);
            t.marks.finish(t.dir.as_deref());
            self.1 = None;
        }
        self.0.cad.raw_input_hook(ctx, raw);
        cadcraft_ui_egui::cmdline::capture_tab(ctx, raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
        if let Some(t) = &mut self.1
            && !t.built
        {
            t.built = true;
            t.marks.mark(Step::FirstUpdate);
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // Brief 038: the shell state and page views, only when they changed (eframe calls this
        // every `auto_save_interval` and on exit).
        let (shell, views) = self.0.shell.to_save();
        if let Some(s) = shell {
            storage.set_string(ecad_trial::shell::state::SHELL_KEY, s);
        }
        if let Some(v) = views {
            storage.set_string(ecad_trial::shell::state::VIEWS_KEY, v);
        }
    }
    fn auto_save_interval(&self) -> std::time::Duration {
        // Saved on change, debounced to once a second (design 038 §5.4).
        std::time::Duration::from_secs(1)
    }
    fn on_exit(&mut self) {
        // Closed before the first present: keep the marks reached so far.
        if let Some(t) = self.1.take() {
            t.marks.finish(t.dir.as_deref());
        }
        self.0.on_exit();
    }
    fn persist_egui_memory(&self) -> bool {
        false
    }
}

fn main() -> eframe::Result {
    let mut timing = StartupTiming::from_env();
    let (snapshots, project, control, cad_ui) = match parse_args(std::env::args_os().skip(1)) {
        Ok(Invocation::Run { snapshots, project, control, cad_ui }) => (snapshots, project, control, cad_ui),
        Ok(Invocation::Help) => {
            println!("{USAGE}");
            return Ok(());
        }
        Ok(Invocation::Version) => {
            println!("ecad-trial {} (CADCraft {})", env!("CARGO_PKG_VERSION"), ecad_cadcraft::UPSTREAM_COMMIT);
            return Ok(());
        }
        Err(e) => {
            eprintln!("{e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    // Before the engine's command list is first built.
    ecad_cadcraft::install();
    // Brief 024: Print Book plots through CADCraft's PDF plotter.
    ecad_cadcraft::printbook::install_io();
    if let Some(t) = &mut timing {
        t.mark(Step::Install);
    }
    let surface = cadcraft_ui_egui::gpu::surface_config(
        Switch::parse("CADCRAFT_VSYNC", std::env::var("CADCRAFT_VSYNC").ok().as_deref()),
        cfg!(all(unix, not(target_os = "macos"))),
    );
    // The window's size and position are remembered between sessions (brief 023).
    let mut options = window::options(window::restores_window());
    options.wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default().with_surface_config(surface);
    // As in CADCraft's desktop app: large command previews outgrow wgpu's default buffer cap.
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
        let base = std::sync::Arc::clone(&create.device_descriptor);
        create.device_descriptor = std::sync::Arc::new(move |adapter| {
            let mut d = base(adapter);
            d.required_limits.max_buffer_size = d.required_limits.max_buffer_size.max(adapter.limits().max_buffer_size);
            d
        });
    }
    if let Some(t) = &mut timing {
        t.mark(Step::RunNative);
    }
    eframe::run_native(
        TITLE,
        options,
        Box::new(move |cc| {
            // The snapshots folder, if any (only looked at when timing).
            let snapshots_for_timing = timing.as_ref().and(snapshots.clone());
            if let Some(t) = &mut timing {
                t.mark(Step::Creator);
                if let Some(rs) = &cc.wgpu_render_state {
                    let info = rs.adapter.get_info();
                    t.set_adapter(Adapter {
                        backend: format!("{:?}", info.backend),
                        name: info.name,
                        device_type: format!("{:?}", info.device_type),
                    });
                }
            }
            let mut trial = Trial::start(snapshots, project.as_deref(), timing.as_mut())?;
            // Brief 038: the shell's remembered state and page views; `--cad-ui` turns the
            // developer switch on.
            trial.shell.state.cad_chrome = cad_ui;
            let stored = |k: &str| cc.storage.and_then(|s| s.get_string(k));
            trial.shell.load(stored(ecad_trial::shell::state::SHELL_KEY).as_deref(), stored(ecad_trial::shell::state::VIEWS_KEY).as_deref());
            // Brief 035: agent access on for this run (not remembered).
            if let Some(port) = control {
                trial.agent.control_flag(port);
            }
            if let Some(rs) = &cc.wgpu_render_state {
                trial.cad.set_wgpu(rs, eframe::egui_wgpu::depth_format_from_bits(0, STENCIL_BITS));
            }
            if let Some(t) = &mut timing {
                t.mark(Step::SetWgpu);
            }
            let timing =
                timing.map(|marks| Timing { marks, dir: preferred_folder(snapshots_for_timing.as_deref(), trial.home.as_deref()), built: false });
            Ok(Box::new(App(trial, timing)))
        }),
    )
}
