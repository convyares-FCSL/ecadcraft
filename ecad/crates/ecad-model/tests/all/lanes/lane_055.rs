//! Brief 055 (design 055 §1.3, §2, FP1, FP4): title block bounds, a page's sheet and margins
//! through its frame, and the frame record as one merge unit.

use ecad_model::sheet::{self, Margins, SheetSpec};
use ecad_model::style::{StyleLayer, StyleRecord, StyleRef};
use ecad_model::title_block::{Binding, BlockItem, MAX_ITEMS, Rect, Source, StyleBody, TextLook, TitleBlockLayout};
use std::collections::BTreeMap;

use ecad_model::merge::{self, Inputs, Scope};
use ecad_model::tag::HighWater;
use ecad_model::{Author, Document, Ledger, Op, PageId, Primitive, Pt};

fn field(x: f64) -> BlockItem {
    BlockItem::Field {
        at: Rect::new(x, 1.0, 10.0, 5.0),
        bind: Binding::new(Source::Page, "number"),
        label: None,
        look: TextLook::default(),
        if_empty: Default::default(),
    }
}

/// Each bound refuses in words and is skipped when drawing (one regression per bound).
#[test]
fn every_bound_refuses_in_words_and_is_skipped_when_drawing() {
    let ok = TitleBlockLayout { size: (180.0, 40.0), items: vec![field(1.0)] };
    assert!(ok.check().is_ok());
    let cases: Vec<(TitleBlockLayout, &str)> = vec![
        (TitleBlockLayout { size: (180.0, 40.0), items: (0..MAX_ITEMS + 1).map(|i| field((i % 100) as f64)).collect() }, "at most 512 items"),
        (TitleBlockLayout { size: (f64::NAN, 40.0), items: vec![] }, "1 to 400 mm high"),
        (TitleBlockLayout { size: (180.0, 401.0), items: vec![] }, "1 to 400 mm high"),
        (
            TitleBlockLayout { size: (180.0, 40.0), items: vec![BlockItem::Line { a: (f64::NAN, 0.0), b: (1.0, 1.0), weight: Default::default() }] },
            "ends must be",
        ),
        (
            TitleBlockLayout { size: (180.0, 40.0), items: vec![BlockItem::Line { a: (0.0, 0.0), b: (2401.0, 1.0), weight: Default::default() }] },
            "ends must be",
        ),
        (
            TitleBlockLayout {
                size: (180.0, 40.0),
                items: vec![BlockItem::Text { at: Rect::new(0.0, 0.0, 0.1, 5.0), text: "x".into(), look: TextLook::default() }],
            },
            "sides of at least",
        ),
        (
            TitleBlockLayout {
                size: (180.0, 40.0),
                items: vec![BlockItem::Text { at: Rect::new(0.0, 0.0, 9.0, 5.0), text: "x".repeat(257), look: TextLook::default() }],
            },
            "longer than 256",
        ),
        (
            TitleBlockLayout {
                size: (180.0, 40.0),
                items: vec![BlockItem::Text {
                    at: Rect::new(0.0, 0.0, 9.0, 5.0),
                    text: "x".into(),
                    look: TextLook { size: f64::INFINITY, ..TextLook::default() },
                }],
            },
            "text size must be",
        ),
        (
            TitleBlockLayout {
                size: (180.0, 40.0),
                items: vec![BlockItem::RevTable { at: Rect::new(0.0, 0.0, 9.0, 5.0), rows: 0, header: true, newest: Default::default() }],
            },
            "1 to 20 rows",
        ),
        (
            TitleBlockLayout {
                size: (180.0, 40.0),
                items: vec![BlockItem::Logo {
                    at: Rect::new(0.0, 0.0, 9.0, 5.0),
                    name: String::new(),
                    prims: vec![Primitive::Point { at: Pt::new(0.0, 0.0) }; 2001],
                }],
            },
            "2 000 at most",
        ),
        (TitleBlockLayout { size: (180.0, 40.0), items: vec![BlockItem::Unknown] }, "does not know"),
    ];
    for (l, want) in cases {
        let e = l.check().unwrap_err();
        assert!(e.contains(want), "{want}: {e}");
        let (usable, skipped) = l.usable();
        assert!(skipped.items >= 1 || !l.size_ok(), "{want}: something skipped");
        assert!(usable.len() <= MAX_ITEMS);
    }
    // The body's shape budget: three full logos, the third skipped.
    let logo =
        || BlockItem::Logo { at: Rect::new(0.0, 0.0, 9.0, 5.0), name: String::new(), prims: vec![Primitive::Point { at: Pt::new(0.0, 0.0) }; 2000] };
    let l = TitleBlockLayout { size: (180.0, 40.0), items: vec![logo(), logo(), logo()] };
    assert!(l.check().unwrap_err().contains("4 000 at most"));
    assert_eq!(l.usable().0, vec![0, 1]);
}

fn doc_with_page() -> (Document, PageId) {
    let mut l = Ledger::new(55);
    let page = PageId(0x55);
    l.commit(Author::User, "page", vec![Op::AddPage { id: page, page: Default::default(), index: 0 }]).unwrap();
    (l.document().clone(), page)
}

