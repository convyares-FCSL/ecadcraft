//! `cargo xtask clean-builds --root <folder> [--dry-run]` (process clean-up, owner 2026-11-12):
//! keep the disk from filling with build folders.
//!
//! # Semantics
//! - The checkouts are the folders directly under `<folder>` and under `<folder>/_tmp`. In each,
//!   `ecad/target` and `target` are deleted.
//! - A build folder in use is skipped with nothing touched. It is first renamed aside
//!   (`target.deleting-<pid>`), which fails while a program holds a file in it (a running test or
//!   app, or a build on Windows), and only the renamed folder is deleted. On Windows a held
//!   `.cargo-lock` also counts as in use; elsewhere, a running `cargo` or `rustc` whose working
//!   folder or command line names the checkout. Folders left renamed by an interrupted run are
//!   deleted on the next.
//! - Worktrees of this repository under `<folder>` whose branch is merged into `origin/dev` (after
//!   `git fetch origin dev`) are removed with `git worktree remove` (never forced). Merged means:
//!   the branch tip is in `origin/dev` but not on its first-parent line, so it came in through a
//!   merge, and no commit was added to the branch after it (its tip would not be in `origin/dev`).
//!   A branch just cut from `dev` with nothing of its own is not merged. Kept, and said so:
//!   - a worktree with uncommitted changes or untracked files;
//!   - one holding ignored files other than its build folders (`target`, `ecad/target`), which
//!     `git worktree remove` would delete with it (notes, `private/`, kept projects);
//!   - a branch with an open pull request (`gh pr list`); without a working `gh`, a branch
//!     `docs/features.md` lists as not done (its State is not "In `dev`").
//!
//!   The main worktree and the one running this are never removed.
//! - `--dry-run` deletes and removes nothing, and says what it would do.
//! - It prints the space freed (or that would be), in GB.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The checkouts under `root`: its folders and those of `root/_tmp`.
pub fn checkouts(root: &Path) -> Vec<PathBuf> {
    let dirs = |p: &Path| -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        v.sort();
        v
    };
    let mut out: Vec<PathBuf> = dirs(root).into_iter().filter(|p| p.file_name().is_none_or(|n| n != "_tmp")).collect();
    out.extend(dirs(&root.join("_tmp")));
    out
}

/// The bytes under `p` (files only; links are not followed).
pub fn size(p: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(p) else { return 0 };
    if !meta.is_dir() {
        return meta.len();
    }
    std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| size(&e.path())).sum()
}

/// Every `.cargo-lock` in the profile folders of the build folder `target` (two levels down:
/// `debug/.cargo-lock`, `wasm32-unknown-unknown/debug/.cargo-lock`).
fn cargo_locks(target: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for a in std::fs::read_dir(target).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        out.push(a.join(".cargo-lock"));
        for b in std::fs::read_dir(&a).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
            out.push(b.join(".cargo-lock"));
        }
    }
    out.into_iter().filter(|p| p.is_file()).collect()
}

/// Whether a cargo is building into `target` (Windows: its `.cargo-lock` is open; it cannot be
/// opened with no sharing).
#[cfg(windows)]
pub fn in_use(target: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    cargo_locks(target).iter().any(|l| std::fs::OpenOptions::new().read(true).share_mode(0).open(l).is_err())
}

/// Whether a cargo or rustc is running in or on `target`'s checkout (its working folder or
/// command line names the folder holding `target`).
#[cfg(not(windows))]
pub fn in_use(target: &Path) -> bool {
    let _ = cargo_locks;
    let Some(checkout) = target.parent() else { return false };
    let Ok(procs) = std::fs::read_dir("/proc") else { return false };
    for p in procs.flatten().map(|e| e.path()) {
        let comm = std::fs::read_to_string(p.join("comm")).unwrap_or_default();
        if !matches!(comm.trim(), "cargo" | "rustc") {
            continue;
        }
        let cwd = std::fs::read_link(p.join("cwd")).unwrap_or_default();
        let cmd = std::fs::read(p.join("cmdline")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        if cwd.starts_with(checkout) || cmd.contains(&*checkout.to_string_lossy()) {
            return true;
        }
    }
    false
}

/// What happened to one build folder.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Deleted(u64),
    WouldDelete(u64),
    /// In use: skipped (the bytes freed before a locked file was met).
    Skipped(u64),
}

/// The prefix of a build folder renamed aside for deletion.
const ASIDE: &str = "target.deleting-";

