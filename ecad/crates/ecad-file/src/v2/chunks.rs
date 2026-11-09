//! Snapshot chunks (EEP-0005 §3.5; overseer, 2026-11-08): the document split by key, each chunk
//! the JSON of its objects, content-addressed by [`super::schema::chunk_hash`].
//!
//! - `page:<PageId>`: the page, the items on it and the representations on it.
//! - `model:project`: page order, settings, named versions, project properties, folders and
//!   folder order.
//! - `model:<kind>:<b>`: the layers, components, terminals, conductors, cables or definitions
//!   whose ID falls in bucket `b` of that kind. A kind with `n` objects has `B` buckets, the
//!   smallest power of two with `B × 1 024 ≥ n` (at least 1), and an object's bucket is a mix of
//!   its ID modulo `B`. An edit therefore rewrites one chunk; all of a kind's chunks are rewritten
//!   only when its count crosses a power of two.
//!
//! A chunk's JSON is serde's form of the model types (struct fields in declaration order, maps in
//! key order): deterministic for a build, so equal content dedupes. Building is incremental: only
//! the keys a range of transactions touched ([`Dirty`]) are rebuilt; the rest keep their hashes.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{
    Cable, CableId, Component, ComponentId, Conductor, ConductorId, Definition, DefinitionId, Document, Folder,
    FolderId, Item, ItemId, Layer, LayerId, ObjectRef, Op, Page, PageId, ProjectSettings, Representation,
    RepresentationId, Terminal, TerminalId, Touched,
};
use serde::{Deserialize, Serialize};

use super::schema::chunk_hash;
use crate::chain::{self, Hash};

/// Objects per model chunk, before bucketing (overseer, 2026-11-08).
pub const CHUNK: usize = 1_024;
/// The chunk holding project-wide state.
pub const PROJECT_KEY: &str = "model:project";
/// The chunk holding state derived from the history that a snapshot must carry (overseer answer
/// C, 2026-11-08): a cache, rebuilt by Verify from the history.
pub const META_KEY: &str = "model:meta";

/// What `model:meta` holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelMeta {
    /// The highest number each tag prefix has had up to the snapshot's point, deleted and
    /// undone tags included (brief 018).
    pub tag_high: ecad_model::tag::HighWater,
}
/// Most buckets a kind may have (a bound on hostile files).
pub const MAX_BUCKETS: u32 = 1 << 20;

/// The model kinds split into chunks, by their key name.
pub const KINDS: [&str; 6] = ["layer", "component", "terminal", "conductor", "cable", "definition"];

/// The key of page `id`'s chunk.
pub fn page_key(id: PageId) -> String {
    format!("page:{}", id.hex())
}

/// The key of bucket `b` of `kind`.
pub fn model_key(kind: &str, b: u32) -> String {
    format!("model:{kind}:{b}")
}

/// How many buckets a kind of `n` objects has.
pub fn bucket_count(n: usize) -> u32 {
    let chunks = n.div_ceil(CHUNK).max(1);
    u32::try_from(chunks.next_power_of_two()).unwrap_or(MAX_BUCKETS).min(MAX_BUCKETS)
}

/// `mix(id)`, part of the file format (EEP-0005 §3.5, overseer answer A): never changes with a
/// crate version. The 128-bit ID's two 64-bit halves XORed, then the SplitMix64 finaliser
/// (add `0x9E3779B97F4A7C15`; `z ^= z >> 30; z *= 0xBF58476D1CE4E5B9; z ^= z >> 27;
/// z *= 0x94D049BB133111EB; z ^= z >> 31`, all wrapping, 64-bit).
pub fn mix(id: u128) -> u64 {
    let mut z = (id as u64) ^ ((id >> 64) as u64);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The bucket of object `id` among `count` buckets (a power of two): `mix(id) mod count`.
pub fn bucket(id: u128, count: u32) -> u32 {
    (mix(id) & u64::from(count.max(1) - 1)) as u32
}

/// An object ID as its 128-bit value.
trait RawId: Copy + Ord + std::fmt::Display {
    fn raw(self) -> u128;
}

macro_rules! raw_id {
    ($($t:ty),*) => {$(impl RawId for $t { fn raw(self) -> u128 { self.0 } })*};
}
raw_id!(LayerId, ComponentId, TerminalId, ConductorId, CableId, DefinitionId);

/// A snapshot's chunk set: key → hash, and the bucket count of each model kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChunkSet {
    pub chunks: BTreeMap<String, Hash>,
    pub buckets: BTreeMap<String, u32>,
}