fn frame(sheet: &str, margins: &str, h: f64) -> StyleRecord {
    let mut r = StyleRecord::new("A2 frame", [("sheet".to_string(), sheet.to_string()), ("margins".to_string(), margins.to_string())].into());
    r.body = Some(StyleBody::title_block(TitleBlockLayout::blank(180.0, h)));
    r
}

/// A page's sheet, margins and keep-out come through its frame (FP4): frame, page, project, A3.
#[test]
fn the_frame_decides_sheet_margins_and_keep_out() {
    let (mut doc, page) = doc_with_page();
    assert_eq!(sheet::sheet_of(&doc, page), SheetSpec::default());
    assert_eq!(sheet::keep_out(&doc, page), sheet::TITLE_BLOCK_MM);
    doc.styles.entry("frame".into()).or_default().insert(StyleRef::Own(7), frame("A2", "15,10,12,8", 55.0));
    // Not chosen yet: nothing changes.
    assert_eq!(sheet::sheet_of(&doc, page), SheetSpec::default());
    doc.properties.insert(sheet::FRAME_STYLE_KEY.into(), StyleRef::Own(7).text());
    assert_eq!(sheet::sheet_of(&doc, page), SheetSpec::preset("A2").unwrap());
    assert_eq!(sheet::margins_of(&doc, page), Margins { left: 15.0, right: 10.0, bottom: 12.0, top: 8.0 });
    assert_eq!(sheet::keep_out(&doc, page), 55.0);
    let g = sheet::Grid::of(&doc, page);
    assert_eq!((g.frame_left(), g.frame_bottom()), (15.0, 12.0));
    // A choice naming no frame falls through to the next level.
    if let Some(p) = doc.pages.get_mut(&page) {
        p.properties.insert(sheet::FRAME_STYLE_KEY.into(), StyleRef::Own(99).text());
        p.properties.insert(sheet::SHEET_KEY.into(), "A1".into());
    }
    assert_eq!(sheet::sheet_of(&doc, page).width, 594, "the project's frame wins over the page's sheet");
    assert_eq!(sheet::own_sheet(&doc, page).width, 841);
    // Unreadable fields count as absent.
    doc.styles.entry("frame".into()).or_default().insert(StyleRef::Own(7), frame("A5", "1,2", 1e9));
    assert_eq!(sheet::sheet_of(&doc, page).width, 841);
    assert_eq!(sheet::margins_of(&doc, page), Margins::default());
    assert_eq!(sheet::keep_out(&doc, page), sheet::TITLE_BLOCK_MM);
    for bad in ["", "1,2,3", "1,2,3,51", "a,b,c,d", "NaN,1,1,1", "-1,1,1,1"] {
        assert_eq!(sheet::parse_margins(bad), None, "{bad}");
    }
    assert_eq!(sheet::margins_text(&Margins::default()), "10,10,10,7");
}

fn run_merge(a: &Document, t: &Document, s: &Document, scope: &Scope) -> merge::Plan {
    let high = HighWater::default();
    let inp = Inputs {
        ancestor: a,
        target: t,
        source: s,
        scope,
        target_name: "main",
        source_name: "branch",
        keep_project_key: &|_| true,
        high: &high,
        edit_rule: &|_, _, v| Ok(v.to_string()),
        join: &|_, _, _| Err("no".into()),
    };
    merge::plan(&inp, &BTreeMap::new())
}

/// Merge treats a frame record, body included, as one unit (row K): two branches editing one
/// frame's block are one conflict named with the frame; two branches editing different frames
/// merge cleanly.
#[test]
fn a_frame_record_is_one_merge_unit() {
    let rec = |x: f64| {
        let mut r = StyleRecord::new("A3 Northfield", Default::default());
        r.body = Some(StyleBody::title_block(TitleBlockLayout { size: (180.0, 40.0), items: vec![field(x)] }));
        r
    };
    let set = |id: u128, before: Option<StyleRecord>, after: StyleRecord| Op::SetStyle {
        layer: StyleLayer::Project,
        catalogue: "frame".into(),
        id: StyleRef::Own(id),
        before,
        after: Some(after),
    };
    let mut base = Ledger::new(55);
    base.commit(Author::User, "base", vec![set(1, None, rec(1.0)), set(2, None, rec(1.0))]).unwrap();
    let a = base.document().clone();
    for (t_id, s_id, conflicts) in [(1, 1, 1), (1, 2, 0)] {
        let (mut t, mut s) = (base.clone(), base.clone());
        let t_ops = vec![set(t_id, Some(rec(1.0)), rec(5.0))];
        let s_ops = vec![set(s_id, Some(rec(1.0)), rec(9.0))];
        t.commit(Author::User, "t", t_ops.clone()).unwrap();
        s.commit(Author::User, "s", s_ops.clone()).unwrap();
        let mut scope = Scope::default();
        scope.add_ops(&t_ops);
        scope.add_ops(&s_ops);
        let plan = run_merge(&a, t.document(), s.document(), &scope);
        let found: Vec<_> = plan.conflicts.iter().filter(|c| c.id.contains("style.")).collect();
        assert_eq!(found.len(), conflicts, "{:?}", plan.conflicts);
        if let Some(c) = found.first() {
            assert!(c.what.contains("A3 Northfield"), "{}", c.what);
        } else {
            assert!(plan.problem.is_none(), "{:?}", plan.problem);
        }
    }
}
