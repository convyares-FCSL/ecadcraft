//! Brief 031 Q3 (design 031 §5, §13 T1, T5-T8, T14, T16, FP5): show and hide per text, runs,
//! hidden text still being data, Show Hidden Text, and symbol keys following their symbol.

use crate::common::{H, id};

use cadcraft_engine::doc::EntityKind;
use ecad_cadcraft::devices::{self, TextOf};
use ecad_model::{ComponentId, PageId, RepresentationId, Subject};
use serde_json::{Value, json};

const SEED: u64 = 0x31_03;

fn place(h: &mut H, symbol: &str, at: [f64; 2], device: Option<ComponentId>) -> Value {
    let mut p = json!({"symbol": symbol, "at": at});
    if let Some(c) = device {
        p["device"] = json!(c.hex());
    }
    h.run("place", p).unwrap()
}

/// `n` terminals of one strip in a row, `step` mm apart from `x0`; the strip and its symbols.
fn strip_row(h: &mut H, x0: f64, y: f64, n: usize, step: f64, strip: Option<ComponentId>) -> (ComponentId, Vec<RepresentationId>) {
    let mut c = strip;
    for i in 0..n {
        let v = place(h, "terminal", [x0 + step * i as f64, y], c);
        c = Some(id(&v, "component"));
    }
    let c = c.unwrap();
    let doc = h.doc();
    let mut reps: Vec<(f64, f64, RepresentationId)> = doc
        .representations_of(Subject::Component(c))
        .filter_map(|(rid, r)| {
            let at = r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
                ecad_model::ItemKind::Symbol(s) => Some(s.placement.origin),
                _ => None,
            })?;
            Some((at.y, at.x, rid))
        })
        .collect();
    reps.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.total_cmp(&b.1)));
    (c, reps.into_iter().map(|r| r.2).collect())
}

/// Tag texts on `page`: (representation, shown).
fn tags(h: &mut H, page: PageId) -> Vec<(RepresentationId, bool)> {
    let doc = h.doc();
    devices::all_texts_on_page(&doc, page).into_iter().filter(|t| t.text.of == TextOf::Tag).map(|t| (t.text.rep, t.shown)).collect()
}

fn shown_tags(h: &mut H, page: PageId, c: ComponentId) -> Vec<RepresentationId> {
    let doc = h.doc();
    let mine: Vec<RepresentationId> = doc.representations_of(Subject::Component(c)).map(|(r, _)| r).collect();
    tags(h, page).into_iter().filter(|(r, s)| *s && mine.contains(r)).map(|(r, _)| r).collect()
}

#[test]
fn a_strip_of_ten_adjacent_terminals_shows_its_tag_once() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let (x1, reps) = strip_row(&mut h, 60.0, 150.0, 10, 10.0, None);
    assert_eq!(reps.len(), 10);
    assert_eq!(shown_tags(&mut h, page, x1), vec![reps[0]], "only the leftmost");
    // The page draws one tag text for the strip.
    let tag = h.p(|p| devices::tag_words(p.ledger.document(), x1));
    let drawn = h.pv(|p, pg| pg.entities_of(p, page)).iter().filter(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == tag)).count();
    assert_eq!(drawn, 1);
    // `pagetexts` lists all ten, nine hidden.
    let v = h.run("pagetexts", json!({})).unwrap();
    let rows: Vec<&Value> = v["texts"].as_array().unwrap().iter().filter(|r| r["kind"] == "tag" && r["value"] == tag.as_str()).collect();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows.iter().filter(|r| r["shown"] == json!(false)).count(), 9);
    h.check();
}

#[test]
fn two_groups_on_a_line_and_another_device_between_split_runs() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    // Two groups of one strip on one line, 50 mm between their starts (about 45 mm between boxes).
    let (x1, a) = strip_row(&mut h, 60.0, 150.0, 5, 10.0, None);
    let (_, b) = strip_row(&mut h, 150.0, 150.0, 5, 10.0, Some(x1));
    let b: Vec<RepresentationId> = b.into_iter().filter(|r| !a.contains(r)).collect();
    assert_eq!(shown_tags(&mut h, page, x1).len(), 2, "two runs: twice");
    // A symbol-level `on` adds a third.
    let last = *b.last().unwrap();
    h.run("showtext", json!({"kind": "tag", "show": "on", "representation": last.hex()})).unwrap();
    assert_eq!(shown_tags(&mut h, page, x1).len(), 3);
    // T16: a project run gap of 50 mm joins the two groups into one run.
    let gap =
        ecad_model::Op::SetProperty { object: ecad_model::ObjectRef::Project, key: "text.run.gap".into(), before: None, after: Some("50".into()) };
    h.project.apply_ops(&mut h.s, "gap", vec![gap]).unwrap();
    let shown = shown_tags(&mut h, page, x1);
    assert_eq!(shown.len(), 2, "one run, plus the symbol set on: {shown:?}");
    // A -X02 terminal between two -X01 terminals of another line splits that line's run.
    let (_, c) = strip_row(&mut h, 60.0, 70.0, 1, 10.0, Some(x1));
    let other = place(&mut h, "terminal", [70.0, 70.0], None);
    assert_ne!(id::<ComponentId>(&other, "component"), x1);
    let (_, d) = strip_row(&mut h, 80.0, 70.0, 1, 10.0, Some(x1));
    let shown = shown_tags(&mut h, page, x1);
    let _ = (c, d);
    assert_eq!(shown.len(), 4, "the -X02 terminal splits the third line's run: {shown:?}");
}

