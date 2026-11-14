//! The document's objects and their relationships (proposal §3 and §4).
//!
//! Stored intent (components, terminals, conductors, cables, definitions) and drawing content
//! (pages, layers, items, representations) live here. Derived results do not (see [`crate::derive`]).

use std::collections::BTreeMap;

use crate::index::{Index, referrers};

use serde::{Deserialize, Serialize};

use crate::error::{ModelError, Result};
use crate::geom::{Placement, Pt};
use crate::id::{CableId, ComponentId, ConductorId, DefinitionId, FolderId, ItemId, LayerId, LibraryId, PageId, RepresentationId, TerminalId};

/// Optional, free-form properties. A missing key is `Unset` (principle P7); an empty string is a
/// confirmed empty value.
pub type Properties = BTreeMap<String, String>;

/// A page of the project.
///
/// Its `properties` (brief 019, decision FP1) hold the page number (`number`), the `=`/`+`
/// overrides (`function`, `location`), its own sheet (`sheet`) and its folder (`folder`); see
/// [`crate::book`] and [`crate::sheet`]. Left out of records while empty.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Page {
    pub title: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: Properties,
}

impl Page {
    /// A page titled `title`, with no properties.
    pub fn new(title: impl Into<String>) -> Self {
        Page { title: title.into(), properties: Properties::new() }
    }
}

/// A folder of pages (brief 019, decision FP3): a name and a place in the tree. Pages name their
/// folder in their `folder` property; sibling order is the relative order in
/// [`Document::folder_order`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub name: String,
    /// The folder it is in; `None` at the top level.
    pub parent: Option<FolderId>,
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
    /// Single-line text in the standard style. `at` is its alignment point: the left end of the
    /// baseline unless `halign`/`valign` say otherwise (brief 031, FP4; both are left out of the
    /// file at left/baseline, so older files read and save the same).
    Text {
        at: Pt,
        height: f64,
        rotation: f64,
        value: String,
        #[serde(default, skip_serializing_if = "is_default")]
        halign: TextHAlign,
        #[serde(default, skip_serializing_if = "is_default")]
        valign: TextVAlign,
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
            Primitive::Line { a, b } => Some(if end == End::A { Primitive::Line { a: to, b: *b } } else { Primitive::Line { a: *a, b: to } }),
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
            Primitive::Arc { center, radius, start, end } => center.is_finite() && radius.is_finite() && start.is_finite() && end.is_finite(),
            Primitive::Circle { center, radius } => center.is_finite() && radius.is_finite(),
            Primitive::Text { at, height, rotation, .. } => at.is_finite() && height.is_finite() && rotation.is_finite(),
            Primitive::Point { at } => at.is_finite(),
        }
    }
}

/// A text's horizontal alignment (brief 031, FP4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextHAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// A text's vertical alignment (brief 031, FP4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextVAlign {
    #[default]
    Baseline,
    Bottom,
    Middle,
    Top,
}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    *t == T::default()
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
    /// How wires are numbered and what each potential defaults to (brief 022, FP1). Left out of
    /// the file while it holds [`WiringSettings::default`].
    pub wiring: crate::wiring::WiringSettings,
    /// The project's sheet: size and grid of every page's frame unless a page sets its own
    /// (brief 019, decision FP2). A3 landscape, `A`–`H` by `1`–`6` by default.
    pub sheet: crate::sheet::SheetSpec,
    /// What a new page's number adds to the number before it (design 019 §6.1, owner answer
    /// Q11): 1 by default.
    pub page_number_step: u32,
    /// How cross-references name a place (owner answer Q1, 2026-11-08): `105-C-4` by default.
    pub reference_style: ReferenceStyle,
    /// Where a device's contact mirror table goes unless the device says otherwise (brief 025,
    /// FP3): `auto` by default, and then left out of every file.
    #[serde(skip_serializing_if = "ContactMirror::is_auto")]
    pub contact_mirror: ContactMirror,
}

/// Where the contact mirror table under a coil goes by default (brief 025, FP3). A device's own
/// `mirror` property overrides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactMirror {
    /// Under the rail the coil's lower wire meets, else under the coil.
    #[default]
    Auto,
    /// Under the coil.
    Below,
    /// Right of the coil.
    Right,
    /// No tables (texts under contacts remain).
    Off,
}

impl ContactMirror {
    /// Whether it is the default (`auto`), which files leave out.
    pub fn is_auto(&self) -> bool {
        *self == ContactMirror::Auto
    }

