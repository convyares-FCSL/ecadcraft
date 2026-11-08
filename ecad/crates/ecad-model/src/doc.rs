//! The document's objects and their relationships (proposal §3 and §4).
//!
//! Stored intent (components, terminals, conductors, cables, definitions) and drawing content
//! (pages, layers, items, representations) live here. Derived results do not (see [`crate::derive`]).

use std::collections::BTreeMap;

use crate::index::{Index, referrers};

use serde::{Deserialize, Serialize};

use crate::error::{ModelError, Result};
use crate::geom::{Placement, Pt};
use crate::id::{
    CableId, ComponentId, ConductorId, DefinitionId, ItemId, LayerId, LibraryId, PageId, RepresentationId, TerminalId,
};

/// Optional, free-form properties. A missing key is `Unset` (principle P7); an empty string is a
/// confirmed empty value.
pub type Properties = BTreeMap<String, String>;

/// A page of the project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub title: String,
}

/// A layer. References use [`LayerId`]; the name is a property (DOC-02).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
}

/// Colour of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    ByLayer,
    ByBlock,
    /// AutoCAD colour index 1..=255.
    Index(u8),
    /// 0xRRGGBB.
    Rgb(u32),
}

/// How an item is drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub layer: LayerId,
    pub color: Color,
    /// Linetype name (`ByLayer`, `Continuous`, …).
    pub linetype: String,
    /// Lineweight code: -1 ByLayer, -2 ByBlock, -3 default, else hundredths of a millimetre.
    pub lineweight: i16,
}

/// A drawn primitive in the ECAD-owned schema (proposal R3). Angles are radians.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Primitive {
    Line {
        a: Pt,
        b: Pt,
    },
    /// Straight segments only.
    Polyline {
        vertices: Vec<Pt>,
        closed: bool,
    },
    Arc {
        center: Pt,
        radius: f64,
        start: f64,
        end: f64,
    },
    Circle {
        center: Pt,
        radius: f64,
    },
    /// Single-line text, left-baseline aligned, standard style.
    Text {
        at: Pt,
        height: f64,
        rotation: f64,
        value: String,
    },
    Point {
        at: Pt,
    },
}

impl Primitive {
    /// The end points of an open path (line or open polyline), as conductor routes use them.
    pub fn path_ends(&self) -> Option<(Pt, Pt)> {
        match self {
            Primitive::Line { a, b } => Some((*a, *b)),
            Primitive::Polyline { vertices, closed: false } => Some((*vertices.first()?, *vertices.last()?)),
            _ => None,
        }
    }

    /// The same path with one end moved (`End::A` = first vertex).
    pub fn with_path_end(&self, end: End, to: Pt) -> Option<Primitive> {
        match self {
            Primitive::Line { a, b } => {
                Some(if end == End::A { Primitive::Line { a: to, b: *b } } else { Primitive::Line { a: *a, b: to } })
            }
            Primitive::Polyline { vertices, closed: false } => {
                let mut v = vertices.clone();
                let slot = if end == End::A { v.first_mut()? } else { v.last_mut()? };
                *slot = to;
                Some(Primitive::Polyline { vertices: v, closed: false })
            }
            _ => None,
        }
    }

    pub(crate) fn is_finite(&self) -> bool {
        match self {
            Primitive::Line { a, b } => a.is_finite() && b.is_finite(),
            Primitive::Polyline { vertices, .. } => vertices.iter().all(|p| p.is_finite()),
            Primitive::Arc { center, radius, start, end } => {
                center.is_finite() && radius.is_finite() && start.is_finite() && end.is_finite()
            }
            Primitive::Circle { center, radius } => center.is_finite() && radius.is_finite(),
            Primitive::Text { at, height, rotation, .. } => {
                at.is_finite() && height.is_finite() && rotation.is_finite()
            }
            Primitive::Point { at } => at.is_finite(),
        }
    }
}

