//! ECAD workspace checks.
//!
//! - `layers`: the layer table (Core, Engine, UI kit, App, Programs, Dev): Core
//!   uses no CADCraft crate and no UI toolkit, and Core crates depend only on Core crates before
//!   them; only the Engine (`ecad-engine`) uses the drawing engine, and App and Programs reach it
//!   only through it, in `Cargo.toml` and in their sources.
//!   Developer tools (`ecad/dev/`) are kept apart from product code: no product
//!   crate depends on one except through its own `dev` feature, dev features are read only in a
//!   crate's `dev` module, and no product source reads a file under `dev/`. ECAD's interface kit
//!   (class UiKit) uses egui and the drawing style only, and ECAD UI sources do not
//!   style locally. Core crates other than
//!   `ecad-store` reach no OS (file system, environment, processes, network, clock). The crate
//!   rules: `anyhow` only in Programs, xtask and ecad-bench, `tokio` nowhere, `rayon` only where
//!   the work is. Every workspace member has a README with the standard sections.
//! - `patches`: every file changed outside `ecad/` since the incorporated CADCraft commit
//!   (`ecad/UPSTREAM.md`) must be listed in `ecad/UPSTREAM-PATCHES.md`.
//! - `ci`: fmt, clippy `-D warnings`, tests (the slow set included), layers, patches. ECAD is
//!   desktop-only, so there is no wasm build.
//! - `bench`: the project load benchmark, release build, run on demand only. It is
//!   deliberately not part of `ci`; arguments after `bench` are passed to `ecad-bench`.
//! - `test-changed`: the fast tests of the crates changed since the last push (the upstream
//!   branch; `--base` for another ref), and with `--with-dependents` of every crate that depends
//!   on them. The worker's default check; CI's quick
//!   check runs it with `--base origin/dev --with-dependents` on pull requests into dev.
//! - `clean-builds`: delete the build folders of every checkout under a folder, skipping those in
//!   use, and remove worktrees whose branch is merged into `origin/dev` (see [`clean`]).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde_json::Value;

mod clean;
mod engine_guards;
mod history_guards;
mod pub_ratchet;
mod starter_guards;
mod types_guards;
mod views_guards;

/// The layer of an ECAD crate: a crate uses only crates in layers above it
/// (and in its own layer where the table allows it). Engine and UI kit are siblings: neither uses the
/// other.
/// The rules apply to normal dependencies; a dev-dependency (tests) is checked only for the layer
/// order (below), and is otherwise free, except on Dev crates.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Class {
    /// What a project is and means, without a screen or a drawing engine: no `cadcraft-*` crate,
    /// no UI toolkit or UI kit, no Engine, App or Programs crate, and only Core crates before it
    /// in [`CORE_ORDER`].
    Core,
    /// The link to CADCraft (`ecad-engine`): Core and [`ENGINE_CADCRAFT`]; no UI toolkit or UI
    /// kit, no App or Programs crate.
    Engine,
    /// ECAD's interface kit: may use egui, epaint and
    /// egui_extras and ecad-model (the canvas palette), but no `cadcraft-*`
    /// crate and no other ECAD crate, so the widgets stay swappable and test without wgpu.
    UiKit,
    /// The application (`ecad-app`): Core, Engine, UI kit and CADCraft's window crates, but never
    /// `cadcraft-engine` directly and no Programs crate.
    App,
    /// Things a person or agent starts, one `main` each (`ecad-desktop`, `ecad-mcp`):
    /// anything in the product (other programs too), but never `cadcraft-engine`
    /// directly.
    Programs,
    /// A developer or test tool under `ecad/dev/`, never shipped: may use anything, and no
    /// product crate may depend on it except through its own `dev` feature.
    Dev,
    /// `xtask`: a developer tool under `dev/` (counted in Dev), exempt from the source guards; no rules.
    Exempt,
}

const TABLE: &[(&str, Class)] = &[
    ("ecad-model", Class::Core),
    ("ecad-store", Class::Core),
    ("ecad-file", Class::Core),
    ("ecad-engine", Class::Engine),
    // The electrical rules.
    ("ecad-electrical", Class::Core),
    // The look of a page, on top of the definitions and the electrical rules.
    ("ecad-drawing-style", Class::Core),
    // The screen tokens; the canvas palette is `ecad_model::palette`.
    ("ecad-widgets", Class::UiKit),
    ("ecad-bench", Class::Dev),
    // The application, and the desktop program that runs it.
    ("ecad-app", Class::App),
    ("ecad-desktop", Class::Programs),
    ("ecad-mcp", Class::Programs),
    ("ecad-inspector", Class::Dev),
    ("ecad-xtask", Class::Exempt),
];

/// The CADCraft crates the Engine may use.
const ENGINE_CADCRAFT: &[&str] = &["cadcraft-engine", "cadcraft-io"];

/// The layer's name as a README's `Layer:` line writes it; `xtask` counts as Dev.
fn layer_name(class: Class) -> &'static str {
    match class {
        Class::Core => "Core",
        Class::Engine => "Engine",
        Class::UiKit => "UI kit",
        Class::App => "App",
        Class::Programs => "Programs",
        Class::Dev | Class::Exempt => "Dev",
    }
}

/// The order inside Core: a Core crate depends only on Core
/// crates before it, so no cycle (such as `ecad-file` → `ecad-electrical`) can be written.
const CORE_ORDER: &[&str] = &["ecad-model", "ecad-store", "ecad-file", "ecad-electrical", "ecad-drawing-style"];

/// Dependencies that break the layer rules for now, each with the change that removes it. An entry
/// no longer needed fails `layers`.
const LAYER_EXCEPTIONS: &[(&str, &str, &str)] = &[];

/// Product crates whose `dev` feature may be on by default: ecad-app, for the desktop app's debugger,
/// while the desktop app is the only app.
const DEV_BY_DEFAULT: &[&str] = &["ecad-app"];

/// Features whose code lives only in a crate's `dev` module.
const DEV_FEATURES: &[&str] = &["dev", "verify-invariants", "fault-injection", "test-ops"];

/// The engine files that read a dev feature outside its `dev` module, and why: the fault-injection and
/// test-ops hooks sit inside the write and access paths they break or open, so a test exercises the
/// real path. One area, no counts: a file listed here that no longer reads one fails `layers`, and any
/// other file that does is a violation.
const DEV_FEATURE_HOOKS: (&[&str], &str) = (
    &[
        "crates/ecad-engine/src/access.rs",
        "crates/ecad-engine/src/properties/forms/mod.rs",
        "crates/ecad-engine/src/lib.rs",
        "crates/ecad-engine/src/project/changes.rs",
        "crates/ecad-engine/src/project/owner.rs",
        "crates/ecad-engine/src/project/persistence.rs",
    ],
    "fault-injection and test-ops hooks inside the paths they test",
);

/// Styling written in place, which ECAD UI code may no longer use: colours, sizes and fonts come from the tokens of `ecad-widgets`.
/// `Tokens::get` reads CADCraft's theme colours directly; the `from_rgba` entries cover every RGBA
/// colour constructor (`Color32::from_rgba_unmultiplied`, `Rgba::from_rgba_premultiplied`, …), not an
/// image's pixels. A plain ban: no file may style locally.
const LOCAL_STYLE: &[&str] = &[
    "Color32::from_rgb",
    "Color32::from_gray",
    "Color32::from_rgba",
    "Rgba::from_rgba",
    "FontId::new",
    "FontId::proportional",
    "FontId::monospace",
    "CornerRadius",
    "Rounding::",
    "Tokens::get",
    // egui's named colours are local styling too.
    "Color32::WHITE",
    "Color32::GRAY",
    "Color32::RED",
    "Color32::YELLOW",
];

/// Raw egui value controls (design `docs/design/components.md`): panels and dialogs use
/// `ecad_widgets::control` (text entry, number with unit, selector, toggle, value box) instead, so
/// anything the person can change looks editable and a read value never looks like an input.
const RAW_CONTROL: &[&str] = &["TextEdit::", "ComboBox::"];

/// ECAD UI files that still use raw egui value controls ([`RAW_CONTROL`]): panels made before the
/// components, converted to `ecad_widgets::control` in a later pass. No counts: a listed file may keep
/// its controls, any other file may not, and a listed file that has none left fails until it is removed.
const RAW_CONTROL_FILES: &[&str] = &[
    "crates/ecad-app/src/developer/mod.rs",
    "crates/ecad-app/src/library/builder.rs",
    "crates/ecad-app/src/library/details.rs",
    "crates/ecad-app/src/settings/style_catalogues.rs",
    "crates/ecad-app/src/pages/project_panel.rs",
    "crates/ecad-app/src/publish/mod.rs",
    "crates/ecad-app/src/library/search.rs",
    "crates/ecad-app/src/settings/project.rs",
    "crates/ecad-app/src/settings/standards.rs",
    "crates/ecad-app/src/shell/placing.rs",
    "crates/ecad-app/src/inspector/layout_tab.rs",
    "crates/ecad-app/src/inspector/mod.rs",
    "crates/ecad-app/src/settings/text_styles.rs",
];

/// Paths a layer's sources (`src/`, outside `//` comments) must not name, each matched where it
/// starts a name. Cargo alone is not enough for App and Programs, because
/// `cadcraft-ui-egui` and `cadcraft-mcp` can hand engine types through; for ecad-electrical the
/// check repeats Cargo's rule with a clearer message. Dev tools such as `ecad-bench` may use the
/// engine directly.
fn forbidden_paths(name: &str, class: Class) -> &'static [&'static str] {
    match class {
        Class::App | Class::Programs => &["cadcraft_engine"],
        Class::Core if name == "ecad-electrical" => &["cadcraft_"],
        _ => &[],
    }
}

/// The one Core crate that may reach the OS: storage under the file and the lock. Every other Core crate stays pure data and rules, so it is testable and portable; disk
/// access belongs to `ecad-store`, and the time is passed in.
const CORE_OS_OK: &str = "ecad-store";

/// Direct OS access in Core code, each matched where it starts a name.
/// Threads and rayon are allowed.
const CORE_OS: &[&str] = &[
    "std::fs",
    "fs::",
    "std::env",
    "env::var",
    "std::process",
    "std::net",
    "SystemTime",
    "Instant::now",
    "RandomState",
    "chrono::Local",
    "Local::now",
    "Utc::now",
    "home_dir",
    "canonicalize",
    "File::",
    "OpenOptions",
];

/// Crates that find the user's folders: Core other than [`CORE_OS_OK`] may not depend on them.
const CORE_OS_CRATES: &[&str] = &["dirs", "dirs-next", "dirs-sys", "directories", "directories-next"];

/// Crates besides Programs that may use `anyhow`: xtask and
/// ecad-bench's binary. A library returns its own error type, so a caller can react to the kind
/// of failure; `anyhow` erases it, which only a program's `main` may do.
const ANYHOW_OK: &[&str] = &["ecad-bench", "ecad-xtask"];

/// Where `rayon` may appear, as a plain dependency (normal, not optional,
/// no target table): where the per-page and per-chunk work is. No wasm `cfg`: ECAD is
/// desktop-only.
const RAYON_OK: &[&str] = &["ecad-model", "ecad-file", "ecad-electrical", "ecad-engine"];

/// UI toolkits and ECAD's own UI kit: Core and Engine crates may depend on none of them (the
/// kit too, so the document and engine crates never draw).
const UI_CRATES: &[&str] =
    &["cadcraft-ui-egui", "craft-ui", "egui", "epaint", "egui_extras", "eframe", "egui-wgpu", "wgpu", "winit", "rfd", "ecad-widgets"];

/// The sections of a crate's README, in order, after its `# <crate name>` title.
const README_SECTIONS: &[&str] = &["## Purpose", "## Contents", "## Must not depend on", "## Key types", "## How to test"];

/// Workspace members whose README is missing or lacks the standard sections. The list only
/// shrinks: a member not
/// listed whose README fails is a violation, and an entry whose README passes (or that is no
/// longer a member) fails `layers` until it is removed.
const README_ALLOWLIST: &[&str] = &[];

/// What a UiKit crate may depend on, besides dev-dependencies.
const UIKIT_ALLOWED: &[&str] = &["egui", "epaint", "egui_extras", "ecad-model"];

fn ecad_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("{cmd:?}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{cmd:?} failed ({status})")) }
}

