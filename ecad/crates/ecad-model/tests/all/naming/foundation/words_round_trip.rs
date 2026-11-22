//! A2: `words_round_trip` (design 072 §14): every `Word` enum reads back what it writes, serde
//! writes the word and refuses any other, and its schema lists exactly its words.

use std::collections::BTreeSet;
use std::fmt::Debug;

use ecad_model::geometry::colour::ColourWords;
use ecad_model::naming::keys::Area;
// ecad-model's own schemars, through its `schema` feature (only the Engine depends on it).
use ecad_model::naming::words::__schemars as schemars;
use ecad_model::{Side, StyleCatalogue, Word};
use serde::Serialize;
use serde::de::DeserializeOwned;

fn round_trip<E>() -> Result<(), serde_json::Error>
where
    E: Word + PartialEq + Debug + Serialize + DeserializeOwned + schemars::JsonSchema,
{
    let name = std::any::type_name::<E>();
    assert_eq!(E::ALL.len(), E::WORDS.len(), "{name}: one word per value");
    assert_eq!(E::WORDS.iter().collect::<BTreeSet<_>>().len(), E::WORDS.len(), "{name}: words are unique");
    for (x, w) in E::ALL.iter().zip(E::WORDS) {
        assert_eq!(x.word(), *w, "{name}: WORDS is in ALL's order");
        assert_eq!(E::parse(x.word()), Some(*x), "{name}: parse(word({x:?}))");
        assert_eq!(E::parse(&format!(" {} ", x.word().to_ascii_uppercase())), Some(*x), "{name}: case and spaces");
        assert_eq!(serde_json::to_string(x)?, format!("\"{w}\""), "{name}: serde writes the word");
        assert_eq!(serde_json::from_str::<E>(&format!("\"{w}\""))?, *x);
    }
    assert_eq!(E::parse("no_such_word"), None, "{name}: no fallback");
    assert_eq!(E::parse(""), None);
    let err = serde_json::from_str::<E>("\"no_such_word\"").err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("`no_such_word`"), "{name}: the error names the word: {err}");
    assert!(E::WORDS.iter().all(|w| err.contains(w)), "{name}: the error lists the allowed words: {err}");
    let schema = serde_json::to_value(schemars::schema_for!(E))?;
    let listed: Vec<&str> = schema["enum"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).collect()).unwrap_or_default();
    assert_eq!(listed, E::WORDS, "{name}: schema equals WORDS");
    Ok(())
}

#[test]
fn words_round_trip() -> Result<(), serde_json::Error> {
    round_trip::<StyleCatalogue>()?;
    round_trip::<Side>()?;
    round_trip::<Area>()?;
    round_trip::<ColourWords>()?;
    Ok(())
}

#[test]
fn words_are_the_stored_ones() {
    assert_eq!(StyleCatalogue::WORDS, ["cable", "frame", "location", "page", "ratingtable", "text", "wire"]);
    // In word order, so maps keyed by catalogue keep the file's order (F2: same bytes).
    assert!(StyleCatalogue::WORDS.is_sorted());
    assert_eq!(Side::WORDS, ["socket", "plug"]);
    assert_eq!(Side::Socket.other(), Side::Plug);
    let registry: Vec<&str> = Area::REGISTRY.iter().map(|a| a.word()).collect();
    assert_eq!(
        registry,
        [
            "text",
            "wiring",
            "wire",
            "tags",
            "revisions",
            "print",
            "symbol",
            "terminal_block",
            "connector",
            "strippage",
            "bom",
            "location",
            "continuation",
            "plc",
            "page",
            "front",
            "rating"
        ]
    );
}
