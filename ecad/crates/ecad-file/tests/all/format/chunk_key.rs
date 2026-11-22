//! `ChunkKey` reads a stored key text as what it names.

use ecad_file::chunks::{ChunkKey, META_KEY, ModelKind, PROJECT_KEY, model_key, page_key};
use ecad_model::PageId;

#[test]
fn keys_read_as_what_they_name() {
    assert_eq!(ChunkKey::parse(META_KEY), Some(ChunkKey::Meta));
    assert_eq!(ChunkKey::parse(PROJECT_KEY), Some(ChunkKey::Project));
    let page = PageId(7);
    assert_eq!(ChunkKey::parse(&page_key(page)), Some(ChunkKey::Page(page)));
    assert_eq!(ChunkKey::parse(&model_key("device", 3)), Some(ChunkKey::Model { kind: ModelKind::Device, bucket: 3 }));
    assert_eq!(ChunkKey::parse(&model_key("loose_part", 0)), Some(ChunkKey::Model { kind: ModelKind::LoosePart, bucket: 0 }));
}

#[test]
fn bad_keys_read_as_nothing() {
    for key in ["", "model:", "model:device", "model:device:x", "model:nothing:1", "page:zz", "other:1"] {
        assert_eq!(ChunkKey::parse(key), None, "{key}");
    }
}
