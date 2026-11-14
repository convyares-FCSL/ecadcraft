//! Brief 039 (U3): the one table. Model tests (plain Rust) and widget tests (kittest, no wgpu),
//! acceptance criteria 1, 2 and the fast part of 6; the `ui` measure (criterion 6) is the ignored
//! `measure_10k_rows`, run in release by hand.

use std::collections::HashMap;
use std::time::Instant;

use ecad_ui::table::{
    Cell, Changed, Column, ColumnId, Dir, Row, Selection, TINT_SECONDS, Table, TableEvent, TableSource, TintMap, ViewModel, Width, export,
    natural_cmp,
};
use egui::Key;
use egui_kittest::kittest::Queryable;

use crate::common::harness;

const TAG: ColumnId = ColumnId(0);
const DESC: ColumnId = ColumnId(1);
const PAGE: ColumnId = ColumnId(2);
const KIND: ColumnId = ColumnId(3);

/// A small device-like source: tag and description set here, page read (a go-to), kind read.
#[derive(Clone, Debug)]
struct Demo {
    cols: Vec<Column>,
    keys: Vec<u32>,
    rows: HashMap<u32, [String; 4]>,
}

impl Demo {
    fn new(rows: &[(u32, &str, &str, &str, &str)]) -> Self {
        Demo {
            cols: vec![
                Column::set(0, "Tag", Width::Initial(120.0)),
                Column::set(1, "Description", Width::Remainder),
                Column::read(2, "Page", Width::Initial(80.0)),
                Column::read(3, "Kind", Width::Initial(100.0)).groupable(),
            ],
            keys: rows.iter().map(|r| r.0).collect(),
            rows: rows.iter().map(|(k, a, b, c, d)| (*k, [a.to_string(), b.to_string(), c.to_string(), d.to_string()])).collect(),
        }
    }

    fn set(&mut self, k: u32, col: usize, v: &str) {
        if let Some(slot) = self.rows.get_mut(&k).and_then(|r| r.get_mut(col)) {
            *slot = v.to_string();
        }
    }
}

impl TableSource for Demo {
    type Key = u32;
    fn columns(&self) -> &[Column] {
        &self.cols
    }
    fn keys(&self) -> &[u32] {
        &self.keys
    }
    fn cell(&self, key: u32, col: ColumnId) -> Cell {
        let Some(r) = self.rows.get(&key) else { return Cell::Empty };
        match col {
            TAG => Cell::Input(r[0].clone()),
            DESC => Cell::Input(r[1].clone()),
            PAGE => Cell::go(r[2].clone()),
            KIND => Cell::read(r[3].clone()),
            _ => Cell::Empty,
        }
    }
    fn row_label(&self, key: u32) -> String {
        self.rows.get(&key).map_or_else(String::new, |r| format!("Device {}", r[0]))
    }
    fn chips(&self) -> &[&'static str] {
        &["Coils"]
    }
    fn chip(&self, key: u32, _chip: usize) -> bool {
        self.rows.get(&key).is_some_and(|r| r[3] == "coil")
    }
}

fn demo() -> Demo {
    Demo::new(&[
        (1, "-K10", "Pump contactor", "3", "coil"),
        (2, "-K2", "Fan contactor", "1", "coil"),
        (3, "-Q1", "Main switch", "1", "switch"),
        (4, "-K1", "Heater contactor", "2", "coil"),
        (5, "-S3", "Stop button", "4", "switch"),
    ])
}

fn tags(v: &ViewModel<u32>, src: &Demo) -> Vec<String> {
    v.rows()
        .iter()
        .map(|r| match r {
            Row::Group { label, count, open } => format!("[{label} {count}{}]", if *open { "" } else { " closed" }),
            Row::Item(k) => src.rows.get(k).map(|r| r[0].clone()).unwrap_or_default(),
        })
        .collect()
}

/// Criterion 1: natural tag order.
#[test]
fn natural_order_puts_k2_before_k10() {
    use std::cmp::Ordering::*;
    for (a, b, o) in
        [("-K2", "-K10", Less), ("-K10", "-K2", Greater), ("-k1", "-K2", Less), ("1.10", "1.9", Greater), ("X", "X", Equal), ("", "a", Less)]
    {
        assert_eq!(natural_cmp(a, b), o, "{a} vs {b}");
    }
}