impl ChunkSet {
    /// The set named by a version row's `chunks` (key → hex hash).
    ///
    /// # Errors
    /// A malformed key or hash.
    pub fn from_record(chunks: &BTreeMap<String, String>) -> Result<Self, String> {
        let mut set = ChunkSet::default();
        for (k, h) in chunks {
            let hash = chain::parse_hex(h).ok_or_else(|| format!("chunk {k}: {h:?} is not a hash"))?;
            if let Some(rest) = k.strip_prefix("model:")
                && let Some((kind, b)) = rest.split_once(':')
            {
                let b: u32 = b.parse().map_err(|_| format!("chunk key {k:?} has no bucket number"))?;
                if !KINDS.contains(&kind) || b >= MAX_BUCKETS {
                    return Err(format!("chunk key {k:?} is not a model chunk"));
                }
                let n = set.buckets.entry(kind.to_string()).or_insert(0);
                *n = (*n).max(b + 1);
            } else if k != PROJECT_KEY && k != META_KEY && !k.starts_with("page:") {
                return Err(format!("unknown chunk key {k:?}"));
            }
            set.chunks.insert(k.clone(), hash);
        }
        for (kind, n) in &mut set.buckets {
            if !n.is_power_of_two() {
                return Err(format!("{kind} has {n} chunks, not a power of two"));
            }
        }
        Ok(set)
    }

    /// Key → hex hash, for a version row.
    pub fn to_record(&self) -> BTreeMap<String, String> {
        self.chunks.iter().map(|(k, h)| (k.clone(), chain::hex(h))).collect()
    }
}

/// What a range of transactions touched, by chunk.
#[derive(Debug, Clone, Default)]
pub struct Dirty {
    pub pages: BTreeSet<PageId>,
    pub objects: BTreeMap<&'static str, BTreeSet<u128>>,
}

impl Dirty {
    /// Add what `ops` touch.
    pub fn add_ops(&mut self, ops: &[Op]) {
        let t = Touched::of(ops);
        let mut add = |kind: &'static str, ids: Vec<u128>| self.objects.entry(kind).or_default().extend(ids);
        add("layer", t.layers.iter().map(|i| i.0).collect());
        add("component", t.components.iter().map(|i| i.0).collect());
        add("terminal", t.terminals.iter().map(|i| i.0).collect());
        add("conductor", t.conductors.iter().map(|i| i.0).collect());
        add("cable", t.cables.iter().map(|i| i.0).collect());
        add("definition", t.definitions.iter().map(|i| i.0).collect());
        for op in ops {
            match op {
                Op::AddPage { id, .. } | Op::RemovePage { id, .. } => {
                    self.pages.insert(*id);
                }
                Op::SetProperty { object: ObjectRef::Page(id), .. } => {
                    self.pages.insert(*id);
                }
                Op::AddItem { item, .. } | Op::RemoveItem { item, .. } => {
                    self.pages.insert(item.page);
                }
                Op::ModifyItem { before, after, .. } => {
                    self.pages.insert(before.page);
                    self.pages.insert(after.page);
                }
                Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => {
                    self.pages.insert(rep.page);
                }
                Op::ModifyRepresentation { before, after, .. } => {
                    self.pages.insert(before.page);
                    self.pages.insert(after.page);
                }
                _ => {}
            }
        }
    }
}