/// Delete (or, with `dry_run`, measure) the build folder `target`: renamed aside first, so a
/// folder in use is left exactly as it was.
pub fn clean_target(target: &Path, dry_run: bool) -> Outcome {
    let bytes = size(target);
    if in_use(target) {
        return Outcome::Skipped(0);
    }
    if dry_run {
        return Outcome::WouldDelete(bytes);
    }
    let aside = target.with_file_name(format!("{ASIDE}{}", std::process::id()));
    if std::fs::rename(target, &aside).is_err() {
        return Outcome::Skipped(0);
    }
    match std::fs::remove_dir_all(&aside) {
        Ok(()) => Outcome::Deleted(bytes),
        Err(_) => Outcome::Skipped(bytes.saturating_sub(size(&aside))),
    }
}

/// Folders an earlier, interrupted run renamed aside in `dir`.
fn leftovers(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(ASIDE)))
        .collect()
}

/// A worktree from `git worktree list --porcelain`: its folder and branch (`None`: detached or
/// bare).
#[derive(Debug, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
}

/// Parse `git worktree list --porcelain`. The first entry is the main worktree.
pub fn parse_worktrees(text: &str) -> Vec<Worktree> {
    let mut out: Vec<Worktree> = Vec::new();
    for line in text.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            out.push(Worktree { path: PathBuf::from(p), branch: None });
        } else if let (Some(b), Some(w)) = (line.strip_prefix("branch "), out.last_mut()) {
            w.branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
        }
    }
    out
}

