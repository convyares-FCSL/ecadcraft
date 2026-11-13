//! ECAD workspace checks.
//!
//! - `layers`: document crates must not depend on CADCraft; only the adapter may use the engine;
//!   only UI crates and apps may use UI toolkits (principle P10, proposal §8). Developer tools
//!   (`ecad/dev/`) are kept apart from product code (design 029 §12.3): no product crate depends
//!   on one except through its own `dev` feature, dev features are read only in a crate's `dev`
//!   module, and no product source reads a file under `dev/`. ECAD's interface kit (`ecad-ui`,
//!   class UiKit) uses egui and `ecad-tokens` only, and ECAD UI sources do not style locally
//!   (design `ui-architecture.md` §2–§3.2, brief 037).
//! - `patches`: every file changed outside `ecad/` since the incorporated CADCraft commit
//!   (`ecad/UPSTREAM.md`) must be listed in `ecad/UPSTREAM-PATCHES.md` (proposal §15, patch guard).
//! - `wasm`: the ECAD library crates build for `wasm32-unknown-unknown`, warnings as errors (as CI).
//! - `ci`: fmt, clippy `-D warnings`, tests (the slow set included), layers, patches, wasm.
//! - `bench`: the project load benchmark (brief 012), release build, run on demand only. It is
//!   deliberately not part of `ci`; arguments after `bench` are passed to `ecad-bench`.
//! - `test-changed`: the fast tests of the crates changed since the last push (the upstream
//!   branch; `--base` for another ref), and with `--with-dependents` of every crate that depends
//!   on them (brief 017; process clean-up, 2026-11-12). The worker's default check; CI's quick
//!   check runs it with `--base origin/dev --with-dependents` on pull requests into dev.
//! - `clean-builds`: delete the build folders of every checkout under a folder, skipping those in
//!   use, and remove worktrees whose branch is merged into `origin/dev` (see [`clean`]).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde_json::Value;

mod clean;

/// How an ECAD crate may depend on others.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Class {
    /// No `cadcraft-*` and no UI toolkit dependency.
    Core,
    /// May use CADCraft crates below the UI.
    Adapter,
    /// May use anything.
    Ui,
    /// ECAD's interface kit (design `ui-architecture.md` §2): may use egui, epaint and
    /// egui_extras and the token crate, but no `cadcraft-*` crate and no other ECAD crate, so the
    /// widgets stay swappable, build for wasm and test without wgpu.
    UiKit,
    /// A developer or test tool under `ecad/dev/`, never shipped: may use anything, and no
    /// product crate may depend on it except through its own `dev` feature (design 029 §12.3).
    Dev,
    Exempt,
}

const TABLE: &[(&str, Class)] = &[
    ("ecad-model", Class::Core),
    ("ecad-store", Class::Core),
    ("ecad-file", Class::Core),
    ("ecad-cadcraft", Class::Adapter),
    ("ecad-inspect", Class::Core),
    ("ecad-tokens", Class::Core),
    ("ecad-ui", Class::UiKit),
    ("ecad-bench", Class::Adapter),
    ("ecad-cli", Class::Ui),
    ("ecad-trial", Class::Ui),
    // Headless by construction: CADCraft crates below the UI only.
    ("ecad-mcp", Class::Adapter),
    ("ecad-inspector", Class::Dev),
    ("ecad-xtask", Class::Exempt),
];

/// Product crates whose `dev` feature may be on by default (owner, 2026-11-12, design 029 Q1: the
/// trial app's debugger, while the trial is the only app).
const DEV_BY_DEFAULT: &[&str] = &["ecad-trial"];

/// Features whose code lives only in a crate's `dev` module (design 029 §12.2, owner Q3).
const DEV_FEATURES: &[&str] = &["dev", "verify-invariants", "translate-oracle", "fault-injection", "test-ops"];