/// Criterion 1: filter, group and sort give the expected order (a table of cases).
#[test]
fn filter_group_and_sort_give_the_expected_order() {
    let src = demo();
    type Case = (Option<(ColumnId, Dir)>, Option<ColumnId>, &'static str, bool, &'static [&'static str]);
    let cases: &[Case] = &[
        (None, None, "", false, &["-K10", "-K2", "-Q1", "-K1", "-S3"]),
        (Some((TAG, Dir::Asc)), None, "", false, &["-K1", "-K2", "-K10", "-Q1", "-S3"]),
        (Some((TAG, Dir::Desc)), None, "", false, &["-S3", "-Q1", "-K10", "-K2", "-K1"]),
        (Some((TAG, Dir::Asc)), None, "contactor", false, &["-K1", "-K2", "-K10"]),
        (Some((TAG, Dir::Asc)), None, "", true, &["-K1", "-K2", "-K10"]),
        (Some((TAG, Dir::Asc)), Some(KIND), "", false, &["[coil 3]", "-K1", "-K2", "-K10", "[switch 2]", "-Q1", "-S3"]),
        (Some((PAGE, Dir::Asc)), Some(KIND), "main", false, &["[switch 1]", "-Q1"]),
    ];
    for (sort, group, query, chip, want) in cases {
        let mut v = ViewModel::default();
        v.rebuild(&src);
        v.set_sort(&src, *sort);
        v.set_group(&src, *group);
        v.set_query(&src, query);
        v.set_chip(&src, 0, *chip);
        assert_eq!(tags(&v, &src), *want, "sort {sort:?} group {group:?} query {query:?} chip {chip}");
    }
    // A closed group keeps its heading and count, without its rows.
    let mut v = ViewModel::default();
    v.rebuild(&src);
    v.set_group(&src, Some(KIND));
    v.toggle_group(&src, "coil");
    assert_eq!(tags(&v, &src), ["[coil 3 closed]", "[switch 2]", "-Q1", "-S3"]);
}

/// Criterion 1: the selection is a set of keys, so it survives sort and filter.
#[test]
fn selection_survives_sort_and_filter() {
    let src = demo();
    let mut v = ViewModel::default();
    v.rebuild(&src);
    let mut sel = Selection::default();
    sel.click(2);
    sel.extend_to(4, &v.visible_keys());
    assert_eq!(sel.keys().iter().copied().collect::<Vec<_>>(), [2, 3, 4], "range in the source order");
    v.set_sort(&src, Some((TAG, Dir::Desc)));
    v.set_query(&src, "contactor");
    assert_eq!(sel.keys().iter().copied().collect::<Vec<_>>(), [2, 3, 4]);
    assert_eq!(sel.in_order(&v.visible_keys()), [2, 4, 3], "shown first (-K2, -K1), then the filtered-out -Q1");
    sel.retain(|k| k != 3);
    assert_eq!(sel.keys().len(), 2);
}

/// Criterion 1: patching a touched key moves it to its sorted place without a rebuild, and only
/// its changed cells are reported for the tint.
#[test]
fn patching_a_key_moves_it_without_a_rebuild() {
    let mut src = demo();
    let mut v = ViewModel::default();
    v.rebuild(&src);
    v.set_sort(&src, Some((TAG, Dir::Asc)));
    let builds = v.builds;
    src.set(4, 0, "-K20");
    let changed = v.patch(&src, &[4]);
    assert_eq!(changed, [(4, TAG)], "only the tag cell changed");
    assert_eq!(tags(&v, &src), ["-K2", "-K10", "-K20", "-Q1", "-S3"]);
    assert_eq!(v.builds, builds, "no rebuild");
    assert_eq!(v.patches, 1);
    // Added and removed rows patch the same way.
    src.keys.push(6);
    src.rows.insert(6, ["-K5".into(), "New".into(), "5".into(), "coil".into()]);
    src.keys.retain(|k| *k != 3);
    src.rows.remove(&3);
    let changed = v.patch(&src, &[6, 3]);
    assert!(changed.is_empty(), "new and removed rows are not tinted: {changed:?}");
    assert_eq!(tags(&v, &src), ["-K2", "-K5", "-K10", "-K20", "-S3"]);
    assert_eq!(v.builds, builds);
}

/// Review of PR #73: with no sort column, a patch follows the source's new order, also for rows
/// that moved without being touched (a renamed device re-sorted in its list).
#[test]
fn patching_under_the_source_order_follows_a_reordered_source() {
    let mut src = demo();
    let mut v = ViewModel::default();
    v.rebuild(&src);
    assert_eq!(tags(&v, &src), ["-K10", "-K2", "-Q1", "-K1", "-S3"]);
    // -S3 renamed -A1: the source lists it first; -Q1 also moves behind -K1 untouched.
    src.set(5, 0, "-A1");
    src.keys = vec![5, 1, 2, 4, 3];
    let builds = v.builds;
    let changed = v.patch(&src, &[5]);
    assert_eq!(changed, [(5, TAG)]);
    assert_eq!(tags(&v, &src), ["-A1", "-K10", "-K2", "-K1", "-Q1"], "the source's order");
    assert_eq!(v.builds, builds, "no rebuild");
    // A row added in the middle of the source.
    src.keys = vec![5, 1, 6, 2, 4, 3];
    src.rows.insert(6, ["-B2".into(), "New".into(), "2".into(), "coil".into()]);
    v.patch(&src, &[6]);
    assert_eq!(tags(&v, &src), ["-A1", "-K10", "-B2", "-K2", "-K1", "-Q1"]);
    assert_eq!(v.item_count(), 6);
}

/// Criterion 1: the tint lasts 1.2 s and is only on the changed cells.
#[test]
fn the_tint_lasts_1_2_s_on_changed_cells_only() {
    let mut t = TintMap::default();
    t.mark([(4u32, TAG)], 10.0);
    assert_eq!(t.level(4, TAG, 10.0), 1.0);
    assert!(t.level(4, TAG, 10.6) > 0.4 && t.level(4, TAG, 10.6) < 0.6, "fading");
    assert_eq!(t.level(4, DESC, 10.1), 0.0, "other cells of the row");
    assert_eq!(t.level(3, TAG, 10.1), 0.0, "other rows");
    assert!(t.live(10.0 + TINT_SECONDS - 0.01));
    assert_eq!(t.level(4, TAG, 10.0 + TINT_SECONDS + 0.01), 0.0);
    assert!(!t.live(10.0 + TINT_SECONDS + 0.01));

    // Through the table's update map entry point.
    let mut src = demo();
    let mut table = Table::new("tint", "Devices");
    table.update_at(&src, Changed::All, 0.0);
    src.set(1, 1, "Pump contactor 2");
    table.update_at(&src, Changed::Keys(vec![1]), 5.0);
    assert_eq!(table.tint.tinted(5.5), [(1, DESC)]);
    assert_eq!(table.view.builds, 1);
    table.update_at(&src, Changed::Nothing, 6.0);
    assert_eq!((table.view.builds, table.view.patches), (1, 1), "nothing changed: nothing done");
}

/// CSV of the shown rows, quoted where needed.
#[test]
fn csv_has_the_shown_rows_and_columns() {
    let mut src = demo();
    src.set(3, 1, "Main, \"big\" switch");
    let csv = export::csv(&src, &src.cols, &[3, 2]);
    assert_eq!(csv, "Tag,Description,Page,Kind\r\n-Q1,\"Main, \"\"big\"\" switch\",1,switch\r\n-K2,Fan contactor,1,coil\r\n");
}

/// A synthetic source of `n` rows.
fn big(n: u32) -> Demo {
    let rows: Vec<(u32, String, String, String, String)> = (0..n)
        .map(|i| {
            (
                i,
                format!("-K{}", (i * 7919) % n),
                format!("Device number {i} of the big list"),
                format!("{}", i % 500),
                ["coil", "switch", "fuse"][(i % 3) as usize].to_string(),
            )
        })
        .collect();
    let refs: Vec<(u32, &str, &str, &str, &str)> = rows.iter().map(|(k, a, b, c, d)| (*k, a.as_str(), b.as_str(), c.as_str(), d.as_str())).collect();
    Demo::new(&refs)
}

/// Criterion 6, fast part: 10 000 rows build, sort, group and filter within 10× the targets
/// (sort or group ≤ 20 ms, filter ≤ 10 ms in release); a generous bound, not a benchmark.
#[test]
fn ten_thousand_rows_sort_and_filter_within_a_generous_bound() {
    let src = big(10_000);
    let mut v = ViewModel::default();
    v.rebuild(&src);
    let t = Instant::now();
    v.set_sort(&src, Some((TAG, Dir::Asc)));
    let sort = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    v.set_group(&src, Some(KIND));
    let group = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    v.set_query(&src, "number 12");
    let filter = t.elapsed().as_secs_f64() * 1e3;
    assert!(sort <= 200.0 && group <= 200.0 && filter <= 100.0, "sort {sort:.1} ms, group {group:.1} ms, filter {filter:.1} ms");
    assert_eq!(v.visible_keys().len(), 111, "12, 120–129, 1200–1299");
}

type State = (Table<u32>, Demo, Vec<TableEvent<u32>>);

fn widget(src: Demo) -> egui_kittest::Harness<'static, State> {
    harness(
        |ui, s: &mut State| {
            let events = s.0.show(ui, &s.1);
            s.2.extend(events);
        },
        (Table::new("demo", "Devices"), src, Vec::new()),
    )
}

fn commits(s: &State) -> Vec<(u32, ColumnId, String)> {
    s.2.iter()
        .filter_map(|e| match e {
            TableEvent::Commit { key, col, value } => Some((*key, *col, value.clone())),
            _ => None,
        })
        .collect()
}

/// Criterion 2: Enter on a set cell starts the edit, Enter commits one `Commit`, Esc reverts, Tab
/// goes to the next set cell.
#[test]
fn enter_edits_enter_commits_esc_reverts_tab_moves() {
    let mut h = widget(demo());
    h.get_by_label("Device -K2").click();
    h.run();
    assert_eq!(h.state().0.selection.keys().iter().copied().collect::<Vec<_>>(), [2]);
    // The click landed in the Description column (the row's middle); start on Tag instead.
    h.state_mut().0.cursor_col = Some(TAG);
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().0.edit.as_ref().is_some_and(|e| e.key == 2 && e.col == TAG), "{:?}", h.state().0.edit);
    let field = h.get_by_label("Edit Tag");
    field.type_text("0");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(commits(h.state()), [(2, TAG, "-K20".to_string())], "one commit");
    assert!(h.state().0.edit.is_none());

    // Esc reverts: nothing committed.
    h.state_mut().2.clear();
    h.key_press(Key::Enter);
    h.run();
    h.get_by_label("Edit Tag").type_text("zz");
    h.run();
    h.key_press(Key::Escape);
    h.run();
    assert!(commits(h.state()).is_empty() && h.state().0.edit.is_none(), "{:?}", h.state().2);

    // Tab commits and goes to the next set cell (Description of the same row).
    h.key_press(Key::Enter);
    h.run();
    h.get_by_label("Edit Tag").type_text("9");
    h.run();
    h.key_press(Key::Tab);
    h.run();
    assert_eq!(commits(h.state()), [(2, TAG, "-K29".to_string())]);
    assert!(h.state().0.edit.as_ref().is_some_and(|e| e.key == 2 && e.col == DESC && e.text == "Fan contactor"), "{:?}", h.state().0.edit);
}

