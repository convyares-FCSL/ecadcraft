//! The crate-map rules: the layer table (Core, Engine, UI kit, App, Programs, Dev) and what each layer may
//! depend on, in `Cargo.toml` and in its sources; Core reaches no OS except `ecad-store`, and the app none but
//! a monotonic clock for UI timing; the crate rules
//! (`anyhow`, `tokio`, `rayon`); and developer tools kept apart from product code (dev features read only in
//! a crate's `dev` module, no product source reads a file under `dev/`). The UI source rules
//! ([`super::ui_style`]) run in the same pass over the product sources.

use super::*;

/// The layer of an ECAD crate: a crate uses only crates in layers above it
/// (and in its own layer where the table allows it). Engine and UI kit are siblings: neither uses the
/// other.
/// The rules apply to normal dependencies; a dev-dependency (tests) is checked only for the layer
/// order (below), and is otherwise free, except on Dev crates.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Class {
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

pub(crate) const TABLE: &[(&str, Class)] = &[
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
pub(crate) const ENGINE_CADCRAFT: &[&str] = &["cadcraft-engine", "cadcraft-io"];

/// The layer's name as a README's `Layer:` line writes it; `xtask` counts as Dev.
pub(crate) fn layer_name(class: Class) -> &'static str {
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
pub(crate) const CORE_ORDER: &[&str] = &["ecad-model", "ecad-store", "ecad-file", "ecad-electrical", "ecad-drawing-style"];

/// Dependencies that break the layer rules for now, each with the change that removes it. An entry
/// no longer needed fails `layers`.
pub(crate) const LAYER_EXCEPTIONS: &[(&str, &str, &str)] = &[];

/// Product crates whose `dev` feature may be on by default: ecad-app, for the desktop app's debugger,
/// while the desktop app is the only app.
pub(crate) const DEV_BY_DEFAULT: &[&str] = &["ecad-app"];

/// Features whose code lives only in a crate's `dev` module.
pub(crate) const DEV_FEATURES: &[&str] = &["dev", "verify-invariants", "fault-injection", "test-ops"];

/// The engine files that read a dev feature outside its `dev` module, and why: the fault-injection and
/// test-ops hooks sit inside the write and access paths they break or open, so a test exercises the
/// real path. One area, no counts: a file listed here that no longer reads one fails `layers`, and any
/// other file that does is a violation.
pub(crate) const DEV_FEATURE_HOOKS: (&[&str], &str) = (
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

/// Paths a layer's sources (`src/`, outside `//` comments) must not name, each matched where it
/// starts a name. Cargo alone is not enough for App and Programs, because
/// `cadcraft-ui-egui` and `cadcraft-mcp` can hand engine types through; for ecad-electrical the
/// check repeats Cargo's rule with a clearer message. Dev tools such as `ecad-bench` may use the
/// engine directly.
pub(crate) fn forbidden_paths(name: &str, class: Class) -> &'static [&'static str] {
    match class {
        Class::App | Class::Programs => &["cadcraft_engine"],
        Class::Core if name == "ecad-electrical" => &["cadcraft_"],
        _ => &[],
    }
}

/// The one Core crate that may reach the OS: storage under the file and the lock. Every other Core crate stays pure data and rules, so it is testable and portable; disk
/// access belongs to `ecad-store`, and the time is passed in.
pub(crate) const CORE_OS_OK: &str = "ecad-store";

/// Direct OS access in Core code, each matched where it starts a name.
/// Threads and rayon are allowed.
pub(crate) const CORE_OS: &[&str] = &[
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

/// What the app may use of [`CORE_OS`]: the monotonic clock, for UI timing (animations, flashes,
/// how long ago an agent called). The wall clock, disk, environment and the rest come from the
/// host program (`ecad_app::shell::host::Host`).
pub(crate) const APP_OS_OK: &[&str] = &["Instant::now"];

/// Crates that find the user's folders: Core other than [`CORE_OS_OK`], and the app, may not depend on them.
pub(crate) const CORE_OS_CRATES: &[&str] = &["dirs", "dirs-next", "dirs-sys", "directories", "directories-next"];

/// Crates besides Programs that may use `anyhow`: xtask and
/// ecad-bench's binary. A library returns its own error type, so a caller can react to the kind
/// of failure; `anyhow` erases it, which only a program's `main` may do.
pub(crate) const ANYHOW_OK: &[&str] = &["ecad-bench", "ecad-xtask"];

/// Where `rayon` may appear, as a plain dependency (normal, not optional,
/// no target table): where the per-page and per-chunk work is. No wasm `cfg`: ECAD is
/// desktop-only.
pub(crate) const RAYON_OK: &[&str] = &["ecad-model", "ecad-file", "ecad-electrical", "ecad-engine"];

/// UI toolkits and ECAD's own UI kit: Core and Engine crates may depend on none of them (the
/// kit too, so the document and engine crates never draw).
pub(crate) const UI_CRATES: &[&str] =
    &["cadcraft-ui-egui", "craft-ui", "egui", "epaint", "egui_extras", "eframe", "egui-wgpu", "wgpu", "winit", "rfd", "ecad-widgets"];

/// What a UiKit crate may depend on, besides dev-dependencies.
pub(crate) const UIKIT_ALLOWED: &[&str] = &["egui", "epaint", "egui_extras", "ecad-model"];

pub(crate) fn class_of(name: &str) -> Option<Class> {
    TABLE.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// Whether `manifest` (a `Cargo.toml` path) is under the workspace's `dev/` folder, `root` being
/// the workspace folder.
pub(crate) fn under_dev(root: &str, manifest: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").to_lowercase();
    let (root, manifest) = (norm(root), norm(manifest));
    manifest.strip_prefix(root.trim_end_matches('/')).is_some_and(|rest| rest.starts_with("/dev/"))
}

/// Why a product crate's dependency on the `Dev` crate `dep` (the entry `d` of package `p`) is not
/// allowed, if it is not: it must be optional, enabled only by the crate's own `dev` feature
/// (through `dep:`, so no feature named after it), and `dev` must not be on by default.
pub(crate) fn dev_dependency_problem(p: &Value, d: &Value, dep: &str) -> Option<String> {
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
pub(crate) fn enabling_features(p: &Value, dep: &str) -> Vec<String> {
    let enables = |list: &Value| {
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|f| f == dep || f == format!("dep:{dep}") || f.starts_with(&format!("{dep}/")) || f.starts_with(&format!("{dep}?/")))
    };
    p["features"].as_object().into_iter().flatten().filter(|(k, v)| enables(v) || k.as_str() == dep).map(|(k, _)| k.clone()).collect()
}

pub(crate) fn layer_violations(meta: &Value) -> Result<Vec<String>, String> {
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

/// The places in one source file (`rel`, `/` separated) of a crate of `class` that reach the OS
/// directly ([`CORE_OS`], less [`APP_OS_OK`] in the app), as `rel:line: text`. `//` comments are
/// skipped, and so is everything from a `#[cfg(test)]` line followed by a `mod` line (the file's
/// test module) on.
pub(crate) fn os_places(class: Class, rel: &str, text: &str) -> Vec<String> {
    let code = strip_line_comments(text);
    let lines: Vec<&str> = code.lines().collect();
    let end = (0..lines.len())
        .find(|&i| {
            lines[i].trim() == "#[cfg(test)]"
                && lines.get(i + 1).is_some_and(|n| n.trim_start().starts_with("mod ") || n.trim_start().starts_with("pub mod "))
        })
        .unwrap_or(lines.len());
    let names: Vec<&str> = CORE_OS.iter().copied().filter(|n| class != Class::App || !APP_OS_OK.contains(n)).collect();
    name_places(rel, text, lines.get(..end).unwrap_or_default(), &names)
}

/// The places in one source file (`rel`) of crate `name` that name a path its layer must not
/// ([`forbidden_paths`]), outside `//` comments, as `rel:line: text`.
pub(crate) fn forbidden_path_places(name: &str, class: Class, rel: &str, text: &str) -> Vec<String> {
    let code = strip_line_comments(text);
    let lines: Vec<&str> = code.lines().collect();
    name_places(rel, text, &lines, forbidden_paths(name, class))
}

/// The layer table's source checks for App, Programs and ecad-electrical ([`forbidden_paths`]): the
/// lines of their `src/` that name a forbidden path. A hard rule: no allowlist.
pub(crate) fn path_violations(meta: &Value) -> Result<Vec<String>, String> {
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

/// Direct OS access in Core crates other than [`CORE_OS_OK`] and in the app: every source
/// line that reaches the OS outside the test module (the app may read the monotonic clock,
/// [`APP_OS_OK`]), and every dependency on a crate that finds the user's folders
/// ([`CORE_OS_CRATES`]). A hard zero.
pub(crate) fn os_violations(meta: &Value) -> Result<Vec<String>, String> {
    const CORE_WHY: &str =
        "Core stays pure data and rules, so it is testable and portable: disk access belongs to ecad-store, and the time is passed in";
    const APP_WHY: &str = "the app has no disk, OS or wall clock of its own: the host program supplies them (shell::host::Host)";
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut out = Vec::new();
    for (name, class, dir, p) in classed_packages(meta)? {
        let why = match class {
            Class::Core if name != CORE_OS_OK => CORE_WHY,
            Class::App => APP_WHY,
            Class::Core | Class::Engine | Class::UiKit | Class::Programs | Class::Dev | Class::Exempt => continue,
        };
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dep = d["name"].as_str().unwrap_or_default();
            if CORE_OS_CRATES.contains(&dep) && d["kind"].as_str() != Some("dev") {
                out.push(format!("{name} must not depend on {dep} ({why})"));
            }
        }
        let Some(dir) = dir else { continue };
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            out.extend(os_places(class, &rel, &text).into_iter().map(|p| format!("{p} ({why})")));
        }
    }
    Ok(out)
}

/// The crate rules on ECAD's own manifests in `meta`; CADCraft's and
/// transitive dependencies are not checked:
/// - `anyhow` only in Programs and [`ANYHOW_OK`] (dev-dependencies aside);
/// - `tokio` nowhere, dev-dependencies included (ECAD runs no async runtime: commands are
///   synchronous on the project's lock, and a second runtime beside CADCraft's event loop would
///   split where work happens);
/// - `rayon` only in [`RAYON_OK`], as a plain dependency.
pub(crate) fn dependency_rule_violations(meta: &Value) -> Result<Vec<String>, String> {
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

/// The places in one product source file (`rel`: its path from the workspace folder, `/`
/// separated) that read a dev feature outside the crate's `dev` module,
/// as `rel:line: text`.
///
/// # Semantics
/// A dev feature is one of [`DEV_FEATURES`], written `feature = "<f>"`. It may appear in
/// `src/dev.rs` or `src/dev/**`, or on an attribute line directly before a single line that uses
/// `dev::` (a one-line hook, ending in `;` or `,`) or declares `mod dev;`. Comment lines are
/// skipped.
pub(crate) fn dev_feature_places(rel: &str, text: &str) -> Vec<String> {
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

/// The places in one product source file (`file`, in the crate folder `krate`, under `root`, the
/// workspace folder) that read a file under `root/dev/`: an
/// `include_str!`, `include_bytes!` or `include!` invocation, or a `#[path]` attribute, read whole
/// (it may span lines), with its string literals joined; a path built on `CARGO_MANIFEST_DIR` is
/// resolved from the crate folder, any other from the file's folder. Comments are skipped.
pub(crate) fn dev_file_reads(root: &Path, krate: &Path, file: &Path, text: &str) -> Vec<String> {
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

/// The developer-tool source rules (dev features read only in a `dev` module; no product source
/// reads a file under `dev/`) over the product crates' `src/` folders in `meta`: the
/// violations, and the allowlist entries no longer needed.
pub(crate) fn source_violations(meta: &Value) -> Result<Vec<String>, String> {
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