fn class_of(name: &str) -> Option<Class> {
    TABLE.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// Whether `manifest` (a `Cargo.toml` path) is under the workspace's `dev/` folder, `root` being
/// the workspace folder.
fn under_dev(root: &str, manifest: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").to_lowercase();
    let (root, manifest) = (norm(root), norm(manifest));
    manifest.strip_prefix(root.trim_end_matches('/')).is_some_and(|rest| rest.starts_with("/dev/"))
}

/// Why a product crate's dependency on the `Dev` crate `dep` (the entry `d` of package `p`) is not
/// allowed, if it is not: it must be optional, enabled only by the crate's own `dev` feature
/// (through `dep:`, so no feature named after it), and `dev` must not be on by default.
fn dev_dependency_problem(p: &Value, d: &Value, dep: &str) -> Option<String> {
    let name = p["name"].as_str().unwrap_or_default();
    if d["kind"].as_str() == Some("dev") || d["optional"].as_bool() != Some(true) {
        return Some(format!("{name} must not depend on the developer tool {dep} (only optionally, through its `dev` feature)"));
    }
    let features = p["features"].as_object();
    let enablers = enabling_features(p, dep);
    if enablers.iter().any(|k| k.as_str() != "dev") {
        return Some(format!("{name}: only its `dev` feature may enable {dep} (enabled by {enablers:?}; use `dev = [\"dep:{dep}\"]`)"));
    }
    let on_by_default =
        features.and_then(|f| f.get("default")).and_then(Value::as_array).is_some_and(|d| d.iter().any(|x| x.as_str() == Some("dev")));
    if on_by_default && !DEV_BY_DEFAULT.contains(&name) {
        return Some(format!("{name}: its `dev` feature (which enables {dep}) must not be on by default"));
    }
    None
}

/// The features of package `p` that enable its optional dependency `dep` (by `dep:`, by its name,
/// or through one of its features), and a feature named after it.
fn enabling_features(p: &Value, dep: &str) -> Vec<String> {
    let enables = |list: &Value| {
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|f| f == dep || f == format!("dep:{dep}") || f.starts_with(&format!("{dep}/")) || f.starts_with(&format!("{dep}?/")))
    };
    p["features"].as_object().into_iter().flatten().filter(|(k, v)| enables(v) || k.as_str() == dep).map(|(k, _)| k.clone()).collect()
}

fn layer_violations(meta: &Value) -> Result<Vec<String>, String> {
    let pkgs = meta["packages"].as_array().ok_or("cargo metadata: no packages")?;
    let root = meta["workspace_root"].as_str().unwrap_or_default();
    let mut out = Vec::new();
    let mut used_exceptions: BTreeSet<(&str, &str)> = BTreeSet::new();
    for p in pkgs {
        let name = p["name"].as_str().unwrap_or_default();
        let Some(class) = class_of(name) else {
            out.push(format!("{name}: not registered in ecad/xtask layering table"));
            continue;
        };
        let in_dev = under_dev(root, p["manifest_path"].as_str().unwrap_or_default());
        if in_dev != matches!(class, Class::Dev | Class::Exempt) {
            out.push(format!("{name}: a crate is under ecad/dev/ exactly when its class is Dev (it is {class:?})"));
        }
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dep = d["name"].as_str().unwrap_or_default();
            let dev = d["kind"].as_str() == Some("dev");
            let ui = UI_CRATES.contains(&dep) || class_of(dep) == Some(Class::UiKit);
            let cadcraft = dep.starts_with("cadcraft-");
            let dep_class = class_of(dep);
            let above_engine = matches!(dep_class, Some(Class::App | Class::Programs));
            // A dev-dependency is held to the layer order only (#53, design `layout.md`): no crate
            // depends on a layer above it, not even in tests. Dev crates are checked below,
            // dev-dependencies included.
            let rank = |c: Class| match c {
                Class::Core => Some(0),
                Class::Engine | Class::UiKit => Some(1),
                Class::App => Some(2),
                Class::Programs => Some(3),
                Class::Dev | Class::Exempt => None,
            };
            let above = dev && matches!((rank(class), dep_class.and_then(rank)), (Some(own), Some(theirs)) if theirs > own);
            let bad = above
                || !dev
                    && match class {
                        Class::Core => {
                            let before = |c: &str| CORE_ORDER.iter().position(|x| *x == c);
                            let out_of_order = dep_class == Some(Class::Core) && !matches!((before(dep), before(name)), (Some(d), Some(n)) if d < n);
                            cadcraft || ui || out_of_order || dep_class == Some(Class::Engine) || above_engine
                        }
                        Class::Engine => ui || (cadcraft && !ENGINE_CADCRAFT.contains(&dep)) || above_engine,
                        Class::UiKit => !UIKIT_ALLOWED.contains(&dep),
                        Class::App => dep == "cadcraft-engine" || dep_class == Some(Class::Programs),
                        Class::Programs => dep == "cadcraft-engine",
                        Class::Dev | Class::Exempt => false,
                    };
            if bad && LAYER_EXCEPTIONS.iter().any(|(n, d, _)| *n == name && *d == dep) {
                used_exceptions.insert((name, dep));
            } else if bad {
                out.push(format!("{name} ({class:?}) must not depend on {dep}"));
            }
            let product = matches!(class, Class::Core | Class::Engine | Class::App | Class::Programs | Class::UiKit);
            if product
                && class_of(dep) == Some(Class::Dev)
                && let Some(why) = dev_dependency_problem(p, d, dep)
            {
                out.push(why);
            }
        }
    }
    // A stale exception is reported only for the real workspace (the tests' small graphs leave
    // the excepted dependency out).
    let present = |n: &str| !root.is_empty() && pkgs.iter().any(|p| p["name"].as_str() == Some(n));
    for (name, dep, why) in LAYER_EXCEPTIONS {
        if present(name) && !used_exceptions.contains(&(*name, *dep)) {
            out.push(format!("{name} -> {dep}: a layer exception ({why}) that is no longer needed; remove it"));
        }
    }
    Ok(out)
}

/// The places in one Core source file (`rel`, `/` separated) that reach the OS directly
/// ([`CORE_OS`]), as `rel:line: text`. `//` comments are skipped, and so is everything from a
/// `#[cfg(test)]` line followed by a `mod` line (the file's test module) on.
fn core_os_places(rel: &str, text: &str) -> Vec<String> {
    let code = strip_line_comments(text);
    let lines: Vec<&str> = code.lines().collect();
    let end = (0..lines.len())
        .find(|&i| {
            lines[i].trim() == "#[cfg(test)]"
                && lines.get(i + 1).is_some_and(|n| n.trim_start().starts_with("mod ") || n.trim_start().starts_with("pub mod "))
        })
        .unwrap_or(lines.len());
    name_places(rel, text, lines.get(..end).unwrap_or_default(), CORE_OS)
}

/// The lines of `code` (the comment-stripped lines of `text`) that contain one of `patterns`
/// where a name starts (not inside a longer name: `fs::` does not match `ecad_fs::`), as
/// `rel:line: text`.
fn name_places(rel: &str, text: &str, code: &[&str], patterns: &[&str]) -> Vec<String> {
    let starts_name = |l: &str, i: usize| l.get(..i).and_then(|s| s.chars().next_back()).is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    code.iter()
        .enumerate()
        .filter(|(_, l)| patterns.iter().any(|p| l.match_indices(p).any(|(i, _)| starts_name(l, i))))
        .map(|(i, _)| format!("{rel}:{}: {}", i + 1, text.lines().nth(i).unwrap_or_default().trim()))
        .collect()
}

/// The places in one source file (`rel`) of crate `name` that name a path its layer must not
/// ([`forbidden_paths`]), outside `//` comments, as `rel:line: text`.
fn forbidden_path_places(name: &str, class: Class, rel: &str, text: &str) -> Vec<String> {
    let code = strip_line_comments(text);
    let lines: Vec<&str> = code.lines().collect();
    name_places(rel, text, &lines, forbidden_paths(name, class))
}

/// The packages of `meta` with their class and folder (packages not in [`TABLE`] are skipped;
/// [`layer_violations`] reports them).
fn classed_packages(meta: &Value) -> Result<Vec<(&str, Class, Option<PathBuf>, &Value)>, String> {
    Ok(meta["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?
        .iter()
        .filter_map(|p| {
            let name = p["name"].as_str().unwrap_or_default();
            let dir = p["manifest_path"].as_str().map(PathBuf::from).and_then(|m| m.parent().map(Path::to_path_buf));
            class_of(name).map(|c| (name, c, dir, p))
        })
        .collect())
}

/// The layer table's source checks for App, Programs and ecad-electrical ([`forbidden_paths`]): the
/// lines of their `src/` that name a forbidden path. A hard rule: no allowlist.
fn path_violations(meta: &Value) -> Result<Vec<String>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut out = Vec::new();
    for (name, class, dir, _) in classed_packages(meta)? {
        let Some(dir) = dir else { continue };
        if forbidden_paths(name, class).is_empty() {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            out.extend(forbidden_path_places(name, class, &rel, &text).into_iter().map(|p| match class {
                Class::Core => format!("{p} (ecad-electrical reads a Document and returns data or ops: no CADCraft crate)"),
                _ => format!("{p} (the engine is reached only through ecad-engine, e.g. ecad_engine::cad)"),
            }));
        }
    }
    Ok(out)
}

/// Direct OS access in Core crates other than [`CORE_OS_OK`]: every source
/// line that reaches the OS outside the test module, and every dependency on a crate that finds
/// the user's folders ([`CORE_OS_CRATES`]). A hard zero.
fn core_os_violations(meta: &Value) -> Result<Vec<String>, String> {
    const WHY: &str = "Core stays pure data and rules, so it is testable and portable: disk access belongs to ecad-store, and the time is passed in";
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut out = Vec::new();
    for (name, class, dir, p) in classed_packages(meta)? {
        if class != Class::Core || name == CORE_OS_OK {
            continue;
        }
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dep = d["name"].as_str().unwrap_or_default();
            if CORE_OS_CRATES.contains(&dep) && d["kind"].as_str() != Some("dev") {
                out.push(format!("{name} must not depend on {dep} ({WHY})"));
            }
        }
        let Some(dir) = dir else { continue };
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            out.extend(core_os_places(&rel, &text).into_iter().map(|p| format!("{p} ({WHY})")));
        }
    }
    Ok(out)
}

/// Drawn text never ignores its style. The crates that draw and
/// lay out pages ([`DRAWN_TEXT_CRATES`]) hold no literal text height, style name or text-size
/// constant outside the places that define styles ([`DRAWN_TEXT_EXEMPT`]): a text's height, weight,
/// font and colour come from its `TextKind`'s style. A hard zero.
const DRAWN_TEXT_CRATES: &[&str] = &["ecad-engine", "ecad-electrical"];

/// The engine's session command files and folders, for the `CmdError` ratchet.
const ENGINE_COMMAND_FILES: &[&str] = &[
    "crates/ecad-engine/src/cables/commands.rs",
    "crates/ecad-engine/src/commands",
    "crates/ecad-engine/src/connectors/commands.rs",
    "crates/ecad-engine/src/devices/commands.rs",
    "crates/ecad-engine/src/pages/commands",
    "crates/ecad-engine/src/plc/commands",
    "crates/ecad-engine/src/references/commands",
    "crates/ecad-engine/src/releases/commands.rs",
    "crates/ecad-engine/src/reports/commands.rs",
    "crates/ecad-engine/src/styles/commands.rs",
    "crates/ecad-engine/src/symbols/commands.rs",
    "crates/ecad-engine/src/terminal_strips/commands.rs",
    "crates/ecad-engine/src/title_blocks/commands.rs",
    "crates/ecad-engine/src/wires/commands.rs",
    "crates/ecad-engine/src/generic.rs",
];

/// Where literal text styling is allowed, as fragments of a file's path under the workspace:
/// the styles and their data define the sizes (`/styles/`, `/defaults/`); symbol definitions draw
/// glyph text as line art in symbol units; screen-only overlays (`interact`, the pointer, the
/// property forms) are sized in pixels; an imported title block keeps the user's own sizes; commands
/// (every `commands` file or folder, and the generic runner) draw nothing.
const DRAWN_TEXT_EXEMPT: &[&str] = &[
    "/styles/",
    "/defaults/",
    "ecad-electrical/src/symbols/",
    "ecad-engine/src/pointer/",
    "ecad-engine/src/properties/forms/",
    "/interact/",
    "/interact.rs",
    "ecad-engine/src/title_blocks/dxf.rs",
    "/commands/",
    "/commands.rs",
    "ecad-engine/src/generic.rs",
];

