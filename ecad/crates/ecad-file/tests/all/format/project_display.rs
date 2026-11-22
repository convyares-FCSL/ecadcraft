//! Lane 073: the document's project-level bag is stored under the key `display` in the project
//! chunk, and read back from it.

use ecad_file::chunks::{ModelMeta, PROJECT_KEY, assemble, build_full};

#[test]
fn the_project_chunk_stores_the_display_bag_under_display() {
    let mut doc = ecad_model::Document::default();
    doc.edit_parts(|parts| {
        parts.display.insert("note".into(), "Rev A".into());
    });
    let (set, chunks) = build_full(&doc, &ModelMeta::default()).unwrap();
    let h = set.chunks[PROJECT_KEY];
    let text = String::from_utf8(chunks.iter().find(|c| c.hash == h).unwrap().body.clone()).unwrap();
    assert!(text.contains(r#""display":{"note":"Rev A"}"#), "{text}");
    assert!(!text.contains("properties"), "{text}");
    let bodies: Vec<(String, Vec<u8>)> =
        set.chunks.iter().map(|(k, h)| (k.clone(), chunks.iter().find(|c| c.hash == *h).unwrap().body.clone())).collect();
    let (back, _) = assemble(&set, bodies).unwrap();
    assert_eq!(back.display(), doc.display());
}
