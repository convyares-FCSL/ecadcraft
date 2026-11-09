//! ECAD workspace checks.
//!
//! - `layers`: document crates must not depend on CADCraft; only the adapter may use the engine;
//!   only UI crates and apps may use UI toolkits (principle P10, proposal §8).
//! - `patches`: every file changed outside `ecad/` since the incorporated CADCraft commit
//!   (`ecad/UPSTREAM.md`) must be listed in `ecad/UPSTREAM-PATCHES.md` (proposal §15, patch guard).
//! - `wasm`: the ECAD library crates build for `wasm32-unknown-unknown`.
//! - `ci`: fmt, clippy `-D warnings`, tests (the slow set included), layers, patches, wasm.
//! - `bench`: the project load benchmark (brief 012), release build, run on demand only. It is
//!   deliberately not part of `ci`; arguments after `bench` are passed to `ecad-bench`.
//! - `test-changed`: the fast tests of the crates changed since `dev` and of every crate that
//!   depends on them (brief 017). The worker's default check.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde_json::Value;

/// How an ECAD crate may depend on others.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Class {
    /// No `cadcraft-*` and no UI toolkit dependency.
    Core,
    /// May use CADCraft crates below the UI.
    Adapter,
    /// May use anything.
    Ui,
    Exempt,
}

const TABLE: &[(&str, Class)] = &[
    ("ecad-model", Class::Core),
    ("ecad-store", Class::Core),
    ("ecad-file", Class::Core),
    ("ecad-cadcraft", Class::Adapter),
    ("ecad-inspect", Class::Core),
    ("ecad-bench", Class::Adapter),
    ("ecad-cli", Class::Ui),
    ("ecad-trial", Class::Ui),
    // Headless by construction: CADCraft crates below the UI only.
    ("ecad-mcp", Class::Adapter),
    ("ecad-xtask", Class::Exempt),
];

const UI_CRATES: &[&str] = &["cadcraft-ui-egui", "craft-ui", "egui", "eframe", "egui-wgpu", "wgpu", "winit", "rfd"];

/// Library crates that must build for wasm.
const WASM_CRATES: &[&str] = &["ecad-model", "ecad-store", "ecad-file", "ecad-inspect", "ecad-cadcraft"];

fn ecad_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("{cmd:?}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{cmd:?} failed ({status})")) }
}

fn layer_violations(meta: &Value) -> Result<Vec<String>, String> {
    let pkgs = meta["packages"].as_array().ok_or("cargo metadata: no packages")?;
    let mut out = Vec::new();
    for p in pkgs {
        let name = p["name"].as_str().unwrap_or_default();
        let Some(class) = TABLE.iter().find(|(n, _)| *n == name).map(|(_, c)| *c) else {
            out.push(format!("{name}: not registered in ecad/xtask layering table"));
            continue;
        };
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dep = d["name"].as_str().unwrap_or_default();
            let dev = d["kind"].as_str() == Some("dev");
            let ui = UI_CRATES.contains(&dep);
            let cadcraft = dep.starts_with("cadcraft-");
            let bad = match class {
                Class::Core => (cadcraft || ui) && !dev,
                Class::Adapter => ui && !dev,
                Class::Ui | Class::Exempt => false,
            };
            if bad {
                out.push(format!("{name} ({class:?}) must not depend on {dep}"));
            }
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
    let v = layer_violations(&meta)?;
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
    for c in WASM_CRATES.iter().filter(|c| present.contains(c)) {
        cmd.args(["-p", c]);
    }
    run(&mut cmd)?;
    println!("wasm: OK");
    Ok(())
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

const USAGE: &str = "usage: cargo xtask <layers|patches|wasm|ci|bench|demo|test-changed>
  test-changed [--base <ref>] [--dry-run] [-- <test arguments>]
        fast tests of the crates changed since <ref> (default dev, committed or not) and of
        every crate that depends on them; --dry-run only names the crates
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
/// A changed file selects the innermost local package whose folder holds it; then every workspace
/// member that depends on a selected package, directly or not, through any kind of dependency (dev
/// included), is selected too. CADCraft's crates used by path are packages in `meta`, so an
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
fn crates_to_test(meta: &Value, repo: &str, changed: &[String]) -> Result<Vec<String>, String> {
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
    // Everything that depends on a selected package, to a fixed point (the graph is finite and
    // each pass adds at least one package, so this ends).
    let nodes = meta["resolve"]["nodes"].as_array().ok_or("cargo metadata: no resolve graph")?;
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

/// `cargo xtask test-changed [--base <ref>] [--dry-run] [-- <test arguments>]`: runs
/// `cargo test --all-features` for [`crates_to_test`] of the files changed since the merge base
/// with `<ref>` (default `dev`, else `origin/dev`), committed, staged, unstaged or untracked.
fn test_changed(args: &[String]) -> Result<(), String> {
    let (ours, passed) = match args.iter().position(|a| a == "--") {
        Some(i) => (args.get(..i).unwrap_or_default(), args.get(i + 1..).unwrap_or_default()),
        None => (args, &[][..]),
    };
    let mut base: Option<&str> = None;
    let mut dry_run = false;
    let mut it = ours.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--base" => base = Some(it.next().ok_or("--base needs a git ref")?),
            "--dry-run" => dry_run = true,
            other => return Err(format!("test-changed: unknown argument {other:?}\n{USAGE}")),
        }
    }
    let root = ecad_root();
    let repo = root.parent().ok_or("no repository root")?;
    let base = match base {
        Some(b) => b.to_string(),
        None => ["dev", "origin/dev"]
            .into_iter()
            .find(|b| git_lines(repo, &["rev-parse", "--verify", "--quiet", b]).is_ok())
            .ok_or("test-changed: no `dev` or `origin/dev` branch; pass --base <ref>")?
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
    let crates = crates_to_test(&meta, &repo.to_string_lossy(), &changed)?;
    println!("test-changed: {} files changed since {base}; crates: {}", changed.len(), crates.join(" "));
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

    #[test]
    fn core_crates_may_not_use_cadcraft() {
        let meta: Value = serde_json::json!({"packages": [
            {"name": "ecad-model", "dependencies": [{"name": "cadcraft-geom", "kind": null}]},
            {"name": "ecad-cadcraft", "dependencies": [{"name": "cadcraft-engine", "kind": null}, {"name": "egui", "kind": null}]},
            {"name": "ecad-store", "dependencies": [{"name": "cadcraft-engine", "kind": "dev"}]},
            {"name": "mystery", "dependencies": []}
        ]});
        let v = layer_violations(&meta).unwrap();
        assert_eq!(v.len(), 3, "{v:?}");
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
            assert_eq!(crates_to_test(&graph(), "c:/r/", &changed).unwrap(), *want, "{changed:?}");
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
        let pick = |f: &str| crates_to_test(&meta, &repo, &[f.to_string()]).unwrap();
        assert_eq!(
            pick("ecad/crates/ecad-model/src/lib.rs"),
            ["ecad-bench", "ecad-cadcraft", "ecad-cli", "ecad-file", "ecad-inspect", "ecad-mcp", "ecad-model", "ecad-trial"]
        );
        assert_eq!(pick("ecad/apps/ecad-trial/src/lib.rs"), ["ecad-trial"]);
        assert_eq!(
            pick("ecad/crates/ecad-store/src/lib.rs"),
            ["ecad-bench", "ecad-cadcraft", "ecad-cli", "ecad-file", "ecad-mcp", "ecad-store", "ecad-trial"]
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
}