#[derive(Serialize)]
struct PageChunkRef<'a> {
    page: &'a Page,
    items: BTreeMap<&'a ItemId, &'a Item>,
    representations: BTreeMap<&'a RepresentationId, &'a Representation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageChunk {
    page: Page,
    items: BTreeMap<ItemId, Item>,
    representations: BTreeMap<RepresentationId, Representation>,
}

#[derive(Serialize)]
struct ProjectChunkRef<'a> {
    page_order: &'a Vec<PageId>,
    settings: &'a ProjectSettings,
    versions: &'a BTreeMap<String, u64>,
    properties: &'a BTreeMap<String, String>,
    folders: &'a BTreeMap<FolderId, Folder>,
    folder_order: &'a Vec<FolderId>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectChunk {
    page_order: Vec<PageId>,
    settings: ProjectSettings,
    versions: BTreeMap<String, u64>,
    properties: BTreeMap<String, String>,
    folders: BTreeMap<FolderId, Folder>,
    folder_order: Vec<FolderId>,
}

/// A chunk's bytes, ready to store (it may already be stored: chunks are inserted only if absent).
#[derive(Debug, Clone)]
pub struct NewChunk {
    pub hash: Hash,
    pub body: Vec<u8>,
}

fn encode<T: Serialize>(v: &T) -> Result<NewChunk, String> {
    let body = serde_json::to_vec(v).map_err(|e| e.to_string())?;
    Ok(NewChunk { hash: chunk_hash(&body), body })
}

/// The model kind `kind`'s objects in bucket `b` (of `count`) as a chunk.
fn model_chunk(doc: &Document, kind: &str, b: u32, count: u32) -> Result<NewChunk, String> {
    fn pick<'a, K: RawId + 'a, V>(map: &'a BTreeMap<K, V>, b: u32, count: u32) -> BTreeMap<&'a K, &'a V> {
        map.iter().filter(|(k, _)| bucket(k.raw(), count) == b).collect()
    }
    match kind {
        "layer" => encode(&pick(&doc.layers, b, count)),
        "component" => encode(&pick(&doc.components, b, count)),
        "terminal" => encode(&pick(&doc.terminals, b, count)),
        "conductor" => encode(&pick(&doc.conductors, b, count)),
        "cable" => encode(&pick(&doc.cables, b, count)),
        "definition" => encode(&pick(&doc.definitions, b, count)),
        other => Err(format!("unknown model kind {other}")),
    }
}

fn kind_len(doc: &Document, kind: &str) -> usize {
    match kind {
        "layer" => doc.layers.len(),
        "component" => doc.components.len(),
        "terminal" => doc.terminals.len(),
        "conductor" => doc.conductors.len(),
        "cable" => doc.cables.len(),
        "definition" => doc.definitions.len(),
        _ => 0,
    }
}

/// The chunks of the pages in `pages` that exist in `doc` (one pass over items and
/// representations).
fn page_chunks(doc: &Document, pages: &BTreeSet<PageId>) -> Result<Vec<(PageId, NewChunk)>, String> {
    let mut items: BTreeMap<PageId, BTreeMap<&ItemId, &Item>> = BTreeMap::new();
    let mut reps: BTreeMap<PageId, BTreeMap<&RepresentationId, &Representation>> = BTreeMap::new();
    let all = pages.len() == doc.pages.len();
    for (id, item) in &doc.items {
        if all || pages.contains(&item.page) {
            items.entry(item.page).or_default().insert(id, item);
        }
    }
    for (id, rep) in &doc.representations {
        if all || pages.contains(&rep.page) {
            reps.entry(rep.page).or_default().insert(id, rep);
        }
    }
    let mut out = Vec::new();
    for id in pages {
        let Some(page) = doc.pages.get(id) else { continue };
        let chunk = PageChunkRef {
            page,
            items: items.remove(id).unwrap_or_default(),
            representations: reps.remove(id).unwrap_or_default(),
        };
        out.push((*id, encode(&chunk)?));
    }
    Ok(out)
}

