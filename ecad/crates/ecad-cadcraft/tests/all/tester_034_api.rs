//! Tester 034: the catalogue API that brief 034 adds (design 034 §2.2, §2.3, §10, §11), written
//! from the design, then adapted in phase 2 where the build's API differs from the design's
//! text (reported): `Starter::quiet` is a `Quiet` enum read through `is_quiet`, `changeover_labels`
//! takes a set, and the prefix is `Starter::prefix()`, read from the defaults registry.

use ecad_cadcraft::symbols::{self, Anchor, Family, HAlign, Kind, STARTERS, TextRole, VAlign};

use crate::tester_034_library::catalogue;

fn s(key: &str) -> &'static symbols::Starter {
    symbols::by_key(key).unwrap_or_else(|| panic!("{key} missing"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn one<'a>(v: &'a [Anchor], f: impl Fn(&TextRole) -> bool, what: &str, key: &str) -> &'a Anchor {
    let m: Vec<&Anchor> = v.iter().filter(|a| f(&a.role)).collect();
    assert_eq!(m.len(), 1, "{key}: one {what} anchor");
    m[0]
}

/// §2.2: every entry has a family; the 21 starters are `Basic`; the new keys by stage.
#[test]
fn families() {
    let old = [
        "coil",
        "no",
        "nc",
        "power3",
        "mcb1",
        "mcb2",
        "fuse",
        "psu",
        "pbno",
        "pbnc",
        "sel",
        "lamp",
        "motor3",
        "motor1",
        "trafo",
        "terminal",
        "di",
        "do",
        "junction",
        "continuation",
        "terminalpe",
    ];
    for k in old {
        assert!(matches!(s(k).family, Family::Basic), "{k}");
    }
    for k in ["co", "swco", "power4", "ssrin", "ssrout"] {
        assert!(matches!(s(k).family, Family::Contacts), "{k}");
    }
    for k in ["rcbo2", "rcd2", "rcd4", "fuseterm", "fusebank4", "fusebank6", "fusebank8", "fusebank10", "fusebank12"] {
        assert!(matches!(s(k).family, Family::Protection), "{k}");
    }
    for k in ["prox3", "floatno", "floatnc", "rollerno", "rollernc", "interlocknc", "interlockno", "tc"] {
        assert!(matches!(s(k).family, Family::Sensors), "{k}");
    }
    for k in ["valve", "valve2", "heater"] {
        assert!(matches!(s(k).family, Family::Actuators | Family::Sensors), "{k}");
    }
    for e in catalogue().iter().filter(|e| e.block == 0x300) {
        assert!(matches!(s(&e.key).family, Family::Connectors), "{}", e.key);
    }
    for e in catalogue().iter().filter(|e| e.block == 0x400) {
        assert!(matches!(s(&e.key).family, Family::Plc), "{}", e.key);
    }
}

/// §2.3: kinds.
#[test]
fn kinds() {
    for k in ["co", "swco"] {
        assert_eq!(s(k).kind, Kind::ChangeOver, "{k}");
    }
    for k in ["power4", "ssrout"] {
        assert_eq!(s(k).kind, Kind::Fixed { exclusive: false }, "{k}");
    }
    for k in ["floatno", "rollerno", "interlockno"] {
        assert_eq!(s(k).kind, Kind::Contact { make: true }, "{k}");
    }
    for k in ["floatnc", "rollernc", "interlocknc"] {
        assert_eq!(s(k).kind, Kind::Contact { make: false }, "{k}");
    }
    for e in catalogue() {
        if !["co", "swco", "power4", "ssrout", "floatno", "rollerno", "interlockno", "floatnc", "rollernc", "interlocknc"].contains(&e.key.as_str()) {
            assert_eq!(s(&e.key).kind, Kind::Fixed { exclusive: true }, "{}", e.key);
        }
    }
}

/// §2.2, §11: quiet points per the design's Quiet lists; none for every other entry.
#[test]
fn quiet_lists() {
    let cat = catalogue();
    for st in STARTERS {
        let Ok(d) = st.definition() else { panic!("{}", st.key) };
        let mut got: Vec<String> = d.terminals.iter().filter(|t| symbols::is_quiet(st, &t.key)).map(|t| t.key.clone()).collect();
        got.sort();
        let mut want: Vec<String> = cat.iter().find(|e| e.key == st.key).map(|e| e.quiet.clone()).unwrap_or_default();
        want.sort();
        assert_eq!(got, want, "{}", st.key);
    }
}

/// AC 11 (rulings: prefixes per symbol key, the project level is `settings.tag_prefixes`; ratings
/// have no registry default): every new symbol has a registered prefix entry with the design's
/// built-in value; a project value changes what resolves; a hostile stored value falls back.
#[test]
fn prefixes_are_registered_defaults() {
    use ecad_cadcraft::defaults;
    for e in catalogue() {
        let entry = defaults::symbols::prefix_entry(&e.key).unwrap_or_else(|| panic!("{}: no registry entry", e.key));
        assert_eq!(entry.key, format!("symbol.{}.prefix", e.key));
        assert!(defaults::find(entry.key).is_some_and(|x| std::ptr::eq(x, entry)), "{}: listed in the registry", e.key);
        assert_eq!(defaults::symbols::builtin_prefix(entry), e.prefix, "{}", e.key);
        assert_eq!(s(&e.key).prefix(), e.prefix, "{}: Starter::prefix reads the registry", e.key);
    }
    let mut doc = ecad_model::Document::default();
    assert_eq!(defaults::symbols::prefix(&doc, "valve"), "MB");
    doc.settings.tag_prefixes.insert("valve".into(), "Y".into());
    assert_eq!(defaults::symbols::prefix(&doc, "valve"), "Y");
    assert_eq!(defaults::symbols::prefix(&doc, "valve2"), "MB", "only that key");
    for bad in ["", "1", "K\u{0}", "TOOLONGPREFIXX", "\u{1F642}"] {
        doc.settings.tag_prefixes.insert("valve".into(), bad.into());
        assert_eq!(defaults::symbols::prefix(&doc, "valve"), "MB", "{bad:?} falls back");
    }
    assert!(defaults::all().all(|x| !x.key.contains("rating")), "ratings have no registry default (ruling)");
}

