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
    /// The same project in container version 2 (brief 021).
    pub v2: Option<crate::v2::MeasuredV2>,
    /// Zstandard codecs compared on that file (brief 021).
    pub compression: Option<crate::v2::Compression>,
}

/// The measured columns: JSON key, table heading.
const COLUMNS: [(&str, &str); 28] = [
    ("open_ms", "open ms"),
    ("derive_ms", "derive ms"),
    ("xref_ms", "xref ms"),
    ("first_page_ms", "1st page ms"),
    ("edit_ms", "edit ms"),
    ("save_ms", "save ms"),
    ("draw_move_ms", "draw move ms"),
    ("draw_window_ms", "move+wires ms"),
    ("draw_wire_ms", "draw wire ms"),
    ("switch_ms", "switch ms"),
    ("preview_ms", "preview ms"),
    ("panel_ms", "panel ms"),
    ("devices_ms", "devices ms"),
    ("goto_ms", "goto ms"),
    ("devupd_ms", "devupd ms"),
    ("place_ms", "place ms"),
    ("join_ms", "join ms"),
    ("tag_ms", "tag ms"),
    ("pnum_ms", "pnum ms"),
    ("frame_ms", "frame ms"),
    ("tbupd_ms", "tbupd ms"),
    ("gridchange_ms", "gridchange ms"),
    ("revopen_ms", "revopen ms"),
    ("convert_ms", "v2 convert ms"),
    ("v2_open_ms", "v2 open ms"),
    ("v2_save_ms", "v2 save ms"),
    ("v2_crash_open_ms", "v2 crash open ms"),
    ("v2_open_x10_ms", "v2 open x10 ms"),
];

