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
//!   the work is. Every workspace member has a README with the standard sections. Each family of
//!   rules is a module: [`layers`], [`ui_style`], [`drawn_text`], [`errors`], [`readme`] and the
//!   `*_guards` modules; this file holds the shared source helpers and the commands.
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

use drawn_text::*;
use errors::*;
use layers::*;
use readme::*;
use ui_style::*;

mod clean;
mod drawn_text;
mod engine_guards;
mod errors;
mod history_guards;
mod layers;
mod readme;
mod starter_guards;
mod types_guards;
mod ui_style;
mod views_guards;

fn ecad_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("{cmd:?}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{cmd:?} failed ({status})")) }
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
    v.extend(history_guards::violations(&meta)?);
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