/// A placed instance of an embedded definition (symbol form, proposal §4.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolInstance {
    pub definition: DefinitionId,
    pub placement: Placement,
}

/// What an item is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ItemKind {
    Primitive(Primitive),
    Symbol(SymbolInstance),
}

/// One drawn item on a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub page: PageId,
    pub style: Style,
    pub kind: ItemKind,
}

/// A component (stored intent).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Component {
    pub properties: Properties,
    /// The embedded definition its symbol representations use, if any.
    pub definition: Option<DefinitionId>,
}

/// A terminal of a component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Terminal {
    pub component: ComponentId,
    pub label: Option<String>,
    /// The definition terminal this one was created from (LIB-02).
    pub key: Option<String>,
}

/// What a representation depicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Subject {
    Component(ComponentId),
    Conductor(ConductorId),
    Cable(CableId),
}

/// The two representation forms (proposal §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Form {
    /// Page items in page coordinates; terminal positions stored as connection points.
    Drawn,
    /// One symbol-instance item; terminal positions derived from its placement.
    Symbol,
}

/// A depiction of a subject on one page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Representation {
    pub subject: Subject,
    pub page: PageId,
    pub form: Form,
    /// Free text such as `main` or `route`.
    pub role: String,
    pub items: Vec<ItemId>,
    /// Drawn form: where each terminal sits on the page. Symbol form (brief 018, decision on
    /// I14): empty for a symbol of the component's own definition, whose terminals are matched to
    /// the definition by key; otherwise where each terminal sits in the **definition's local
    /// frame**, so the terminals follow the instance's placement.
    pub connection_points: BTreeMap<TerminalId, Pt>,
}

/// One end of a conductor. End A is the first route vertex, end B the last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum End {
    A,
    B,
}

impl End {
    pub const BOTH: [End; 2] = [End::A, End::B];
    fn index(self) -> usize {
        match self {
            End::A => 0,
            End::B => 1,
        }
    }
}

/// A conductor (stored intent). Attachments are references (DOC-03).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Conductor {
    pub properties: Properties,
    pub ends: [Option<TerminalId>; 2],
}

impl Conductor {
    pub fn end(&self, end: End) -> Option<TerminalId> {
        self.ends.get(end.index()).copied().flatten()
    }
    pub fn set_end(&mut self, end: End, t: Option<TerminalId>) {
        if let Some(slot) = self.ends.get_mut(end.index()) {
            *slot = t;
        }
    }
}

/// A conductor's membership of a cable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CableMember {
    pub conductor: ConductorId,
    pub core: Option<String>,
}

/// A cable groups existing conductors (DOC-04).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Cable {
    pub properties: Properties,
    pub members: Vec<CableMember>,
}

/// SHA-256 of a definition's canonical form (proposal §20.10), as 64 lower-case hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// The 64-hex-digit form.
    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Parse the 64-hex-digit form.
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.as_bytes();
        if t.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, slot) in out.iter_mut().enumerate() {
            let pair = std::str::from_utf8(t.get(2 * i..2 * i + 2)?).ok()?;
            *slot = u8::from_str_radix(pair, 16).ok()?;
        }
        Some(Self(out))
    }
}

impl Serialize for ContentHash {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("content hash must be 64 hex digits"))
    }
}

/// Where an embedded definition came from (LIB-01, proposal §20.9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Origin {
    /// The library's stable identity.
    pub library: LibraryId,
    /// The definition's ID inside that library.
    pub definition: DefinitionId,
    /// The library transaction that last changed the definition (its version).
    pub library_seq: u64,
    /// Hash of the definition content as embedded.
    pub content_hash: ContentHash,
}

/// Units of lengths and coordinates in the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LengthUnit {
    #[default]
    Millimetre,
    Inch,
}