fn values(r: &Row) -> [Option<f64>; 28] {
    let m = &r.measured;
    let v = r.v2.as_ref();
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
        Some(m.switch_ms),
        Some(m.preview_ms),
        Some(m.panel_ms),
        Some(m.devices_ms),
        Some(m.goto_ms),
        Some(m.devupd_ms),
        Some(m.place_ms),
        Some(m.join_ms),
        Some(m.tag_ms),
        Some(m.pnum_ms),
        Some(m.frame_ms),
        Some(m.tbupd_ms),
        Some(m.gridchange_ms),
        Some(m.revopen_ms),
        v.map(|v| v.convert_ms),
        v.map(|v| v.open_ms),
        v.map(|v| v.save_ms),
        v.map(|v| v.crash_open_ms),
        v.map(|v| v.open_x10_ms),
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
        "{:<8} {:<8} {:>9} {:>8} {:>10} {:>10} {:>9} {:>12} {:>9} {:>9} {:>13} {:>14} {:>13} {:>10} {:>11} {:>9} {:>11} {:>8} {:>10} {:>9} {:>8} {:>7} {:>8} {:>9} {:>9} {:>14} {:>11}\n",
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
        "switch ms",
        "preview ms",
        "panel ms",
        "devices ms",
        "goto ms",
        "devupd ms",
        "place ms",
        "join ms",
        "tag ms",
        "pnum ms",
        "frame ms",
        "tbupd ms",
        "gridchange ms",
        "revopen ms"
    );
    for r in rows {
        let m = &r.measured;
        out.push_str(&format!(
            "{:<8} {:<8} {:>9.2} {:>8} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}\n",
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
            cell(Some(m.switch_ms), 10, 2),
            cell(Some(m.preview_ms), 11, 2),
            cell(Some(m.panel_ms), 9, 3),
            cell(Some(m.devices_ms), 11, 2),
            cell(Some(m.goto_ms), 8, 2),
            cell(Some(m.devupd_ms), 10, 3),
            cell(Some(m.place_ms), 9, 2),
            cell(Some(m.join_ms), 8, 2),
            cell(Some(m.tag_ms), 7, 2),
            cell(Some(m.pnum_ms), 8, 2),
            cell(Some(m.frame_ms), 9, 3),
            cell(Some(m.tbupd_ms), 9, 2),
            cell(Some(m.gridchange_ms), 14, 2),
            cell(Some(m.revopen_ms), 11, 1),
        ));
    }
    out.push_str(&format!(
        "Median of {runs} runs each; generation not timed. xref is brief 010's cable reference derivation \
         on the opened document. draw move, move+wires and draw wire (the draw edit columns): one edit on \
         page 1 made through the engine in a headless CADCraft session. switch (brief 016): `openpage` \
         making another page the one active page, and back to page 1; preview: project, build and \
         rasterise a page at 480 x 320; panel: rebuild the Project panel's rows after one renamepage; \
         each a median over up to 10 pages spread over the project. Targets (design 016): switch <= 5 ms \
         and preview <= 15 ms on large. devices: rebuild the Devices tab's list; goto: show a device symbol's page and select it; devupd: bring the list up to date after one device property edit. place, join and tag (brief 018): a starter coil placed as a \
         new device, a NO contact added to it, and its tag renamed. pnum, frame, tbupd, gridchange and \
         revopen (brief 019): renumber page 2 with its references refreshed; work out one page's frame \
         and title block; change the project title; change the grid (A3 and A2 in turn); open Rev A \
         read-only. Peak memory is not measured. Since brief 021 the app opens version 2 files: every \
         column from 1st page on is measured on the version 2 file; open, edit and save are version 1.\n"
    ));
    out.push_str(&format!(
        "\nContainer version 2 (brief 021):\n{:<8} {:<8} {:>9} {:>11} {:>11} {:>11} {:>14} {:>15} {:>11}\n",
        "tier", "history", "v1 MB", "v2 MB", "convert ms", "v2 open ms", "v2 save ms", "crash open ms", "open x10 ms"
    ));
    for r in rows {
        let Some(v) = &r.v2 else { continue };
        out.push_str(&format!(
            "{:<8} {:<8} {:>9.2} {:>11.2} {:>11.0} {:>11.1} {:>14.2} {:>15.1} {:>11.1}   (x10: {:.2} MB, {:.2} MB stored, {:.2} MB compacted)\n",
            r.tier,
            r.variant,
            mb(r.measured.bytes),
            mb(v.bytes),
            v.convert_ms,
            v.open_ms,
            v.save_ms,
            v.crash_open_ms,
            v.open_x10_ms,
            mb(v.bytes_x10),
            mb(v.stored_x10),
            mb(v.compacted_x10)
        ));
    }
    out.push_str(
        "v2 open: the latest snapshot and its tail (DiskProjectV2::open). v2 save: one add-wire transaction \
         saved, durable. crash open: after a child saved and exited without closing the file (redb quick \
         repair). open x10: after the history grew tenfold.\n",
    );
    for r in rows {
        let Some(c) = &r.compression else { continue };
        out.push_str(&format!(
            "\nZstandard on {} {}: {:.2} MB of records, {:.2} MB of chunks uncompressed\n",
            r.tier,
            r.variant,
            mb(c.raw_rows),
            mb(c.raw_chunks)
        ));
        for k in &c.codecs {
            out.push_str(&format!(
                "  {:<18} records {:>8.2} MB, chunks {:>8.2} MB, compress {:>9.1} ms, decompress {:>9.1} ms\n",
                k.name,
                mb(k.rows_bytes),
                mb(k.parts_bytes),
                k.compress_ms,
                k.decompress_ms
            ));
        }
    }
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
            let mut row = json!({
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
                "switch_ms": m.switch_ms,
                "preview_ms": m.preview_ms,
                "panel_ms": m.panel_ms,
                "devices_ms": m.devices_ms,
                "goto_ms": m.goto_ms,
                "devupd_ms": m.devupd_ms,
                "place_ms": m.place_ms,
                "join_ms": m.join_ms,
                "tag_ms": m.tag_ms,
                "pnum_ms": m.pnum_ms,
                "frame_ms": m.frame_ms,
                "tbupd_ms": m.tbupd_ms,
                "gridchange_ms": m.gridchange_ms,
                "revopen_ms": m.revopen_ms,
            });
            let v2 = json!({
                "v2_bytes": r.v2.as_ref().map(|v| v.bytes),
                "convert_ms": r.v2.as_ref().map(|v| v.convert_ms),
                "v2_open_ms": r.v2.as_ref().map(|v| v.open_ms),
                "v2_save_ms": r.v2.as_ref().map(|v| v.save_ms),
                "v2_crash_open_ms": r.v2.as_ref().map(|v| v.crash_open_ms),
                "v2_open_x10_ms": r.v2.as_ref().map(|v| v.open_x10_ms),
                "v2_bytes_x10": r.v2.as_ref().map(|v| v.bytes_x10),
                "v2_stored_x10": r.v2.as_ref().map(|v| v.stored_x10),
                "v2_compacted_x10": r.v2.as_ref().map(|v| v.compacted_x10),
                "compression": r.compression.as_ref().map(|c| json!({
                    "raw_rows": c.raw_rows,
                    "raw_chunks": c.raw_chunks,
                    "codecs": c.codecs.iter().map(|k| json!({
                        "name": k.name, "rows_bytes": k.rows_bytes, "parts_bytes": k.parts_bytes,
                        "compress_ms": k.compress_ms, "decompress_ms": k.decompress_ms,
                    })).collect::<Vec<_>>(),
                })),
            });
            if let (Some(o), Value::Object(extra)) = (row.as_object_mut(), v2) {
                o.extend(extra);
            }
            row
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
        for ((key, _), now) in COLUMNS.iter().zip(values(r)) {
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