/// The lines of one source file (`rel`) that style drawn text literally, before its test module,
/// outside `//` comments: a numeric `height:`, a quoted `style:` or `"Standard"`, the style names
/// `STYLE_BOLD` / `STYLE_REGULAR`, or a text-size constant (`..._TEXT_MM`, `..._HEIGHT_MM`,
/// `LABEL_HEIGHT`…; bounds named `MIN_`/`MAX_` are guards and allowed).
fn drawn_text_places(rel: &str, text: &str) -> Vec<String> {
    let code = strip_line_comments(text);
    let lines: Vec<&str> = code.lines().collect();
    let end = (0..lines.len())
        .find(|&i| {
            lines[i].trim() == "#[cfg(test)]"
                && lines.get(i + 1).is_some_and(|n| n.trim_start().starts_with("mod ") || n.trim_start().starts_with("pub mod "))
        })
        .unwrap_or(lines.len());
    let numeric_after = |l: &str, key: &str| {
        l.match_indices(key).any(|(i, _)| {
            let rest = l.get(i + key.len()..).unwrap_or_default().trim_start().trim_start_matches('-');
            rest.chars().next().is_some_and(|c| c.is_ascii_digit())
        })
    };
    let size_const = |l: &str| {
        let t = l.trim_start().trim_start_matches("pub ").trim_start_matches("pub(crate) ");
        let Some(rest) = t.strip_prefix("const ") else { return false };
        let name: String = rest.chars().take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_').collect();
        let sizes = ["TEXT_MM", "TEXT_HEIGHT", "HEIGHT_MM", "LABEL_HEIGHT", "CAP_HEIGHT", "TEXT_SIZE"];
        !name.starts_with("MIN_") && !name.starts_with("MAX_") && sizes.iter().any(|s| name.contains(s)) && numeric_after(rest, "f64 =")
    };
    let mut out: Vec<String> = lines
        .get(..end)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter(|(_, l)| numeric_after(l, "height:") || l.contains("style: \"") || l.contains("\"Standard\"") || size_const(l))
        .map(|(i, _)| format!("{rel}:{}: {}", i + 1, text.lines().nth(i).unwrap_or_default().trim()))
        .collect();
    out.extend(name_places(rel, text, lines.get(..end).unwrap_or_default(), &["STYLE_BOLD", "STYLE_REGULAR"]));
    out.sort();
    out.dedup();
    out
}

/// [`drawn_text_places`] over [`DRAWN_TEXT_CRATES`], outside [`DRAWN_TEXT_EXEMPT`].
fn drawn_text_violations(meta: &Value) -> Result<Vec<String>, String> {
    const WHY: &str = "drawn text reads its TextKind's style: height, weight, font and colour come from the styles and their TOML data";
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut out = Vec::new();
    for (name, _, dir, _) in classed_packages(meta)? {
        let (true, Some(dir)) = (DRAWN_TEXT_CRATES.contains(&name), dir) else { continue };
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        for f in files {
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            if DRAWN_TEXT_EXEMPT.iter().any(|e| rel.contains(e)) {
                continue;
            }
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            out.extend(drawn_text_places(&rel, &text).into_iter().map(|p| format!("{p} ({WHY})")));
        }
    }
    Ok(out)
}

/// Per crate, the lines that still report an error as text: `Result<_, String>` and
/// `Err(format!(`. Each crate's two counts only fall as its errors move to its `thiserror` enum.
/// A crate without an entry must have none (`ecad-file` and `ecad-store`, since batch 8); `xtask`
/// is a tool.
const STRING_ERROR_ALLOWLIST: &[(&str, usize, usize)] = &[
    ("ecad-app", 57, 6),
    ("ecad-bench", 94, 22),
    ("ecad-engine", 143, 35),
    ("ecad-desktop", 3, 5),
    ("ecad-electrical", 0, 0),
    ("ecad-inspector", 5, 2),
    ("ecad-mcp", 34, 10),
    ("ecad-model", 6, 7),
];

/// The command files (`crates/ecad-engine/src/cmd`) on their own, as the crate rows.
const CMD_STRING_ERRORS: (usize, usize) = (0, 0);

/// The migration bridges `impl From<String> for CmdError` and `impl From<&'static str>
/// for CmdError` in ecad-electrical: none left.
const CMD_ERROR_BRIDGES: usize = 0;

/// Crates [`string_error_violations`] does not count.
const STRING_ERROR_SKIP: &[&str] = &["ecad-xtask"];

/// The lines of `text` (outside `//` comments) with a `Result<_, String>`, and with an
/// `Err(format!(`.
fn string_error_counts(text: &str) -> (usize, usize) {
    let code = strip_line_comments(text);
    let results = code.lines().filter(|l| l.contains("Result<") && (l.contains(", String>") || l.contains(",String>"))).count();
    let formats = code.lines().filter(|l| l.contains("Err(format!(")).count();
    (results, formats)
}

/// [`STRING_ERROR_ALLOWLIST`] against the sources: a count that rose, fell (lower it) or a crate
/// missing from the list.
fn string_error_violations(meta: &Value) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (name, _, dir, _) in classed_packages(meta)? {
        let Some(dir) = dir else { continue };
        if STRING_ERROR_SKIP.contains(&name) {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        let (mut results, mut formats) = (0, 0);
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let (r, e) = string_error_counts(&text);
            (results, formats) = (results + r, formats + e);
        }
        match STRING_ERROR_ALLOWLIST.iter().find(|(n, _, _)| *n == name) {
            Some((n, r, e)) => {
                seen.insert(n);
                const FIX: &str = "return a typed error: the crate's thiserror enum, or anyhow in a program or dev tool";
                out.extend(ratchet_problem(n, results, *r, "lines with `Result<_, String>`", FIX));
                out.extend(ratchet_problem(n, formats, *e, "lines with `Err(format!(`", FIX));
            }
            None if results + formats > 0 => {
                out.push(format!("{name}: {results} lines with `Result<_, String>` and {formats} with `Err(format!(` but no entry in STRING_ERROR_ALLOWLIST; return a typed error: the crate's thiserror enum, or anyhow in a program or dev tool"));
            }
            None => {}
        }
    }
    for (n, _, _) in STRING_ERROR_ALLOWLIST {
        if !seen.contains(n) {
            out.push(format!("{n}: in STRING_ERROR_ALLOWLIST but not a counted crate; remove the entry"));
        }
    }
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let count_in = |dir: &Path, f: &dyn Fn(&str) -> (usize, usize)| -> Result<(usize, usize), String> {
        let mut files = Vec::new();
        rust_files(dir, &mut files);
        let mut sum = (0, 0);
        for p in files {
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            let (a, b) = f(&text);
            sum = (sum.0 + a, sum.1 + b);
        }
        Ok(sum)
    };
    const FIX: &str = "return CmdError";
    // The engine's session command files (0.2.3: each in its feature folder, plus `commands/`).
    let cmd = "ecad-engine commands";
    let (mut r, mut e) = (0, 0);
    for p in ENGINE_COMMAND_FILES {
        let path = root.join(p);
        let (a, b) = if path.is_file() {
            let text = std::fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))?;
            string_error_counts(&text)
        } else {
            count_in(&path, &string_error_counts)?
        };
        r += a;
        e += b;
    }
    out.extend(ratchet_problem(cmd, r, CMD_STRING_ERRORS.0, "lines with `Result<_, String>`", FIX));
    out.extend(ratchet_problem(cmd, e, CMD_STRING_ERRORS.1, "lines with `Err(format!(`", FIX));
    let bridges = |t: &str| {
        let code = strip_line_comments(t);
        (code.lines().filter(|l| l.contains("impl From<String> for CmdError") || l.contains("impl From<&'static str> for CmdError")).count(), 0)
    };
    let (b, _) = count_in(&root.join("crates/ecad-electrical/src"), &bridges)?;
    out.extend(ratchet_problem("ecad-electrical", b, CMD_ERROR_BRIDGES, "CmdError bridges from text", "return a CmdError variant"));
    Ok(out)
}

/// The crate rules on ECAD's own manifests in `meta`; CADCraft's and
/// transitive dependencies are not checked:
/// - `anyhow` only in Programs and [`ANYHOW_OK`] (dev-dependencies aside);
/// - `tokio` nowhere, dev-dependencies included (ECAD runs no async runtime: commands are
///   synchronous on the project's lock, and a second runtime beside CADCraft's event loop would
///   split where work happens);
/// - `rayon` only in [`RAYON_OK`], as a plain dependency.
fn dependency_rule_violations(meta: &Value) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for p in meta["packages"].as_array().ok_or("cargo metadata: no packages")? {
        let name = p["name"].as_str().unwrap_or_default();
        let class = class_of(name);
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dep = d["name"].as_str().unwrap_or_default();
            let dev = d["kind"].as_str() == Some("dev");
            let plain = d["kind"].is_null() && d["target"].is_null() && d["optional"].as_bool() != Some(true);
            match dep {
                "anyhow" if !dev && class != Some(Class::Programs) && !ANYHOW_OK.contains(&name) => out.push(format!(
                    "{name} must not depend on anyhow: a library returns its own error type so callers can tell failures apart (only Programs and {ANYHOW_OK:?} may)"
                )),
                "tokio" => out.push(format!("{name} must not depend on tokio: ECAD runs no async runtime beside CADCraft's event loop")),
                "rayon" if !(RAYON_OK.contains(&name) && plain) => out.push(format!(
                    "{name}: rayon only in {RAYON_OK:?}, as a plain dependency (normal, not optional, no target table: ECAD is desktop-only)"
                )),
                _ => {}
            }
        }
    }
    Ok(out)
}

/// What is wrong with the README of crate `name` (in layer `class`), `text` being its content if
/// it exists: the first heading is `# <name>`, the [`README_SECTIONS`] follow in
/// order (other headings may come between), and a `Layer:` line names the crate's layer. Headings
/// inside fenced code blocks are not headings.
fn readme_problem(name: &str, class: Class, text: Option<&str>) -> Option<String> {
    let Some(text) = text else { return Some(format!("{name}: no README.md at the crate root")) };
    let mut headings = Vec::new();
    let mut fenced = false;
    for l in text.lines().map(str::trim_end) {
        if l.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced && (l.starts_with("# ") || l.starts_with("## ")) {
            headings.push(l);
        }
    }
    let title = format!("# {name}");
    if headings.first() != Some(&title.as_str()) {
        return Some(format!("{name}: its README must start with the heading `{title}`"));
    }
    let mut rest = headings.iter().skip(1);
    if let Some(missing) = README_SECTIONS.iter().find(|s| !rest.any(|h| h == *s)) {
        return Some(format!("{name}: its README lacks the heading `{missing}`, or has it out of order"));
    }
    let want = layer_name(class);
    match text.lines().find_map(|l| l.strip_prefix("Layer:")).map(str::trim) {
        Some(layer) if layer == want => None,
        Some(layer) => Some(format!("{name}: its README says `Layer: {layer}`, but the crate is in {want}")),
        None => Some(format!("{name}: its README has no `Layer: {want}` line")),
    }
}

/// The README ratchet: `results` holds each workspace member's [`readme_problem`]; a problem
/// fails unless the member is in `allowlist`, and an allowlist entry whose member passes or is
/// not in `results` fails too, so the list only shrinks.
fn readme_ratchet(results: &[(&str, Option<String>)], allowlist: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = results.iter().filter(|(n, _)| !allowlist.contains(n)).filter_map(|(_, p)| p.clone()).collect();
    for entry in allowlist {
        match results.iter().find(|(n, _)| n == entry) {
            Some((_, Some(_))) => {}
            Some((_, None)) => out.push(format!("{entry}: its README passes; remove it from the README allowlist")),
            None => out.push(format!("{entry}: in the README allowlist but not a workspace member; remove the entry")),
        }
    }
    out
}

/// The files and folders a README's `## Contents` section names: each backticked `name/` or `name.rs`, a path
/// inside the crate (one that starts with `ecad/` names a place elsewhere and is skipped).
fn readme_contents_paths(text: &str) -> Vec<&str> {
    let Some(start) = text.find(
        "
## Contents
",
    ) else {
        return Vec::new();
    };
    let body = &text[start
        + "
## Contents
"
        .len()..];
    let body = body
        .find(
            "
## ",
        )
        .map_or(body, |end| &body[..end]);
    body.split('`')
        .skip(1)
        .step_by(2)
        .filter(|s| {
            (s.ends_with('/') || s.ends_with(".rs"))
                && !s.starts_with("ecad/")
                && s.len() > 1
                && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'))
        })
        .collect()
}

