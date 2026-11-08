//! Tester probe for brief 014 (hand check, not an acceptance test of its own): on a kept small
//! benchmark project, move components that have wires attached the way a person does (select,
//! MOVE, click a base point on a terminal, click the second point) and write what changed to a
//! text file, so the same probe can be run on the branch and on its base and the two compared.
//!
//! Runs only when `ECAD_014_PROJECT` names a kept `.ecad` file; `ECAD_014_OUT` names the output.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use cadcraft_engine::doc::Handle;
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use ecad_cadcraft::{Mapped, ProjectHandle};
use ecad_model::derive::terminal_positions;
use ecad_model::{ComponentId, Document, PageId, Subject};

fn page_items(doc: &Document, page: PageId) -> BTreeMap<String, String> {
    doc.items
        .iter()
        .filter(|(_, it)| it.page == page)
        .map(|(id, it)| (format!("{id:?}"), format!("{:?}", it.kind)))
        .collect()
}

fn ends(doc: &Document) -> BTreeMap<String, String> {
    doc.conductors.iter().map(|(id, c)| (format!("{id:?}"), format!("{:?}", c.ends))).collect()
}

fn chosen_first(picks: &[(Handle, ComponentId, [f64; 2], usize)]) -> ComponentId {
    picks[0].1
}

fn diff(text: &mut String, before: &Document, after: &Document, page: PageId) {
    let (ib, ia) = (page_items(before, page), page_items(after, page));
    for (id, kind) in &ia {
        match ib.get(id) {
            Some(k) if k == kind => {}
            Some(k) => writeln!(text, "item {id} changed\n  before {k}\n  after  {kind}").unwrap(),
            None => writeln!(text, "item {id} added\n  after  {kind}").unwrap(),
        }
    }
    for id in ib.keys().filter(|id| !ia.contains_key(*id)) {
        writeln!(text, "item {id} removed").unwrap();
    }
    let (eb, ea) = (ends(before), ends(after));
    for (id, e) in &ea {
        if eb.get(id) != Some(e) {
            writeln!(text, "conductor {id} ends {:?} -> {e}", eb.get(id)).unwrap();
        }
    }
}