fn project_chunk(doc: &Document) -> Result<NewChunk, String> {
    encode(&ProjectChunkRef {
        page_order: &doc.page_order,
        settings: &doc.settings,
        versions: &doc.versions,
        properties: &doc.properties,
        folders: &doc.folders,
        folder_order: &doc.folder_order,
    })
}

/// Every chunk of `doc`: its chunk set and the chunks' bytes.
///
/// # Errors
/// An object could not be encoded.
pub fn build_full(doc: &Document, meta: &ModelMeta) -> Result<(ChunkSet, Vec<NewChunk>), String> {
    let mut dirty = Dirty { pages: doc.pages.keys().copied().collect(), objects: BTreeMap::new() };
    for kind in KINDS {
        dirty.objects.insert(kind, BTreeSet::new());
    }
    build(doc, &ChunkSet::default(), &dirty, true, meta)
}

/// The chunk set of `doc`, given `prev` (the chunk set of a state that differs from `doc` only in
/// what `dirty` names): only touched keys are rebuilt. Returns the new set and the chunks it
/// names that `prev` does not.
///
/// # Errors
/// An object could not be encoded.
pub fn build_incremental(
    doc: &Document,
    prev: &ChunkSet,
    dirty: &Dirty,
    meta: &ModelMeta,
) -> Result<(ChunkSet, Vec<NewChunk>), String> {
    build(doc, prev, dirty, false, meta)
}

fn build(
    doc: &Document,
    prev: &ChunkSet,
    dirty: &Dirty,
    full: bool,
    meta: &ModelMeta,
) -> Result<(ChunkSet, Vec<NewChunk>), String> {
    let mut set = prev.clone();
    let mut new = Vec::new();
    let mut put = |set: &mut ChunkSet, key: String, chunk: NewChunk| {
        if set.chunks.get(&key) != Some(&chunk.hash) {
            new.push(chunk.clone());
        }
        set.chunks.insert(key, chunk.hash);
    };
    put(&mut set, PROJECT_KEY.into(), project_chunk(doc)?);
    put(&mut set, META_KEY.into(), encode(meta)?);
    // Pages: rebuild the touched ones; drop the chunks of pages that no longer exist.
    for (id, chunk) in page_chunks(doc, &dirty.pages)? {
        put(&mut set, page_key(id), chunk);
    }
    for id in &dirty.pages {
        if !doc.pages.contains_key(id) {
            set.chunks.remove(&page_key(*id));
        }
    }
    for kind in KINDS {
        let count = bucket_count(kind_len(doc, kind));
        let old = if full { 0 } else { prev.buckets.get(kind).copied().unwrap_or(0) };
        let rebuild: BTreeSet<u32> = if count != old {
            set.chunks.retain(|k, _| !k.starts_with(&format!("model:{kind}:")));
            (0..count).collect()
        } else {
            dirty.objects.get(kind).map(|ids| ids.iter().map(|id| bucket(*id, count)).collect()).unwrap_or_default()
        };
        for b in rebuild {
            put(&mut set, model_key(kind, b), model_chunk(doc, kind, b, count)?);
        }
        set.buckets.insert(kind.to_string(), count);
    }
    // One copy of each new body.
    let mut seen = BTreeSet::new();
    new.retain(|p| seen.insert(p.hash));
    Ok((set, new))
}

/// Insert `from` into `into`, refusing an ID already present.
fn merge<K: Ord + Copy + std::fmt::Display, V>(
    into: &mut BTreeMap<K, V>,
    from: BTreeMap<K, V>,
    what: &str,
) -> Result<(), String> {
    for (k, v) in from {
        if into.insert(k, v).is_some() {
            return Err(format!("{what} {k} is in two chunks"));
        }
    }
    Ok(())
}