/// Criterion 2: read cells cannot be edited; Enter or a click on one with a target emits `GoTo`.
#[test]
fn read_cells_do_not_edit_and_go_to() {
    let mut h = widget(demo());
    h.get_by_label("Device -Q1").click();
    h.run();
    h.state_mut().0.cursor_col = Some(PAGE);
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().0.edit.is_none(), "a read cell does not edit");
    assert!(h.state().2.contains(&TableEvent::GoTo { key: 3, col: PAGE }), "{:?}", h.state().2);
    h.key_press(Key::F2);
    h.run();
    assert!(h.state().0.edit.is_none());
    h.state_mut().2.clear();
    if let Some(n) = h.get_all_by_label("4").next() {
        n.click();
    }
    h.run();
    assert!(h.state().2.contains(&TableEvent::GoTo { key: 5, col: PAGE }), "{:?}", h.state().2);
}

/// Criterion 2: Shift-click selects a range in the shown order; Ctrl-click toggles.
#[test]
fn shift_click_selects_a_range() {
    let mut h = widget(demo());
    h.get_by_label("Device -K2").click();
    h.run();
    h.get_by_label("Device -S3").click_modifiers(egui::Modifiers::SHIFT);
    h.run();
    let sel: Vec<u32> = h.state().0.selection.keys().iter().copied().collect();
    assert_eq!(sel, [2, 3, 4, 5]);
    assert_eq!(h.state().2.last(), Some(&TableEvent::Selected(vec![2, 3, 4, 5])));
    h.get_by_label("Device -Q1").click_modifiers(egui::Modifiers::COMMAND);
    h.run();
    assert_eq!(h.state().0.selection.keys().iter().copied().collect::<Vec<_>>(), [2, 4, 5]);
}