/// Project-wide settings (proposal §3.2, §20.13). Set at creation; changed only by a recorded op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSettings {
    pub length_unit: LengthUnit,
    /// How many digits automatic tag numbers are padded to (brief 018, owner decision
    /// 2026-11-08): [`DEFAULT_TAG_PADDING`] unless the project says otherwise. Values above
    /// [`crate::tag::MAX_PADDING`] count as that maximum.
    pub tag_padding: u8,
    /// The project's own tag prefix per starter symbol key (`"coil" → "KA"`), overriding the
    /// built-in IEC 81346 default. Empty by default.
    pub tag_prefixes: BTreeMap<String, String>,
}

/// The tag padding of a project that sets none (owner decision 2026-11-08).
pub const DEFAULT_TAG_PADDING: u8 = 2;

impl Default for ProjectSettings {
    fn default() -> Self {
        Self { length_unit: LengthUnit::default(), tag_padding: DEFAULT_TAG_PADDING, tag_prefixes: BTreeMap::new() }
    }
}

/// A terminal of a definition, in the symbol's local frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalDef {
    pub key: String,
    pub label: Option<String>,
    pub at: Pt,
}

/// A project-local copy of a library definition (§4.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub origin: Option<Origin>,
    pub graphics: Vec<Primitive>,
    pub terminals: Vec<TerminalDef>,
}

/// The authoritative document: everything stored, nothing derived.
///
/// # Semantics
/// Maps are ordered by ID so iteration, serialisation and comparison are deterministic.
/// [`Document::check`] verifies every invariant and [`Document::check_touched`] those a change
/// could have affected; [`crate::ledger::Ledger`] runs the scoped check after each transaction so
/// an invalid state is never committed (see [`crate::invariant`]).
///
/// The document also keeps reverse indexes (the private `index` module) for queries such as
/// [`Document::cables_of`]. They are derived: not serialised, not compared by `==`, and kept up
/// to date by [`crate::Op::apply`]. Code that edits the public maps directly must call
/// [`Document::rebuild_index`] before querying.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(from = "DocumentData")]
pub struct Document {
    pub pages: BTreeMap<PageId, Page>,
    pub page_order: Vec<PageId>,
    pub layers: BTreeMap<LayerId, Layer>,
    pub items: BTreeMap<ItemId, Item>,
    pub components: BTreeMap<ComponentId, Component>,
    pub terminals: BTreeMap<TerminalId, Terminal>,
    pub representations: BTreeMap<RepresentationId, Representation>,
    pub conductors: BTreeMap<ConductorId, Conductor>,
    pub cables: BTreeMap<CableId, Cable>,
    pub definitions: BTreeMap<DefinitionId, Definition>,
    pub settings: ProjectSettings,
    /// Named versions (bookmarks): name → the `seq` it names (§6.1). History, never pruned.
    pub versions: BTreeMap<String, u64>,
    /// Derived reverse indexes; never serialised.
    #[serde(skip)]
    pub(crate) index: Index,
}

/// The serialised form of [`Document`]: its stored fields, without the derived index.
#[derive(Deserialize)]
struct DocumentData {
    pages: BTreeMap<PageId, Page>,
    page_order: Vec<PageId>,
    layers: BTreeMap<LayerId, Layer>,
    items: BTreeMap<ItemId, Item>,
    components: BTreeMap<ComponentId, Component>,
    terminals: BTreeMap<TerminalId, Terminal>,
    representations: BTreeMap<RepresentationId, Representation>,
    conductors: BTreeMap<ConductorId, Conductor>,
    cables: BTreeMap<CableId, Cable>,
    definitions: BTreeMap<DefinitionId, Definition>,
    settings: ProjectSettings,
    versions: BTreeMap<String, u64>,
}

impl From<DocumentData> for Document {
    fn from(d: DocumentData) -> Self {
        let DocumentData {
            pages,
            page_order,
            layers,
            items,
            components,
            terminals,
            representations,
            conductors,
            cables,
            definitions,
            settings,
            versions,
        } = d;
        let mut doc = Document {
            pages,
            page_order,
            layers,
            items,
            components,
            terminals,
            representations,
            conductors,
            cables,
            definitions,
            settings,
            versions,
            index: Index::default(),
        };
        doc.rebuild_index();
        doc
    }
}

