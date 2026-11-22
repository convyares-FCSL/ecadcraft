//! The starter catalogue and the symbol rules (formerly the inline tests of `symbols`).

use std::collections::{BTreeMap, BTreeSet};

use ecad_electrical::symbols::{anchors::*, catalogue::*, project::*, rules::*, *};
use ecad_model::{Pt, Word};

/// The first definition id of the starter catalogue.
const DEF_BASE: u128 = 0x0000_ecad_0018_5ea1_0000_0000_0001_0000;

#[test]
fn starters_are_keyed_in_order() {
    // One row per StarterKey, in variant order, and each key reads back from its word.
    assert_eq!(STARTERS.len(), StarterKey::ALL.len());
    for (i, (s, k)) in STARTERS.iter().zip(StarterKey::ALL).enumerate() {
        assert_eq!(s.key, *k, "row {i}");
        assert_eq!(starter(*k).key, *k);
        assert_eq!(StarterKey::parse(k.word()), Some(*k));
        assert_eq!(by_key(&k.word().to_uppercase()).map(|x| x.key), Some(*k));
    }
    assert!(by_key("nosuch").is_none());
}

#[test]
fn every_starter_is_valid_and_on_the_grid() {
    let mut keys = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    for s in STARTERS {
        assert!(keys.insert(s.key) && ids.insert(s.id), "{} repeated", s.key);
        let d = s.definition().unwrap();
        assert!(!d.graphics.is_empty() && !d.connection_points.is_empty(), "{}", s.key);
        for t in &d.connection_points {
            let on = |v: f64| (v / SYMBOL_GRID_MM - (v / SYMBOL_GRID_MM).round()).abs() < 1e-9;
            assert!(on(t.at.x) && on(t.at.y), "{} terminal {} off the grid", s.key, t.key);
        }
        assert_eq!(of_definition(d).map(|x| x.key), Some(s.key));
        // The origin's hash is the content's.
        let o = d.origin.clone().unwrap();
        let mut bare = d.clone();
        bare.origin = None;
        assert_eq!(ecad_file::definition_hash(&bare).unwrap(), o.content_hash);
    }
    // 18 symbols (brief 018), the continuation (brief 022), the PE terminal (028),
    // brief 034's stage 1 (14), stage 2 (11), stage 3a (27) and stage 3b (8), brief 036's
    // multi-level terminals (20), brief 044's connector pins (3), brief 045's modules (3) and
    // channel cells (6).
    assert_eq!(STARTERS.len(), 20 + 14 + 11 + 27 + 8 + 20 + 3 + 3 + 6);
    // Brief 034's definition IDs sit in their family's block.
    for s in STARTERS.iter().filter(|s| s.family != Family::Basic) {
        let n = s.id.0 - DEF_BASE;
        let block = match s.family {
            Family::Contacts | Family::Protection => 0x100,
            Family::Sensors | Family::Actuators => 0x200,
            Family::Connectors => 0x300,
            Family::Plc => 0x400,
            Family::Basic => 0,
        };
        assert!(n > block && n < block + 0x100, "{} id {n:#x} outside its block", s.key);
    }
}