    /// Its word: `auto`, `below`, `right` or `off`.
    pub fn word(self) -> &'static str {
        match self {
            ContactMirror::Auto => "auto",
            ContactMirror::Below => "below",
            ContactMirror::Right => "right",
            ContactMirror::Off => "off",
        }
    }

    /// The placement a word names (case-insensitive), if any.
    pub fn parse(word: &str) -> Option<ContactMirror> {
        [ContactMirror::Auto, ContactMirror::Below, ContactMirror::Right, ContactMirror::Off]
            .into_iter()
            .find(|m| m.word().eq_ignore_ascii_case(word.trim()))
    }
}

/// How a cross-reference names a page and a place on it (owner answer Q1, 2026-11-08; UI round 2
/// and rev 3: `2/B5`). The zone's letter comes first whichever axis is lettered
/// ([`crate::derive::place_text`]). Other styles (US) come later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ReferenceStyle {
    /// `105-B-3`: page number, then the zone, letter first.
    PageColumnRow,
    /// `/105.3`: page number and column.
    PageColumn,
    /// `105/B3`: page number, then the zone, letter first (UI round 2 and rev 3; the default of a
    /// new project).
    #[default]
    PageZone,
}

/// The page number step of a project that sets none.
pub const DEFAULT_PAGE_NUMBER_STEP: u32 = 1;

/// The tag padding of a project that sets none (owner decision 2026-11-08).
pub const DEFAULT_TAG_PADDING: u8 = 2;

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            length_unit: LengthUnit::default(),
            tag_padding: DEFAULT_TAG_PADDING,
            tag_prefixes: BTreeMap::new(),
            wiring: crate::wiring::WiringSettings::default(),
            sheet: crate::sheet::SheetSpec::default(),
            page_number_step: DEFAULT_PAGE_NUMBER_STEP,
            reference_style: ReferenceStyle::default(),
            contact_mirror: ContactMirror::default(),
        }
    }
}

/// A terminal of a definition, in the symbol's local frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalDef {
    pub key: String,
    pub label: Option<String>,
    pub at: Pt,
}

/// Where a device text of a placed copy goes, in the symbol's frame (brief 048, FP1; proposal
/// §4.2's text slots). An anchor only: size and style come from the text styles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextSlot {
    /// The device text it places: one of [`TextSlot::TEXTS`], or `rating.<label>`.
    pub text: String,
    pub at: Pt,
    #[serde(default, skip_serializing_if = "is_default")]
    pub h: TextHAlign,
    #[serde(default, skip_serializing_if = "is_default")]
    pub v: TextVAlign,
    /// Radians, counter-clockwise.
    #[serde(default, skip_serializing_if = "is_default")]
    pub rotation: f64,
}

impl TextSlot {
    /// The device texts a slot may place (design 048 §2.3), besides `rating.<label>`.
    pub const TEXTS: [&'static str; 8] = ["tag", "description", "rating", "part_number", "maker", "article_number", "function", "location"];

    /// Whether `text` names a device text a slot may place.
    pub fn is_known_text(text: &str) -> bool {
        Self::TEXTS.contains(&text)
            || text.strip_prefix("rating.").is_some_and(|l| !l.is_empty() && l.len() <= 64 && !l.chars().any(char::is_control))
    }
}

/// The keys a definition's [`Definition::defaults`] may hold (brief 048, FP2; design §2.4).
pub const DEFINITION_DEFAULT_KEYS: [&str; 7] = ["key", "prefix", "description", "rating", "join", "aliases", "family"];

/// A project-local copy of a library definition (§4.2).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub origin: Option<Origin>,
    pub graphics: Vec<Primitive>,
    pub terminals: Vec<TerminalDef>,
    /// Groups of terminal keys joined inside the device (brief 022, FP5): every terminal of one
    /// group is on one net. Empty for almost every definition, and then left out of the file so
    /// existing content hashes do not change. A bridge may name a key the placement writes rather
    /// than one of the definition's own (brief 044, FP2: a connector half bridges the side keys
    /// `.s` and `.p`; [`crate::invariant::is_placement_key`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bridges: Vec<Vec<String>>,
    /// Where the device texts of a placed copy go (brief 048, FP1). Empty for the starters, whose
    /// anchors are catalogue code; left out of the file when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<TextSlot>,
    /// The symbol's own default values (brief 048, FP2), keyed by [`DEFINITION_DEFAULT_KEYS`].
    /// Empty for the starters; left out of the file when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub defaults: BTreeMap<String, String>,
    /// The origin of the definition an edited copy came from (brief 048, FP4). Never followed by
    /// the reader, so a cycle cannot loop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<Origin>,
}

