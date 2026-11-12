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
    /// History and branches on that file (brief 020a).
    pub history: Option<crate::history::MeasuredHistory>,
    /// Locations, reports and printing on that file (brief 024).
    pub outputs: Option<crate::outputs::MeasuredOutputs>,
    /// Terminal strips and cable cores on that file (brief 028).
    pub strips: Option<crate::strips::MeasuredStrips>,
    /// The pre-release check and the release on that file (brief 027).
    pub release: Option<crate::release::MeasuredRelease>,
}

/// The measured columns: JSON key, table heading.
const COLUMNS: [(&str, &str); 42] = [
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
    ("nets_ms", "nets ms"),
    ("netupd_ms", "netupd ms"),
    ("netcut_ms", "netcut ms"),
    ("rail_ms", "rail ms"),
    ("pnum_ms", "pnum ms"),
    ("frame_ms", "frame ms"),
    ("tbupd_ms", "tbupd ms"),
    ("gridchange_ms", "gridchange ms"),
    ("revopen_ms", "revopen ms"),
    ("renumprev_ms", "renumprev ms"),
    ("renum_ms", "renum ms"),
    ("renumw_ms", "renumw ms"),
    ("renumu_ms", "renumu ms"),
    ("swap_ms", "swap ms"),
    ("finalise_ms", "finalise ms"),
    ("xbuild_ms", "xbuild ms"),
    ("xmove_ms", "xmove ms"),
    ("xrenum_ms", "xrenum ms"),
    ("xmirror_ms", "xmirror ms"),
    ("convert_ms", "v2 convert ms"),
    ("v2_open_ms", "v2 open ms"),
    ("v2_save_ms", "v2 save ms"),
    ("v2_crash_open_ms", "v2 crash open ms"),
    ("v2_open_x10_ms", "v2 open x10 ms"),
];

