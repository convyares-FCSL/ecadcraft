//! ECAD workspace checks.
//!
//! - `layers`: document crates must not depend on CADCraft; only the adapter may use the engine;
//!   only UI crates and apps may use UI toolkits (principle P10, proposal §8).
//! - `patches`: every file changed outside `ecad/` since the incorporated CADCraft commit
//!   (`ecad/UPSTREAM.md`) must be listed in `ecad/UPSTREAM-PATCHES.md` (proposal §15, patch guard).
//! - `wasm`: the ECAD library crates build for `wasm32-unknown-unknown`.
//! - `ci`: fmt, clippy `-D warnings`, tests, layers, patches, wasm.

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
    ("ecad-cadcraft", Class::Adapter),
    ("ecad-inspect", Class::Core),
    ("ecad-cli", Class::Ui),
    ("ecad-xtask", Class::Exempt),
];

const UI_CRATES: &[&str] = &["cadcraft-ui-egui", "craft-ui", "egui", "eframe", "egui-wgpu", "wgpu", "winit", "rfd"];

/// Library crates that must build for wasm.
const WASM_CRATES: &[&str] = &["ecad-model", "ecad-store", "ecad-inspect", "ecad-cadcraft"];

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
    let out = Command::new("git")
        .args(["diff", "--name-only", &format!("{base}...HEAD")])
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("git diff against {base} failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let changed: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    let unlisted: Vec<&String> = changed.iter().filter(|p| !p.starts_with("ecad/") && !listed.contains(p)).collect();
    let stale: Vec<&String> = listed.iter().filter(|p| !changed.contains(p)).collect();
    println!(
        "patches: {} upstream-tree files differ from {base}",
        changed.iter().filter(|p| !p.starts_with("ecad/")).count()
    );
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
    let present: Vec<&str> =
        meta["packages"].as_array().into_iter().flatten().filter_map(|p| p["name"].as_str()).collect();
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
    run(Command::new(cargo)
        .args(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])
        .current_dir(&root))?;
    run(Command::new(cargo).args(["test", "--workspace"]).current_dir(&root))?;
    layers()?;
    patches()?;
    wasm()
}

fn main() -> ExitCode {
    let arg = std::env::args().nth(1).unwrap_or_default();
    let r = match arg.as_str() {
        "layers" => layers(),
        "patches" => patches(),
        "wasm" => wasm(),
        "ci" => ci(),
        _ => Err("usage: cargo xtask <layers|patches|wasm|ci>".into()),
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

    #[test]
    fn inventory_and_commit_are_parsed() {
        let inv = "| File | Why |\n| --- | --- |\n| `crates/engine/src/lib.rs` | X3 |\n| `README.md` | ours |\n";
        assert_eq!(inventory(inv), vec!["crates/engine/src/lib.rs", "README.md"]);
        assert_eq!(upstream_commit("x\nCommit: `00c7613` (0.6.2)\n").as_deref(), Some("00c7613"));
    }
}
