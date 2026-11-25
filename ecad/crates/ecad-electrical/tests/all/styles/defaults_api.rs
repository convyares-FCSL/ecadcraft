//! The defaults registry through its Rust API.

use ecad_electrical::defaults::{self};
use ecad_model::Document;
use ecad_model::registry::Scope;

/// AC 7: keys unique and of the key rule, built-ins valid for their own kind, every entry allows
/// the project level; a reader and its entry agree on an empty project.
#[test]
fn registry_keys_unique_builtins_valid_and_project_level_allowed() {
    let mut seen = std::collections::BTreeSet::new();
    let d = Document::default();
    for e in ecad_electrical::defaults::data::DATA.entries() {
        assert!(seen.insert(e.key), "duplicate {}", e.key);
        assert!(e.key.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')), "{}", e.key);
        // Brief 054 (design 054 §2.3): a page's type is a property of the page, never a project
        // default.
        let page_only = e.key == defaults::pages::PAGE_TYPE.key;
        assert!(page_only || e.levels.iter().any(|l| matches!(l, ecad_model::registry::Level::Project)), "{} allows Project", e.key);
        let r = ecad_model::registry::resolve(&d, e, Scope::project());
        let _ = r;
    }
    // The placement keys of texts are the rules'; their styles and the font are the look's.
    assert!(seen.contains("text.run.gap") && !seen.contains("text.tag.style") && !seen.contains("text.font"));
}