/// §2.3: the change-over rule itself: s = 1 + the highest sequence digit; refused after 9.
#[test]
fn changeover_rule() {
    let set = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<std::collections::BTreeSet<String>>();
    let used = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<String>>();
    assert_eq!(symbols::changeover_labels(&set(&["A1", "A2"]), "-K01"), Ok(used(&["11", "12", "14"])));
    assert_eq!(symbols::changeover_labels(&set(&["A1", "A2", "13", "14"]), "-K01"), Ok(used(&["21", "22", "24"])));
    assert_eq!(symbols::changeover_labels(&set(&["81", "82", "84"]), "-K01"), Ok(used(&["91", "92", "94"])));
    assert_eq!(symbols::changeover_labels(&set(&["93", "94"]), "-K01"), Err("-K01 already has 9 contacts.".to_string()));
    // Only `[1-9][1-4]` counts: odd labels never overflow or count.
    assert_eq!(symbols::changeover_labels(&set(&["99", "15", "0", "1234", "I0.0", "\u{1F642}"]), "-K01"), Ok(used(&["11", "12", "14"])));
}

/// §10: the default rule for the 21 starters (none for junction and continuation) and every
/// override of the new families.
#[test]
fn anchors() {
    for st in STARTERS {
        let v = symbols::anchors(st);
        if matches!(st.key, "junction" | "continuation") {
            assert!(v.is_empty(), "{}", st.key);
            continue;
        }
        let Ok(d) = st.definition() else { panic!("{}", st.key) };
        let (lo, hi) = symbols::bounds(d);
        let mid = (lo.y + hi.y) / 2.0;
        let tag = one(&v, |r| matches!(r, TextRole::Tag), "Tag", st.key);
        let desc = one(&v, |r| matches!(r, TextRole::Description), "Description", st.key);
        let fam = st.family;
        let key = st.key;
        let breaker = matches!(key, "rcbo2" | "rcd2" | "rcd4");
        let card = matches!(fam, Family::Plc);
        let connector = matches!(fam, Family::Connectors);
        if card {
            assert!(close(tag.at.x, lo.x) && close(tag.at.y, hi.y + 2.0), "{key} tag {:?}", tag.at);
            assert!(matches!((tag.h, tag.v), (HAlign::Left, VAlign::Bottom)), "{key}");
            assert!(close(desc.at.x, -13.0) && close(desc.at.y, 12.0), "{key} description {:?}", desc.at);
            assert!(matches!((desc.h, desc.v), (HAlign::Left, VAlign::Top)), "{key}");
        } else {
            assert!(close(tag.at.x, lo.x - 2.0) && close(tag.at.y, mid), "{key} tag {:?}", tag.at);
            assert!(matches!((tag.h, tag.v), (HAlign::Right, VAlign::Middle)), "{key}");
            let dy = if breaker { mid - 8.0 } else { mid - 4.0 };
            assert!(close(desc.at.x, lo.x - 2.0) && close(desc.at.y, dy), "{key} description {:?}", desc.at);
            assert!(matches!((desc.h, desc.v), (HAlign::Right, VAlign::Top)), "{key}");
        }
        let ratings: Vec<&Anchor> = v.iter().filter(|a| matches!(a.role, TextRole::Rating)).collect();
        if connector {
            assert!(ratings.is_empty(), "{key}: connectors have no Rating anchor");
        } else {
            assert_eq!(ratings.len(), 1, "{key}: one Rating anchor");
            let r = ratings[0];
            if breaker {
                assert!(close(r.at.x, lo.x - 2.0) && close(r.at.y, mid - 4.0), "{key} rating {:?}", r.at);
                assert!(matches!((r.h, r.v), (HAlign::Right, VAlign::Top)), "{key}");
            } else {
                assert!(close(r.at.x, hi.x + 2.0) && close(r.at.y, mid), "{key} rating {:?}", r.at);
                assert!(matches!((r.h, r.v), (HAlign::Left, VAlign::Middle)), "{key}");
            }
        }
        // Fuse banks: one RatingOf per fuse at (x_i + 3, −8), left, middle, rotated 90°.
        let per: Vec<&Anchor> = v.iter().filter(|a| matches!(a.role, TextRole::RatingOf(_))).collect();
        if let Some(n) = key.strip_prefix("fusebank").and_then(|n| n.parse::<usize>().ok()) {
            assert_eq!(per.len(), n, "{key}");
            for i in 1..=n {
                let a =
                    per.iter().find(|a| matches!(&a.role, TextRole::RatingOf(l) if *l == i.to_string())).unwrap_or_else(|| panic!("{key} fuse {i}"));
                assert!(close(a.at.x, 10.0 * (i as f64 - 1.0) + 3.0) && close(a.at.y, -8.0), "{key} fuse {i} {:?}", a.at);
                assert!(matches!((a.h, a.v), (HAlign::Left, VAlign::Middle)) && close(a.rotation, 90.0), "{key} fuse {i}");
            }
        } else {
            assert!(per.is_empty(), "{key}: RatingOf only on fuse banks");
        }
    }
}