/// Whether `path` is under `root` (case-insensitive, `\` and `/` alike).
pub fn under(path: &Path, root: &Path) -> bool {
    // Canonical when the folder exists, so a short Windows name (`CARLVO~1`) and the long one git
    // prints compare equal.
    let norm = |p: &Path| {
        let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let t = p.to_string_lossy().replace('\\', "/");
        t.strip_prefix("//?/").unwrap_or(&t).trim_end_matches('/').to_lowercase()
    };
    let (p, r) = (norm(path), norm(root));
    p.strip_prefix(&r).is_some_and(|rest| rest.starts_with('/'))
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").args(args).current_dir(dir).output().map_err(|e| format!("git {args:?}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// The worktrees under `root` whose branch is merged into `origin/dev`, as defined above (before
/// the checks that keep one).
fn merged_worktrees(repo: &Path, root: &Path) -> Result<Vec<Worktree>, String> {
    let all = parse_worktrees(&git(repo, &["worktree", "list", "--porcelain"])?);
    let first_parent: BTreeSet<String> = git(repo, &["rev-list", "--first-parent", "origin/dev"])?.lines().map(str::to_string).collect();
    let here = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
    let mut out = Vec::new();
    for w in all.into_iter().skip(1) {
        let Some(branch) = w.branch.clone() else { continue };
        let same = std::fs::canonicalize(&w.path).is_ok_and(|p| p == here);
        if same || !under(&w.path, root) {
            continue;
        }
        let Ok(tip) = git(repo, &["rev-parse", &format!("refs/heads/{branch}")]) else { continue };
        let tip = tip.trim().to_string();
        let merged = git(repo, &["merge-base", "--is-ancestor", &tip, "origin/dev"]).is_ok();
        if merged && !first_parent.contains(&tip) {
            out.push(w);
        }
    }
    Ok(out)
}

/// Why `w` (merged) must be kept, if it must: uncommitted or untracked files, ignored files other
/// than its build folders, an open pull request (`pr_open`: `None` when it cannot be asked), or,
/// when that cannot be asked, a branch in `not_done`.
pub fn keep_reason(w: &Worktree, pr_open: &dyn Fn(&str) -> Option<bool>, not_done: &BTreeSet<String>) -> Option<String> {
    let status = match git(&w.path, &["status", "--porcelain", "--ignored"]) {
        Ok(s) => s,
        Err(e) => return Some(format!("its status could not be read: {e}")),
    };
    let build = |p: &str| {
        let p = p.trim().trim_matches('"').replace('\\', "/");
        p == "target/" || p == "ecad/target/" || p.starts_with("target/") || p.starts_with("ecad/target/") || p.contains(ASIDE)
    };
    let mut changed = Vec::new();
    let mut ignored = Vec::new();
    for line in status.lines() {
        match line.strip_prefix("!! ") {
            Some(p) if build(p) => {}
            Some(p) => ignored.push(p.to_string()),
            None => changed.push(line.get(3..).unwrap_or(line).to_string()),
        }
    }
    if !changed.is_empty() {
        return Some(format!("uncommitted changes or untracked files ({})", changed.iter().take(3).cloned().collect::<Vec<_>>().join(", ")));
    }
    if !ignored.is_empty() {
        return Some(format!("it holds ignored files git would delete with it ({})", ignored.iter().take(3).cloned().collect::<Vec<_>>().join(", ")));
    }
    let branch = w.branch.as_deref().unwrap_or_default();
    match pr_open(branch) {
        Some(true) => Some("it has an open pull request".into()),
        Some(false) => None,
        None if not_done.contains(branch) => Some("docs/features.md lists it as not done (and gh is not available to ask)".into()),
        None => None,
    }
}

/// The branches `docs/features.md` lists for features whose State is not "In `dev`".
pub fn not_done_branches(features_md: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in features_md.lines().filter(|l| l.starts_with('|')) {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let (Some(branches), Some(state)) = (cells.get(2), cells.get(4)) else { continue };
        if state.starts_with("In `dev`") || state.starts_with("State") || state.starts_with("---") {
            continue;
        }
        out.extend(branches.split('`').skip(1).step_by(2).filter(|b| b.contains('/')).map(str::to_string));
    }
    out
}

/// Whether `branch` has an open pull request, by `gh` in `repo`; `None` when `gh` cannot say.
fn gh_pr_open(repo: &Path, branch: &str) -> Option<bool> {
    let out = Command::new("gh").args(["pr", "list", "--head", branch, "--state", "open", "--json", "number"]).current_dir(repo).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    Some(v.as_array().is_some_and(|a| !a.is_empty()))
}

fn gb(bytes: u64) -> f64 {
    bytes as f64 / 1e9
}

/// `cargo xtask clean-builds --root <folder> [--dry-run]`; `ecad` is this checkout's `ecad/`.
pub fn clean_builds(args: &[String], ecad: &Path) -> Result<(), String> {
    let mut root: Option<PathBuf> = None;
    let mut dry_run = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => root = Some(PathBuf::from(it.next().ok_or("--root needs a folder")?)),
            "--dry-run" => dry_run = true,
            other => return Err(format!("clean-builds: unknown argument {other:?}")),
        }
    }
    let root = root.ok_or("clean-builds: --root <folder> is required (the folder holding the feature checkouts)")?;
    if !root.is_dir() {
        return Err(format!("clean-builds: {} is not a folder", root.display()));
    }
    let (mut freed, mut skipped) = (0u64, 0usize);
    for c in checkouts(&root) {
        for old in leftovers(&c).into_iter().chain(leftovers(&c.join("ecad"))) {
            let b = size(&old);
            if dry_run {
                println!("would delete {} (left by an earlier run, {:.2} GB)", old.display(), gb(b));
                freed += b;
            } else if std::fs::remove_dir_all(&old).is_ok() {
                println!("deleted {} (left by an earlier run, {:.2} GB)", old.display(), gb(b));
                freed += b;
            }
        }
        for t in [c.join("ecad").join("target"), c.join("target")] {
            if !t.is_dir() {
                continue;
            }
            match clean_target(&t, dry_run) {
                Outcome::Deleted(b) => {
                    freed += b;
                    println!("deleted {} ({:.2} GB)", t.display(), gb(b));
                }
                Outcome::WouldDelete(b) => {
                    freed += b;
                    println!("would delete {} ({:.2} GB)", t.display(), gb(b));
                }
                Outcome::Skipped(b) => {
                    freed += b;
                    skipped += 1;
                    println!("skipped {} (in use by a cargo or rustc)", t.display());
                }
            }
        }
    }
    let repo = ecad.parent().ok_or("no repository root")?;
    if let Err(e) = git(repo, &["fetch", "--quiet", "origin", "dev"]) {
        println!("note: git fetch origin dev failed ({e}); using the origin/dev already here");
    }
    let not_done = not_done_branches(&std::fs::read_to_string(ecad.join("docs").join("features.md")).unwrap_or_default());
    let pr_open = |b: &str| gh_pr_open(repo, b);
    match merged_worktrees(repo, &root) {
        Ok(ws) => {
            for w in ws {
                let name = w.branch.as_deref().unwrap_or_default();
                if let Some(why) = keep_reason(&w, &pr_open, &not_done) {
                    println!("kept worktree {} ({name}, merged into origin/dev): {why}", w.path.display());
                    continue;
                }
                if dry_run {
                    println!("would remove worktree {} ({name}, merged into origin/dev)", w.path.display());
                    continue;
                }
                let path = w.path.to_string_lossy().into_owned();
                match git(repo, &["worktree", "remove", &path]) {
                    Ok(_) => println!("removed worktree {} ({name}, merged into origin/dev)", w.path.display()),
                    Err(e) => println!("kept worktree {} ({name}): {e}", w.path.display()),
                }
            }
        }
        Err(e) => println!("note: worktrees not checked: {e}"),
    }
    let verb = if dry_run { "would free" } else { "freed" };
    println!("clean-builds: {verb} {:.2} GB; {skipped} build folder(s) skipped", gb(freed));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("ecad-xtask-clean-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn file(p: &Path, bytes: usize) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, vec![7u8; bytes]).unwrap();
    }

    #[test]
    fn it_finds_the_checkouts_and_deletes_their_build_folders() {
        let root = temp("root");
        file(&root.join("a/ecad/target/debug/x.rlib"), 1000);
        file(&root.join("a/target/release/y"), 500);
        file(&root.join("_tmp/t-1/ecad/target/debug/z"), 200);
        file(&root.join("b/src/keep.rs"), 10);
        let found: Vec<String> = checkouts(&root).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(found, ["a", "b", "t-1"]);
        let t = root.join("a/ecad/target");
        assert_eq!(clean_target(&t, true), Outcome::WouldDelete(1000));
        assert!(t.is_dir(), "a dry run deletes nothing");
        // What clean_builds does for each checkout (without its git fetch).
        for c in checkouts(&root) {
            for t in [c.join("ecad").join("target"), c.join("target")].into_iter().filter(|t| t.is_dir()) {
                assert!(matches!(clean_target(&t, false), Outcome::Deleted(_)), "{}", t.display());
            }
        }
        assert!(!t.exists() && !root.join("a/target").exists() && !root.join("_tmp/t-1/ecad/target").exists());
        assert!(root.join("b/src/keep.rs").is_file(), "only build folders go");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn a_build_folder_whose_cargo_lock_is_held_is_skipped() {
        let root = temp("busy");
        let t = root.join("c/ecad/target");
        file(&t.join("debug/.cargo-lock"), 0);
        file(&t.join("debug/big"), 4000);
        let held = std::fs::OpenOptions::new().read(true).write(true).open(t.join("debug/.cargo-lock")).unwrap();
        assert_eq!(clean_target(&t, false), Outcome::Skipped(0));
        assert!(t.join("debug/big").is_file());
        drop(held);
        assert_eq!(clean_target(&t, false), Outcome::Deleted(4000));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Reviewer 029: a build folder with any file held open deep inside is left as it was.
    #[cfg(windows)]
    #[test]
    fn a_build_folder_with_a_file_held_deep_inside_is_left_untouched() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = temp("held-deep");
        let t = root.join("c/ecad/target");
        let deep = t.join("debug/deps/incremental/x/held.bin");
        file(&deep, 100);
        file(&t.join("debug/other"), 300);
        // As a running test executable is held: shared for reading only.
        let held = std::fs::OpenOptions::new().read(true).share_mode(1).open(&deep).unwrap();
        assert_eq!(clean_target(&t, false), Outcome::Skipped(0));
        assert!(deep.is_file() && t.join("debug/other").is_file(), "nothing touched");
        assert!(leftovers(&root.join("c/ecad")).is_empty());
        drop(held);
        assert_eq!(clean_target(&t, false), Outcome::Deleted(400));
        assert!(!t.exists() && leftovers(&root.join("c/ecad")).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    fn sh(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
    }

    /// A worktree `name` on `feature/<name>` with one commit, merged into `dev` with a merge commit.
    fn merged_feature(repo: &Path, wt: &Path, name: &str) -> PathBuf {
        let path = wt.join(name);
        let branch = format!("feature/{name}");
        sh(repo, &["worktree", "add", "-q", "-b", &branch, path.to_str().unwrap(), "dev"]);
        std::fs::write(path.join(format!("{name}.txt")), name).unwrap();
        sh(&path, &["add", "."]);
        sh(&path, &["commit", "-q", "-m", name]);
        sh(repo, &["merge", "-q", "--no-ff", "-m", &format!("merge {name}"), &branch]);
        path
    }

    /// Reviewer 029: merged worktrees are removed only when nothing would be lost.
    #[test]
    fn merged_worktrees_are_kept_when_something_would_be_lost() {
        let base = temp("worktrees");
        let repo = base.join("repo");
        let wt = base.join("features");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        sh(&repo, &["init", "-q", "-b", "dev"]);
        std::fs::write(repo.join(".gitignore"), "target/\nprivate/\n").unwrap();
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-q", "-m", "start"]);
        let clean = merged_feature(&repo, &wt, "clean");
        let ignored = merged_feature(&repo, &wt, "ignored");
        let dirty = merged_feature(&repo, &wt, "dirty");
        let _pr = merged_feature(&repo, &wt, "pr");
        let later = merged_feature(&repo, &wt, "later");
        sh(&repo, &["worktree", "add", "-q", "-b", "feature/fresh", wt.join("fresh").to_str().unwrap(), "dev"]);
        sh(&repo, &["update-ref", "refs/remotes/origin/dev", "dev"]);
        // After the merge: a commit on `later`, build output in `clean`, a note in `ignored`, an
        // edit in `dirty`.
        std::fs::write(later.join("more.txt"), "more").unwrap();
        sh(&later, &["add", "."]);
        sh(&later, &["commit", "-q", "-m", "more"]);
        file(&clean.join("target/debug/x"), 10);
        file(&ignored.join("private/notes.md"), 10);
        std::fs::write(dirty.join("dirty.txt"), "edited").unwrap();
        let names = |ws: &[Worktree]| -> Vec<String> { ws.iter().map(|w| w.path.file_name().unwrap().to_string_lossy().into_owned()).collect() };
        let merged = merged_worktrees(&repo, &wt).unwrap();
        assert_eq!(names(&merged), ["clean", "dirty", "ignored", "pr"], "not `later` (a commit after its merge) nor `fresh` (nothing of its own)");
        let pr_open = |b: &str| Some(b == "feature/pr");
        let none = BTreeSet::new();
        let reasons: Vec<(String, Option<String>)> =
            merged.iter().map(|w| (names(std::slice::from_ref(w))[0].clone(), keep_reason(w, &pr_open, &none))).collect();
        for (name, why) in &reasons {
            match name.as_str() {
                "clean" => assert_eq!(why, &None, "build output alone does not keep it"),
                "dirty" => assert!(why.as_deref().is_some_and(|w| w.contains("uncommitted")), "{why:?}"),
                "ignored" => assert!(why.as_deref().is_some_and(|w| w.contains("ignored") && w.contains("private")), "{why:?}"),
                "pr" => assert!(why.as_deref().is_some_and(|w| w.contains("open pull request")), "{why:?}"),
                other => panic!("{other}"),
            }
        }
        // Without gh: features.md's not-done branches are kept.
        let no_gh = |_: &str| None;
        let md = "| Feature | Branch | Briefs | State |\n| --- | --- | --- | --- |\n| A | `feature/pr` | 1 | Designing |\n| B | `feature/clean` | 2 | In `dev` |\n";
        let not_done = not_done_branches(md);
        assert_eq!(not_done.iter().collect::<Vec<_>>(), ["feature/pr"]);
        let pr_w = merged.iter().find(|w| w.branch.as_deref() == Some("feature/pr")).unwrap();
        assert!(keep_reason(pr_w, &no_gh, &not_done).is_some_and(|w| w.contains("features.md")));
        let clean_w = merged.iter().find(|w| w.branch.as_deref() == Some("feature/clean")).unwrap();
        assert_eq!(keep_reason(clean_w, &no_gh, &not_done), None);
        let _ = Command::new("git").args(["worktree", "prune"]).current_dir(&repo).output();
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn worktrees_are_parsed_and_scoped_to_the_root() {
        let text = "worktree C:/dev/ecadcraft\nHEAD 1\nbranch refs/heads/dev\n\nworktree C:/dev/ecad-features/cables\nHEAD 2\nbranch refs/heads/feature/cables\n\nworktree C:/dev/ecad-features/_tmp/t\nHEAD 3\ndetached\n";
        let w = parse_worktrees(text);
        assert_eq!(w.len(), 3);
        assert_eq!(w[1].branch.as_deref(), Some("feature/cables"));
        assert_eq!(w[2].branch, None);
        assert!(under(Path::new("C:\\dev\\ecad-features\\cables"), Path::new("c:/dev/ecad-features")));
        assert!(!under(Path::new("C:/dev/ecad-features-old/x"), Path::new("C:/dev/ecad-features")));
        assert!(!under(Path::new("C:/dev/ecad-features"), Path::new("C:/dev/ecad-features")));
    }

    #[test]
    fn root_is_required() {
        assert!(clean_builds(&[], Path::new(".")).unwrap_err().contains("--root"));
    }
}
