//! `ecad-trial`: the ECAD desktop trial harness (experiment).
//!
//! Usage: `ecad-trial [--snapshots <dir>] [<project.ecad>]`
//!
//! With a project file, the trial opens it; otherwise it starts a new, unsaved project.
//! `--snapshots <dir>` enables exporting inspector snapshots (a banner button, before a project is
//! discarded, and at exit). Snapshots are diagnostic files, not saved projects.
//! See `ecad/docs/quick-start.md` for building, running and the manual trial.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::path::PathBuf;

use cadcraft_ui_egui::switch::Switch;
use ecad_trial::{TITLE, Trial};
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

const USAGE: &str = "usage: ecad-trial [--snapshots <dir>] [<project.ecad>]\n\n\
ECAD desktop trial harness (experiment). Opens <project.ecad> if given, otherwise starts a new\n\
project. Save and Save As write .ecad project files; keep them in your Linux home folder.\n\
--snapshots <dir>  allow exporting inspector snapshots (diagnostic files, not saved projects)\n\
See ecad/docs/quick-start.md.";

/// Stencil bits of the window's render pass, as in CADCraft's desktop app (the canvas needs them).
const STENCIL_BITS: u8 = 8;

fn main() -> eframe::Result {
    let mut snapshots = None;
    let mut project: Option<String> = None;
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
            other if other.starts_with('-') => {
                eprintln!("unknown argument {other:?}\n\n{USAGE}");
                std::process::exit(2);
            }
            path if project.is_none() => project = Some(path.to_string()),
            extra => {
                eprintln!("only one project file can be opened ({extra:?})\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
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
