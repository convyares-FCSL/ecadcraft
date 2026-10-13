//! `ecad-trial`: the ECAD desktop trial harness (experiment; no project persistence).
//!
//! Usage: `ecad-trial [--snapshots <dir>]`
//!
//! `--snapshots <dir>` enables exporting inspector snapshots (a banner button, before a project is
//! discarded, and at exit). Snapshots are diagnostic files, not saved projects.
//! See `ecad/apps/ecad-trial/README.md` for the WSL build and the manual trial.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::path::PathBuf;

use ecad_trial::{TITLE, Trial};
use eframe::egui;

struct App(Trial);

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.0.cad.raw_input_hook(raw);
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

const USAGE: &str = "usage: ecad-trial [--snapshots <dir>]\n\n\
ECAD desktop trial harness (experiment). Projects cannot be saved: Save and Save As report that\n\
persistence is unavailable, and closing an edited project discards its design.\n\
--snapshots <dir>  allow exporting inspector snapshots (diagnostic files, not saved projects)";

fn main() -> eframe::Result {
    let mut snapshots = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--snapshots" => match args.next() {
                Some(d) => snapshots = Some(PathBuf::from(d)),
                None => {
                    eprintln!("--snapshots needs a folder\n\n{USAGE}");
                    std::process::exit(2);
                }
            },
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--version" | "-V" => {
                println!("ecad-trial {} (CADCraft {})", env!("CARGO_PKG_VERSION"), ecad_cadcraft::UPSTREAM_COMMIT);
                return Ok(());
            }
            other => {
                eprintln!("unknown argument {other:?}\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    // Before the engine's command list is first built.
    ecad_cadcraft::install();
    let surface = cadcraft_ui_egui::gpu::surface_config(
        std::env::var("CADCRAFT_VSYNC").ok().as_deref(),
        cfg!(all(unix, not(target_os = "macos"))),
    );
    let mut options = eframe::NativeOptions {
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration::default().with_surface_config(surface),
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
        persist_window: false,
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
            let mut trial = Trial::new(snapshots)?;
            if let Some(rs) = &cc.wgpu_render_state {
                trial.cad.set_wgpu(rs);
            }
            Ok(Box::new(App(trial)))
        }),
    )
}