/// The README checks over the workspace members in `meta`: the shape, with [`README_ALLOWLIST`], and, a hard
/// rule, that every file or folder its Contents names exists under the crate's `src/` or the crate folder.
fn readme_violations(meta: &Value) -> Result<Vec<String>, String> {
    let mut missing = Vec::new();
    let results: Vec<(&str, Option<String>)> = classed_packages(meta)?
        .into_iter()
        .map(|(name, class, dir, _)| {
            let text = dir.as_ref().and_then(|d| std::fs::read_to_string(d.join("README.md")).ok());
            if let (Some(d), Some(t)) = (&dir, &text) {
                for p in readme_contents_paths(t) {
                    let rel = p.trim_end_matches('/');
                    if !d.join("src").join(rel).exists() && !d.join(rel).exists() {
                        missing.push(format!("{name}: its README's Contents names `{p}`, which does not exist"));
                    }
                }
            }
            (name, readme_problem(name, class, text.as_deref()))
        })
        .collect();
    let mut out = readme_ratchet(&results, README_ALLOWLIST);
    out.extend(missing);
    Ok(out)
}

/// The places in one product source file (`rel`: its path from the workspace folder, `/`
/// separated) that read a dev feature outside the crate's `dev` module,
/// as `rel:line: text`.
///
/// # Semantics
/// A dev feature is one of [`DEV_FEATURES`], written `feature = "<f>"`. It may appear in
/// `src/dev.rs` or `src/dev/**`, or on an attribute line directly before a single line that uses
/// `dev::` (a one-line hook, ending in `;` or `,`) or declares `mod dev;`. Comment lines are
/// skipped.
fn dev_feature_places(rel: &str, text: &str) -> Vec<String> {
    let rel = rel.replace('\\', "/");
    if rel.contains("/src/dev/") || rel.ends_with("/src/dev.rs") {
        return Vec::new();
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let code = line.trim();
        if code.starts_with("//") {
            continue;
        }
        let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        let reads = DEV_FEATURES.iter().any(|f| compact.contains(&format!("feature=\"{f}\"")));
        if !reads {
            continue;
        }
        let attribute = code.starts_with("#[") && code.ends_with(']');
        let next = lines.iter().skip(i + 1).map(|l| l.trim()).find(|l| !l.is_empty() && !l.starts_with("#["));
        let hook = next.is_some_and(|n| {
            let one_line_use = n.contains("dev::") && (n.ends_with(';') || n.ends_with(','));
            let module = matches!(n, "mod dev;" | "pub mod dev;" | "pub(crate) mod dev;");
            one_line_use || module
        });
        if !(attribute && hook) {
            out.push(format!("{rel}:{}: {code}", i + 1));
        }
    }
    out
}

/// The places in one ECAD UI source file (`rel`, `/` separated) that style locally
/// ([`LOCAL_STYLE`]), as `rel:line: text`; comment lines and `//` comments are skipped.
fn local_style_places(rel: &str, text: &str) -> Vec<String> {
    places_matching(rel, text, LOCAL_STYLE)
}

/// The places in one ECAD UI source file that use a raw egui value control ([`RAW_CONTROL`]),
/// as `rel:line: text`; comment lines and `//` comments are skipped.
fn raw_control_places(rel: &str, text: &str) -> Vec<String> {
    places_matching(rel, text, RAW_CONTROL)
}

/// What is wrong with an entry of a ratchet allowlist that has `found` places against its
/// `allowed` count, if anything: more is a violation (`fix` says what to do instead); fewer means
/// lower the count, and none means remove the entry. `what` names the places, plural.
fn ratchet_problem(path: &str, found: usize, allowed: usize, what: &str, fix: &str) -> Option<String> {
    match found.cmp(&allowed) {
        std::cmp::Ordering::Greater => Some(format!("{path}: {found} {what}, the allowlist allows {allowed}; {fix}")),
        std::cmp::Ordering::Less if found == 0 => Some(format!("{path}: no {what} left; remove its allowlist entry")),
        std::cmp::Ordering::Less => Some(format!("{path}: {found} {what}, fewer than the allowlist's {allowed}; lower the count")),
        std::cmp::Ordering::Equal => None,
    }
}

/// The lines of `text` (outside `//` comments) that contain one of `patterns`, as `rel:line: text`.
fn places_matching(rel: &str, text: &str, patterns: &[&str]) -> Vec<String> {
    let code = strip_line_comments(text);
    code.lines()
        .enumerate()
        .filter(|(_, l)| patterns.iter().any(|p| l.contains(p)))
        .map(|(i, _)| format!("{rel}:{}: {}", i + 1, text.lines().nth(i).unwrap_or_default().trim()))
        .collect()
}

/// A path as components, `.` and `..` resolved (never above the start).
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `text` without `//` comments (a `//` inside a string literal is kept).
fn strip_line_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let mut in_str = false;
        let mut prev = '\0';
        let mut cut = line.len();
        for (i, c) in line.char_indices() {
            if c == '"' && prev != '\\' {
                in_str = !in_str;
            } else if !in_str && c == '/' && prev == '/' {
                cut = i - 1;
                break;
            }
            prev = c;
        }
        out.push_str(line.get(..cut).unwrap_or(line));
        out.push('\n');
    }
    out
}

/// The string literals in `span`, in order.
fn literals(span: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<String> = None;
    let mut prev = '\0';
    for c in span.chars() {
        match (&mut cur, c) {
            (None, '"') => cur = Some(String::new()),
            (Some(s), '"') if prev != '\\' => {
                out.push(std::mem::take(s));
                cur = None;
            }
            (Some(s), c) => s.push(c),
            (None, _) => {}
        }
        prev = c;
    }
    out
}