/// Criterion 2: a refusal from the owner shows under the cell, and the value reverts.
#[test]
fn a_refusal_shows_under_the_cell_and_the_value_reverts() {
    let mut h = widget(demo());
    h.get_by_label("Device -K2").click();
    h.run();
    h.state_mut().0.cursor_col = Some(TAG);
    h.key_press(Key::Enter);
    h.run();
    h.get_by_label("Edit Tag").type_text("0");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(commits(h.state()).len(), 1);
    // The owner's command refused it: the source keeps -K2.
    h.state_mut().0.refuse(2, TAG, "-K20 is used by another device.");
    h.run();
    h.get_by_label("-K20 is used by another device.");
    assert!(h.query_by_label("Device -K2").is_some(), "the row shows the source's value");
    h.get_by_label("Device -Q1").click();
    h.run();
    assert!(h.query_by_label("-K20 is used by another device.").is_none(), "gone at the next click");
}

/// Virtualised: 10 000 rows lay out only the visible ones.
#[test]
fn ten_thousand_rows_lay_out_only_the_visible_ones() {
    let h = widget(big(10_000));
    let drawn = h.state().0.rows_drawn;
    assert!(drawn > 5 && drawn < 40, "{drawn} rows laid out");
}

/// The `ui` measure (criterion 6): 10 000 rows, scrolled frame by frame; prints the table body's
/// time per frame, sort, group and filter. Run in release:
/// `cargo test -p ecad-ui --release table::measure -- --ignored --nocapture`.
#[test]
#[ignore = "measure: run by hand in release (criterion 6)"]
fn measure_10k_rows() {
    let src = big(10_000);
    let mut v = ViewModel::default();
    let t = Instant::now();
    v.rebuild(&src);
    let build = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    v.set_sort(&src, Some((TAG, Dir::Asc)));
    let sort = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    v.set_group(&src, Some(KIND));
    let group = t.elapsed().as_secs_f64() * 1e3;
    let mut filter = 0.0f64;
    for q in ["n", "nu", "num", "numb", "number 1", "number 12"] {
        let t = Instant::now();
        v.set_query(&src, q);
        filter = filter.max(t.elapsed().as_secs_f64() * 1e3);
    }
    // Scrolling: the table body drawn while the scroll offset moves down the list.
    let mut times = Vec::new();
    let mut h = harness(
        |ui, s: &mut (Table<u32>, Demo, Vec<f64>)| {
            let t = Instant::now();
            s.0.show(ui, &s.1);
            s.2.push(t.elapsed().as_secs_f64() * 1e3);
        },
        (Table::new("measure", "Rows"), src, Vec::new()),
    );
    for _ in 0..300 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -600.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        });
        h.input_mut().events.push(egui::Event::PointerMoved(egui::pos2(450.0, 400.0)));
        h.step();
    }
    times.extend(h.state().2.iter().skip(5).copied());
    times.sort_by(f64::total_cmp);
    let p = |q: f64| times.get(((times.len() as f64 - 1.0) * q) as usize).copied().unwrap_or(0.0);
    println!(
        "ui measure, 10 000 rows: build {build:.1} ms, sort {sort:.1} ms, group {group:.1} ms, filter keystroke max {filter:.2} ms; \
         table body per frame median {:.2} ms, p95 {:.2} ms, max {:.2} ms over {} frames",
        p(0.5),
        p(0.95),
        p(1.0),
        times.len()
    );
}