/// Files that still read a dev feature outside their crate's `dev` module, with the brief that
/// will move the code (design 029 §12.4). This list only shrinks: an entry whose file no longer
/// needs it fails `layers`, and a new place is a violation.
const DEV_FEATURE_ALLOWLIST: &[(&str, &str)] = &[
    ("crates/ecad-cadcraft/src/access.rs", "dev tree, part 2 (after 026, 027, 028)"),
    ("crates/ecad-cadcraft/src/interact/forms.rs", "dev tree, part 2 (after 026, 027, 028)"),
    ("crates/ecad-cadcraft/src/lib.rs", "dev tree, part 2 (after 026, 027, 028)"),
    ("crates/ecad-cadcraft/src/project.rs", "dev tree, part 2 (after 026, 027, 028)"),
    ("crates/ecad-cadcraft/src/translate.rs", "dev tree, part 2 (after 026, 027, 028)"),
];

/// Styling written in place, which ECAD UI code may no longer use (design `ui-architecture.md`
/// §3.2, brief 037): colours, sizes and fonts come from `ecad-tokens` through `ecad-ui`.
/// `Tokens::get` reads CADCraft's theme colours directly; `from_rgba` covers every RGBA
/// constructor (`Color32::from_rgba_unmultiplied`, `Rgba::from_rgba_premultiplied`, …).
const LOCAL_STYLE: &[&str] = &[
    "Color32::from_rgb",
    "Color32::from_gray",
    "from_rgba",
    "FontId::new",
    "FontId::proportional",
    "FontId::monospace",
    "CornerRadius",
    "Rounding::",
    "Tokens::get",
];

/// ECAD UI files that still style locally: the number of lines that do, and the brief that
/// restyles the file (design §3.2). The counts only fall: more lines than the count fail `layers`,
/// fewer fail too until the count is lowered (an entry at none is removed), and a file not listed
/// that styles locally is a violation.
const STYLE_ALLOWLIST: &[(&str, usize, &str)] = &[
    ("apps/ecad-trial/src/checks_panel.rs", 4, "039 (U3): Checks becomes a table"),
    ("apps/ecad-trial/src/debugger.rs", 1, "developer tool: restyled when U2 moves the Developer section"),
    ("apps/ecad-trial/src/devices_panel.rs", 5, "039 (U3): Devices becomes a table"),
    ("apps/ecad-trial/src/history_panel.rs", 7, "038 (U2) moves it; restyled by U4d"),
    ("apps/ecad-trial/src/merge_dialog.rs", 2, "restyled with History (U4d)"),
    ("apps/ecad-trial/src/outputs_ui.rs", 5, "restyled with Publish (after U2)"),
    ("apps/ecad-trial/src/preview.rs", 5, "restyled with the page view (U4a)"),
    ("apps/ecad-trial/src/release_ui.rs", 4, "restyled with Publish (after U2)"),
    ("apps/ecad-trial/src/renumber_dialog.rs", 2, "restyled with Lists (after U3)"),
    ("apps/ecad-trial/src/sheets_ui.rs", 5, "restyled with Publish (after U2)"),
];

/// UI toolkits and ECAD's own UI kit: Core and Adapter crates may depend on none of them (the
/// kit too, so the document and adapter crates never draw; design `ui-architecture.md` §2).
const UI_CRATES: &[&str] =
    &["cadcraft-ui-egui", "craft-ui", "egui", "epaint", "egui_extras", "eframe", "egui-wgpu", "wgpu", "winit", "rfd", "ecad-ui"];

/// Library crates that must build for wasm.
const WASM_CRATES: &[&str] = &["ecad-model", "ecad-store", "ecad-file", "ecad-inspect", "ecad-cadcraft", "ecad-tokens", "ecad-ui"];

/// What a UiKit crate may depend on, besides dev-dependencies (design §2).
const UIKIT_ALLOWED: &[&str] = &["egui", "epaint", "egui_extras", "ecad-tokens"];