fn values(r: &Row) -> [Option<f64>; 42] {
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
        Some(m.nets_ms),
        Some(m.netupd_ms),
        Some(m.netcut_ms),
        Some(m.rail_ms),
        Some(m.pnum_ms),
        Some(m.frame_ms),
        Some(m.tbupd_ms),
        Some(m.gridchange_ms),
        Some(m.revopen_ms),
        Some(m.numbering.renumprev_ms),
        Some(m.numbering.renum_ms),
        Some(m.numbering.renumw_ms),
        Some(m.numbering.renumu_ms),
        Some(m.numbering.swap_ms),
        Some(m.numbering.finalise_ms),
        Some(m.xbuild_ms),
        Some(m.xmove_ms),
        Some(m.xrenum_ms),
        Some(m.xmirror_ms),
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
pub fn table(rows: &[Row], runs: usize, print: Option<&crate::release::MeasuredPrint>) -> String {
    let mut out = format!(
        "{:<8} {:<8} {:>9} {:>8} {:>10} {:>10} {:>9} {:>12} {:>9} {:>9} {:>13} {:>14} {:>13} {:>10} {:>11} {:>9} {:>11} {:>8} {:>10} {:>9} {:>8} {:>7} {:>8} {:>10} {:>10} {:>8} {:>8} {:>9} {:>9} {:>14} {:>11}
",
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
        "nets ms",
        "netupd ms",
        "netcut ms",
        "rail ms",
        "pnum ms",
        "frame ms",
        "tbupd ms",
        "gridchange ms",
        "revopen ms"
    );
    for r in rows {
        let m = &r.measured;
        out.push_str(&format!(
            "{:<8} {:<8} {:>9.2} {:>8} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}
",
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
            cell(Some(m.nets_ms), 8, 2),
            cell(Some(m.netupd_ms), 10, 3),
            cell(Some(m.netcut_ms), 10, 3),
            cell(Some(m.rail_ms), 8, 3),
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
         new device, a NO contact added to it, and its tag renamed. nets (brief 022): build the nets from scratch, \
         as on open (target <= 100 ms on large); netupd: one wire attached at both ends committed with the \
         numbering pass, Checks and page 1's labels updated, then undone (target about 1 ms, <= 2 ms); netcut: \
         delete one conductor of the largest net (target <= 5 ms); rail: the same on a synthetic net of 25,000 \
         conductors round one point (a 0V rail), which the generated tiers do not have. \
         pnum, frame, tbupd, gridchange and \
         revopen (brief 019): renumber page 2 with its references refreshed; work out one page's frame \
         and title block; change the project title; change the grid (A3 and A2 in turn); open Rev A \
         read-only. Peak memory is not measured. Since brief 021 the app opens version 2 files: every \
         column from 1st page on is measured on the version 2 file; open, edit and save are version 1.\n"
    ));
    out.push_str(&format!(
        "\nCross-references (brief 025):\n{:<8} {:<8} {:>10} {:>10} {:>10} {:>11}\n",
        "tier", "history", "xbuild ms", "xmove ms", "xrenum ms", "xmirror ms"
    ));
    for r in rows {
        let m = &r.measured;
        out.push_str(&format!(
            "{:<8} {:<8} {:>10.2} {:>10.2} {:>10.2} {:>11.2}\n",
            r.tier, r.variant, m.xbuild_ms, m.xmove_ms, m.xrenum_ms, m.xmirror_ms
        ));
    }
    out.push_str(
        "On a copy with 1 000 coils added (a power3 on each coil's page, a NO and an NC on the next page, \
         half with an offer). xbuild: the reference index built from scratch (target < 10 ms on large); xmove: \
         a coil moved, committed and redrawn (target: < 1 ms beyond draw move); xrenum: a page with 40 contact \
         targets renumbered; xmirror: a page with 10 tables brought to the front (compare with switch).\n",
    );
    out.push_str(&format!(
        "\nContainer version 2 (brief 021):\n{:<8} {:<8} {:>9} {:>11} {:>11} {:>11} {:>14} {:>15} {:>11}\n",
        "tier", "history", "v1 MB", "v2 MB", "convert ms", "v2 open ms", "v2 save ms", "crash open ms", "open x10 ms"
    ));
    for r in rows {
        let Some(v) = &r.v2 else { continue };
        out.push_str(&format!(
            "{:<8} {:<8} {:>9.2} {:>11.2} {:>11.0} {:>11.1} {:>14.2} {:>15.1} {:>11.1}   (x10: {:.2} MB, {:.2} MB stored, {:.2} MB compacted; undo {:.2} ms, first past 1 024: {:.1} ms)\n",
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
            mb(v.compacted_x10),
            v.undo_ms,
            v.undo_past_cap_ms
        ));
    }
    out.push_str(
        "v2 open: the latest snapshot and its tail (DiskProjectV2::open). v2 save: one add-wire transaction \
         saved, durable. crash open: after a child saved and exited without closing the file (redb quick \
         repair). open x10: after the history grew tenfold.\n",
    );
    for r in rows {
        let Some(h) = &r.history else { continue };
        out.push_str(&format!(
            "\nHistory on {} {} ({} rows, {} branches): histshow {:.1} ms, histupd {:.1} ms, histsel {:.1} ms, \
             pointshow {:.1} ms, pointstep {:.1} ms, brswitch {:.1} ms, restore {:.1} ms, mergeprev {:.1} ms, merge {:.1} ms \
             ({} changes, {} conflict)\n",
            r.tier,
            r.variant,
            h.nodes,
            h.branches,
            h.histshow_ms,
            h.histupd_ms,
            h.histsel_ms,
            h.pointshow_ms,
            h.pointstep_ms,
            h.brswitch_ms,
            h.restore_ms,
            h.mergeprev_ms,
            h.merge_ms,
            h.merge_changes,
            h.merge_conflicts
        ));
    }
    for r in rows {
        let Some(o) = &r.outputs else { continue };
        out.push_str(&format!(
            "\nOutputs on {} {} ({} devices; box holds {}): reportfirst {:.1} ms, locset {:.2} ms, boxmove {:.2} ms \
             (plain move {:.2} ms), symin {:.2} ms, projloc {:.2} ms, bomupd {:.2} ms, wireupd {:.2} ms; most rows one edit \
             recomputed: {}\n",
            r.tier,
            r.variant,
            o.devices,
            o.inside,
            o.reportfirst_ms,
            o.locset_ms,
            o.boxmove_ms,
            o.plainmove_ms,
            o.symin_ms,
            o.projloc_ms,
            o.bomupd_ms,
            o.wireupd_ms,
            o.work
        ));
    }
    for r in rows {
        let Some(t) = &r.strips else { continue };
        out.push_str(&format!(
            "\nTerminal strips on {} {} ({} strips, {} terminals, {} cables): stripbuild {:.1} ms (target < 50), \
             tplace {:.2} ms (< 16), tfence {:.2} ms (< 16), stripupd {:.2} ms (< 2 beyond attach {:.2} ms), relocate {:.2} ms (< 16), \
             tplan {:.1} ms (< 100), trenum {:.2} ms (< 16); most strips one edit recomputed: {}\n",
            r.tier,
            r.variant,
            t.strips,
            t.terminals,
            t.cables,
            t.stripbuild_ms,
            t.tplace_ms,
            t.tfence_ms,
            t.stripupd_ms,
            t.attach_ms,
            t.relocate_ms,
            t.tplan_ms,
            t.trenum_ms,
            t.work
        ));
    }
    for r in rows {
        let Some(o) = &r.release else { continue };
        out.push_str(&format!(
            "\nRelease check on {} {} ({} check rows): prerelease {:.2} ms, orphanupd {:.2} ms (orphan work {:.3} ms)\n",
            r.tier, r.variant, o.rows, o.prerelease_ms, o.orphanupd_ms, o.orphanwork_ms
        ));
    }
    if let Some(p) = print {
        out.push_str(&format!(
            "\nPrint and release on the {}-page project: print {:.2} s ({} sheets), release {:.2} s ({} sheets, {} files; \
             target 5 s), rerelease {:.2} s\n",
            p.pages,
            p.print_ms / 1000.0,
            p.print_sheets,
            p.release_s,
            p.sheets,
            p.files,
            p.rerelease_s
        ));
    }
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
pub fn json(rows: &[Row], runs: usize, commit: &str, when: &str, print: Option<&crate::release::MeasuredPrint>) -> Value {
    let machine = machine_name();
    let cpus = std::thread::available_parallelism().map_or(0, |n| n.get());
    let print_v = print.map(|p| {
        json!({
            "pages": p.pages, "print_ms": p.print_ms, "print_sheets": p.print_sheets, "release_s": p.release_s,
            "rerelease_s": p.rerelease_s, "sheets": p.sheets, "files": p.files,
        })
    });
    let mut out = json!({
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
                "nets_ms": m.nets_ms,
                "netupd_ms": m.netupd_ms,
                "netcut_ms": m.netcut_ms,
                "netcut_size": m.netcut_size,
                "rail_ms": m.rail_ms,
                "pnum_ms": m.pnum_ms,
                "frame_ms": m.frame_ms,
                "tbupd_ms": m.tbupd_ms,
                "gridchange_ms": m.gridchange_ms,
                "revopen_ms": m.revopen_ms,
            });
            // Brief 026 (design 026 §11), outside the macro (its recursion limit).
            let n = &m.numbering;
            for (k, v) in [
                ("renumprev_ms", n.renumprev_ms),
                ("renum_ms", n.renum_ms),
                ("renumw_ms", n.renumw_ms),
                ("renumu_ms", n.renumu_ms),
                ("swap_ms", n.swap_ms),
                ("finalise_ms", n.finalise_ms),
            ] {
                row[k] = json!(v);
            }
            row["renum_count"] = json!(n.renum_count);
            // Brief 025 (set apart: one `json!` of every column reaches the macro's recursion limit).
            for (k, v) in [("xbuild_ms", m.xbuild_ms), ("xmove_ms", m.xmove_ms), ("xrenum_ms", m.xrenum_ms), ("xmirror_ms", m.xmirror_ms)] {
                row[k] = json!(v);
            }
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
                "v2_undo_ms": r.v2.as_ref().map(|v| v.undo_ms),
                "v2_undo_past_cap_ms": r.v2.as_ref().map(|v| v.undo_past_cap_ms),
                "outputs": r.outputs.as_ref().map(|o| json!({
                    "reportfirst_ms": o.reportfirst_ms, "locset_ms": o.locset_ms, "boxmove_ms": o.boxmove_ms,
                    "plainmove_ms": o.plainmove_ms, "symin_ms": o.symin_ms, "projloc_ms": o.projloc_ms,
                    "bomupd_ms": o.bomupd_ms, "wireupd_ms": o.wireupd_ms,
                    "work": o.work, "devices": o.devices, "inside": o.inside,
                })),
                "release": r.release.as_ref().map(|o| json!({
                    "prerelease_ms": o.prerelease_ms, "orphanupd_ms": o.orphanupd_ms, "orphanwork_ms": o.orphanwork_ms,
                    "rows": o.rows,
                })),
                "strips": r.strips.as_ref().map(|t| json!({
                    "stripbuild_ms": t.stripbuild_ms, "tplace_ms": t.tplace_ms, "tfence_ms": t.tfence_ms,
                    "stripupd_ms": t.stripupd_ms, "attach_ms": t.attach_ms, "relocate_ms": t.relocate_ms, "tplan_ms": t.tplan_ms,
                    "trenum_ms": t.trenum_ms, "strips": t.strips, "terminals": t.terminals, "cables": t.cables,
                    "work": t.work,
                })),
                "history": r.history.as_ref().map(|h| json!({
                    "histshow_ms": h.histshow_ms, "histupd_ms": h.histupd_ms, "histsel_ms": h.histsel_ms,
                    "pointshow_ms": h.pointshow_ms, "pointstep_ms": h.pointstep_ms, "brswitch_ms": h.brswitch_ms,
                    "restore_ms": h.restore_ms, "mergeprev_ms": h.mergeprev_ms, "merge_ms": h.merge_ms,
                    "merge_changes": h.merge_changes, "merge_conflicts": h.merge_conflicts,
                    "nodes": h.nodes, "branches": h.branches,
                })),
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
    });
    // Print Book and release on the 25-page project (brief 027), once per run.
    out["print"] = print_v.unwrap_or(Value::Null);
    out
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
        let before =
            old.iter().find(|o| what(o, "tier").as_deref() == Some(r.tier.as_str()) && what(o, "variant").as_deref() == Some(r.variant.as_str()));
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
