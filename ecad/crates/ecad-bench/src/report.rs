//! The table on stdout, the JSON file and `--compare` (brief 012 §4).

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

use crate::measure::Measured;

/// One measured project.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub tier: String,
    pub variant: String,
    pub measured: Measured,
}

/// The measured columns: JSON key, table heading.
const COLUMNS: [(&str, &str); 11] = [
    ("open_ms", "open ms"),
    ("derive_ms", "derive ms"),
    ("xref_ms", "xref ms"),
    ("first_page_ms", "1st page ms"),
    ("edit_ms", "edit ms"),
    ("save_ms", "save ms"),
    ("draw_move_ms", "draw move ms"),
    ("draw_window_ms", "move+wires ms"),
    ("draw_wire_ms", "draw wire ms"),
    ("switch_first_ms", "1st view ms"),
    ("switch_back_ms", "back ms"),
];

fn values(m: &Measured) -> [Option<f64>; 11] {
    [
        Some(m.open_ms),
        Some(m.derive_ms),
        m.xref_ms,
        Some(m.first_page_ms),
        Some(m.edit_ms),
        Some(m.save_ms),
        Some(m.draw_move_ms),
        Some(m.draw_window_ms),
        Some(m.draw_wire_ms),
        Some(m.switch_first_ms),
        Some(m.switch_back_ms),
    ]
}

/// A table cell: `—` where nothing was measured.
fn cell(v: Option<f64>, width: usize, places: usize) -> String {
    match v {
        Some(v) => format!("{v:>width$.places$}"),
        None => format!("{:>width$}", "—"),
    }
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// The plain table.
pub fn table(rows: &[Row], runs: usize) -> String {
    let mut out = format!(
        "{:<8} {:<8} {:>9} {:>8} {:>10} {:>10} {:>9} {:>12} {:>9} {:>9} {:>13} {:>14} {:>13} {:>12} {:>8}\n",
        "tier",
        "history",
        "file MB",
        "txs",
        "open ms",
        "derive ms",
        "xref ms",
        "1st page ms",
        "edit ms",
        "save ms",
        "draw move ms",
        "move+wires ms",
        "draw wire ms",
        "1st view ms",
        "back ms"
    );
    for r in rows {
        let m = &r.measured;
        out.push_str(&format!(
            "{:<8} {:<8} {:>9.2} {:>8} {} {} {} {} {} {} {} {} {} {} {}\n",
            r.tier,
            r.variant,
            mb(m.bytes),
            m.transactions,
            cell(Some(m.open_ms), 10, 1),
            cell(Some(m.derive_ms), 10, 2),
            cell(m.xref_ms, 9, 2),
            cell(Some(m.first_page_ms), 12, 1),
            cell(Some(m.edit_ms), 9, 2),
            cell(Some(m.save_ms), 9, 2),
            cell(Some(m.draw_move_ms), 13, 2),
            cell(Some(m.draw_window_ms), 14, 2),
            cell(Some(m.draw_wire_ms), 13, 2),
            cell(Some(m.switch_first_ms), 12, 2),
            cell(Some(m.switch_back_ms), 8, 2),
        ));
    }
    out.push_str(&format!(
        "Median of {runs} runs each; generation not timed. xref is brief 010's cable reference derivation \
         on the opened document. draw move, move+wires and draw wire (the draw edit columns): one edit on \
         page 1 made through the engine in a headless CADCraft session. 1st view and back (switch page): \
         with page 1 shown, `openpage` of a page not viewed before, then back to page 1; median over up \
         to 10 pages spread over the project. Peak memory is not measured.\n"
    ));
    out
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).current_dir(root).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The short commit of `root`'s checkout, with `-dirty` when it has uncommitted changes.
pub fn commit(root: &Path) -> String {
    let Some(head) = git(root, &["rev-parse", "--short", "HEAD"]) else { return "unknown".into() };
    match git(root, &["status", "--porcelain"]) {
        Some(s) if !s.is_empty() => format!("{head}-dirty"),
        _ => head,
    }
}

/// The results file's name stem for this moment, `load-<YYYY-MM-DD>-<HHMMSS>-<commit>` (UTC), and
/// the full UTC time.
pub fn stem_and_time(commit: &str) -> (String, String) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let full = ecad_file::disk::utc(secs);
    let date: String = full.chars().take(10).collect();
    let time: String = full.chars().skip(11).take(8).filter(|c| *c != ':').collect();
    (format!("load-{date}-{time}-{commit}"), full)
}