#[test]
fn the_narrowest_level_wins() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let k = place(&mut h, "coil", [100.0, 150.0], None);
    let c: ComponentId = id(&k, "component");
    let n = place(&mut h, "no", [140.0, 150.0], Some(c));
    let c2: ComponentId = id(&n, "component");
    assert_eq!(c, c2);
    let reps: Vec<RepresentationId> = h.doc().representations_of(Subject::Component(c)).map(|(r, _)| r).collect();
    assert_eq!(shown_tags(&mut h, page, c).len(), 2);
    let head = h.head();
    h.run("showtext", json!({"kind": "tag", "show": "off"})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert!(shown_tags(&mut h, page, c).is_empty(), "hidden in the project");
    h.run("showtext", json!({"kind": "tag", "show": "on", "page": page.hex()})).unwrap();
    assert_eq!(shown_tags(&mut h, page, c).len(), 2, "shown on the page");
    h.run("showtext", json!({"kind": "tag", "show": "off", "component": c.hex()})).unwrap();
    assert!(shown_tags(&mut h, page, c).is_empty(), "hidden on the device");
    h.run("showtext", json!({"kind": "tag", "show": "on", "representation": reps[0].hex()})).unwrap();
    assert_eq!(shown_tags(&mut h, page, c), vec![reps[0]], "shown on one symbol");
    // Refusals.
    for (p, words) in [
        (json!({"kind": "reference", "show": "off", "component": c.hex()}), "At device level the kinds are"),
        (json!({"kind": "description", "show": "first"}), "Shown is on, off or first"),
        (json!({"kind": "tag", "show": "off", "page": page.hex(), "component": c.hex()}), "at most one"),
        (json!({"kind": "tag", "show": "off", "representation": "00000000000000000000000000000001"}), "No representation"),
    ] {
        let e = h.run("showtext", p.clone()).unwrap_err();
        assert!(e.contains(words), "{p}: {e}");
    }
    h.check();
}

#[test]
fn hidden_texts_are_still_data_and_show_hidden_text_draws_them_dimmed() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let k = place(&mut h, "coil", [100.0, 150.0], None);
    let c: ComponentId = id(&k, "component");
    let tag = h.p(|p| devices::tag_words(p.ledger.document(), c));
    h.run("showtext", json!({"kind": "tag", "show": "off"})).unwrap();
    // Still found by `devices`.
    let found = h.run("devices", json!({"tag": tag.trim_start_matches('-')})).unwrap();
    assert!(found.to_string().contains(&tag), "{found}");
    let texts = |h: &mut H| -> Vec<(String, String)> {
        h.pv(|p, pg| pg.entities_of(p, page))
            .iter()
            .filter_map(|e| match &e.kind {
                EntityKind::Text(t) if t.value == tag => Some((e.common.layer.clone(), format!("{:?}", e.common.color))),
                _ => None,
            })
            .collect()
    };
    assert!(texts(&mut h).is_empty(), "hidden: not drawn");
    let head = h.head();
    h.run("hiddentext", json!({"on": true})).unwrap();
    assert_eq!(h.head(), head, "display only: nothing recorded");
    let t = texts(&mut h);
    assert_eq!(t.len(), 1, "{t:?}");
    assert_eq!(t[0].0, ecad_cadcraft::text_style::HIDDEN_LAYER);
    assert!(t[0].1.contains('8'), "{t:?}");
    h.check();
    // The layer does not plot.
    let d = h.pv(|p, pg| pg.drawing_of(p, page)).unwrap();
    assert_eq!(d.layer(ecad_cadcraft::text_style::HIDDEN_LAYER).map(|l| l.plot), Some(false));
    h.run("hiddentext", json!({})).unwrap();
    assert!(texts(&mut h).is_empty(), "toggled off");
    assert_eq!(h.head(), head);
}

