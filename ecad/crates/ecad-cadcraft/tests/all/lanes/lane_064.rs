//! Lane 064 (design 064 §10): one update pipeline for derived views.

use ecad_cadcraft::defaults;
use ecad_model::Word;
use ecad_model::keys::{self, Area};

/// C3: every fixed key and every defaults entry classifies to an area other than `Free`, and the
/// registry's area words are `Area::REGISTRY`'s, in order.
#[test]
fn areas_cover_the_keys() {
    for k in keys::KEYS {
        assert_ne!(keys::area_of(k.key.as_str()), Area::Free, "{}", k.key);
        assert_eq!(keys::area_of(k.key.as_str()), k.area, "{}", k.key);
    }
    for e in defaults::all() {
        assert_ne!(keys::area_of(e.key), Area::Free, "{}", e.key);
    }
    let words: Vec<&str> = Area::REGISTRY.iter().map(|a| a.word()).collect();
    assert_eq!(words, defaults::AREAS);
}