/// A path in `dir` for `stem` that no earlier file has: `<stem>.json`, else `<stem>-2.json`, ….
pub fn unused_path(dir: &Path, stem: &str) -> std::path::PathBuf {
    let mut path = dir.join(format!("{stem}.json"));
    let mut n = 2u32;
    while path.exists() && n < 10_000 {
        path = dir.join(format!("{stem}-{n}.json"));
        n += 1;
    }
    path
}

/// This machine's name: `COMPUTERNAME` (Windows), `HOSTNAME`, then `/etc/hostname`; empty when
/// none is set.
fn machine_name() -> String {
    ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .chain(std::fs::read_to_string("/etc/hostname").ok())
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
        .unwrap_or_default()
}

/// The JSON written to `target/bench/`.
pub fn json(rows: &[Row], runs: usize, commit: &str, when: &str) -> Value {
    let machine = machine_name();
    let cpus = std::thread::available_parallelism().map_or(0, |n| n.get());
    json!({
        "benchmark": "ecad project load (brief 012)",
        "when": when,
        "commit": commit,
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "machine": { "name": machine, "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "cpus": cpus },
        "runs": runs,
        "peak_memory": "not measured",
        "rows": rows.iter().map(|r| {
            let m = &r.measured;
            json!({
                "tier": r.tier,
                "variant": r.variant,
                "bytes": m.bytes,
                "transactions": m.transactions,
                "open_ms": m.open_ms,
                "derive_ms": m.derive_ms,
                "xref_ms": m.xref_ms,
                "first_page_ms": m.first_page_ms,
                "edit_ms": m.edit_ms,
                "save_ms": m.save_ms,
                "draw_move_ms": m.draw_move_ms,
                "draw_window_ms": m.draw_window_ms,
                "draw_wire_ms": m.draw_wire_ms,
                "switch_first_ms": m.switch_first_ms,
                "switch_back_ms": m.switch_back_ms,
            })
        }).collect::<Vec<_>>(),
    })
}

/// The change of each row against an earlier JSON file's matching row (same tier and history).
///
/// # Errors
/// The earlier file is not a benchmark result.
pub fn compare(rows: &[Row], earlier: &Value) -> Result<String, String> {
    let old = earlier.get("rows").and_then(Value::as_array).ok_or("the earlier file has no `rows`")?;
    let what = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let mut out = format!(
        "Compared with {} ({}):\n{:<8} {:<8} {}\n",
        what(earlier, "commit").unwrap_or_default(),
        what(earlier, "when").unwrap_or_default(),
        "tier",
        "history",
        COLUMNS.iter().map(|(_, h)| format!("{h:>22}")).collect::<String>()
    );
    for r in rows {
        let before = old.iter().find(|o| {
            what(o, "tier").as_deref() == Some(r.tier.as_str())
                && what(o, "variant").as_deref() == Some(r.variant.as_str())
        });
        out.push_str(&format!("{:<8} {:<8} ", r.tier, r.variant));
        let Some(before) = before else {
            out.push_str("(not in the earlier run)\n");
            continue;
        };
        for ((key, _), now) in COLUMNS.iter().zip(values(&r.measured)) {
            let cell = match (before.get(*key).and_then(Value::as_f64), now) {
                (Some(was), Some(now)) if was > 0.0 => {
                    format!("{was:.1} → {now:.1} ({:+.0}%)", (now - was) / was * 100.0)
                }
                (Some(was), Some(now)) => format!("{was:.1} → {now:.1}"),
                (None, Some(now)) => format!("— → {now:.1}"),
                (Some(was), None) => format!("{was:.1} → —"),
                (None, None) => "—".into(),
            };
            out.push_str(&format!("{cell:>22}"));
        }
        out.push('\n');
    }
    Ok(out)
}