#[test]
fn description_and_part_number_lines_are_hidden_until_asked_for() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let k = place(&mut h, "coil", [100.0, 150.0], None);
    let c: ComponentId = id(&k, "component");
    h.run("setprop", json!({"component": c.hex(), "key": "description", "value": "Pump contactor"})).unwrap();
    h.run("setprop", json!({"component": c.hex(), "key": "part_number", "value": "LC1D09"})).unwrap();
    let lines = |h: &mut H| -> Vec<(TextOf, bool)> {
        let doc = h.doc();
        devices::all_texts_on_page(&doc, page)
            .into_iter()
            .filter(|t| matches!(t.text.of, TextOf::Description | TextOf::PartNumber))
            .map(|t| (t.text.of, t.shown))
            .collect()
    };
    assert_eq!(lines(&mut h), vec![(TextOf::Description, false), (TextOf::PartNumber, false)]);
    h.run("showtext", json!({"kind": "description", "show": "on", "component": c.hex()})).unwrap();
    assert_eq!(lines(&mut h), vec![(TextOf::Description, true), (TextOf::PartNumber, false)]);
    h.check();
}

#[test]
fn a_symbols_keys_go_when_it_is_erased_and_move_when_its_device_merges() {
    let mut h = H::new(SEED);
    let k = place(&mut h, "coil", [100.0, 150.0], None);
    let c: ComponentId = id(&k, "component");
    let n = place(&mut h, "no", [140.0, 150.0], Some(c));
    let no_rep: RepresentationId = h
        .doc()
        .representations_of(Subject::Component(c))
        .find(|(_, r)| devices::rep_starter(&h.doc(), r).is_some_and(|s| s.key == "no"))
        .map(|(r, _)| r)
        .unwrap();
    let _ = n;
    h.run("showtext", json!({"kind": "tag", "show": "off", "representation": no_rep.hex()})).unwrap();
    let key = ecad_cadcraft::defaults::symbol_key("text.tag.show", no_rep);
    assert!(h.doc().components[&c].properties.contains_key(&key));
    // Erase the contact: its key goes in the same transaction.
    let page = h.first_page();
    let item = h.doc().representations[&no_rep].items[0];
    let hd = h.handle_of(page, item);
    let head = h.head();
    h.run("erase", json!({"handles": [hd.hex()]})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert!(!h.doc().components[&c].properties.keys().any(|k| k.contains(&no_rep.hex())), "the symbol's keys went with it");
    h.cmdline("u");
    assert!(h.doc().components[&c].properties.contains_key(&key), "undo brings the key back");

    // Merge another coil device's contact into this one: its keys move to the new representation.
    let k2 = place(&mut h, "no", [180.0, 150.0], None);
    let c2: ComponentId = id(&k2, "component");
    let r2: RepresentationId = h.doc().representations_of(Subject::Component(c2)).map(|(r, _)| r).next().unwrap();
    h.run("showtext", json!({"kind": "tag", "show": "on", "representation": r2.hex()})).unwrap();
    let tag = h.p(|p| devices::tag_of(p.ledger.document(), c).map(str::to_string)).unwrap();
    h.run("setprop", json!({"component": c2.hex(), "key": "tag", "value": tag, "merge": true})).unwrap();
    let doc = h.doc();
    assert!(!doc.components.contains_key(&c2));
    let moved: Vec<&String> = doc.components[&c].properties.keys().filter(|k| k.starts_with("text.") && k.ends_with(".tag.show")).collect();
    assert_eq!(moved.len(), 2, "{moved:?}");
    h.check();
}

#[test]
fn a_copied_symbol_carries_its_own_keys_under_the_new_representation() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let k = place(&mut h, "coil", [100.0, 150.0], None);
    let c: ComponentId = id(&k, "component");
    place(&mut h, "no", [140.0, 150.0], Some(c));
    let reps: Vec<RepresentationId> = h.doc().representations_of(Subject::Component(c)).map(|(r, _)| r).collect();
    let coil = reps.iter().copied().find(|r| devices::rep_starter(&h.doc(), &h.doc().representations[r]).is_some_and(|s| s.key == "coil")).unwrap();
    let other = reps.iter().copied().find(|r| *r != coil).unwrap();
    h.run("showtext", json!({"kind": "tag", "show": "off", "representation": coil.hex()})).unwrap();
    h.run("showtext", json!({"kind": "tag", "show": "on", "representation": other.hex()})).unwrap();
    h.run("movetext", json!({"representation": coil.hex(), "kind": "tag", "to": [80, 170]})).unwrap();
    let item = h.doc().representations[&coil].items[0];
    let hd = h.handle_of(page, item);
    let head = h.head();
    h.run("copy", json!({"handles": [hd.hex()], "delta": [0, 60]})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    let doc = h.doc();
    let (new_c, new_rep) = doc
        .representations
        .iter()
        .find_map(|(r, x)| match x.subject {
            Subject::Component(n) if n != c => Some((n, *r)),
            _ => None,
        })
        .expect("the copy");
    let props = &doc.components[&new_c].properties;
    let keys: Vec<&String> = props.keys().filter(|k| k.starts_with("text.")).collect();
    let new_hex = new_rep.hex();
    assert!(keys.iter().all(|k| k.contains(&new_hex)), "only the new symbol's keys: {keys:?}");
    assert_eq!(props.get(&ecad_cadcraft::defaults::symbol_key("text.tag.show", new_rep)).map(String::as_str), Some("off"));
    assert!(props.contains_key(&format!("text.{new_hex}.tag.at")), "{keys:?}");
    assert_eq!(keys.len(), 2, "{keys:?}");
    h.check();
}

/// AC 5 / T14, counted (the counter-test pattern): a commit works texts out again only for the
/// page on screen (which the window redraws after every commit, whatever it changed), never for
/// another page; `changes_since` names exactly the pages a text change can alter, so other pages
/// redraw when shown and never at the commit.
/// Commands run through the session directly, so the harness's own checks (which project every
/// page again) are not counted. (Showing a page always projects it in the one page window, so
/// switching is not what is counted.)
#[test]
fn text_views_are_worked_out_again_only_where_a_change_shows() {
    let mut h = H::new(SEED);
    let p1 = h.first_page();
    let k = place(&mut h, "coil", [100.0, 200.0], None);
    let k1: ComponentId = id(&k, "component");
    let p2: PageId = id(&h.run("newpage", json!({"title": "P2"})).unwrap(), "page");
    place(&mut h, "no", [100.0, 200.0], Some(k1));
    let p3: PageId = id(&h.run("newpage", json!({"title": "P3"})).unwrap(), "page");
    let k2: ComponentId = id(&place(&mut h, "coil", [100.0, 200.0], None), "component");
    h.project.open_page(&mut h.s, p1).unwrap();
    // Run `cmd`; the passes it caused per page, and the pages it names.
    let counted = |h: &mut H, cmd: &str, params: Value| -> ([u64; 3], Option<std::collections::BTreeSet<PageId>>) {
        let before = [p1, p2, p3].map(devices::text_passes);
        let n = h.p(|p| p.change_count());
        h.s.execute(cmd, &params).unwrap();
        let after = [p1, p2, p3].map(devices::text_passes);
        ([after[0] - before[0], after[1] - before[1], after[2] - before[2]], h.p(|p| p.changes_since(n)))
    };
    // -K01's tag (on pages 1 and 2), page 1 on screen: page 1 now, page 2 named, page 3 neither.
    let (d, named) = counted(&mut h, "showtext", json!({"kind": "tag", "show": "off", "component": k1.hex()}));
    assert!(d[0] >= 1 && d[1] == 0 && d[2] == 0, "{d:?}");
    let named = named.expect("pages known");
    assert!(named.contains(&p1) && named.contains(&p2) && !named.contains(&p3), "{named:?}");
    // A key of page 3, page 1 on screen: page 3 not worked out now; only page 3 named.
    let (d, named) = counted(&mut h, "showtext", json!({"kind": "tag", "show": "off", "page": p3.hex()}));
    assert_eq!([d[1], d[2]], [0, 0], "{d:?}");
    assert_eq!(named.expect("pages known").into_iter().collect::<Vec<_>>(), vec![p3]);
    // A project key: the page on screen now, every page named.
    let (d, named) = counted(&mut h, "textstyle", json!({"kind": "tag", "height": 3}));
    assert!(d[0] >= 1 && d[1] == 0 && d[2] == 0, "{d:?}");
    assert!(named.is_none_or(|s| [p1, p2, p3].iter().all(|p| s.contains(p))));
    // An unrelated edit (-K02, on page 3), page 1 on screen: no other page's texts worked out;
    // page 1 not named.
    let (d, named) = counted(&mut h, "setprop", json!({"component": k2.hex(), "key": "maker", "value": "Acme"}));
    assert_eq!([d[1], d[2]], [0, 0], "unrelated edit: {d:?}");
    assert!(!named.expect("pages known").contains(&p1));
    h.check();
}
