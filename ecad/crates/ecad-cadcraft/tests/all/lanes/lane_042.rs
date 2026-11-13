//! Brief 042 (U4c) step 2: `setdefault`, the one writer for the Standards screen and the
//! inspector. Table-driven over the whole registry: each level each entry allows writes the value
//! the registry reads back at that level, in one undoable transaction; reset falls back one
//! level; a level the entry does not allow is refused with the levels it does. Also the read-only
//! helpers `chain` and `users`.

use crate::common::{H, id};

use ecad_cadcraft::default_cmds::{self, Target};
use ecad_cadcraft::defaults::{self, Entry, Kind, Level, Scope, Source};
use ecad_model::{ComponentId, PageId, RepresentationId};
use serde_json::{Value, json};

const SEED: u64 = 42;

/// A valid value for `entry` that is not its built-in, as typed (`None` for a one-word choice).
fn other_value(e: &Entry) -> Option<String> {
    let builtin = defaults::Resolved { value: e.builtin, from: Source::Builtin }.text();
    let candidates: Vec<String> = match e.kind {
        Kind::Length { min, max } | Kind::Number { min, max } => vec![format!("{max}"), format!("{min}")],
        Kind::Choice(words) => words.iter().map(|w| w.to_string()).collect(),
        Kind::Text { .. } | Kind::Parsed { .. } => {
            ["ZZ", "X", "Y", "1,N,3", "1,2,3", "1,2", "A,B", "none", "rev, date, description", "rev@12, description", "Plex"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        }
    };
    candidates.into_iter().find(|c| e.parse(c).is_some() && !c.eq_ignore_ascii_case(&builtin))
}

/// A project with a coil: its page, component and symbol.
fn fixture() -> (H, PageId, ComponentId, RepresentationId) {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let r = h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap();
    let c: ComponentId = id(&r, "component");
    let item: ecad_model::ItemId = id(&r, "item");
    let rep = h.p(|p| p.ledger.document().representation_of_item(item).map(|(r, _)| r)).unwrap();
    (h, page, c, rep)
}

/// The levels a call can name, with the target, the scope to read at and the source the value
/// then comes from.
fn targets(page: PageId, c: ComponentId, rep: RepresentationId, doc: &ecad_model::Document) -> Vec<(Level, Target, Scope, Source)> {
    vec![
        (Level::Project, Target::Project, Scope::project(), Source::Project),
        (Level::Page, Target::Page(page), Scope::page(page), Source::Page),
        (Level::Object, Target::Component(c), Scope { page: Some(page), component: Some(c), ..Scope::project() }, Source::Object),
        (Level::Object, Target::Symbol(rep), Scope::symbol(doc, rep), Source::Symbol),
    ]
}

/// The params naming `t`.
fn target_json(t: Target) -> Value {
    match t {
        Target::Project => json!({}),
        Target::Page(p) => json!({"page": p.hex()}),
        Target::Component(c) => json!({"component": c.hex()}),
        Target::Conductor(c) => json!({"conductor": c.hex()}),
        Target::Symbol(r) => json!({"representation": r.hex()}),
    }
}

fn call(key: &str, level: Level, target: &Value, value: Option<&str>) -> Value {
    let mut p = target.clone();
    p["key"] = json!(key);
    p["level"] = json!(default_cmds::level_word(level));
    match value {
        Some(v) => p["value"] = json!(v),
        None => p["reset"] = json!(true),
    }
    p
}

/// The whole registry, on the document (no session, so it stays fast): each level each entry
/// allows writes what the registry reads back there; reset falls back; a level it does not allow
/// is refused with the levels it does.
#[test]
fn every_allowed_level_of_every_entry_writes_reads_back_and_resets() {
    let (mut h, page, c, rep) = fixture();
    let doc = h.doc();
    let mut written = 0;
    for e in defaults::all() {
        let Some(value) = other_value(e) else {
            // A one-word choice: only its built-in exists, written as the project's own.
            assert!(matches!(e.kind, Kind::Choice(w) if w.len() == 1), "{}: no test value", e.key);
            continue;
        };
        let stored = default_cmds::stored_text(e, &json!(value)).unwrap_or_else(|m| panic!("{}: {m}", e.key));
        for (level, target, scope, source) in targets(page, c, rep, &doc) {
            let base = defaults::resolve(&doc, e, scope);
            let ops = default_cmds::ops(&doc, e, level, target, Some(stored.clone()));
            if !e.allows(level) {
                let m = ops.expect_err(&format!("{} at {level:?}", e.key));
                assert!(m.contains("cannot be set at") && m.contains(default_cmds::level_word(level)), "{}: {m}", e.key);
                continue;
            }
            let ops = ops.unwrap_or_else(|m| panic!("{} at {level:?}: {m}", e.key));
            assert_eq!(ops.len(), 1, "{} at {level:?}: one op", e.key);
            let mut d = doc.clone();
            for op in &ops {
                op.apply(&mut d).unwrap();
            }
            let got = defaults::resolve(&d, e, scope);
            assert_eq!(got.from, source, "{} at {level:?}", e.key);
            assert_eq!(got.text(), e.parse(&value).map(|x| defaults::Resolved { value: x, from: source }.text()).unwrap(), "{}", e.key);
            assert!(default_cmds::ops(&d, e, level, target, Some(stored.clone())).unwrap().is_empty(), "{}: again, nothing", e.key);
            let reset = default_cmds::ops(&d, e, level, target, None).unwrap();
            for op in &reset {
                op.apply(&mut d).unwrap();
            }
            assert_eq!(defaults::resolve(&d, e, scope), base, "{} at {level:?}: reset falls back", e.key);
            written += 1;
        }
    }
    assert!(written > 40, "only {written} levels written");
}

/// Through the command: one transaction per call, reset is one more, undo takes it back; for a
/// project property, a project setting, a page and a symbol.
#[test]
fn setdefault_is_one_undoable_transaction() {
    let (mut h, page, c, rep) = fixture();
    let doc = h.doc();
    let t = targets(page, c, rep, &doc);
    for (key, value, (level, target, scope, source)) in
        [("text.tag.height", "3", t[0]), ("wiring.label_rule", "each", t[0]), ("text.tag.show", "off", t[1]), ("text.tag.show", "first", t[3])]
    {
        let e = defaults::find(key).unwrap();
        let d0 = h.doc();
        let base = defaults::resolve(&d0, e, scope);
        let head = h.head();
        let v = h.run("setdefault", call(key, level, &target_json(target), Some(value))).unwrap();
        assert_eq!((v["changed"].clone(), v["from"].clone(), h.head()), (json!(true), json!(source.as_str()), head + 1), "{key}");
        let again = h.run("setdefault", call(key, level, &target_json(target), Some(value))).unwrap();
        assert_eq!((again["changed"].clone(), h.head()), (json!(false), head + 1), "{key}: nothing recorded");
        h.run("setdefault", call(key, level, &target_json(target), None)).unwrap();
        assert_eq!(h.head(), head + 2, "{key}: reset is one transaction");
        assert_eq!(defaults::resolve(&h.doc(), e, scope), base, "{key}");
        h.cmdline("u");
        assert_eq!(defaults::resolve(&h.doc(), e, scope).from, source, "{key}: undo");
        let last = h.p(|p| p.ledger.log().iter().rev().nth(1).map(|t| t.label.clone()));
        assert_eq!(last.as_deref(), Some(format!("Default {}", e.label).as_str()));
    }
    h.check();
}

#[test]
fn refusals_say_why_and_name_the_allowed_levels() {
    let (mut h, _, _, _) = fixture();
    let head = h.head();
    // Text heights: project only (owner answer 4: page level for show and hide only).
    let m = h.run("setdefault", json!({"key": "text.tag.height", "level": "page", "value": 3})).unwrap_err();
    assert!(m.contains("cannot be set at page level") && m.contains("It can be set at: project"), "{m}");
    let m = h.run("setdefault", json!({"key": "text.tag.height", "level": "standard", "value": 3})).unwrap_err();
    assert_eq!(m, default_cmds::STANDARD_READ_ONLY);
    for (params, words) in [
        (json!({"key": "text.tag.height", "value": 11}), "length from 0.5 to 10"),
        (json!({"key": "text.tag.height", "value": "NaN"}), "length"),
        (json!({"key": "text.tag.height"}), "reset: true"),
        (json!({"key": "text.tag.height", "value": 3, "reset": true}), "not both"),
        (json!({"key": "text.nope", "value": 3}), "No default"),
        (json!({"key": "text.tag.height", "level": "sideways", "value": 3}), "Unknown level"),
        (json!({"key": "text.tag.show", "level": "object", "value": "off"}), "needs a `component`"),
        (json!({"key": "symbol.coil.prefix", "value": "K1"}), "A prefix is 1 to"),
        (json!({"key": "symbol.coil.prefix", "value": &"x".repeat(10_000)}), "letters"),
        (json!({"key": "text.font", "value": "a\u{7}b"}), "control characters"),
        (json!({"key": "text.tag.show", "level": "object", "component": "zz", "value": "off"}), "hex id"),
    ] {
        let m = h.run("setdefault", params.clone()).unwrap_err();
        assert!(m.contains(words), "{params}: {m}");
    }
    assert_eq!(h.head(), head, "refusals record nothing");
}

#[test]
fn a_prefix_goes_to_the_tag_settings_and_counts_its_devices() {
    let (mut h, _, _, _) = fixture();
    h.run("place", json!({"symbol": "no", "at": [150, 100]})).unwrap();
    h.run("place", json!({"symbol": "fuse", "at": [200, 100]})).unwrap();
    let e = defaults::find("symbol.coil.prefix").unwrap();
    // Two K devices (the coil and the NO contact), not the fuse.
    assert_eq!(defaults::users(&h.doc(), e), 2);
    h.run("setdefault", json!({"key": e.key, "value": "-q"})).unwrap();
    let d = h.doc();
    assert_eq!(d.settings.tag_prefixes.get("coil").map(String::as_str), Some("Q"), "as Tag Settings writes it");
    assert!(!d.properties.contains_key(e.key), "never a project property too");
    assert_eq!(defaults::resolve(&d, e, Scope::project()).from, Source::Project);
    // The chain: built-in K, the standard (no value of its own), the project's Q.
    let chain = defaults::chain(&d, e, Scope::project());
    let words: Vec<(Source, Option<String>)> = chain.iter().map(|(s, v)| (*s, v.map(|v| defaults::Resolved { value: v, from: *s }.text()))).collect();
    assert_eq!(words, vec![(Source::Builtin, Some("K".into())), (Source::Standard, None), (Source::Project, Some("Q".into()))]);
    h.run("setdefault", json!({"key": e.key, "reset": true})).unwrap();
    assert!(!h.doc().settings.tag_prefixes.contains_key("coil"));
}

#[test]
fn the_chain_shows_a_level_that_holds_a_value_or_is_allowed() {
    let (mut h, page, c, rep) = fixture();
    let show = defaults::find("text.tag.show").unwrap();
    let height = defaults::find("text.tag.height").unwrap();
    let doc = h.doc();
    let at = Scope::symbol(&doc, rep);
    let levels = |e: &Entry, d: &ecad_model::Document| defaults::chain(d, e, at).iter().map(|(s, _)| *s).collect::<Vec<_>>();
    assert_eq!(levels(show, &doc), vec![Source::Builtin, Source::Standard, Source::Project, Source::Page, Source::Object, Source::Symbol]);
    // A height has no page level (owner answer 4).
    assert!(!levels(height, &doc).contains(&Source::Page));
    let _ = c;
    h.run("setdefault", json!({"key": show.key, "level": "page", "page": page.hex(), "value": "off"})).unwrap();
    let d = h.doc();
    let chain = defaults::chain(&d, show, at);
    assert_eq!(chain.iter().find(|(s, _)| *s == Source::Page).and_then(|(_, v)| *v), Some(defaults::Value::Choice("off")));
    assert_eq!(defaults::resolve(&d, show, at).from, Source::Page);
}
