//! `ecad-trial`: the ECAD desktop trial harness (experiment).
//!
//! Usage: `ecad-trial [--snapshots <dir>] [<project.ecad>]`
//!
//! With a project file, the trial opens it; otherwise it starts a new, unsaved project.
//! `--snapshots <dir>` enables exporting inspector snapshots (a banner button, before a project is
//! discarded, and at exit). Snapshots are diagnostic files, not saved projects.
//! See `ecad/docs/quick-start.md` for building, running and the manual trial.
//!
//! Release builds on Windows open no console window beside the trial, as CADCraft's desktop app;
//! their `--help`, `--version` and usage errors are then not shown (use a debug build).
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use cadcraft_ui_egui::switch::Switch;
use ecad_trial::{Invocation, TITLE, Trial, USAGE, parse_args};
use eframe::egui;

struct App(Trial);

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.0.cad.raw_input_hook(ctx, raw);
        cadcraft_ui_egui::cmdline::capture_tab(ctx, raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }
    fn on_exit(&mut self) {
        self.0.on_exit();
    }
    fn persist_egui_memory(&self) -> bool {
        false
    }
}

/// Stencil bits of the window's render pass, as in CADCraft's desktop app (the canvas needs them).
const STENCIL_BITS: u8 = 8;

fn main() -> eframe::Result {
    let (snapshots, project) = match parse_args(std::env::args_os().skip(1)) {
        Ok(Invocation::Run { snapshots, project }) => (snapshots, project),
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
    let surface = cadcraft_ui_egui::gpu::surface_config(
        Switch::parse("CADCRAFT_VSYNC", std::env::var("CADCRAFT_VSYNC").ok().as_deref()),
        cfg!(all(unix, not(target_os = "macos"))),
    );
    let mut options = eframe::NativeOptions {
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration::default().with_surface_config(surface),
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        persist_window: false,
        stencil_buffer: STENCIL_BITS,
        ..Default::default()
    };
    // As in CADCraft's desktop app: large command previews outgrow wgpu's default buffer cap.
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
        let base = std::sync::Arc::clone(&create.device_descriptor);
        create.device_descriptor = std::sync::Arc::new(move |adapter| {
            let mut d = base(adapter);
            d.required_limits.max_buffer_size = d.required_limits.max_buffer_size.max(adapter.limits().max_buffer_size);
            d
        });
    }
    eframe::run_native(
        TITLE,
        options,
        Box::new(move |cc| {
            let mut trial = match &project {
                Some(p) => Trial::with_file(snapshots, p)?,
                None => Trial::new(snapshots)?,
            };
            if let Some(rs) = &cc.wgpu_render_state {
                trial.cad.set_wgpu(rs, eframe::egui_wgpu::depth_format_from_bits(0, STENCIL_BITS));
            }
            Ok(Box::new(App(trial)))
        }),
    )
}