/// Design 034 §3–§6: each new symbol's box (width, height) and labels in definition order.
const NEW: &[(&str, f64, f64, &str)] = &[
    ("co", 5.0, 20.0, "11/12/14"),
    ("swco", 13.0, 20.0, "11/12/14"),
    ("power4", 34.0, 20.0, "1/2/3/4/5/6/7/8"),
    ("rcbo2", 23.0, 20.0, "1/2/N1/N2"),
    ("rcd2", 23.0, 20.0, "1/2/N1/N2"),
    ("rcd4", 43.0, 20.0, "1/2/3/4/5/6/N1/N2"),
    ("ssrin", 12.0, 20.0, "A1/A2"),
    ("ssrout", 9.0, 20.0, "1/2"),
    ("fuseterm", 7.0, 20.0, "1/2"),
    ("fusebank4", 50.0, 35.0, "L+/M/1/2/3/4"),
    ("fusebank6", 70.0, 35.0, "L+/M/1/2/3/4/5/6"),
    ("fusebank8", 90.0, 35.0, "L+/M/1/2/3/4/5/6/7/8"),
    ("fusebank10", 110.0, 35.0, "L+/M/1/2/3/4/5/6/7/8/9/10"),
    ("fusebank12", 130.0, 35.0, "L+/M/1/2/3/4/5/6/7/8/9/10/11/12"),
    ("prox3", 20.0, 20.0, "1/3/4"),
    ("floatno", 9.5, 20.0, "13/14"),
    ("floatnc", 11.5, 20.0, "11/12"),
    ("rollerno", 9.2, 20.0, "13/14"),
    ("rollernc", 11.2, 20.0, "11/12"),
    ("interlocknc", 17.0, 20.0, "11/12"),
    ("interlockno", 13.0, 20.0, "13/14"),
    ("valve", 20.0, 20.0, "A1/A2"),
    ("valve2", 32.0, 20.0, "A1/A2/B1/B2"),
    ("tc", 10.5, 15.5, "+/-"),
    ("heater", 5.0, 20.0, "1/2"),
    ("socket2", 12.0, 7.5, "1/2"),
    ("socket3", 17.0, 7.5, "1/2/3"),
    ("socket4", 22.0, 7.5, "1/2/3/4"),
    ("socket6", 32.0, 7.5, "1/2/3/4/5/6"),
    ("socket8", 42.0, 7.5, "1/2/3/4/5/6/7/8"),
    ("socket12", 62.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12"),
    ("plug2", 12.0, 7.5, "1/2"),
    ("plug3", 17.0, 7.5, "1/2/3"),
    ("plug4", 22.0, 7.5, "1/2/3/4"),
    ("plug6", 32.0, 7.5, "1/2/3/4/5/6"),
    ("plug8", 42.0, 7.5, "1/2/3/4/5/6/7/8"),
    ("plug12", 62.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12"),
    ("dsub9s", 47.0, 7.5, "1/2/3/4/5/6/7/8/9"),
    ("dsub15s", 77.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12/13/14/15"),
    ("dsub25s", 127.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12/13/14/15/16/17/18/19/20/21/22/23/24/25"),
    ("dsub9p", 47.0, 7.5, "1/2/3/4/5/6/7/8/9"),
    ("dsub15p", 77.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12/13/14/15"),
    ("dsub25p", 127.0, 7.5, "1/2/3/4/5/6/7/8/9/10/11/12/13/14/15/16/17/18/19/20/21/22/23/24/25"),
    ("m12s4", 27.0, 7.5, "1/2/3/4"),
    ("m12s5", 32.0, 7.5, "1/2/3/4/5"),
    ("m12s8", 47.0, 7.5, "1/2/3/4/5/6/7/8"),
    ("m12p4", 27.0, 7.5, "1/2/3/4"),
    ("m12p5", 32.0, 7.5, "1/2/3/4/5"),
    ("m12p8", 47.0, 7.5, "1/2/3/4/5/6/7/8"),
    ("rj45", 50.0, 10.0, "1/2/3/4/5/6/7/8/SH"),
    ("usba", 30.0, 10.0, "1/2/3/4/SH"),
    ("usbb", 30.0, 10.0, "1/2/3/4/SH"),
    // Brief 051 (design 051 §2.2, §2.4, FP3): cells at the pack's pitch, bodies as power ends.
    ("plccpu", 20.0, 10.0, "L+/M"),
    ("plcps", 25.0, 50.0, "L/N/PE/L+/M"),
    ("plcdi", 20.0, 10.0, "L+/M"),
    ("plcdq", 20.0, 10.0, "L+/M"),
    ("plcai", 20.0, 10.0, "L+/M"),
    ("plcao", 20.0, 10.0, "L+/M"),
    ("plcrtd", 20.0, 10.0, "L+/M"),
    ("plciol", 20.0, 10.0, "L+/M"),
    // Brief 045 (design 045 §3.1).
    ("plcim", 20.0, 10.0, "L+/M"),
    ("plcpm", 20.0, 10.0, "L+/M"),
    ("plccm", 20.0, 10.0, "P1/P2"),
    ("chdi", 20.0, 19.0, ".in"),
    ("chdq", 20.0, 19.0, ".out"),
    ("chai", 20.0, 20.5, ".+/.-"),
    ("chao", 20.0, 20.5, ".+/.-"),
    ("chrtd", 40.0, 19.0, ".I+/.S+/.S-/.I-"),
    ("chiol", 30.0, 19.0, ".L+/.C/Q/.L-"),
    // Brief 044 (design 044 §3.2): the definition keys are the side keys.
    ("pinpair", 3.0, 10.0, ".s/.p"),
    ("pins", 3.0, 5.0, ".s"),
    ("pinp", 1.0, 6.0, ".p"),
];

#[test]
fn new_symbols_have_the_designed_box_and_labels() {
    let mut wrong = Vec::new();
    for (key, w, h, _) in NEW {
        let (sw, sh) = by_key(key).unwrap().size();
        if (sw - w).abs() > 1e-6 || (sh - h).abs() > 1e-6 {
            wrong.push(format!("{key}: box {sw}×{sh}, want {w}×{h}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    for (key, _, _, labels) in NEW {
        let s = by_key(key).unwrap();
        assert_eq!(s.labels().join("/"), *labels, "{key}");
    }
    assert_eq!(STARTERS.iter().filter(|s| s.family != Family::Basic).count(), NEW.len());
}

/// Overseer ruling 4: the 21 starters of briefs 018, 022 and 028 keep their content hashes
/// (a changed hash would make every old project's embedded copy a different symbol).
#[test]
fn the_basic_starters_keep_their_content_hashes() {
    let want: &[(&str, &str)] = &[
        ("coil", "05b6bddc45534d29fc93c421e06a13084cae82ef44285f4482d39cda4c99ea78"),
        ("no", "e6e1e24b6c7bd33d35e3d024514ce5dd6f62a105422ce42c0750183040271dab"),
        ("nc", "de13074bc1f72b98a93f83f896e4b07a25bb7f4ef541d0b3cf91c899d0501631"),
        ("power3", "82faf1478e1d9b589cc8462365a97147b4d14228e66ae81de26188d080b50032"),
        ("mcb1", "f71a787bc5231e494dd4d3b212338bc5aa9ebb2f386d080077329c081b335e9e"),
        ("mcb2", "83da96479e58451044ca0ec7df3b7eeb06ff201b964f0be2e4834ea973f8b400"),
        ("fuse", "5ad58d42b7db9a3be54f4a7e7ed9656579d2eb0d3163be839fdd37b01a6f24c6"),
        ("psu", "8533d527e794630088786d9150259f7507252d805af325b88f380e095558af0a"),
        ("pbno", "b07c02ed07473d84b2f622453bd9bfa446731eded74dc1bd622c6e410fc716eb"),
        ("pbnc", "9faea62fe300da57a72e512639a89c9c4a41b272d84b06b8aaa1fef204f4d0ab"),
        ("sel", "c60160c8e3add0fb75d9f17df4a57401226a54ae530da9a5982a560bcfaaa5bd"),
        ("lamp", "57775972955d04b3b45cfdd7875d86b6fb899c8e0194fcfaf79c7565e1b428bc"),
        ("motor3", "9455fc346ec441fff99e4c88506343755b6009e95e7ddea374ef219852a25d9c"),
        ("motor1", "6d9866b2882a1748cd5a0dc1e19c65954f5e2dbce192ff8b3006987b3b7857e9"),
        ("trafo", "b700be20cb69c8cee00ee1bea53d3c281305df697d7ef6975b0ce95f9dba7ae2"),
        ("terminal_block", "c904e326ee49fd84e63370c40f64130178894087d5e791e0b8030dbc0ce0d550"),
        ("terminal_block_pe", "7884a98a0463a90da4d3b49bb72b2929dd65a829ffd0b02027f904e4f9805da8"),
        ("di", "ff9461723031db3de576373b4a19614c8b238fdb7c2a7fc3a35ddd8d3eaacc53"),
        ("do", "ae6fece507c338ee0e3ed27f84d1b3cf5a510c5e263208b20ed579572d76e485"),
        ("continuation", "934a7f03dce20dd5bbc8697f0440c92b2b7e0e2da486b70df0327408407b25f4"),
    ];
    // Brief 036's starters are new; the hashes pinned here are the earlier basic ones.
    let basic: Vec<_> = STARTERS
        .iter()
        .filter(|s| s.family == Family::Basic && !ecad_electrical::terminal_strips::terminal_blocks::is_level_starter(s.key))
        .collect();
    assert_eq!(basic.len(), want.len());
    for (key, hash) in want {
        let o = by_key(key).unwrap().definition().unwrap().origin.clone().unwrap();
        assert_eq!(o.content_hash.hex(), *hash, "{key}'s content hash changed");
    }
}

/// Definition IDs are unique within each block (and overall: `every_starter_is_valid…`).
#[test]
fn ids_are_unique_within_their_blocks() {
    let mut by_block: BTreeMap<u128, BTreeSet<u128>> = BTreeMap::new();
    for s in STARTERS {
        let n = s.id.0 - DEF_BASE;
        assert!(by_block.entry(n & !0xff).or_default().insert(n & 0xff), "{} repeats an ID in its block", s.key);
    }
}

#[test]
fn changeover_labels_follow_the_sequence_digit() {
    let set = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<BTreeSet<_>>();
    assert_eq!(changeover_labels(&set(&["A1", "A2"]), "-K01").unwrap(), ["11", "12", "14"]);
    assert_eq!(changeover_labels(&set(&["A1", "11", "12", "14", "23", "24"]), "-K01").unwrap(), ["31", "32", "34"]);
    assert_eq!(changeover_labels(&set(&["91", "92"]), "-K01").unwrap_err().to_string(), "-K01 already has 9 contacts.");
    // Interplay with 018's NO/NC digits, both ways: a change-over after NO and NC takes the
    // next digit, and its `s1`/`s2`/`s4` count as that digit for later contacts.
    assert_eq!(changeover_labels(&set(&["13", "14", "21", "22"]), "-K01").unwrap(), ["31", "32", "34"]);
    assert_eq!(changeover_labels(&set(&["11", "12", "14", "23", "24"]), "-K01").unwrap(), ["31", "32", "34"]);
    // A fuse bank's outputs are skipped for the digit, and their labels are never reused.
    let bank12 = set(&["L+", "M", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"]);
    let outs = set(&["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"]);
    assert_eq!(sequence_labels(&bank12, &outs, &['3', '4'], "-F01").unwrap(), ["13", "14"]);
    assert_eq!(sequence_labels(&bank12, &outs, &['1', '2'], "-F01").unwrap(), ["21", "22"]);
    assert_eq!(sequence_labels(&bank12, &outs, &['1', '2', '4'], "-F01").unwrap(), ["21", "22", "24"]);
    // Without the skip, outputs 11 and 12 would count as contact 1.
    assert_eq!(sequence_labels(&bank12, &BTreeSet::new(), &['3', '4'], "-F01").unwrap(), ["23", "24"]);
    // Labels that are not sequence labels do not count.
    assert_eq!(changeover_labels(&set(&["15", "x1", "1"]), "-K01").unwrap(), ["11", "12", "14"]);
}

#[test]
fn quiet_points_table() {
    let quiet = |key: &str| by_key(key).map(|s| s.labels().into_iter().filter(|l| is_quiet(s, l)).collect::<Vec<_>>()).unwrap_or_default();
    assert_eq!(quiet("fusebank4"), ["M", "1", "2", "3", "4"]);
    assert_eq!(quiet("rj45").len(), 9);
    assert!(quiet("co").is_empty());
    assert!(quiet("coil").is_empty());
}

#[test]
fn every_tagged_starter_has_anchors() {
    for s in STARTERS {
        let a = anchors(s);
        if s.kind == Kind::Tagless {
            assert!(a.is_empty(), "{}", s.key);
            continue;
        }
        assert!(a.iter().any(|x| x.role == TextRole::Tag), "{}", s.key);
        assert!(a.iter().all(|x| x.at.is_finite()), "{}", s.key);
    }
    let coil = anchors(by_key("coil").unwrap());
    assert_eq!(coil[0], Anchor { role: TextRole::Tag, at: Pt::new(-8.0, 0.0), h: HAlign::Right, v: VAlign::Middle, rotation: 0.0 });
    let bank = anchors(by_key("fusebank4").unwrap());
    let fuse3 = bank.iter().find(|x| x.role == TextRole::RatingOf("3".into())).unwrap();
    assert_eq!((fuse3.at.x, fuse3.at.y, fuse3.rotation), (23.0, -8.0, 90.0));
    assert_eq!(bank.iter().filter(|x| matches!(x.role, TextRole::RatingOf(_))).count(), 4);
    // RCBO: rating under the tag, description under that.
    let rcbo = anchors(by_key("rcbo2").unwrap());
    let rating = rcbo.iter().find(|x| x.role == TextRole::Rating).unwrap();
    let tag = rcbo.iter().find(|x| x.role == TextRole::Tag).unwrap();
    assert!(rating.at.y < tag.at.y && rating.at.x == tag.at.x);
}

#[test]
fn search_table() {
    let keys = |q: &str| search(q).into_iter().map(|s| s.key.word()).collect::<Vec<_>>();
    assert_eq!(keys("-Q"), ["mcb1", "mcb2", "rcbo2", "rcd2", "rcd4"]);
    // A bare letter is also a substring of an alias (`dq`).
    assert_eq!(keys("q"), ["mcb1", "mcb2", "rcbo2", "rcd2", "rcd4", "plcdq", "chdq"]);
    assert_eq!(
        keys("plc"),
        [
            "di", "do", "plccpu", "plcps", "plcdi", "plcdq", "plcai", "plcao", "plcrtd", "plciol", "plcim", "plcpm", "plccm", "chdi", "chdq", "chai",
            "chao", "chrtd", "chiol"
        ]
    );
    assert_eq!(keys("contact"), ["coil", "no", "nc", "power3", "co", "swco", "power4", "ssrin", "ssrout"]);
    assert_eq!(keys("fuse"), ["fuse", "fuseterm", "fusebank4", "fusebank6", "fusebank8", "fusebank10", "fusebank12", "termfuse"]);
    assert_eq!(keys("protection"), ["rcbo2", "rcd2", "rcd4", "fuseterm", "fusebank4", "fusebank6", "fusebank8", "fusebank10", "fusebank12"]);
    assert!(keys("xyz").is_empty());
    assert!(keys("basic").is_empty());
    assert_eq!(keys("").len(), STARTERS.len());
    assert!(keys(&"z".repeat(10_000)).is_empty());
}