impl Definition {
    /// The keys bridged to `key` inside the device (not `key` itself), from [`Definition::bridges`].
    pub fn bridged_with<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.bridges.iter().filter(move |g| g.iter().any(|k| k == key)).flat_map(|g| g.iter().map(String::as_str)).filter(move |k| *k != key)
    }
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
    /// The project's own properties (brief 019, decision FP1): title, drawing number, customer,
    /// status, default function and location, revisions (`rev.<n>.<field>`) and free ones.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: Properties,
    /// Folders of pages (brief 019, decision FP3).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub folders: BTreeMap<FolderId, Folder>,
    /// Every folder once; siblings are ordered as they appear here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub folder_order: Vec<FolderId>,
    /// The project's standard (brief 050a, FP1); `None` is IEC (built in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard: Option<crate::style::Standard>,
    /// The project layer of style records (brief 050a, FP2).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub styles: crate::style::StyleCatalogues,
    /// Derived reverse indexes; never serialised.
    #[serde(skip)]
    pub(crate) index: Index,
    /// Derived nets (brief 022), kept by [`crate::Op::apply`]; never serialised.
    #[serde(skip)]
    pub(crate) conn: crate::connectivity::Connectivity,
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
    #[serde(default)]
    properties: Properties,
    #[serde(default)]
    folders: BTreeMap<FolderId, Folder>,
    #[serde(default)]
    folder_order: Vec<FolderId>,
    #[serde(default)]
    standard: Option<crate::style::Standard>,
    #[serde(default)]
    styles: crate::style::StyleCatalogues,
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
            properties,
            folders,
            folder_order,
            standard,
            styles,
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
            properties,
            folders,
            folder_order,
            standard,
            styles,
            index: Index::default(),
            conn: crate::connectivity::Connectivity::default(),
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
            properties,
            folders,
            folder_order,
            standard,
            styles,
            index: _,
            conn: _,
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
            && *properties == other.properties
            && *folders == other.folders
            && *folder_order == other.folder_order
            && *standard == other.standard
            && *styles == other.styles
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
        self.conn = crate::connectivity::Connectivity::build(self);
    }

    /// Whether the index built on threads (as on open) equals one built on a single thread.
    /// For tests.
    #[doc(hidden)]
    pub fn index_build_is_deterministic(&self) -> bool {
        Index::build(self) == Index::build_serial(self)
    }

    /// Whether the kept index equals one rebuilt from the stored objects.
    ///
    /// # Errors
    /// [`ModelError::Invariant`] when the index is stale.
    pub fn check_index(&self) -> Result<()> {
        if self.index != Index::build(self) {
            return Err(ModelError::Invariant("the reverse indexes differ from the stored objects".into()));
        }
        // The kept nets (brief 022) are derived the same way: they must equal a fresh build.
        if let Some(diff) = self.conn.differs_from(&crate::connectivity::Connectivity::build(self)) {
            return Err(ModelError::Invariant(format!("the kept nets differ from a fresh build: {diff}")));
        }
        Ok(())
    }

    /// The nets, kept up to date by every op (brief 022).
    pub fn connectivity(&self) -> &crate::connectivity::Connectivity {
        &self.conn
    }

    /// Continuation components whose name has the key `key` ([`crate::connectivity::name_key`]),
    /// in ID order, through the name index.
    pub fn continuations_named(&self, key: &str) -> Vec<ComponentId> {
        referrers(&self.index.continuations, &key.to_string()).collect()
    }

    /// Every continuation name key and its components, through the name index.
    pub fn continuation_names(&self) -> impl Iterator<Item = (&str, ComponentId)> {
        self.index.continuations.iter().flat_map(|(k, ids)| ids.keys().map(move |c| (k.as_str(), *c)))
    }

    /// Components made from `definition`, in ID order.
    pub fn components_using(&self, definition: DefinitionId) -> Vec<ComponentId> {
        referrers(&self.index.definition_components, &definition).collect()
    }

    /// Symbol items placing `definition`, in ID order (brief 048: a definition's placed copies).
    pub fn items_using(&self, definition: DefinitionId) -> Vec<ItemId> {
        referrers(&self.index.definition_items, &definition).collect()
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

    /// Representations that show `terminal` through an explicit connection point, in ID order
    /// (brief 036: a level terminal is always shown that way).
    pub fn representations_showing(&self, terminal: TerminalId) -> impl Iterator<Item = (RepresentationId, &Representation)> {
        referrers(&self.index.terminal_reps, &terminal).filter_map(|id| self.representations.get(&id).map(|r| (id, r)))
    }

    /// The definition a symbol representation places (its one symbol instance), if any.
    pub fn rep_definition(&self, r: &Representation) -> Option<&Definition> {
        r.items.iter().find_map(|i| match &self.items.get(i)?.kind {
            ItemKind::Symbol(s) => self.definitions.get(&s.definition),
            ItemKind::Primitive(_) => None,
        })
    }

    /// Terminals of `component`, in ID order.
    pub fn terminals_of(&self, component: ComponentId) -> impl Iterator<Item = (TerminalId, &Terminal)> {
        referrers(&self.index.component_terminals, &component).filter_map(|id| self.terminals.get(&id).map(|t| (id, t)))
    }

    /// Whether any conductor end is attached to `terminal`, through the index, without listing them.
    pub fn has_ends(&self, terminal: TerminalId) -> bool {
        self.index.terminal_ends.get(&terminal).is_some_and(|m| !m.is_empty())
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

    /// Cables whose tag is the same tag as `tag` (brief 026), in ID order, through the tag index.
    pub fn cables_tagged(&self, tag: &str) -> Vec<CableId> {
        referrers(&self.index.cable_tags, &crate::tag::key(tag)).collect()
    }

    /// Every tagged cable through the tag index (brief 026), by tag key, then ID.
    pub fn tagged_cables(&self) -> impl Iterator<Item = (&str, CableId)> {
        self.index.cable_tags.iter().flat_map(|(k, ids)| ids.keys().map(move |c| (k.as_str(), *c)))
    }

    /// Tag keys starting with `start` held by devices or cables (brief 026), through a range of
    /// the tag indexes: the cost follows the keys with that start, not the project.
    pub fn tag_keys_starting(&self, start: &str) -> impl Iterator<Item = &str> {
        let from = start.to_string();
        let a = self.index.tags.range(from.clone()..).map(|(k, _)| k.as_str());
        let b = self.index.cable_tags.range(from..).map(|(k, _)| k.as_str());
        let s1 = start.to_string();
        let s2 = start.to_string();
        a.take_while(move |k| k.starts_with(s1.as_str())).chain(b.take_while(move |k| k.starts_with(s2.as_str())))
    }

    /// Whether a device or a cable holds `tag` (brief 026: one namespace).
    pub fn tag_taken(&self, tag: &str) -> bool {
        let k = crate::tag::key(tag);
        self.index.tags.get(&k).is_some_and(|m| !m.is_empty()) || self.index.cable_tags.get(&k).is_some_and(|m| !m.is_empty())
    }

    /// Pages whose `folder` property names `folder`, in ID order (through the folder index).
    pub fn pages_in_folder(&self, folder: FolderId) -> Vec<PageId> {
        referrers(&self.index.folder_pages, &folder).collect()
    }

    /// Folders whose parent is `folder`, in ID order (through the folder index).
    pub fn subfolders(&self, folder: FolderId) -> Vec<FolderId> {
        referrers(&self.index.folder_children, &folder).collect()
    }

    /// Objects whose own style choice `key` names the stored style ID `value` (brief 050a, the
    /// style's explicit users), in order, through the index: no scan of the project.
    pub fn style_users(&self, key: &str, value: &str) -> impl Iterator<Item = crate::op::ObjectRef> + '_ {
        referrers(&self.index.style_users, &(value.trim().to_string(), key.to_string()))
    }

    /// Every style choice an object holds, through the index (brief 050a): (stored style ID,
    /// choice key, object), by ID then key.
    pub fn style_choices(&self) -> impl Iterator<Item = (&str, &str, crate::op::ObjectRef)> + '_ {
        self.index.style_users.iter().flat_map(|((v, k), objs)| objs.keys().map(move |o| (v.as_str(), k.as_str(), *o)))
    }

    /// Every (stored style ID, choice key) with at least one explicit user, and its users, from
    /// `value` on (brief 050a), through the index.
    pub fn style_users_of(&self, value: &str) -> impl Iterator<Item = (&str, crate::op::ObjectRef)> + '_ {
        let v = value.trim().to_string();
        self.index
            .style_users
            .range((v.clone(), String::new())..)
            .take_while(move |((sv, _), _)| *sv == v)
            .flat_map(|((_, k), objs)| objs.keys().map(move |o| (k.as_str(), *o)))
    }

    /// Cables `conductor` is a member of, in ID order.
    pub fn cables_of(&self, conductor: ConductorId) -> Vec<CableId> {
        referrers(&self.index.conductor_cables, &conductor).collect()
    }
}