/// Check that every ID of a chunk belongs in it.
fn check_bucket<K: RawId, V>(map: &BTreeMap<K, V>, key: &str, b: u32, count: u32) -> Result<(), String> {
    match map.keys().find(|k| bucket(k.raw(), count) != b) {
        Some(k) => Err(format!("{k} does not belong in chunk {key}")),
        None => Ok(()),
    }
}

/// The document a chunk set describes, from its chunks' uncompressed bodies (key, body).
///
/// # Semantics
/// Every object must be in exactly the chunk its key says (its page's chunk, its kind's bucket),
/// and the set must hold the project chunk. The document's derived index is not built here.
///
/// # Errors
/// A chunk that does not parse, an object in the wrong chunk or in two chunks, or a missing project
/// chunk.
pub fn assemble(set: &ChunkSet, bodies: Vec<(String, Vec<u8>)>) -> Result<(Document, Option<ModelMeta>), String> {
    let mut doc = Document::default();
    let mut project = false;
    let mut meta = None;
    for (key, body) in bodies {
        if key == META_KEY {
            meta = Some(serde_json::from_slice::<ModelMeta>(&body).map_err(|e| format!("chunk {key}: {e}"))?);
        } else if key == PROJECT_KEY {
            let p: ProjectChunk = serde_json::from_slice(&body).map_err(|e| format!("chunk {key}: {e}"))?;
            doc.page_order = p.page_order;
            doc.settings = p.settings;
            doc.versions = p.versions;
            doc.properties = p.properties;
            doc.folders = p.folders;
            doc.folder_order = p.folder_order;
            project = true;
        } else if let Some(hex) = key.strip_prefix("page:") {
            let id = PageId::parse(hex).ok_or_else(|| format!("chunk key {key:?} names no page"))?;
            let p: PageChunk = serde_json::from_slice(&body).map_err(|e| format!("chunk {key}: {e}"))?;
            if let Some((i, _)) = p.items.iter().find(|(_, it)| it.page != id) {
                return Err(format!("item {i} is in the chunk of page {id} but on another page"));
            }
            if let Some((r, _)) = p.representations.iter().find(|(_, r)| r.page != id) {
                return Err(format!("representation {r} is in the chunk of page {id} but on another page"));
            }
            if doc.pages.insert(id, p.page).is_some() {
                return Err(format!("page {id} is in two chunks"));
            }
            merge(&mut doc.items, p.items, "item")?;
            merge(&mut doc.representations, p.representations, "representation")?;
        } else if let Some((kind, b)) = key.strip_prefix("model:").and_then(|r| r.split_once(':')) {
            let b: u32 = b.parse().map_err(|_| format!("chunk key {key:?} has no bucket number"))?;
            let count = set.buckets.get(kind).copied().ok_or_else(|| format!("chunk {key}: unknown kind"))?;
            let parse = |what: &str| format!("chunk {key} ({what})");
            match kind {
                "layer" => {
                    let m: BTreeMap<LayerId, Layer> =
                        serde_json::from_slice(&body).map_err(|e| parse(&e.to_string()))?;
                    check_bucket(&m, &key, b, count)?;
                    merge(&mut doc.layers, m, "layer")?;
                }
                "component" => {
                    let m: BTreeMap<ComponentId, Component> =
                        serde_json::from_slice(&body).map_err(|e| parse(&e.to_string()))?;
                    check_bucket(&m, &key, b, count)?;
                    merge(&mut doc.components, m, "component")?;
                }
                "terminal" => {
                    let m: BTreeMap<TerminalId, Terminal> =
                        serde_json::from_slice(&body).map_err(|e| parse(&e.to_string()))?;
                    check_bucket(&m, &key, b, count)?;
                    merge(&mut doc.terminals, m, "terminal")?;
                }
                "conductor" => {
                    let m: BTreeMap<ConductorId, Conductor> =
                        serde_json::from_slice(&body).map_err(|e| parse(&e.to_string()))?;
                    check_bucket(&m, &key, b, count)?;
                    merge(&mut doc.conductors, m, "conductor")?;
                }
                "cable" => {
                    let m: BTreeMap<CableId, Cable> =
                        serde_json::from_slice(&body).map_err(|e| parse(&e.to_string()))?;
                    check_bucket(&m, &key, b, count)?;
                    merge(&mut doc.cables, m, "cable")?;
                }
                "definition" => {
                    let m: BTreeMap<DefinitionId, Definition> =
                        serde_json::from_slice(&body).map_err(|e| parse(&e.to_string()))?;
                    check_bucket(&m, &key, b, count)?;
                    merge(&mut doc.definitions, m, "definition")?;
                }
                other => return Err(format!("chunk {key}: unknown kind {other}")),
            }
        } else {
            return Err(format!("unknown chunk key {key:?}"));
        }
    }
    if !project {
        return Err("the snapshot has no project chunk".into());
    }
    // Every page in the order has its chunk, and every chunk's page is in the order.
    let ordered: BTreeSet<PageId> = doc.page_order.iter().copied().collect();
    let chunked: BTreeSet<PageId> = doc.pages.keys().copied().collect();
    if ordered != chunked || ordered.len() != doc.page_order.len() {
        return Err("the snapshot's page order does not match its page chunks".into());
    }
    Ok((doc, meta))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn bucket_counts_are_powers_of_two_and_buckets_are_stable() {
        assert_eq!(bucket_count(0), 1);
        assert_eq!(bucket_count(1_024), 1);
        assert_eq!(bucket_count(1_025), 2);
        assert_eq!(bucket_count(25_000), 32);
        for id in [0u128, 1, u128::MAX, 0x1234_5678_9abc_def0_1122_3344_5566_7788] {
            assert_eq!(bucket(id, 1), 0);
            assert!(bucket(id, 32) < 32);
            assert_eq!(bucket(id, 32), bucket(id, 32));
        }
        // Sequential IDs spread over the buckets.
        let mut counts = [0u32; 8];
        for id in 0..8_000u128 {
            counts[bucket(1_000_000 + id, 8) as usize] += 1;
        }
        assert!(counts.iter().all(|c| (800..1_200).contains(c)), "{counts:?}");
        // The function is part of the format: fixed values.
        assert_eq!(mix(0), 0xE220_A839_7B1D_CDAF);
        assert_eq!(mix(1), 0x910A_2DEC_8902_5CC1);
    }

    #[test]
    fn one_insert_rewrites_one_chunk() {
        use ecad_model::{Component, ComponentId, Document, Op};
        let mut doc = Document::default();
        for i in 0..5_000u128 {
            ecad_model::Op::CreateComponent { id: ComponentId(0x1000 + i * 7), component: Component::default() }
                .apply(&mut doc)
                .unwrap();
        }
        let meta = ModelMeta::default();
        let (set, _) = build_full(&doc, &meta).unwrap();
        assert_eq!(set.buckets.get("component"), Some(&8));
        let add = Op::CreateComponent { id: ComponentId(0x9_9999), component: Component::default() };
        add.apply(&mut doc).unwrap();
        let mut dirty = Dirty::default();
        dirty.add_ops(std::slice::from_ref(&add));
        let (next, new) = build_incremental(&doc, &set, &dirty, &meta).unwrap();
        assert_eq!(new.len(), 1, "one chunk written");
        let changed: Vec<_> =
            next.chunks.iter().filter(|(k, h)| set.chunks.get(*k) != Some(*h)).map(|(k, _)| k).collect();
        assert_eq!(changed, vec![&model_key("component", bucket(0x9_9999, 8))]);
        // The same set as building everything again.
        assert_eq!(next, build_full(&doc, &meta).unwrap().0);
    }
}