/// Documents are equal when their stored objects are equal; the derived index is not compared.
impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        let Document {
            pages,
            page_order,
            layers,
            items,
            components,
            terminals,
            representations,
            conductors,
            cables,
            definitions,
            settings,
            versions,
            index: _,
        } = self;
        *pages == other.pages
            && *page_order == other.page_order
            && *layers == other.layers
            && *items == other.items
            && *components == other.components
            && *terminals == other.terminals
            && *representations == other.representations
            && *conductors == other.conductors
            && *cables == other.cables
            && *definitions == other.definitions
            && *settings == other.settings
            && *versions == other.versions
    }
}

impl Document {
    /// Recompute the reverse indexes from the stored objects.
    ///
    /// # Semantics
    /// Needed only after changing the public maps directly; [`crate::Op::apply`] keeps the index
    /// current on its own.
    pub fn rebuild_index(&mut self) {
        self.index = Index::build(self);
    }

    /// Whether the kept index equals one rebuilt from the stored objects.
    ///
    /// # Errors
    /// [`ModelError::Invariant`] when the index is stale.
    pub fn check_index(&self) -> Result<()> {
        if self.index == Index::build(self) {
            Ok(())
        } else {
            Err(ModelError::Invariant("the reverse indexes differ from the stored objects".into()))
        }
    }

    /// The representation an item belongs to, if any (DOC-05 allows at most one).
    pub fn representation_of_item(&self, item: ItemId) -> Option<(RepresentationId, &Representation)> {
        referrers(&self.index.item_reps, &item).find_map(|id| self.representations.get(&id).map(|r| (id, r)))
    }

    /// Representations of `subject`, in ID order.
    pub fn representations_of(&self, subject: Subject) -> impl Iterator<Item = (RepresentationId, &Representation)> {
        referrers(&self.index.subject_reps, &subject).filter_map(|id| self.representations.get(&id).map(|r| (id, r)))
    }

    /// Representations on `page`, in ID order (through the page index, without going through the
    /// whole document).
    pub fn representations_on(&self, page: PageId) -> impl Iterator<Item = (RepresentationId, &Representation)> {
        referrers(&self.index.page_reps, &page).filter_map(|id| self.representations.get(&id).map(|r| (id, r)))
    }

    /// Terminals of `component`, in ID order.
    pub fn terminals_of(&self, component: ComponentId) -> impl Iterator<Item = (TerminalId, &Terminal)> {
        referrers(&self.index.component_terminals, &component).filter_map(|id| self.terminals.get(&id).map(|t| (id, t)))
    }

    /// Conductor ends attached to `terminal`, in conductor ID order, end A before end B.
    pub fn ends_at(&self, terminal: TerminalId) -> Vec<(ConductorId, End)> {
        referrers(&self.index.terminal_ends, &terminal).collect()
    }

    /// Components whose tag is the same tag as `tag` (equal [`crate::tag::key`]), in ID order,
    /// through the tag index: the cost does not grow with the project.
    pub fn components_tagged(&self, tag: &str) -> Vec<ComponentId> {
        referrers(&self.index.tags, &crate::tag::key(tag)).collect()
    }

    /// Every tagged component through the tag index (brief 018), by tag key ([`crate::tag::key`]),
    /// then ID: no scan of the components.
    pub fn tagged(&self) -> impl Iterator<Item = (&str, ComponentId)> {
        self.index.tags.iter().flat_map(|(k, ids)| ids.keys().map(move |c| (k.as_str(), *c)))
    }

    /// Cables `conductor` is a member of, in ID order.
    pub fn cables_of(&self, conductor: ConductorId) -> Vec<CableId> {
        referrers(&self.index.conductor_cables, &conductor).collect()
    }
}