fn ecad_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
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
    let enables = |list: &Value| {
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|f| f == dep || f == format!("dep:{dep}") || f.starts_with(&format!("{dep}/")) || f.starts_with(&format!("{dep}?/")))
    };
    let enablers: Vec<&String> = features.into_iter().flatten().filter(|(k, v)| enables(v) || k.as_str() == dep).map(|(k, _)| k).collect();
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

fn layer_violations(meta: &Value) -> Result<Vec<String>, String> {
    let pkgs = meta["packages"].as_array().ok_or("cargo metadata: no packages")?;
    let root = meta["workspace_root"].as_str().unwrap_or_default();
    let mut out = Vec::new();
    for p in pkgs {
        let name = p["name"].as_str().unwrap_or_default();
        let Some(class) = class_of(name) else {
            out.push(format!("{name}: not registered in ecad/xtask layering table"));
            continue;
        };
        let in_dev = under_dev(root, p["manifest_path"].as_str().unwrap_or_default());
        if in_dev != (class == Class::Dev) {
            out.push(format!("{name}: a crate is under ecad/dev/ exactly when its class is Dev (it is {class:?})"));
        }
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dep = d["name"].as_str().unwrap_or_default();
            let dev = d["kind"].as_str() == Some("dev");
            let ui = UI_CRATES.contains(&dep) || class_of(dep) == Some(Class::UiKit);
            let cadcraft = dep.starts_with("cadcraft-");
            let bad = match class {
                Class::Core => (cadcraft || ui) && !dev,
                Class::Adapter => ui && !dev,
                Class::UiKit => !dev && !UIKIT_ALLOWED.contains(&dep),
                Class::Ui | Class::Dev | Class::Exempt => false,
            };
            if bad {
                out.push(format!("{name} ({class:?}) must not depend on {dep}"));
            }
            let product = matches!(class, Class::Core | Class::Adapter | Class::Ui | Class::UiKit);
            if product
                && class_of(dep) == Some(Class::Dev)
                && let Some(why) = dev_dependency_problem(p, d, dep)
            {
                out.push(why);
            }
        }
    }
    Ok(out)
}

/// The places in one product source file (`rel`: its path from the workspace folder, `/`
/// separated) that read a dev feature outside the crate's `dev` module (design 029 §12.3 rule 2),
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
    let code = strip_line_comments(text);
    code.lines()
        .enumerate()
        .filter(|(_, l)| LOCAL_STYLE.iter().any(|p| l.contains(p)))
        .map(|(i, _)| format!("{rel}:{}: {}", i + 1, text.lines().nth(i).unwrap_or_default().trim()))
        .collect()
}

