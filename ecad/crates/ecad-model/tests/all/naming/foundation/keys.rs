//! A3: `keys_are_registered` (design 072 §14): `KEYS`, `FAMILIES` and `RESERVED` are disjoint and
//! every key classifies to an area other than `Free`.

use std::collections::BTreeSet;

use ecad_model::naming::keys::{Area, FAMILIES, KEYS, PropKey, PropsExt, RESERVED, area_of, family_of, is_reserved, spec};
use ecad_model::{Properties, Word};

#[test]
fn keys_are_registered() {
    let mut seen = BTreeSet::new();
    for s in KEYS {
        assert!(seen.insert(s.key.0), "{} is listed twice", s.key);
        assert!(!s.holders.is_empty(), "{} has no holder", s.key);
        assert_ne!(s.area, Area::Free, "{} is classified Free", s.key);
        assert_eq!(area_of(s.key.0), s.area, "{} classifies to its own area", s.key);
        assert!(family_of(s.key.0).is_none(), "{} is also a family's key", s.key);
        assert!(!is_reserved(s.key.0), "{} is both a key and reserved", s.key);
    }
    for k in RESERVED {
        assert!(seen.insert(k.0), "{k} is reserved twice or also a key");
        assert!(family_of(k.0).is_none(), "{k} is reserved and a family's key");
    }
    let mut patterns = BTreeSet::new();
    for f in FAMILIES {
        assert!(patterns.insert(f.pattern), "{} is listed twice", f.pattern);
        assert_ne!(f.area, Area::Free, "{} is classified Free", f.pattern);
        assert!(f.arity() > 0, "{} builds no key", f.pattern);
        assert!(!f.holders.is_empty(), "{} has no holder", f.pattern);
    }
}

#[test]
fn families_build_and_parse_their_keys() {
    let parts = ["ab12", "tag", "-K1"];
    for f in FAMILIES {
        let mine = &parts[..f.arity()];
        let key = f.build(mine).unwrap_or_else(|| panic!("{} builds from {mine:?}", f.pattern));
        assert_eq!(f.parse(&key).as_deref(), Some(mine), "{} parses {key} back", f.pattern);
        assert_eq!(family_of(&key).map(|g| g.pattern), Some(f.pattern), "{key} is found in its own family");
        assert_eq!(area_of(&key), f.area);
        assert!(f.build(&parts[..f.arity() - 1]).is_none(), "{}: too few parts", f.pattern);
    }
}

#[test]
fn family_parts_are_whole_segments() {
    let style = FAMILIES
        .iter()
        .find(|f| f.pattern == "text.{}.{}.style")
        .map(|f| (f.parse("text.ab.tag.style"), f.parse("text.tag.style"), f.parse("text.a.b.c.style"), f.build(&["a.b", "tag"])));
    assert_eq!(style, Some((Some(vec!["ab", "tag"]), None, None, None)));
    let high = FAMILIES.iter().find(|f| f.pattern == "numbering.high.{}").map(|f| (f.parse("numbering.high.-K.A"), f.parse("numbering.high.")));
    assert_eq!(high, Some((Some(vec!["-K.A"]), None)));
}

#[test]
fn areas_classify_by_first_word() {
    let table: &[(&str, Area)] = &[
        ("tag", Area::Identity),
        ("number", Area::Free),
        ("text.tag.height", Area::Text),
        ("strippage.columns", Area::StripPage),
        ("print.frame.style", Area::Print),
        ("link.3", Area::Link),
        ("numbering.high.-K", Area::Numbering),
        ("text", Area::Free),
        ("my_own_field", Area::Free),
        ("printer.name", Area::Free),
    ];
    for (key, want) in table {
        assert_eq!(area_of(key), *want, "area_of({key:?})");
    }
    assert_eq!(Area::REGISTRY.len(), 17);
    assert!(Area::REGISTRY.iter().all(|a| a.is_registry()));
    assert!(!Area::Free.is_registry());
    assert_eq!(Area::StripPage.word(), "strippage");
}

#[test]
fn typed_readers_read_shapes() {
    let mut p = Properties::new();
    p.insert("tag_fixed".into(), "Yes".into());
    p.insert("count".into(), "07".into());
    p.insert("bad".into(), "y".into());
    assert_eq!(p.flag(PropKey("tag_fixed")), Some(true));
    assert_eq!(p.flag(PropKey("bad")), None);
    assert_eq!(p.flag(PropKey("missing")), None);
    assert_eq!(p.uint(PropKey("count"), 9), Some(7));
    assert_eq!(p.uint(PropKey("count"), 6), None);
    assert_eq!(p.text(PropKey("bad")), Some("y"));
    assert!(spec("tag").is_some() && spec("nope").is_none());
}