#[test]
fn move_components_with_wires_on_the_kept_small_project() {
    let (Some(path), Some(out)) = (std::env::var_os("ECAD_014_PROJECT"), std::env::var_os("ECAD_014_OUT")) else {
        eprintln!("ECAD_014_PROJECT / ECAD_014_OUT not set: probe skipped");
        return;
    };
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    // Work on a copy, so the kept project is left as it is.
    let dir = std::env::temp_dir().join(format!("ecad-014-probe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let copy = dir.join("probe.ecad");
    std::fs::copy(&path, &copy).unwrap();
    let mut s = Session::empty();
    s.viewport_px = (1280.0, 800.0);
    let (project, _) =
        ProjectHandle::open_file_in(&mut s, copy.to_str().unwrap(), 14, ecad_cadcraft::views::Settings::none())
            .unwrap();

    // Components on page 1 with at least one wire attached, with their symbol handle and the
    // position of their first terminal.
    let (page, picks) = project
        .read(&mut s, |p, pg| {
            let doc = p.ledger.document();
            let page = *doc.page_order.first().unwrap();
            let map = &p.pages[&page].map;
            let pos = terminal_positions(doc, page);
            let mut picks: Vec<(Handle, ComponentId, [f64; 2], usize)> = Vec::new();
            for e in pg.entities_of(p, page) {
                let Some(Mapped::Item(item)) = map.get(e.handle) else { continue };
                let Some((_, r)) = doc.representation_of_item(item) else { continue };
                let Subject::Component(c) = r.subject else { continue };
                if r.items.first() != Some(&item) {
                    continue;
                }
                let terms: Vec<_> =
                    pos.iter().filter(|(t, _, _)| doc.terminals.get(t).is_some_and(|x| x.component == c)).collect();
                let wired: usize = terms.iter().map(|(t, _, _)| doc.ends_at(*t).len()).sum();
                if wired == 0 {
                    continue;
                }
                let Some((_, _, at)) = terms.first() else { continue };
                picks.push((e.handle, c, [at.x, at.y], wired));
            }
            picks.sort_by_key(|p| p.0);
            (page, picks)
        })
        .unwrap();
    assert!(picks.len() >= 3, "too few wired components on page 1: {} (page {page:?})", picks.len());

    let mut text = String::new();
    let doc_of =
        |project: &ProjectHandle, s: &mut Session| project.read(s, |p, _| p.ledger.document().clone()).unwrap();
    // Three components, each moved by two deltas (one short, one past a neighbour).
    let chosen: Vec<_> = [0, picks.len() / 2, picks.len() - 1].into_iter().map(|i| picks[i]).collect();
    for (h, comp, at, wired) in chosen {
        for delta in [[12.5, -7.5], [-40.0, 25.0]] {
            let before = doc_of(&project, &mut s);
            let head = project.read(&mut s, |p, _| p.ledger.head()).unwrap();
            // Select the symbol, MOVE, click on its terminal, click the second point.
            s.set_selection(vec![h]);
            s.start("move").unwrap();
            let mut prompts = vec![s.current_prompt().map(|p| p.display()).unwrap_or_default()];
            if prompts[0].to_lowercase().contains("select objects") {
                s.input(Input::Enter).unwrap();
                prompts.push(s.current_prompt().map(|p| p.display()).unwrap_or_default());
            }
            let base = Vec2::new(at[0], at[1]);
            s.cursor = base;
            s.input(Input::Point(base)).unwrap();
            prompts.push(s.current_prompt().map(|p| p.display()).unwrap_or_default());
            let to = Vec2::new(at[0] + delta[0], at[1] + delta[1]);
            s.cursor = to;
            s.input(Input::Point(to)).unwrap();
            let after = doc_of(&project, &mut s);
            let (ops, label, head_after) = project
                .read(&mut s, |p, _| {
                    let l = p.ledger.log().last().cloned();
                    (l.as_ref().map(|t| format!("{:#?}", t.ops)), l.map(|t| t.label), p.ledger.head())
                })
                .unwrap();
            writeln!(text, "=== component {comp:?} (symbol {}, {wired} wire ends) moved by {delta:?}", h.hex())
                .unwrap();
            writeln!(text, "prompts: {prompts:?}").unwrap();
            writeln!(text, "head {head} -> {head_after}, label {label:?}").unwrap();
            let (ib, ia) = (page_items(&before, page), page_items(&after, page));
            for (id, kind) in &ia {
                match ib.get(id) {
                    Some(k) if k == kind => {}
                    Some(k) => writeln!(text, "item {id} changed\n  before {k}\n  after  {kind}").unwrap(),
                    None => writeln!(text, "item {id} added\n  after  {kind}").unwrap(),
                }
            }
            for id in ib.keys().filter(|id| !ia.contains_key(*id)) {
                writeln!(text, "item {id} removed").unwrap();
            }
            let (eb, ea) = (ends(&before), ends(&after));
            for (id, e) in &ea {
                if eb.get(id) != Some(e) {
                    writeln!(text, "conductor {id} ends {:?} -> {e}", eb.get(id)).unwrap();
                }
            }
            let moved_terms: Vec<_> = terminal_positions(&after, page)
                .into_iter()
                .filter(|x| !terminal_positions(&before, page).contains(x))
                .map(|(t, _, p)| format!("{t:?}@({:.3},{:.3})", p.x, p.y))
                .collect();
            writeln!(text, "terminals at new positions: {moved_terms:?}").unwrap();
            writeln!(text, "ops:\n{}", ops.unwrap_or_default()).unwrap();
            // Undo restores the document exactly.
            s.execute("undo", &serde_json::json!({"count": 1})).unwrap();
            assert_eq!(doc_of(&project, &mut s), before, "undo after moving {comp:?} by {delta:?}");
        }
    }
    // MOVE with nothing selected first: at "Select objects" type W (window) or C (crossing), click
    // two corners around the first chosen component's terminals, Enter, then base and second
    // point. The crossing box also picks the wires that run into it, so they move with it.
    let first = chosen_first(&picks);
    let terms: Vec<[f64; 2]> = project
        .read(&mut s, |p, _| {
            let doc = p.ledger.document();
            terminal_positions(doc, page)
                .into_iter()
                .filter(|(t, _, _)| doc.terminals.get(t).is_some_and(|x| x.component == first))
                .map(|(_, _, p)| [p.x, p.y])
                .collect()
        })
        .unwrap();
    let (x0, x1) = terms.iter().fold((f64::MAX, f64::MIN), |(a, b), t| (a.min(t[0]), b.max(t[0])));
    let (y0, y1) = terms.iter().fold((f64::MAX, f64::MIN), |(a, b), t| (a.min(t[1]), b.max(t[1])));
    for (mode, margin) in [("W", 9.0), ("C", 4.0)] {
        for delta in [[12.5, -7.5], [-40.0, 25.0]] {
            let before = doc_of(&project, &mut s);
            let head = project.read(&mut s, |p, _| p.ledger.head()).unwrap();
            s.set_selection(Vec::new());
            s.start("move").unwrap();
            let mut prompts = vec![s.current_prompt().map(|p| p.display()).unwrap_or_default()];
            s.cmdline(mode).unwrap();
            for corner in [Vec2::new(x0 - margin, y0 - margin), Vec2::new(x1 + margin, y1 + margin)] {
                s.cursor = corner;
                s.input(Input::Point(corner)).unwrap();
            }
            prompts.push(s.current_prompt().map(|p| p.display()).unwrap_or_default());
            let picked = s.selection().len();
            s.input(Input::Enter).unwrap();
            prompts.push(s.current_prompt().map(|p| p.display()).unwrap_or_default());
            let base = Vec2::new(terms[0][0], terms[0][1]);
            s.cursor = base;
            s.input(Input::Point(base)).unwrap();
            let to = Vec2::new(base.x + delta[0], base.y + delta[1]);
            s.cursor = to;
            s.input(Input::Point(to)).unwrap();
            let after = doc_of(&project, &mut s);
            let (ops, head_after) = project
                .read(&mut s, |p, _| (p.ledger.log().last().map(|t| format!("{:#?}", t.ops)), p.ledger.head()))
                .unwrap();
            writeln!(text, "=== {mode}-window move of {first:?} by {delta:?} (selection {picked})").unwrap();
            writeln!(text, "prompts: {prompts:?}").unwrap();
            writeln!(text, "head {head} -> {head_after}").unwrap();
            diff(&mut text, &before, &after, page);
            assert!(head_after > head, "{mode}-window move recorded nothing");
            writeln!(text, "ops:\n{}", ops.unwrap_or_default()).unwrap();
            s.execute("undo", &serde_json::json!({"count": 1})).unwrap();
            assert_eq!(doc_of(&project, &mut s), before, "undo after the {mode}-window move");
        }
    }
    // M1 (reroute by a vertex grip) and M3 (an end grip onto another terminal, then into free
    // space), on routes attached to the first chosen component, the way a grip drag reaches them.
    let routes = project
        .read(&mut s, |p, pg| {
            let doc = p.ledger.document();
            let map = &p.pages[&page].map;
            let mut out = Vec::new();
            for e in pg.entities_of(p, page) {
                let Some(Mapped::Item(item)) = map.get(e.handle) else { continue };
                let Some((_, r)) = doc.representation_of_item(item) else { continue };
                let Subject::Conductor(c) = r.subject else { continue };
                let Some(cond) = doc.conductors.get(&c) else { continue };
                let attached =
                    cond.ends.iter().flatten().any(|t| doc.terminals.get(t).is_some_and(|x| x.component == first));
                if !attached {
                    continue;
                }
                if let Some(ecad_model::Item {
                    kind: ecad_model::ItemKind::Primitive(ecad_model::Primitive::Polyline { vertices, .. }),
                    ..
                }) = doc.items.get(&item)
                {
                    out.push((e.handle, vertices.clone()));
                }
            }
            out.sort_by_key(|r| r.0);
            out
        })
        .unwrap();
    assert!(routes.len() >= 3, "too few routes at {first:?}");
    let other_terminal = picks.last().unwrap().2;
    let mut grips: Vec<(String, Handle, usize, [f64; 2])> = Vec::new();
    // Polyline grips are interleaved: vertex i is grip 2i, the midpoint of segment i is grip 2i+1.
    for (h, v) in routes.iter().take(4) {
        let i = 2.min(v.len() - 2);
        grips.push((format!("M1 vertex {i} of {}", h.hex()), *h, 2 * i, [v[i].x + 5.0, v[i].y + 5.0]));
        let m = [(v[1].x + v[2].x) / 2.0, (v[1].y + v[2].y) / 2.0];
        grips.push((format!("M1 segment 1 of {}", h.hex()), *h, 3, [m[0] - 4.0, m[1] + 6.0]));
    }
    let (h, v) = &routes[0];
    let last = v.len() - 1;
    grips.push((format!("M3 end {last} of {} onto a terminal", h.hex()), *h, 2 * last, other_terminal));
    grips.push((
        format!("M3 end {last} of {} into free space", h.hex()),
        *h,
        2 * last,
        [v[last].x + 3.0, v[last].y - 9.0],
    ));
    grips.push((format!("M3 end 0 of {} onto a terminal", h.hex()), *h, 0, other_terminal));
    grips.push((format!("M3 end 0 of {} into free space", h.hex()), *h, 0, [v[0].x - 7.5, v[0].y + 2.5]));
    for (name, h, index, to) in grips {
        let before = doc_of(&project, &mut s);
        let head = project.read(&mut s, |p, _| p.ledger.head()).unwrap();
        let r = s.execute("grip.stretch", &serde_json::json!({"handle": h.hex(), "index": index, "to": to}));
        let after = doc_of(&project, &mut s);
        let (ops, head_after) = project
            .read(&mut s, |p, _| (p.ledger.log().last().map(|t| format!("{:#?}", t.ops)), p.ledger.head()))
            .unwrap();
        writeln!(text, "=== grip {name} to {to:?}: {}", if r.is_ok() { "ok".to_string() } else { format!("{r:?}") })
            .unwrap();
        writeln!(text, "head {head} -> {head_after}").unwrap();
        diff(&mut text, &before, &after, page);
        if head_after != head {
            writeln!(text, "ops:\n{}", ops.unwrap_or_default()).unwrap();
            s.execute("undo", &serde_json::json!({"count": 1})).unwrap();
        }
        assert_eq!(doc_of(&project, &mut s), before, "undo after {name}");
    }
    assert!(project.read(&mut s, |p, _| p.ledger.replay_matches().unwrap()).unwrap());
    std::fs::write(out, text).unwrap();
    drop(project);
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}