/// What is wrong with an allowlisted file that has `found` locally styled lines against its
/// `allowed` count, if anything: more is a violation; fewer means the count must come down.
fn style_count_problem(path: &str, found: usize, allowed: usize, brief: &str) -> Option<String> {
    match found.cmp(&allowed) {
        std::cmp::Ordering::Greater => {
            Some(format!("{path}: {found} locally styled lines, the allowlist allows {allowed} ({brief}); take the new ones from ecad-ui"))
        }
        std::cmp::Ordering::Less if found == 0 => Some(format!("{path}: no longer styles locally; remove its allowlist entry ({brief})")),
        std::cmp::Ordering::Less => Some(format!("{path}: {found} locally styled lines, fewer than the allowlist's {allowed}; lower the count")),
        std::cmp::Ordering::Equal => None,
    }
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
/// workspace folder) that read a file under `root/dev/` (design 029 §12.3 rule 3): an
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

/// Rules 2 and 3 of design 029 §12.3 over the product crates' `src/` folders in `meta`: the
/// violations, and the allowlist entries no longer needed.
fn source_violations(meta: &Value) -> Result<Vec<String>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut out = Vec::new();
    let mut allowed_used: BTreeSet<&str> = BTreeSet::new();
    let mut style_seen: BTreeSet<&str> = BTreeSet::new();
    for p in meta["packages"].as_array().ok_or("cargo metadata: no packages")? {
        let name = p["name"].as_str().unwrap_or_default();
        let class = class_of(name);
        if !matches!(class, Some(Class::Core | Class::Adapter | Class::Ui | Class::UiKit)) {
            continue;
        }
        let Some(dir) = p["manifest_path"].as_str().map(PathBuf::from).and_then(|m| m.parent().map(Path::to_path_buf)) else { continue };
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            let places = dev_feature_places(&rel, &text);
            match DEV_FEATURE_ALLOWLIST.iter().find(|(path, _)| *path == rel) {
                Some((path, _)) if !places.is_empty() => {
                    allowed_used.insert(path);
                }
                _ => out.extend(places.into_iter().map(|p| format!("{p} (dev features are read only in the crate's `dev` module)"))),
            }
            out.extend(dev_file_reads(&root, &dir, &f, &text).into_iter().map(|p| format!("{p} (product code must not read files under dev/)")));
            if class == Some(Class::Ui) {
                let places = local_style_places(&rel, &text);
                match STYLE_ALLOWLIST.iter().find(|(path, _, _)| *path == rel) {
                    Some((path, allowed, brief)) => {
                        style_seen.insert(path);
                        out.extend(style_count_problem(path, places.len(), *allowed, brief));
                    }
                    None => out.extend(places.into_iter().map(|p| format!("{p} (no local styling: take it from ecad-tokens through ecad-ui)"))),
                }
            }
        }
    }
    for (path, _, brief) in STYLE_ALLOWLIST {
        if !style_seen.contains(path) {
            out.push(format!("{path}: in the local-styling allowlist ({brief}) but not found; remove the entry"));
        }
    }
    for (path, brief) in DEV_FEATURE_ALLOWLIST {
        if !allowed_used.contains(path) {
            out.push(format!("{path}: in the dev-feature allowlist ({brief}) but no longer needs it; remove the entry"));
        }
    }
    Ok(out)
}

fn layers() -> Result<(), String> {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(ecad_root())
        .output()
        .map_err(|e| e.to_string())?;
    let meta: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let mut v = layer_violations(&meta)?;
    v.extend(source_violations(&meta)?);
    if v.is_empty() {
        println!("layers: OK");
        Ok(())
    } else {
        Err(format!("layering violations:\n  {}", v.join("\n  ")))
    }
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

fn wasm() -> Result<(), String> {
    let root = ecad_root();
    let meta = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(&root)
        .output()
        .map_err(|e| e.to_string())?;
    let meta: Value = serde_json::from_slice(&meta.stdout).map_err(|e| e.to_string())?;
    let present: Vec<&str> = meta["packages"].as_array().into_iter().flatten().filter_map(|p| p["name"].as_str()).collect();
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["build", "--target", "wasm32-unknown-unknown"]).current_dir(&root);
    // Warnings are errors, as in CI (`RUSTFLAGS: -D warnings`): code used only off wasm left
    // dead code on wasm that passed here and failed the QA gate (PR #66). With `--target`,
    // RUSTFLAGS reaches only the wasm crates, not build scripts or proc macros.
    let (var, flags) = deny_warnings(std::env::var("CARGO_ENCODED_RUSTFLAGS").ok().as_deref(), std::env::var("RUSTFLAGS").ok().as_deref());
    cmd.env(var, flags);
    for c in WASM_CRATES.iter().filter(|c| present.contains(c)) {
        cmd.args(["-p", c]);
    }
    run(&mut cmd)?;
    println!("wasm: OK");
    Ok(())
}

