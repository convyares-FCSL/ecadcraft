//! Lane 064 (design 064 §10): one update pipeline for derived views.

use ecad_cadcraft::defaults;
use ecad_model::Word;
use ecad_model::keys::{self, Area};

/// C3: every fixed key and every defaults entry classifies to an area other than `Free`, and the
/// registry's area words, in order, are the 17 written out before design 064.
#[test]
fn areas_cover_the_keys() {
    for k in keys::KEYS {
        assert_ne!(keys::area_of(k.key.as_str()), Area::Free, "{}", k.key);
        assert_eq!(keys::area_of(k.key.as_str()), k.area, "{}", k.key);
    }
    for e in defaults::all() {
        assert_ne!(keys::area_of(e.key), Area::Free, "{}", e.key);
    }
    // The registry's words as they were written out before design 064 (`defaults::AREAS` is now
    // derived from `Area::REGISTRY`, so compare both with the old list).
    let old = [
        "text",
        "wiring",
        "wire",
        "tags",
        "revisions",
        "print",
        "symbol",
        "terminal",
        "connector",
        "strippage",
        "bom",
        "location",
        "continuation",
        "plc",
        "page",
        "front",
        "rating",
    ];
    let words: Vec<&str> = Area::REGISTRY.iter().map(|a| a.word()).collect();
    assert_eq!(words, old, "Area::REGISTRY");
    assert_eq!(defaults::AREAS, old, "defaults::AREAS");
}