/// The places in one product source file (`file`, in the crate folder `krate`, under `root`, the
/// workspace folder) that read a file under `root/dev/`: an
/// `include_str!`, `include_bytes!` or `include!` invocation, or a `#[path]` attribute, read whole
/// (it may span lines), with its string literals joined; a path built on `CARGO_MANIFEST_DIR` is
/// resolved from the crate folder, any other from the file's folder. Comments are skipped.
fn dev_file_reads(root: &Path, krate: &Path, file: &Path, text: &str) -> Vec<String> {
    let dev = normalise(&root.join("dev"));
    let dir = file.parent().unwrap_or(root);
    let code = strip_line_comments(text);
    let mut out = Vec::new();
    for marker in ["include_str!", "include_bytes!", "include!", "#[path"] {
        let mut from = 0;
        while let Some(found) = code.get(from..).and_then(|c| c.find(marker)) {
            let at = from + found;
            from = at + marker.len();
            // The invocation runs to the matching close of its first bracket.
            let rest = code.get(from..).unwrap_or_default();
            let mut depth = 0i32;
            let mut end = rest.len();
            for (i, c) in rest.char_indices() {
                match c {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => {
                        depth -= 1;
                        if depth <= 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let span = rest.get(..end).unwrap_or(rest);
            let joined: String = literals(span).into_iter().filter(|l| l != "CARGO_MANIFEST_DIR").collect();
            if joined.is_empty() {
                continue;
            }
            let target = if span.contains("CARGO_MANIFEST_DIR") { krate.join(joined.trim_start_matches(['/', '\\'])) } else { dir.join(&joined) };
            if normalise(&target).starts_with(&dev) {
                let line = code.get(..at).map_or(0, |c| c.matches('\n').count()) + 1;
                let shown = text.lines().nth(line - 1).unwrap_or_default().trim();
                out.push(format!("{}:{line}: {shown}", file.display()));
            }
        }
    }
    out
}

/// Every `.rs` file under `dir`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The developer-tool source rules (dev features read only in a `dev` module; no product source
/// reads a file under `dev/`) over the product crates' `src/` folders in `meta`: the
/// violations, and the allowlist entries no longer needed.
fn source_violations(meta: &Value) -> Result<Vec<String>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut out = Vec::new();
    let mut hooks_used: BTreeSet<&str> = BTreeSet::new();
    let mut raw_seen: BTreeSet<&str> = BTreeSet::new();
    for p in meta["packages"].as_array().ok_or("cargo metadata: no packages")? {
        let name = p["name"].as_str().unwrap_or_default();
        let class = class_of(name);
        if !matches!(class, Some(Class::Core | Class::Engine | Class::App | Class::Programs | Class::UiKit)) {
            continue;
        }
        let Some(dir) = p["manifest_path"].as_str().map(PathBuf::from).and_then(|m| m.parent().map(Path::to_path_buf)) else { continue };
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            let places = dev_feature_places(&rel, &text);
            match DEV_FEATURE_HOOKS.0.iter().find(|path| **path == rel) {
                Some(path) if !places.is_empty() => {
                    hooks_used.insert(path);
                }
                _ => out.extend(places.into_iter().map(|p| format!("{p} (dev features are read only in the crate's `dev` module)"))),
            }
            out.extend(dev_file_reads(&root, &dir, &f, &text).into_iter().map(|p| format!("{p} (product code must not read files under dev/)")));
            if matches!(class, Some(Class::App | Class::Programs)) {
                out.extend(
                    local_style_places(&rel, &text).into_iter().map(|p| format!("{p} (no local styling: take it from the ecad-widgets tokens)")),
                );
                let places = raw_control_places(&rel, &text);
                match RAW_CONTROL_FILES.iter().find(|path| **path == rel) {
                    Some(path) if !places.is_empty() => {
                        raw_seen.insert(path);
                    }
                    _ => out.extend(places.into_iter().map(|p| format!("{p} (no raw egui value controls: use ecad_widgets::control)"))),
                }
            }
        }
    }
    for path in RAW_CONTROL_FILES {
        if !raw_seen.contains(path) {
            out.push(format!("{path}: in RAW_CONTROL_FILES but has no raw egui value control left; remove the entry"));
        }
    }
    for path in DEV_FEATURE_HOOKS.0 {
        if !hooks_used.contains(path) {
            out.push(format!("{path}: in DEV_FEATURE_HOOKS ({}) but reads no dev feature; remove the entry", DEV_FEATURE_HOOKS.1));
        }
    }
    Ok(out)
}

/// `cargo metadata` of the ECAD workspace, without dependencies.
fn metadata() -> Result<Value, String> {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(ecad_root())
        .output()
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

fn layers() -> Result<(), String> {
    let meta = metadata()?;
    let mut v = layer_violations(&meta)?;
    v.extend(source_violations(&meta)?);
    v.extend(path_violations(&meta)?);
    v.extend(dependency_rule_violations(&meta)?);
    v.extend(core_os_violations(&meta)?);
    v.extend(readme_violations(&meta)?);
    v.extend(string_error_violations(&meta)?);
    v.extend(drawn_text_violations(&meta)?);
    v.extend(types_guards::violations(&meta)?);
    v.extend(views_guards::violations(&meta)?);
    v.extend(engine_guards::violations(&meta)?);
    v.extend(starter_guards::violations(&meta)?);
    v.extend(pub_ratchet::violations(&meta)?);
    v.extend(history_guards::violations(&meta)?);
    if v.is_empty() {
        println!("layers: OK");
        Ok(())
    } else {
        Err(format!("layering violations:\n  {}", v.join("\n  ")))
    }
}

/// `cargo xtask pub-unreachable`: the `pub` items no other crate uses ([`pub_ratchet`]).
fn pub_unreachable() -> Result<(), String> {
    let meta = metadata()?;
    for (krate, list) in pub_ratchet::unreachable(&meta)? {
        println!("{krate}: {}", list.len());
        for (file, line, name) in list {
            println!("  {file}:{line}: {name}");
        }
    }
    Ok(())
}

/// Paths quoted as `path` in the first column of the inventory tables.
fn inventory(text: &str) -> Vec<String> {
    text.lines().filter(|l| l.starts_with("| `")).filter_map(|l| l.split('`').nth(1)).map(str::to_string).collect()
}

fn upstream_commit(text: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix("Commit: `").and_then(|r| r.split('`').next()).map(str::to_string))
}

fn patches() -> Result<(), String> {
    let root = ecad_root();
    let repo = root.parent().ok_or("no repository root")?;
    let upstream = std::fs::read_to_string(root.join("UPSTREAM.md")).map_err(|e| e.to_string())?;
    let base = upstream_commit(&upstream).ok_or("ecad/UPSTREAM.md has no `Commit:` line")?;
    let listed = inventory(&std::fs::read_to_string(root.join("UPSTREAM-PATCHES.md")).map_err(|e| e.to_string())?);
    let out = Command::new("git").args(["diff", "--name-only", &format!("{base}...HEAD")]).current_dir(repo).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("git diff against {base} failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let changed: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    let unlisted: Vec<&String> = changed.iter().filter(|p| !p.starts_with("ecad/") && !listed.contains(p)).collect();
    let stale: Vec<&String> = listed.iter().filter(|p| !changed.contains(p)).collect();
    println!("patches: {} upstream-tree files differ from {base}", changed.iter().filter(|p| !p.starts_with("ecad/")).count());
    if !stale.is_empty() {
        println!("patches: listed but unchanged (fine before the patch lands): {stale:?}");
    }
    if unlisted.is_empty() {
        println!("patches: OK");
        Ok(())
    } else {
        Err(format!(
            "files changed outside ecad/ but not in ecad/UPSTREAM-PATCHES.md:\n  {}",
            unlisted.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  ")
        ))
    }
}

/// Every check of [`ci`] except the tests: formatting, clippy with all targets and features and
/// with the default ones, layers and the patch guard. The QA gate's CI runs this in one job
/// and the tests in sharded nextest jobs (`.github/workflows/ci.yml`), so a check added here
/// reaches CI without a workflow change.
fn lint() -> Result<(), String> {
    let root = ecad_root();
    let cargo = env!("CARGO");
    run(Command::new(cargo).args(["fmt", "--all", "--check"]).current_dir(&root))?;
    run(Command::new(cargo).args(["clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"]).current_dir(&root))?;
    run(Command::new(cargo).args(["clippy", "--workspace", "--", "-D", "warnings"]).current_dir(&root))?;
    layers()?;
    patches()
}

fn ci() -> Result<(), String> {
    lint()?;
    // All features: the fault-injection containment tests run in CI, never in normal builds.
    // The slow set too (`#[ignore = "slow: …"]`): this is the QA gate's full suite.
    run(Command::new(env!("CARGO")).args(["test", "--workspace", "--all-features", "--", "--include-ignored"]).current_dir(ecad_root()))
}

const USAGE: &str = "usage: cargo xtask <layers|patches|lint|ci|bench|demo|test-changed|clean-builds>
  test-changed [--base <ref>] [--with-dependents] [--dry-run] [-- <test arguments>]
        fast tests of the crates changed since <ref> (default: the branch's upstream, so what
        was not pushed yet; for a branch never pushed, since its merge base with origin/dev;
        committed or not); never the slow set (#[ignore = \"slow: …\"]), which runs at the QA
        gate; --with-dependents also tests every crate that depends on them (CI runs that on
        pull requests into dev, so workers leave it off); --dry-run only names the crates
  lint
        every check of ci except the tests: fmt, clippy (all features and default), layers,
        patches
  clean-builds --root <folder> [--dry-run]
        delete ecad/target and target in each folder under <folder> and under <folder>/_tmp,
        skipping folders a cargo or rustc is using; remove git worktrees whose branch is merged
        into origin/dev (never with uncommitted changes); print the space freed
  demo [<path.ecad>] [--force]
        write the demo project (default C:\\dev\\ecad-projects\\demo.ecad, ~/ecad-projects/demo.ecad
        elsewhere) from ecad/examples/demo/commands.jsonl; an existing file needs --force
  bench [--tier small|medium|large|all] [--keep <folder>] [--compare <earlier.json>]
        project load benchmark (release build), run on demand only; not part of ci";

/// The demo project (`ecad-demo`, built from `ecad/examples/demo/commands.jsonl` through the MCP
/// `execute` tool), with `args` passed through: `[<path.ecad>] [--force]`.
fn demo(args: &[String]) -> Result<(), String> {
    run(Command::new(env!("CARGO"))
        .args(["run", "--release", "--package", "ecad-mcp", "--bin", "ecad-demo", "--"])
        .args(args)
        .current_dir(ecad_root()))
}

/// The load benchmark in release, with `args` passed through. Never called by [`ci`].
fn bench(args: &[String]) -> Result<(), String> {
    run(Command::new(env!("CARGO")).args(["run", "--release", "--package", "ecad-bench", "--"]).args(args).current_dir(ecad_root()))
}

/// Files that change every crate's build: the workspace manifest and lock file, cargo's
/// configuration and the toolchain file; and at the repository root, the manifest (CADCraft's
/// crates take their version, lints and dependencies from it), cargo's configuration (merged into
/// every build below it) and the toolchain file, but not its lock file (the workspace has its
/// own). `path` is lower case, relative to the repository root; `workspace` is the workspace
/// folder the same way, with a trailing `/` (empty at the root).
fn affects_everything(path: &str, workspace: &str) -> bool {
    let build = |p: &str| matches!(p, "cargo.toml" | "rust-toolchain.toml" | "rust-toolchain") || p.starts_with(".cargo/");
    path.strip_prefix(workspace).is_some_and(|p| build(p) || p == "cargo.lock") || build(path)
}

/// Folders of the workspace that belong to no package but are read into one at build time
/// (`include_str!`), with the package that reads them: a change there selects that package.
const READ_BY: &[(&str, &str)] = &[
    // The demo's command list, embedded by `ecad-mcp`'s demo module (`cargo xtask demo`).
    ("examples/", "ecad-mcp"),
];

/// The workspace members whose tests to run after `changed` (paths relative to the repository
/// root `repo`).
///
/// # Semantics
///
/// A changed file selects the innermost local package whose folder holds it; then, with
/// `with_dependents`, every workspace member that depends on a selected package, directly or not,
/// through any kind of dependency (dev included), is selected too. CADCraft's crates used by path are packages in `meta`, so an
/// upstream change selects the ECAD crates that use it. The files [`affects_everything`] names
/// (the workspace manifest and lock file, the repository root manifest, cargo configuration and
/// toolchain files) select every member. Markdown files and files in no package
/// (docs, tools, CI) select nothing, except the folders [`READ_BY`] names, which select the
/// package that reads them. `meta` is `cargo metadata --format-version 1` output with
/// its resolve graph. The result is sorted by name.
///
/// # Errors
///
/// `meta` has no package list or no resolve graph.
fn crates_to_test(meta: &Value, repo: &str, changed: &[String], with_dependents: bool) -> Result<Vec<String>, String> {
    let norm = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let repo = norm(repo);
    let relative = |dir: &str| dir.strip_prefix(&repo).map(|r| format!("{}/", r.trim_start_matches('/')));
    let workspace_root = norm(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let workspace = relative(&workspace_root).map(|w| if w == "/" { String::new() } else { w }).unwrap_or_default();
    let pkgs = meta["packages"].as_array().ok_or("cargo metadata: no packages")?;
    let members: BTreeSet<&str> = meta["workspace_members"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    // (id, name, folder relative to the repository root with a trailing `/`) of each local package.
    let mut local: Vec<(&str, &str, String)> = Vec::new();
    for p in pkgs.iter().filter(|p| p["source"].is_null()) {
        let (Some(id), Some(name), Some(manifest)) = (p["id"].as_str(), p["name"].as_str(), p["manifest_path"].as_str()) else {
            continue;
        };
        let manifest = norm(manifest);
        let dir = manifest.strip_suffix("/cargo.toml").unwrap_or(&manifest);
        if let Some(rel) = relative(dir) {
            local.push((id, name, rel));
        }
    }
    let mut hit: BTreeSet<&str> = BTreeSet::new();
    for path in changed.iter().map(|p| norm(p)) {
        if path.ends_with(".md") {
            continue;
        }
        if affects_everything(&path, &workspace) {
            hit.extend(members.iter().copied());
            continue;
        }
        let owner = local.iter().filter(|(_, _, dir)| path.starts_with(dir.as_str())).max_by_key(|(_, _, dir)| dir.len());
        if let Some((id, _, _)) = owner {
            hit.insert(id);
        } else if let Some(rest) = path.strip_prefix(workspace.as_str()) {
            // A folder read into a package at build time.
            for (folder, reader) in READ_BY {
                if rest.starts_with(folder)
                    && let Some((id, _, _)) = local.iter().find(|(_, name, _)| name == reader)
                {
                    hit.insert(id);
                }
            }
        }
    }
    // With dependents: everything that depends on a selected package, to a fixed point (the
    // graph is finite and each pass adds at least one package, so this ends).
    let nodes = meta["resolve"]["nodes"].as_array().ok_or("cargo metadata: no resolve graph")?;
    if with_dependents {
        loop {
            let before = hit.len();
            for n in nodes {
                let Some(id) = n["id"].as_str() else { continue };
                let deps = n["deps"].as_array().into_iter().flatten().filter_map(|d| d["pkg"].as_str());
                if !hit.contains(id) && deps.clone().any(|d| hit.contains(d)) {
                    hit.insert(id);
                }
            }
            if hit.len() == before {
                break;
            }
        }
    }
    let mut out: Vec<String> =
        local.iter().filter(|(id, _, _)| hit.contains(id) && members.contains(id)).map(|(_, name, _)| name.to_string()).collect();
    out.sort();
    out.dedup();
    Ok(out)
}

/// `git <args>` in `dir`, its output split into lines. Paths are printed as they are, not quoted
/// (`core.quotepath=off`), so a non-ASCII file name still matches its package.
fn git_lines(dir: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    let out =
        Command::new("git").args(["-c", "core.quotepath=off"]).args(args).current_dir(dir).output().map_err(|e| format!("git {args:?}: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).filter(|l| !l.is_empty()).collect())
}

/// `cargo xtask test-changed [--base <ref>] [--with-dependents] [--dry-run] [-- <test arguments>]`:
/// runs `cargo test --all-features` for [`crates_to_test`] of the files changed since the merge
/// base with `<ref>` (default: the branch's upstream, `@{upstream}`, so what was not pushed yet;
/// without one, `origin/dev`, so everything since the branch left it), committed, staged,
/// unstaged or untracked. Dependents only with `--with-dependents`.
fn test_changed(args: &[String]) -> Result<(), String> {
    let (ours, passed) = match args.iter().position(|a| a == "--") {
        Some(i) => (args.get(..i).unwrap_or_default(), args.get(i + 1..).unwrap_or_default()),
        None => (args, &[][..]),
    };
    let mut base: Option<&str> = None;
    let mut dry_run = false;
    let mut with_dependents = false;
    let mut it = ours.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--base" => base = Some(it.next().ok_or("--base needs a git ref")?),
            "--dry-run" => dry_run = true,
            "--with-dependents" => with_dependents = true,
            other => return Err(format!("test-changed: unknown argument {other:?}\n{USAGE}")),
        }
    }
    let root = ecad_root();
    let repo = root.parent().ok_or("no repository root")?;
    let base = match base {
        Some(b) => b.to_string(),
        // The upstream (what was not pushed yet); a branch never pushed: since it left `origin/dev`
        // (the diff below is from the merge base).
        None => ["@{upstream}", "origin/dev", "HEAD"]
            .into_iter()
            .find(|b| git_lines(repo, &["rev-parse", "--verify", "--quiet", b]).is_ok())
            .ok_or("test-changed: no upstream, no origin/dev and no HEAD; pass --base <ref>")?
            .to_string(),
    };
    let mut changed: BTreeSet<String> = BTreeSet::new();
    changed.extend(git_lines(repo, &["diff", "--no-renames", "--name-only", &format!("{base}...HEAD")])?);
    changed.extend(git_lines(repo, &["diff", "--no-renames", "--name-only", "HEAD"])?);
    changed.extend(git_lines(repo, &["ls-files", "--others", "--exclude-standard"])?);
    let out = Command::new(env!("CARGO")).args(["metadata", "--format-version", "1"]).current_dir(&root).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let meta: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let changed: Vec<String> = changed.into_iter().collect();
    let crates = crates_to_test(&meta, &repo.to_string_lossy(), &changed, with_dependents)?;
    let scope = if with_dependents { "crates and their dependents" } else { "crates" };
    println!("test-changed: {} files changed since {base}; {scope}: {}", changed.len(), crates.join(" "));
    if crates.is_empty() {
        println!("test-changed: nothing to test");
        return Ok(());
    }
    if dry_run {
        return Ok(());
    }
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["test", "--all-features"]).current_dir(&root);
    for c in &crates {
        cmd.args(["-p", c]);
    }
    if !passed.is_empty() {
        cmd.arg("--").args(passed);
    }
    run(&mut cmd)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    let r = match args.first().map(String::as_str).unwrap_or_default() {
        "layers" => layers(),
        "pub-unreachable" => pub_unreachable(),
        "patches" => patches(),
        "lint" => lint(),
        "ci" => ci(),
        "bench" => bench(rest),
        "demo" => demo(rest),
        "test-changed" => test_changed(rest),
        "clean-builds" => clean::clean_builds(rest, &ecad_root()),
        _ => Err(USAGE.into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    /// Literal text styling in drawing code is found; styles, bounds, comments and the
    /// test module are not.
    #[test]
    fn drawn_text_guard_finds_literal_text_styling() {
        let cases: &[(&str, usize)] = &[
            ("fn f() { let t = Text { height: 2.5, style: x }; }", 1),
            ("fn f() { let t = Text { height: -1.0 }; }", 1),
            ("fn f() { t.style = \"Standard\".into(); }", 1),
            ("fn f() { let s = STYLE_BOLD; }", 1),
            ("pub const XREF_TEXT_MM: f64 = 2.5;", 1),
            ("const FULL_HEIGHT_MM: f64 = 260.0;", 1),
            ("const MAX_TEXT_MM: f64 = 10.0;", 0),
            ("fn f() { let t = Text { height: style.height }; }", 0),
            ("// height: 2.5 in a comment", 0),
            ("const GAP_MM: f64 = 1.0;", 0),
            (
                "fn f() {}
#[cfg(test)]
mod tests {
    fn t() { let h = Text { height: 2.5 }; }
}",
                0,
            ),
        ];
        for (text, n) in cases {
            assert_eq!(drawn_text_places("x.rs", text).len(), *n, "{text}");
        }
    }

    use super::*;

    #[test]
    fn core_crates_may_not_use_cadcraft() {
        let meta: Value = serde_json::json!({"packages": [
            {"name": "ecad-model", "dependencies": [{"name": "cadcraft-geom", "kind": null}]},
            {"name": "ecad-engine", "dependencies": [{"name": "cadcraft-engine", "kind": null}, {"name": "egui", "kind": null}]},
            {"name": "ecad-store", "dependencies": [{"name": "cadcraft-engine", "kind": "dev"}]},
            {"name": "mystery", "dependencies": []}
        ], "workspace_root": ""});
        let v = layer_violations(&meta).unwrap();
        assert_eq!(v.len(), 3, "{v:?}");
    }

    /// A product crate may depend on a `dev/` crate only
    /// optionally, through its own `dev` feature, never on by default (except the desktop app); a
    /// crate is under `dev/` exactly when its class is Dev.
    #[test]
    fn product_crates_reach_developer_tools_only_through_their_dev_feature() {
        let pkg = |name: &str, dir: &str, deps: Value, features: Value| serde_json::json!({"name": name, "manifest_path": format!("C:\\w\\{dir}\\Cargo.toml"), "dependencies": deps, "features": features});
        let insp = |optional: bool, kind: Option<&str>| serde_json::json!([{"name": "ecad-inspector", "kind": kind, "optional": optional}]);
        let rows: &[(Value, usize, &str)] = &[
            (pkg("ecad-file", "crates\\ecad-file", insp(false, None), serde_json::json!({})), 1, "a plain dependency"),
            (pkg("ecad-file", "crates\\ecad-file", insp(false, Some("dev")), serde_json::json!({})), 1, "a dev-dependency"),
            (pkg("ecad-file", "crates\\ecad-file", insp(true, None), serde_json::json!({"dev": ["dep:ecad-inspector"]})), 0, "optional, by `dev`"),
            (
                pkg(
                    "ecad-file",
                    "crates\\ecad-file",
                    insp(true, None),
                    serde_json::json!({"dev": ["dep:ecad-inspector"], "extra": ["ecad-inspector/x"]}),
                ),
                1,
                "also enabled by another feature",
            ),
            (
                pkg(
                    "ecad-file",
                    "crates\\ecad-file",
                    insp(true, None),
                    serde_json::json!({"ecad-inspector": ["dep:ecad-inspector"], "dev": ["ecad-inspector"]}),
                ),
                1,
                "an implicit feature named after it",
            ),
            (
                pkg("ecad-file", "crates\\ecad-file", insp(true, None), serde_json::json!({"default": ["dev"], "dev": ["dep:ecad-inspector"]})),
                1,
                "dev on by default",
            ),
            (
                pkg("ecad-app", "crates\\ecad-app", insp(true, None), serde_json::json!({"default": ["dev"], "dev": ["dep:ecad-inspector"]})),
                0,
                "the desktop app's dev on by default (Q1)",
            ),
            (pkg("ecad-inspector", "crates\\ecad-inspector", serde_json::json!([]), serde_json::json!({})), 1, "a Dev crate outside dev/"),
            (pkg("ecad-file", "dev\\ecad-file", serde_json::json!([]), serde_json::json!({})), 1, "a product crate inside dev/"),
            (
                pkg("ecad-inspector", "dev\\ecad-inspector", serde_json::json!([{"name": "egui", "kind": null}]), serde_json::json!({})),
                0,
                "a Dev crate may use anything",
            ),
        ];
        for (p, want, what) in rows {
            let meta = serde_json::json!({"workspace_root": "C:\\w", "packages": [p]});
            let v = layer_violations(&meta).unwrap();
            assert_eq!(v.len(), *want, "{what}: {v:?}");
        }
    }

    /// Rule 2 (and 4): dev features are read only in the crate's `dev` module, or on the attribute
    /// line before a one-line hook into it.
    #[test]
    fn dev_features_are_read_only_in_the_dev_module() {
        let rows: &[(&str, &str, usize)] = &[
            ("crates/c/src/dev.rs", "fn f() -> bool { cfg!(feature = \"verify-invariants\") }", 0),
            ("crates/c/src/dev/oracle.rs", "#[cfg(feature = \"fault-injection\")]\nfn big() {\n}\n", 0),
            ("crates/c/src/lib.rs", "#[cfg(feature = \"test-ops\")]\nmod dev;\n", 0),
            ("crates/c/src/ledger.rs", "    #[cfg(feature = \"fault-injection\")]\n    crate::dev::maybe_panic(at);\n", 0),
            ("crates/c/src/ledger.rs", "    #[cfg(feature = \"fault-injection\")]\n    if crate::dev::fail() {\n        return;\n    }\n", 1),
            ("crates/c/src/ledger.rs", "fn f() -> bool { cfg!(feature = \"verify-invariants\") }", 1),
            ("crates/c/src/ledger.rs", "#[cfg(any(test, feature=\"dev\"))]\nfn helper() {}\n", 1),
            ("crates/c/src/ledger.rs", "// #[cfg(feature = \"dev\")] in a comment\n#[cfg(feature = \"serde\")]\nfn f() {}\n", 0),
        ];
        for (rel, text, want) in rows {
            let v = dev_feature_places(rel, text);
            assert_eq!(v.len(), *want, "{rel}: {text:?}: {v:?}");
        }
    }

    /// Rule 3 (and 4): no product source reads a file under `dev/`.
    #[test]
    fn product_sources_do_not_read_files_under_dev() {
        let root = Path::new("C:/w");
        let krate = Path::new("C:/w/apps/ecad-mcp");
        let file = Path::new("C:/w/apps/ecad-mcp/src/demo.rs");
        let rows: &[(&str, usize)] = &[
            // The path on the next line, `include!`, and a path built on the crate
            // folder.
            ("pub const T: &str = include_str!(\n    \"../../../dev/ecad-inspector/page/index.html\"\n);", 1),
            ("include!(\"../../../dev/ecad-inspector/src/text.rs\");", 1),
            ("const P: &str = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../dev/ecad-inspector/page/index.html\"));", 1),
            ("const Q: &str = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../examples/demo/commands.jsonl\"));", 0),
            ("let url = \"http://x\"; // include_str!(\"../../../dev/x\")", 0),
            ("pub const C: &str = include_str!(\"../../../examples/demo/commands.jsonl\");", 0),
            ("pub const P: &str = include_str!(\"../../../dev/ecad-inspector/page/index.html\");", 1),
            ("static B: &[u8] = include_bytes!(\"../../../dev/x.bin\");", 1),
            ("#[path = \"../../../dev/ecad-inspector/src/text.rs\"]", 1),
            ("// include_str!(\"../../../dev/x\") in a comment", 0),
        ];
        for (text, want) in rows {
            assert_eq!(dev_file_reads(root, krate, file, text).len(), *want, "{text}");
        }
    }

    /// Every `layers` rule on this workspace: no violation, and every allowlist entry is still needed.
    #[test]
    fn this_workspace_keeps_developer_tools_apart() {
        let root = ecad_root();
        let out = Command::new(env!("CARGO")).args(["metadata", "--format-version", "1", "--no-deps"]).current_dir(&root).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let meta: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(layer_violations(&meta).unwrap(), Vec::<String>::new());
        assert_eq!(source_violations(&meta).unwrap(), Vec::<String>::new());
        assert_eq!(path_violations(&meta).unwrap(), Vec::<String>::new());
        assert_eq!(dependency_rule_violations(&meta).unwrap(), Vec::<String>::new());
        assert_eq!(core_os_violations(&meta).unwrap(), Vec::<String>::new());
        assert_eq!(readme_violations(&meta).unwrap(), Vec::<String>::new());
    }

    /// A small graph: `core` ← `mid` ← `app`, `mid` ← `bench` (dev only), `lone`, and an upstream
    /// crate `up` ← `mid`, with the workspace in `ws/`.
    fn graph() -> Value {
        let pkg = |name: &str, dir: &str| serde_json::json!({"id": name, "name": name, "source": null, "manifest_path": format!("C:\\r\\{dir}\\Cargo.toml")});
        let node =
            |id: &str, deps: &[&str]| serde_json::json!({"id": id, "deps": deps.iter().map(|d| serde_json::json!({"pkg": d})).collect::<Vec<_>>()});
        serde_json::json!({
            "packages": [
                pkg("core", "ws\\crates\\core"), pkg("mid", "ws\\crates\\mid"), pkg("app", "ws\\apps\\app"),
                pkg("bench", "ws\\crates\\bench"), pkg("lone", "ws\\apps\\lone"), pkg("up", "crates\\up"),
                {"id": "serde", "name": "serde", "source": "registry", "manifest_path": "C:\\cargo\\serde\\Cargo.toml"}
            ],
            "workspace_root": "C:\\r\\ws",
            "workspace_members": ["core", "mid", "app", "bench", "lone"],
            "resolve": {"nodes": [
                node("core", &["serde"]), node("mid", &["core", "up"]), node("app", &["mid"]),
                node("bench", &["mid"]), node("lone", &["serde"]), node("up", &[]), node("serde", &[])
            ]}
        })
    }

    #[test]
    fn test_changed_picks_changed_crates_and_their_dependents() {
        let rows: &[(&[&str], &[&str])] = &[
            (&["ws/crates/core/src/lib.rs"], &["app", "bench", "core", "mid"]),
            (&["ws/apps/lone/src/main.rs"], &["lone"]),
            (&["ws/apps/app/tests/all/x.rs"], &["app"]),
            (&["crates/up/src/lib.rs"], &["app", "bench", "mid"]),
            (&["ws/Cargo.lock"], &["app", "bench", "core", "lone", "mid"]),
            (&["ws/.cargo/config.toml"], &["app", "bench", "core", "lone", "mid"]),
            // The repository root: CADCraft's manifest and cargo configuration reach every build; its
            // lock file does not (the workspace has its own).
            (&["Cargo.toml"], &["app", "bench", "core", "lone", "mid"]),
            (&[".cargo/config.toml"], &["app", "bench", "core", "lone", "mid"]),
            (&["Cargo.lock"], &[]),
            // A file moved between crates: with `--no-renames` git lists both paths, and both count.
            (&["ws/apps/lone/src/moved.rs", "ws/crates/core/src/moved.rs"], &["app", "bench", "core", "lone", "mid"]),
            (&["ws/crates/core/README.md", "ws/docs/x.rs", ".github/workflows/ci.yml"], &[]),
            (&["WS\\Apps\\Lone\\src\\main.rs"], &["lone"]),
        ];
        for (changed, want) in rows {
            let changed: Vec<String> = changed.iter().map(|s| s.to_string()).collect();
            assert_eq!(crates_to_test(&graph(), "c:/r/", &changed, true).unwrap(), *want, "{changed:?}");
        }
    }

    /// Without `--with-dependents` (the worker's default), only the changed crates themselves.
    #[test]
    fn test_changed_without_dependents_picks_only_the_changed_crates() {
        let rows: &[(&[&str], &[&str])] = &[
            (&["ws/crates/core/src/lib.rs"], &["core"]),
            (&["crates/up/src/lib.rs"], &[]),
            (&["ws/apps/app/tests/all/x.rs", "ws/crates/mid/src/m.rs"], &["app", "mid"]),
            (&["ws/Cargo.lock"], &["app", "bench", "core", "lone", "mid"]),
        ];
        for (changed, want) in rows {
            let changed: Vec<String> = changed.iter().map(|s| s.to_string()).collect();
            assert_eq!(crates_to_test(&graph(), "c:/r/", &changed, false).unwrap(), *want, "{changed:?}");
        }
    }

    /// `test-changed` on the real workspace graph.
    #[test]
    fn test_changed_on_this_workspace() {
        let root = ecad_root();
        let out = Command::new(env!("CARGO")).args(["metadata", "--format-version", "1"]).current_dir(&root).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let meta: Value = serde_json::from_slice(&out.stdout).unwrap();
        let repo = root.parent().unwrap().to_string_lossy().to_string();
        let pick = |f: &str| crates_to_test(&meta, &repo, &[f.to_string()], true).unwrap();
        assert_eq!(
            pick("ecad/crates/ecad-model/src/lib.rs"),
            [
                "ecad-app",
                "ecad-bench",
                "ecad-desktop",
                "ecad-drawing-style",
                "ecad-electrical",
                "ecad-engine",
                "ecad-file",
                "ecad-inspector",
                "ecad-mcp",
                "ecad-model",
                "ecad-widgets"
            ]
        );
        assert_eq!(pick("ecad/crates/ecad-app/src/lib.rs"), ["ecad-app", "ecad-desktop", "ecad-mcp"]);
        assert_eq!(
            pick("ecad/crates/ecad-store/src/lib.rs"),
            [
                "ecad-app",
                "ecad-bench",
                "ecad-desktop",
                "ecad-drawing-style",
                "ecad-electrical",
                "ecad-engine",
                "ecad-file",
                "ecad-inspector",
                "ecad-mcp",
                "ecad-store"
            ]
        );
        assert_eq!(pick("crates/engine/src/lib.rs"), ["ecad-app", "ecad-bench", "ecad-desktop", "ecad-engine", "ecad-mcp"]);
        assert!(pick("ecad/docs/quick-start.md").is_empty());
        // The demo's command list is embedded by ecad-mcp (#53: the app's tests no longer open it).
        assert_eq!(pick("ecad/examples/demo/commands.jsonl"), ["ecad-mcp"]);
        assert_eq!(pick("Cargo.toml").len(), meta["workspace_members"].as_array().map_or(0, Vec::len));
    }

    #[test]
    fn inventory_and_commit_are_parsed() {
        let inv = "| File | Why |\n| --- | --- |\n| `crates/engine/src/lib.rs` | X3 |\n| `README.md` | ours |\n";
        assert_eq!(inventory(inv), vec!["crates/engine/src/lib.rs", "README.md"]);
        assert_eq!(upstream_commit("x\nCommit: `00c7613` (0.6.2)\n").as_deref(), Some("00c7613"));
    }

    /// #53: a dev-dependency is held to the layer order too: no crate depends on a layer above it, not even in tests.
    #[test]
    fn dev_dependencies_may_not_reach_a_layer_above() {
        let one = |name: &str, dep: &str, kind: Value| serde_json::json!({"packages": [{"name": name, "dependencies": [{"name": dep, "kind": kind}]}], "workspace_root": ""});
        let rows: &[(&str, &str, Value, usize, &str)] = &[
            ("ecad-app", "ecad-mcp", serde_json::json!("dev"), 1, "App tests using a program"),
            ("ecad-app", "ecad-desktop", serde_json::json!("dev"), 1, "App tests using a program"),
            ("ecad-mcp", "ecad-app", serde_json::json!("dev"), 0, "a program's tests use the app"),
            ("ecad-electrical", "ecad-engine", serde_json::json!("dev"), 1, "Core tests using the engine"),
            ("ecad-engine", "ecad-model", serde_json::json!("dev"), 0, "the engine's tests use Core"),
        ];
        for (name, dep, kind, n, what) in rows {
            let v = layer_violations(&one(name, dep, kind.clone())).unwrap();
            assert_eq!(v.len(), *n, "{what}: {v:?}");
        }
    }

    /// The UiKit crate may use egui, epaint, egui_extras and
    /// ecad-model only (dev-dependencies aside); a `cadcraft-*` crate or another ECAD crate is a
    /// violation.
    #[test]
    fn the_ui_kit_uses_egui_and_the_tokens_only() {
        let kit = |deps: Value| serde_json::json!({"packages": [{"name": "ecad-widgets", "dependencies": deps}], "workspace_root": ""});
        let rows: &[(Value, usize, &str)] = &[
            (
                serde_json::json!([{"name": "egui", "kind": null}, {"name": "ecad-model", "kind": null}, {"name": "egui_extras", "kind": null}]),
                0,
                "allowed",
            ),
            (serde_json::json!([{"name": "egui_kittest", "kind": "dev"}]), 0, "a test-only dependency"),
            (serde_json::json!([{"name": "cadcraft-ui-egui", "kind": null}]), 1, "a CADCraft crate"),
            (serde_json::json!([{"name": "cadcraft-fonts", "kind": null}]), 1, "a CADCraft crate below the UI"),
            (serde_json::json!([{"name": "ecad-file", "kind": null}]), 1, "another ECAD crate"),
            (serde_json::json!([{"name": "eframe", "kind": null}]), 1, "an app shell"),
        ];
        for (deps, n, what) in rows {
            let v = layer_violations(&kit(deps.clone())).unwrap();
            assert_eq!(v.len(), *n, "{what}: {v:?}");
        }
    }

    /// ECAD UI sources take colours, sizes and fonts from the
    /// tokens; local styling is found, comments are not.
    #[test]
    fn local_styling_is_found() {
        let rows: &[(&str, usize)] = &[
            ("let c = egui::Color32::from_rgb(1, 2, 3);", 1),
            (
                "let c = Color32::from_gray(9);
let f = FontId::new(12.0, FontFamily::Proportional);",
                2,
            ),
            ("let r = egui::CornerRadius::same(4);", 1),
            ("let c = egui::Color32::from_rgba_unmultiplied(1, 2, 3, 4);", 1),
            ("let f = egui::FontId::proportional(12.0);", 1),
            ("let f = egui::FontId::monospace(12.0);", 1),
            ("let r = egui::Rounding::same(2.0);", 1),
            ("let c = theme::Tokens::get().text;", 1),
            (
                "// Color32::from_rgb(1, 2, 3) in a comment
let c = ecad_widgets::color(SHELL.text);",
                0,
            ),
            ("let c = theme::tokens().text; // not Color32::from_rgb", 0),
        ];
        for (text, n) in rows {
            assert_eq!(local_style_places("crates/ecad-app/src/x.rs", text).len(), *n, "{text}");
        }
    }

    /// The text-error counter counts lines, outside comments.
    #[test]
    fn text_errors_are_counted_by_line() {
        let text = "fn a() -> Result<u8, String> {
    Err(format!(\"x\"))
}
// fn b() -> Result<(), String>
fn c() -> std::result::Result<Vec<u8>,String> { Ok(vec![]) }
";
        assert_eq!(string_error_counts(text), (2, 1));
    }

    /// The ratchet message for each case.
    #[test]
    fn a_ratchet_count_only_falls() {
        assert!(ratchet_problem("a.rs", 3, 3, "things", "fix").is_none());
        assert!(ratchet_problem("a.rs", 4, 3, "things", "fix").is_some_and(|m| m.contains("allows 3")));
        assert!(ratchet_problem("a.rs", 2, 3, "things", "fix").is_some_and(|m| m.contains("lower the count")));
        assert!(ratchet_problem("a.rs", 0, 3, "things", "fix").is_some_and(|m| m.contains("remove")));
    }

    /// Direct OS access in Core is found where a name starts, outside comments and the
    /// test module.
    #[test]
    fn core_os_access_is_found_outside_comments_and_tests() {
        let rows: &[(&str, usize)] = &[
            ("let b = std::fs::read(p)?;", 1),
            ("use std::fs;\nlet b = fs::read(p)?;", 2),
            ("let v = std::env::var(\"X\");", 1),
            ("let v = env::var(\"X\");", 1),
            ("let id = std::process::id();", 1),
            ("let t = std::time::SystemTime::now();", 1),
            ("let t = Instant::now();", 1),
            ("let t = chrono::Utc::now();", 1),
            ("let r = prefs::get();", 0),
            ("// std::fs::read in a comment", 0),
            ("let a = 1;\n#[cfg(test)]\nmod tests {\n    fn t() { std::fs::read(\"x\"); }\n}", 0),
        ];
        for (text, n) in rows {
            assert_eq!(core_os_places("crates/ecad-model/src/x.rs", text).len(), *n, "{text}");
        }
    }

    /// `anyhow` only in Programs, xtask and ecad-bench, `tokio` nowhere,
    /// `rayon` only in model, file, electrical and cadcraft as a plain dependency.
    #[test]
    fn the_dependency_rules() {
        let dep = |name: &str, kind: Value, target: Value| serde_json::json!({"name": name, "kind": kind, "target": target});
        let opt = |name: &str| serde_json::json!({"name": name, "kind": null, "target": null, "optional": true});
        let none = serde_json::json!({});
        let wasm: Value = "cfg(not(target_arch = \"wasm32\"))".into();
        let rows: &[(&str, Value, &Value, usize, &str)] = &[
            ("ecad-model", dep("anyhow", Value::Null, Value::Null), &none, 1, "anyhow in a library"),
            ("ecad-model", dep("anyhow", "dev".into(), Value::Null), &none, 0, "anyhow in a library's tests"),
            ("ecad-desktop", dep("anyhow", Value::Null, Value::Null), &none, 0, "anyhow in a program"),
            ("ecad-mcp", dep("anyhow", Value::Null, Value::Null), &none, 0, "anyhow in a program"),
            ("ecad-app", dep("anyhow", Value::Null, Value::Null), &none, 1, "anyhow in the app library"),
            ("ecad-engine", dep("anyhow", Value::Null, Value::Null), &none, 1, "anyhow in the Engine"),
            ("ecad-xtask", dep("anyhow", Value::Null, Value::Null), &none, 0, "anyhow in xtask"),
            ("ecad-bench", dep("anyhow", Value::Null, Value::Null), &none, 0, "anyhow in ecad-bench"),
            ("ecad-inspector", dep("anyhow", Value::Null, Value::Null), &none, 1, "anyhow in another Dev crate"),
            ("ecad-desktop", dep("tokio", Value::Null, Value::Null), &none, 1, "tokio in a program"),
            ("ecad-store", dep("tokio", "dev".into(), Value::Null), &none, 1, "tokio in tests"),
            ("ecad-model", dep("rayon", Value::Null, Value::Null), &none, 0, "rayon in the model"),
            ("ecad-electrical", dep("rayon", Value::Null, Value::Null), &none, 0, "rayon in the rules"),
            ("ecad-engine", dep("rayon", Value::Null, Value::Null), &none, 0, "rayon in the Engine"),
            ("ecad-file", dep("rayon", Value::Null, wasm.clone()), &none, 1, "rayon in a target table"),
            ("ecad-file", dep("rayon", "dev".into(), Value::Null), &none, 1, "rayon as a dev-dependency"),
            ("ecad-file", opt("rayon"), &none, 1, "rayon optional"),
            ("ecad-app", dep("rayon", Value::Null, Value::Null), &none, 1, "rayon in the app"),
            ("ecad-store", dep("rayon", Value::Null, Value::Null), &none, 1, "rayon in the store"),
            ("ecad-model", dep("serde", Value::Null, Value::Null), &none, 0, "anything else"),
        ];
        for (pkg, d, features, n, what) in rows {
            let meta = serde_json::json!({"packages": [{"name": pkg, "dependencies": [d], "features": features}]});
            assert_eq!(dependency_rule_violations(&meta).unwrap().len(), *n, "{what}: {pkg}: {d}");
        }
    }

    /// App and Programs sources name no `cadcraft_engine` path, ecad-electrical's
    /// no `cadcraft_` path; comments and longer names do not count, and other layers are free.
    #[test]
    fn forbidden_paths_are_found_in_app_programs_and_the_rules() {
        let rows: &[(&str, &str, usize)] = &[
            ("ecad-app", "use cadcraft_engine::Document;", 1),
            ("ecad-desktop", "fn f(e: &cadcraft_engine::Entity) {}", 1),
            ("ecad-mcp", "use cadcraft_engine as e;", 1),
            ("ecad-mcp", "use ecad_engine::cad::Entity;", 0),
            ("ecad-app", "// cadcraft_engine::Document in a comment", 0),
            ("ecad-app", "use cadcraft_ui_egui::App;", 0),
            ("ecad-electrical", "use cadcraft_io::dxf;", 1),
            ("ecad-electrical", "let m = ecad_engine_like::x();", 0),
            ("ecad-electrical", "use ecad_model::Document; // not cadcraft_engine", 0),
            ("ecad-model", "use cadcraft_engine::Document;", 0),
            ("ecad-engine", "use cadcraft_engine::Document;", 0),
            ("ecad-bench", "use cadcraft_engine::Document;", 0),
        ];
        for (krate, text, n) in rows {
            let class = class_of(krate).unwrap();
            assert_eq!(forbidden_path_places(krate, class, "x.rs", text).len(), *n, "{krate}: {text}");
        }
    }

    /// A Core crate other than ecad-store may not depend on a crate that finds
    /// the user's folders; ecad-store and tests may.
    #[test]
    fn core_crates_do_not_find_the_users_folders() {
        let rows: &[(&str, &str, Value, usize)] = &[
            ("ecad-model", "dirs", Value::Null, 1),
            ("ecad-electrical", "directories", Value::Null, 1),
            ("ecad-file", "dirs", "dev".into(), 0),
            ("ecad-store", "dirs", Value::Null, 0),
            ("ecad-app", "dirs", Value::Null, 0),
            ("ecad-model", "serde", Value::Null, 0),
        ];
        for (krate, dep, kind, n) in rows {
            let meta = serde_json::json!({"workspace_root": "", "packages": [{"name": krate, "dependencies": [{"name": dep, "kind": kind}]}]});
            assert_eq!(core_os_violations(&meta).unwrap().len(), *n, "{krate} -> {dep}");
        }
    }

    /// A README starts with `# <crate name>`, has the five sections in order and
    /// names the crate's layer; headings in code blocks do not count.
    #[test]
    fn a_readme_has_the_six_headings_and_the_layer() {
        let sections = "## Purpose\nx\n## Contents\n## Must not depend on\n## Key types\n## How to test\n";
        let good = format!("# ecad-model\n\nLayer: Core\n\n{sections}");
        let long_title = format!("# ecad-model: the model\nLayer: Core\n{sections}");
        let no_layer = format!("# ecad-model\n{sections}");
        let fenced = format!("# ecad-model\nLayer: Core\n## Purpose\n```sh\n{sections}```\n");
        let kit = format!("# ecad-widgets\nLayer: UI kit\n{sections}");
        let xtask = format!("# ecad-xtask\nLayer: Dev\n{sections}");
        let rows: &[(&str, Class, Option<&str>, bool, &str)] = &[
            ("ecad-model", Class::Core, Some(&good), true, "the template"),
            ("ecad-model", Class::Core, None, false, "no README"),
            ("ecad-model", Class::Engine, Some(&good), false, "the wrong layer"),
            ("ecad-store", Class::Core, Some(&good), false, "another crate's title"),
            ("ecad-model", Class::Core, Some(&long_title), false, "a longer title"),
            (
                "ecad-model",
                Class::Core,
                Some("# ecad-model\nLayer: Core\n## Contents\n## Purpose\n## Must not depend on\n## Key types\n## How to test\n"),
                false,
                "sections out of order",
            ),
            (
                "ecad-model",
                Class::Core,
                Some("# ecad-model\nLayer: Core\n## Purpose\n## Contents\n## Must not depend on\n## How to test\n"),
                false,
                "a section missing",
            ),
            ("ecad-model", Class::Core, Some(&no_layer), false, "no Layer line"),
            (
                "ecad-model",
                Class::Core,
                Some(
                    "# ecad-model\nLayer: Core\n## Purpose\n### Detail\n## Notes\n## Contents\n## Must not depend on\n## Key types\n## How to test\n",
                ),
                true,
                "extra headings between",
            ),
            ("ecad-model", Class::Core, Some(&fenced), false, "sections only in a code block"),
            ("ecad-widgets", Class::UiKit, Some(&kit), true, "the UI kit"),
            ("ecad-xtask", Class::Exempt, Some(&xtask), true, "xtask is Dev"),
        ];
        for (krate, class, text, ok, what) in rows {
            let p = readme_problem(krate, *class, *text);
            assert_eq!(p.is_none(), *ok, "{what}: {p:?}");
        }
    }

    /// A README's Contents names its files and folders in backticks; other backticked words, paths elsewhere
    /// (`ecad/…`) and the other sections are not read.
    #[test]
    fn readme_contents_paths_are_read() {
        let text =
            "# c\n## Purpose\n`purpose.rs`\n## Contents\n- `cables/`: cables; `lib.rs`, `Kind`, `ecad/docs/`\n- `a b.rs`\n## Key types\n`keys.rs`\n";
        assert_eq!(readme_contents_paths(text), vec!["cables/", "lib.rs"]);
        assert!(readme_contents_paths("# c\n## Purpose\n`x.rs`\n").is_empty());
    }

    /// The README allowlist only shrinks: an unlisted failure fails, a listed one
    /// does not, and a listed crate that passes or is gone must be removed.
    #[test]
    fn the_readme_allowlist_only_shrinks() {
        let fail = || Some("no README".to_string());
        let rows: Vec<(Vec<(&str, Option<String>)>, &[&str], usize, &str)> = vec![
            (vec![("a", None), ("b", None)], &[], 0, "all pass"),
            (vec![("a", fail()), ("b", None)], &[], 1, "an unlisted failure"),
            (vec![("a", fail()), ("b", None)], &["a"], 0, "a listed failure"),
            (vec![("a", None)], &["a"], 1, "a listed crate that passes"),
            (vec![("a", None)], &["gone"], 1, "a listed crate that is gone"),
            (vec![("a", fail()), ("b", fail())], &["a"], 1, "one listed, one not"),
        ];
        for (results, allow, n, what) in rows {
            assert_eq!(readme_ratchet(&results, allow).len(), n, "{what}");
        }
    }

    /// App and Programs reach the engine only through ecad-engine; App may
    /// dev-depend on a program (ecad-app's tests use ecad-desktop's startup), never depend on one.
    #[test]
    fn app_and_programs_do_not_use_the_engine_directly() {
        let rows: &[(&str, &str, Value, usize, &str)] = &[
            ("ecad-mcp", "cadcraft-engine", Value::Null, 1, "a program on the engine"),
            ("ecad-app", "cadcraft-engine", Value::Null, 1, "the app on the engine"),
            ("ecad-app", "cadcraft-engine", "dev".into(), 0, "the app's tests on the engine"),
            ("ecad-app", "ecad-desktop", Value::Null, 1, "the app on a program"),
            ("ecad-app", "ecad-desktop", "dev".into(), 1, "the app's tests on a program"),
            ("ecad-app", "cadcraft-ui-egui", Value::Null, 0, "the app on CADCraft's window"),
            ("ecad-desktop", "ecad-app", Value::Null, 0, "a program on the app"),
            ("ecad-mcp", "ecad-desktop", Value::Null, 0, "a program on another program"),
            ("ecad-mcp", "cadcraft-mcp", Value::Null, 0, "a program on CADCraft's MCP"),
            ("ecad-engine", "cadcraft-engine", Value::Null, 0, "the Engine on the engine"),
            ("ecad-engine", "cadcraft-io", Value::Null, 0, "the Engine on CADCraft's io"),
            ("ecad-engine", "cadcraft-ui-egui", Value::Null, 1, "the Engine on CADCraft's window"),
            ("ecad-engine", "cadcraft-fonts", Value::Null, 1, "the Engine on another CADCraft crate"),
            ("ecad-engine", "cadcraft-ui-egui", "dev".into(), 0, "the Engine's tests on CADCraft's window"),
            ("ecad-engine", "ecad-app", Value::Null, 1, "the Engine on the app"),
            ("ecad-engine", "ecad-mcp", Value::Null, 1, "the Engine on a program"),
            ("ecad-electrical", "ecad-engine", Value::Null, 1, "Core on the Engine"),
            ("ecad-model", "ecad-app", Value::Null, 1, "Core on the app"),
            ("ecad-model", "ecad-mcp", Value::Null, 1, "Core on a program"),
            ("ecad-xtask", "cadcraft-engine", Value::Null, 0, "xtask on the engine"),
        ];
        for (krate, dep, kind, n, what) in rows {
            let meta = serde_json::json!({"packages": [{"name": krate, "manifest_path": if *krate == "ecad-xtask" { "/dev/xtask/Cargo.toml" } else { "" }, "dependencies": [{"name": dep, "kind": kind}]}], "workspace_root": ""});
            assert_eq!(layer_violations(&meta).unwrap().len(), *n, "{what}: {krate} -> {dep}");
        }
    }

    /// A UI file that styles locally fails `layers`, whichever file it is.
    #[test]
    fn a_ui_file_that_styles_locally_fails_layers() {
        let root = std::env::temp_dir().join(format!("ecad-xtask-style-{}", std::process::id()));
        let src = root.join("crates").join("ecad-app").join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("new_panel.rs"),
            "fn f() -> egui::Color32 { egui::Color32::from_rgb(1, 2, 3) }
",
        )
        .unwrap();
        std::fs::write(
            {
                let d = src.join("pages");
                std::fs::create_dir_all(&d).unwrap();
                d.join("preview.rs")
            },
            "fn f() -> egui::Color32 { egui::Color32::from_gray(7) }
",
        )
        .unwrap();
        let meta = serde_json::json!({
            "workspace_root": root.to_string_lossy(),
            "packages": [{"name": "ecad-app", "manifest_path": root.join("crates").join("ecad-app").join("Cargo.toml").to_string_lossy(), "dependencies": []}],
        });
        let v = source_violations(&meta).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(v.iter().any(|m| m.starts_with("crates/ecad-app/src/new_panel.rs:1:") && m.contains("no local styling")), "{v:?}");
        assert!(
            v.iter().any(|m| m.starts_with("crates/ecad-app/src/pages/preview.rs:1:") && m.contains("no local styling")),
            "no file is exempt: {v:?}"
        );
    }

    /// A UI file not in [`RAW_CONTROL_FILES`] that uses `egui::TextEdit` or a `ComboBox` fails
    /// `layers`; a listed one does not.
    #[test]
    fn a_new_ui_file_with_a_raw_text_edit_fails_layers() {
        let root = std::env::temp_dir().join(format!("ecad-xtask-raw-{}", std::process::id()));
        let src = root.join("crates").join("ecad-app").join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("new_panel.rs"), "fn f(ui: &mut egui::Ui, s: &mut String) { ui.add(egui::TextEdit::singleline(s)); }\n").unwrap();
        std::fs::write(
            {
                let d = src.join("library");
                std::fs::create_dir_all(&d).unwrap();
                d.join("search.rs")
            },
            "fn f(ui: &mut egui::Ui, s: &mut String) { ui.add(egui::TextEdit::singleline(s)); }\n",
        )
        .unwrap();
        let meta = serde_json::json!({
            "workspace_root": root.to_string_lossy(),
            "packages": [{"name": "ecad-app", "manifest_path": root.join("crates").join("ecad-app").join("Cargo.toml").to_string_lossy(), "dependencies": []}],
        });
        let v = source_violations(&meta).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(v.iter().any(|m| m.starts_with("crates/ecad-app/src/new_panel.rs:1:") && m.contains("no raw egui value controls")), "{v:?}");
        assert!(!v.iter().any(|m| m.contains("search.rs") && m.contains("raw egui value controls")), "allowlisted: {v:?}");
        assert_eq!(
            raw_control_places("x.rs", "// egui::ComboBox::from_id_salt(1)\nlet c = ecad_widgets::control::Select::new(&mut i, &o, \"n\");").len(),
            0
        );
    }

    /// Core and Engine crates may not depend on ECAD's UI kit or the egui family.
    #[test]
    fn core_and_engine_crates_may_not_use_the_ui_kit() {
        let rows: &[(&str, &str, usize)] = &[
            ("ecad-model", "ecad-widgets", 1),
            ("ecad-engine", "ecad-widgets", 1),
            ("ecad-engine", "epaint", 1),
            ("ecad-mcp", "egui_extras", 0),
            ("ecad-model", "ecad-widgets", 1),
            // The Core order: forward edges only.
            ("ecad-file", "ecad-store", 0),
            ("ecad-electrical", "ecad-file", 0),
            ("ecad-file", "ecad-electrical", 1),
            ("ecad-model", "ecad-store", 1),
            ("ecad-engine", "ecad-widgets", 1),
            ("ecad-app", "ecad-widgets", 0),
            ("ecad-desktop", "ecad-widgets", 0),
        ];
        for (krate, dep, n) in rows {
            let meta = serde_json::json!({"packages": [{"name": krate, "manifest_path": if *krate == "ecad-xtask" { "/dev/xtask/Cargo.toml" } else { "" }, "dependencies": [{"name": dep, "kind": null}]}], "workspace_root": ""});
            assert_eq!(layer_violations(&meta).unwrap().len(), *n, "{krate} -> {dep}");
        }
    }
}