/// The rustflags variable to set and its value: the caller's flags with `-D warnings` added.
/// Cargo reads `CARGO_ENCODED_RUSTFLAGS` (flags split by `\x1f`) before `RUSTFLAGS`, so a
/// non-empty one is extended; otherwise `RUSTFLAGS` is. Flags already denying warnings are kept
/// as they are.
fn deny_warnings(encoded: Option<&str>, plain: Option<&str>) -> (&'static str, String) {
    const SEP: char = '\x1f';
    if let Some(e) = encoded.filter(|e| !e.is_empty()) {
        let flags: Vec<&str> = e.split(SEP).collect();
        if flags.windows(2).any(|w| w == ["-D", "warnings"]) {
            return ("CARGO_ENCODED_RUSTFLAGS", e.to_string());
        }
        return ("CARGO_ENCODED_RUSTFLAGS", format!("{e}{SEP}-D{SEP}warnings"));
    }
    let p = plain.unwrap_or_default().trim();
    let flags: Vec<&str> = p.split_whitespace().collect();
    if flags.windows(2).any(|w| w == ["-D", "warnings"]) {
        return ("RUSTFLAGS", p.to_string());
    }
    ("RUSTFLAGS", if p.is_empty() { "-D warnings".to_string() } else { format!("{p} -D warnings") })
}

fn ci() -> Result<(), String> {
    let root = ecad_root();
    let cargo = env!("CARGO");
    run(Command::new(cargo).args(["fmt", "--all", "--check"]).current_dir(&root))?;
    run(Command::new(cargo).args(["clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"]).current_dir(&root))?;
    // All features: the fault-injection containment tests run in CI, never in normal builds.
    // The slow set too (`#[ignore = "slow: …"]`, brief 017): this is the QA gate's full suite.
    run(Command::new(cargo).args(["test", "--workspace", "--all-features", "--", "--include-ignored"]).current_dir(&root))?;
    run(Command::new(cargo).args(["clippy", "--workspace", "--", "-D", "warnings"]).current_dir(&root))?;
    layers()?;
    patches()?;
    wasm()
}

const USAGE: &str = "usage: cargo xtask <layers|patches|wasm|ci|bench|demo|test-changed|clean-builds>
  test-changed [--base <ref>] [--with-dependents] [--dry-run] [-- <test arguments>]
        fast tests of the crates changed since <ref> (default: the branch's upstream, so what
        was not pushed yet; for a branch never pushed, since its merge base with origin/dev;
        committed or not); never the slow set (#[ignore = \"slow: …\"]), which runs at the QA
        gate; --with-dependents also tests every crate that depends on them (CI runs that on
        pull requests into dev, so workers leave it off); --dry-run only names the crates
  wasm
        build the ECAD library crates for wasm32-unknown-unknown, warnings as errors (as CI)
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
        "patches" => patches(),
        "wasm" => wasm(),
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
    use super::*;

    /// `cargo xtask wasm` denies warnings like CI does, keeping any flags the caller set.
    #[test]
    fn wasm_build_denies_warnings() {
        assert_eq!(deny_warnings(None, None), ("RUSTFLAGS", "-D warnings".to_string()));
        assert_eq!(deny_warnings(Some(""), Some("  ")), ("RUSTFLAGS", "-D warnings".to_string()));
        assert_eq!(deny_warnings(None, Some("-C debuginfo=0")), ("RUSTFLAGS", "-C debuginfo=0 -D warnings".to_string()));
        assert_eq!(deny_warnings(None, Some("-D warnings")), ("RUSTFLAGS", "-D warnings".to_string()));
        assert_eq!(deny_warnings(Some("-Cdebuginfo=0"), Some("ignored")), ("CARGO_ENCODED_RUSTFLAGS", "-Cdebuginfo=0\x1f-D\x1fwarnings".to_string()));
        assert_eq!(deny_warnings(Some("-D\x1fwarnings"), None), ("CARGO_ENCODED_RUSTFLAGS", "-D\x1fwarnings".to_string()));
    }

    #[test]
    fn core_crates_may_not_use_cadcraft() {
        let meta: Value = serde_json::json!({"packages": [
            {"name": "ecad-model", "dependencies": [{"name": "cadcraft-geom", "kind": null}]},
            {"name": "ecad-cadcraft", "dependencies": [{"name": "cadcraft-engine", "kind": null}, {"name": "egui", "kind": null}]},
            {"name": "ecad-store", "dependencies": [{"name": "cadcraft-engine", "kind": "dev"}]},
            {"name": "mystery", "dependencies": []}
        ], "workspace_root": ""});
        let v = layer_violations(&meta).unwrap();
        assert_eq!(v.len(), 3, "{v:?}");
    }

    /// Design 029 §12.3 rule 1 (and 4): a product crate may depend on a `dev/` crate only
    /// optionally, through its own `dev` feature, never on by default (except the trial, Q1); a
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
                pkg("ecad-trial", "apps\\ecad-trial", insp(true, None), serde_json::json!({"default": ["dev"], "dev": ["dep:ecad-inspector"]})),
                0,
                "the trial's dev on by default (Q1)",
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
            ("crates/c/src/dev/oracle.rs", "#[cfg(feature = \"translate-oracle\")]\nfn big() {\n}\n", 0),
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
            // Tester 029 F1: the path on the next line, `include!`, and a path built on the crate
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

    /// Rules 1–3 on this workspace: no violation, and every allowlist entry is still needed.
    #[test]
    fn this_workspace_keeps_developer_tools_apart() {
        let root = ecad_root();
        let out = Command::new(env!("CARGO")).args(["metadata", "--format-version", "1", "--no-deps"]).current_dir(&root).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let meta: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(layer_violations(&meta).unwrap(), Vec::<String>::new());
        assert_eq!(source_violations(&meta).unwrap(), Vec::<String>::new());
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

    /// Brief 017 criterion 4 on the real workspace graph.
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
            ["ecad-bench", "ecad-cadcraft", "ecad-cli", "ecad-file", "ecad-inspect", "ecad-inspector", "ecad-mcp", "ecad-model", "ecad-trial"]
        );
        assert_eq!(pick("ecad/apps/ecad-trial/src/lib.rs"), ["ecad-trial"]);
        assert_eq!(
            pick("ecad/crates/ecad-store/src/lib.rs"),
            ["ecad-bench", "ecad-cadcraft", "ecad-cli", "ecad-file", "ecad-inspector", "ecad-mcp", "ecad-store", "ecad-trial"]
        );
        assert_eq!(pick("crates/engine/src/lib.rs"), ["ecad-bench", "ecad-cadcraft", "ecad-mcp", "ecad-trial"]);
        assert!(pick("ecad/docs/quick-start.md").is_empty());
        // The demo's command list is embedded by ecad-mcp; the trial's Checks UI test opens the demo
        // through it (brief 022).
        assert_eq!(pick("ecad/examples/demo/commands.jsonl"), ["ecad-mcp", "ecad-trial"]);
        assert_eq!(pick("Cargo.toml").len(), meta["workspace_members"].as_array().map_or(0, Vec::len));
    }

    #[test]
    fn inventory_and_commit_are_parsed() {
        let inv = "| File | Why |\n| --- | --- |\n| `crates/engine/src/lib.rs` | X3 |\n| `README.md` | ours |\n";
        assert_eq!(inventory(inv), vec!["crates/engine/src/lib.rs", "README.md"]);
        assert_eq!(upstream_commit("x\nCommit: `00c7613` (0.6.2)\n").as_deref(), Some("00c7613"));
    }

    /// Design §2 (brief 037 criterion 3): the UiKit crate may use egui, epaint, egui_extras and
    /// ecad-tokens only (dev-dependencies aside); a `cadcraft-*` crate or another ECAD crate is a
    /// violation.
    #[test]
    fn the_ui_kit_uses_egui_and_the_tokens_only() {
        let kit = |deps: Value| serde_json::json!({"packages": [{"name": "ecad-ui", "dependencies": deps}], "workspace_root": ""});
        let rows: &[(Value, usize, &str)] = &[
            (
                serde_json::json!([{"name": "egui", "kind": null}, {"name": "ecad-tokens", "kind": null}, {"name": "egui_extras", "kind": null}]),
                0,
                "allowed",
            ),
            (serde_json::json!([{"name": "egui_kittest", "kind": "dev"}]), 0, "a test-only dependency"),
            (serde_json::json!([{"name": "cadcraft-ui-egui", "kind": null}]), 1, "a CADCraft crate"),
            (serde_json::json!([{"name": "cadcraft-fonts", "kind": null}]), 1, "a CADCraft crate below the UI"),
            (serde_json::json!([{"name": "ecad-model", "kind": null}]), 1, "another ECAD crate"),
            (serde_json::json!([{"name": "eframe", "kind": null}]), 1, "an app shell"),
        ];
        for (deps, n, what) in rows {
            let v = layer_violations(&kit(deps.clone())).unwrap();
            assert_eq!(v.len(), *n, "{what}: {v:?}");
        }
    }

    /// Design §3.2 (brief 037 criterion 3): ECAD UI sources take colours, sizes and fonts from the
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
let c = ecad_ui::color(SHELL.text);",
                0,
            ),
            ("let c = theme::tokens().text; // not Color32::from_rgb", 0),
        ];
        for (text, n) in rows {
            assert_eq!(local_style_places("apps/ecad-trial/src/x.rs", text).len(), *n, "{text}");
        }
    }

    /// A UI file not in the allowlist that styles locally fails `layers`; one in it does not.
    #[test]
    fn a_new_ui_file_that_styles_locally_fails_layers() {
        let root = std::env::temp_dir().join(format!("ecad-xtask-style-{}", std::process::id()));
        let src = root.join("apps").join("ecad-trial").join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("new_panel.rs"),
            "fn f() -> egui::Color32 { egui::Color32::from_rgb(1, 2, 3) }
",
        )
        .unwrap();
        std::fs::write(
            src.join("checks_panel.rs"),
            "fn f() -> egui::Color32 { egui::Color32::from_gray(7) }
",
        )
        .unwrap();
        let meta = serde_json::json!({
            "workspace_root": root.to_string_lossy(),
            "packages": [{"name": "ecad-trial", "manifest_path": root.join("apps").join("ecad-trial").join("Cargo.toml").to_string_lossy(), "dependencies": []}],
        });
        let v = source_violations(&meta).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(v.iter().any(|m| m.starts_with("apps/ecad-trial/src/new_panel.rs:1:") && m.contains("no local styling")), "{v:?}");
        assert!(!v.iter().any(|m| m.contains("checks_panel.rs") && m.contains("no local styling")), "allowlisted: {v:?}");
    }

    /// Core and Adapter crates may not depend on ECAD's UI kit or the egui family (design §2).
    #[test]
    fn core_and_adapter_crates_may_not_use_the_ui_kit() {
        let rows: &[(&str, &str, usize)] = &[
            ("ecad-model", "ecad-ui", 1),
            ("ecad-cadcraft", "ecad-ui", 1),
            ("ecad-bench", "ecad-ui", 1),
            ("ecad-cadcraft", "epaint", 1),
            ("ecad-mcp", "egui_extras", 1),
            ("ecad-model", "ecad-tokens", 0),
            ("ecad-cadcraft", "ecad-tokens", 0),
            ("ecad-trial", "ecad-ui", 0),
        ];
        for (krate, dep, n) in rows {
            let meta = serde_json::json!({"packages": [{"name": krate, "dependencies": [{"name": dep, "kind": null}]}], "workspace_root": ""});
            assert_eq!(layer_violations(&meta).unwrap().len(), *n, "{krate} -> {dep}");
        }
    }

    /// The allowlist's counts only fall: more fails, fewer asks for a lower count, none asks for
    /// the entry to go.
    #[test]
    fn local_styling_counts_only_fall() {
        assert_eq!(style_count_problem("a.rs", 3, 3, "U2"), None);
        assert!(style_count_problem("a.rs", 4, 3, "U2").is_some_and(|m| m.contains("allows 3")));
        assert!(style_count_problem("a.rs", 2, 3, "U2").is_some_and(|m| m.contains("lower the count")));
        assert!(style_count_problem("a.rs", 0, 3, "U2").is_some_and(|m| m.contains("remove its allowlist entry")));
    }
}
