//! Schema version 1 records (proposal §20.2-§20.3): the file format's own types.
//!
//! These mirror `ecad-model` deliberately: the model may change shape freely, while these types
//! change only with a schema version and an upcast. Every struct refuses unknown fields (E5).
//! IDs use the model's ID types, whose serialised form (32 hex digits) is itself part of this
//! schema and fixed by the golden-file test.

use std::collections::BTreeMap;

use ecad_model as m;
use ecad_model::{CableId, ComponentId, ConductorId, DefinitionId, FolderId, ItemId, LayerId, LibraryId, PageId, RepresentationId, TerminalId, TxId};
use serde::{Deserialize, Serialize};

/// A point `[x, y]`.
pub type Pt = [f64; 2];

fn pt(p: m::Pt) -> Pt {
    [p.x, p.y]
}
fn mpt(p: Pt) -> m::Pt {
    m::Pt::new(p[0], p[1])
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub title: String,
    /// Brief 019 (decision FP1): left out while empty, so a page without properties is written
    /// exactly as before.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
}

/// A folder of pages (brief 019, decision FP3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub name: String,
    pub parent: Option<FolderId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GridLabels {
    Letters,
    Numbers,
}

/// A sheet (brief 019, decision FP2), in whole millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetSpec {
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub rows: u32,
    pub column_labels: GridLabels,
    pub row_labels: GridLabels,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceStyle {
    PageColumnRow,
    PageColumn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Color {
    ByLayer,
    ByBlock,
    Index(u8),
    Rgb(u32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    pub layer: LayerId,
    pub color: Color,
    pub linetype: String,
    pub lineweight: i16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Primitive {
    Line { a: Pt, b: Pt },
    Polyline { vertices: Vec<Pt>, closed: bool },
    Arc { center: Pt, radius: f64, start: f64, end: f64 },
    Circle { center: Pt, radius: f64 },
    Text { at: Pt, height: f64, rotation: f64, value: String },
    Point { at: Pt },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub origin: Pt,
    /// Radians, counter-clockwise.
    pub rotation: f64,
    pub mirror: bool,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolInstance {
    pub definition: DefinitionId,
    pub placement: Placement,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ItemKind {
    Primitive(Primitive),
    Symbol(SymbolInstance),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub page: PageId,
    pub style: Style,
    pub kind: ItemKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub properties: BTreeMap<String, String>,
    pub definition: Option<DefinitionId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Terminal {
    pub component: ComponentId,
    pub label: Option<String>,
    pub key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Subject {
    Component(ComponentId),
    Conductor(ConductorId),
    Cable(CableId),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Form {
    Drawn,
    Symbol,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Representation {
    pub subject: Subject,
    pub page: PageId,
    pub form: Form,
    pub role: String,
    pub items: Vec<ItemId>,
    pub connection_points: BTreeMap<TerminalId, Pt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum End {
    A,
    B,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ends {
    pub a: Option<TerminalId>,
    pub b: Option<TerminalId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conductor {
    pub properties: BTreeMap<String, String>,
    pub ends: Ends,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CableMember {
    pub conductor: ConductorId,
    pub core: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cable {
    pub properties: BTreeMap<String, String>,
    pub members: Vec<CableMember>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub library: LibraryId,
    pub definition: DefinitionId,
    pub library_seq: u64,
    pub content_hash: m::ContentHash,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalDef {
    pub key: String,
    pub label: Option<String>,
    pub at: Pt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub name: String,
    pub origin: Option<Origin>,
    pub graphics: Vec<Primitive>,
    pub terminals: Vec<TerminalDef>,
    /// Groups of terminal keys joined inside the device (brief 022, FP5). Left out when empty, so
    /// a definition without bridges is written, and hashed, exactly as before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bridges: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    Millimetre,
    Inch,
}

/// Project settings. `tag_padding` and `tag_prefixes` (brief 018, owner decision 2026-11-08) are
/// left out while they hold their defaults, so a project that never changes them is written
/// exactly as before; a file that sets them is refused by older builds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSettings {
    pub length_unit: LengthUnit,
    #[serde(default = "default_padding", skip_serializing_if = "is_default_padding")]
    pub tag_padding: u8,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tag_prefixes: BTreeMap<String, String>,
    /// Wire numbering and potential defaults (brief 022, FP1): left out at its default, so a
    /// project that never changes them is written exactly as before.
    #[serde(default, skip_serializing_if = "is_default_wiring")]
    pub wiring: WiringSettings,
    /// Brief 019 (decision FP2): left out at the default A3 sheet.
    #[serde(default = "default_sheet", skip_serializing_if = "is_default_sheet")]
    pub sheet: SheetSpec,
    /// Brief 019 (decision FP2): left out at the default step 1.
    #[serde(default = "default_step", skip_serializing_if = "is_default_step")]
    pub page_number_step: u32,
    /// Brief 019 (owner answer Q1): left out at the default `page_column_row`.
    #[serde(default = "default_style", skip_serializing_if = "is_default_style")]
    pub reference_style: ReferenceStyle,
}

fn is_default_wiring(w: &WiringSettings) -> bool {
    *w == WiringSettings::from(&m::wiring::WiringSettings::default())
}

/// How automatic wire numbers are made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scheme {
    #[serde(rename = "potential")]
    Potential,
    #[serde(rename = "page")]
    Page,
    #[serde(rename = "page-column")]
    PageColumn,
}

/// One row of the potentials table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PotentialDef {
    pub name: String,
    pub prefix: String,
    pub numbered: bool,
    pub colour: String,
    pub cross_section: String,
}

/// How label colours are written (brief 022 fix round).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColourStyle {
    #[serde(rename = "iec")]
    Iec,
    #[serde(rename = "three-letter")]
    ThreeLetter,
}

fn is_iec(s: &ColourStyle) -> bool {
    *s == ColourStyle::Iec
}

fn iec() -> ColourStyle {
    ColourStyle::Iec
}

fn default_label_height() -> f64 {
    m::wiring::DEFAULT_LABEL_HEIGHT
}

fn is_default_label_height(h: &f64) -> bool {
    *h == m::wiring::DEFAULT_LABEL_HEIGHT
}

/// Wiring settings (brief 022). `colour_style` and `label_height` (fix round, 2026-11-09) are left
/// out at their defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WiringSettings {
    pub scheme: Scheme,
    pub digits: u8,
    pub unclassified_prefix: String,
    pub potentials: Vec<PotentialDef>,
    #[serde(default = "iec", skip_serializing_if = "is_iec")]
    pub colour_style: ColourStyle,
    #[serde(default = "default_label_height", skip_serializing_if = "is_default_label_height")]
    pub label_height: f64,
}

impl Default for WiringSettings {
    fn default() -> Self {
        WiringSettings::from(&m::wiring::WiringSettings::default())
    }
}

impl From<&m::wiring::WiringSettings> for WiringSettings {
    fn from(w: &m::wiring::WiringSettings) -> Self {
        WiringSettings {
            scheme: match w.scheme {
                m::wiring::Scheme::Potential => Scheme::Potential,
                m::wiring::Scheme::Page => Scheme::Page,
                m::wiring::Scheme::PageColumn => Scheme::PageColumn,
            },
            digits: w.digits,
            unclassified_prefix: w.unclassified_prefix.clone(),
            potentials: w
                .potentials
                .iter()
                .map(|p| PotentialDef {
                    name: p.name.clone(),
                    prefix: p.prefix.clone(),
                    numbered: p.numbered,
                    colour: p.colour.clone(),
                    cross_section: p.cross_section.clone(),
                })
                .collect(),
            colour_style: match w.colour_style {
                m::wiring::ColourStyle::Iec => ColourStyle::Iec,
                m::wiring::ColourStyle::ThreeLetter => ColourStyle::ThreeLetter,
            },
            label_height: w.label_height,
        }
    }
}

fn default_sheet() -> SheetSpec {
    (&m::SheetSpec::default()).into()
}

fn is_default_sheet(s: &SheetSpec) -> bool {
    *s == default_sheet()
}

fn default_step() -> u32 {
    m::DEFAULT_PAGE_NUMBER_STEP
}

fn is_default_step(s: &u32) -> bool {
    *s == m::DEFAULT_PAGE_NUMBER_STEP
}

fn default_style() -> ReferenceStyle {
    ReferenceStyle::PageColumnRow
}

fn is_default_style(s: &ReferenceStyle) -> bool {
    *s == ReferenceStyle::PageColumnRow
}

fn labels(l: m::GridLabels) -> GridLabels {
    match l {
        m::GridLabels::Letters => GridLabels::Letters,
        m::GridLabels::Numbers => GridLabels::Numbers,
    }
}

fn mlabels(l: GridLabels) -> m::GridLabels {
    match l {
        GridLabels::Letters => m::GridLabels::Letters,
        GridLabels::Numbers => m::GridLabels::Numbers,
    }
}

impl From<&m::SheetSpec> for SheetSpec {
    fn from(s: &m::SheetSpec) -> Self {
        SheetSpec {
            width: s.width,
            height: s.height,
            columns: s.columns,
            rows: s.rows,
            column_labels: labels(s.column_labels),
            row_labels: labels(s.row_labels),
        }
    }
}

impl From<WiringSettings> for m::wiring::WiringSettings {
    fn from(w: WiringSettings) -> Self {
        m::wiring::WiringSettings {
            scheme: match w.scheme {
                Scheme::Potential => m::wiring::Scheme::Potential,
                Scheme::Page => m::wiring::Scheme::Page,
                Scheme::PageColumn => m::wiring::Scheme::PageColumn,
            },
            digits: w.digits,
            unclassified_prefix: w.unclassified_prefix,
            potentials: w
                .potentials
                .into_iter()
                .map(|p| m::wiring::PotentialDef {
                    name: p.name,
                    prefix: p.prefix,
                    numbered: p.numbered,
                    colour: p.colour,
                    cross_section: p.cross_section,
                })
                .collect(),
            colour_style: match w.colour_style {
                ColourStyle::Iec => m::wiring::ColourStyle::Iec,
                ColourStyle::ThreeLetter => m::wiring::ColourStyle::ThreeLetter,
            },
            label_height: w.label_height,
        }
    }
}

impl From<SheetSpec> for m::SheetSpec {
    fn from(s: SheetSpec) -> Self {
        m::SheetSpec {
            width: s.width,
            height: s.height,
            columns: s.columns,
            rows: s.rows,
            column_labels: mlabels(s.column_labels),
            row_labels: mlabels(s.row_labels),
        }
    }
}

fn default_padding() -> u8 {
    m::DEFAULT_TAG_PADDING
}

fn is_default_padding(p: &u8) -> bool {
    *p == m::DEFAULT_TAG_PADDING
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ObjectRef {
    Component(ComponentId),
    Conductor(ConductorId),
    Cable(CableId),
    /// Brief 019 (decision FP1).
    Page(PageId),
    /// Brief 019 (decision FP1).
    Project,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Author {
    User,
    Agent(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TxKind {
    Edit,
    Undo(TxId),
    Redo(TxId),
    /// A merge (brief 020b, FQ1): `{"merge": {"source": hex, "source_head": {seq, chain},
    /// "ancestor": {branch, seq, chain}}}`. An older reader refuses the unknown kind.
    Merge(MergeKind),
}

/// Where a merge came from, as recorded (hex IDs and chain values).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeKind {
    pub source: Hex<16>,
    pub source_head: MergeStep,
    pub ancestor: MergeAncestor,
}

/// A step of the source: its `seq` and chain value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeStep {
    pub seq: u64,
    pub chain: Hex<32>,
}

/// The common ancestor: a branch, its step and chain value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeAncestor {
    pub branch: Hex<16>,
    pub seq: u64,
    pub chain: Hex<32>,
}

/// Bytes written as lower-case hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hex<const N: usize>(pub [u8; N]);

impl<const N: usize> Serialize for Hex<N> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.iter().map(|b| format!("{b:02x}")).collect::<String>())
    }
}

impl<'de, const N: usize> Deserialize<'de> for Hex<N> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let t = String::deserialize(d)?;
        let bad = || serde::de::Error::custom(format!("{t:?} is not {N} bytes of lower-case hex"));
        if t.len() != N * 2 || !t.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(bad());
        }
        let mut out = [0u8; N];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = t.get(i * 2..i * 2 + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).ok_or_else(bad)?;
        }
        Ok(Hex(out))
    }
}

/// An op, version 1 of every op. In the file each op is written flat:
/// `{"op": "<Name>", "v": 1, …fields…}` (see [`crate::record`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Op {
    AddPage {
        id: PageId,
        page: Page,
        index: u64,
    },
    RemovePage {
        id: PageId,
        page: Page,
        index: u64,
    },
    AddLayer {
        id: LayerId,
        layer: Layer,
    },
    RemoveLayer {
        id: LayerId,
        layer: Layer,
    },
    AddItem {
        id: ItemId,
        item: Item,
    },
    ModifyItem {
        id: ItemId,
        before: Item,
        after: Item,
    },
    RemoveItem {
        id: ItemId,
        item: Item,
    },
    CreateComponent {
        id: ComponentId,
        component: Component,
    },
    DeleteComponent {
        id: ComponentId,
        component: Component,
    },
    AddTerminal {
        id: TerminalId,
        terminal: Terminal,
    },
    RemoveTerminal {
        id: TerminalId,
        terminal: Terminal,
    },
    LinkRepresentation {
        id: RepresentationId,
        rep: Representation,
    },
    ModifyRepresentation {
        id: RepresentationId,
        before: Representation,
        after: Representation,
    },
    UnlinkRepresentation {
        id: RepresentationId,
        rep: Representation,
    },
    CreateConductor {
        id: ConductorId,
        conductor: Conductor,
    },
    DeleteConductor {
        id: ConductorId,
        conductor: Conductor,
    },
    AttachEnd {
        conductor: ConductorId,
        end: End,
        before: Option<TerminalId>,
        after: Option<TerminalId>,
    },
    CreateCable {
        id: CableId,
        cable: Cable,
    },
    DeleteCable {
        id: CableId,
        cable: Cable,
    },
    AddCableMember {
        cable: CableId,
        index: u64,
        member: CableMember,
    },
    RemoveCableMember {
        cable: CableId,
        index: u64,
        member: CableMember,
    },
    SetProperty {
        object: ObjectRef,
        key: String,
        before: Option<String>,
        after: Option<String>,
    },
    EmbedDefinition {
        id: DefinitionId,
        definition: Definition,
    },
    RemoveDefinition {
        id: DefinitionId,
        definition: Definition,
    },
    SetProjectSettings {
        before: ProjectSettings,
        after: ProjectSettings,
    },
    NameVersion {
        name: String,
        at: u64,
        /// A version's description (brief 020, FQ5); left out of the record when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    UnnameVersion {
        name: String,
        at: u64,
        /// A version's description (brief 020, FQ5); left out of the record when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// Brief 019 (decision FP3).
    AddFolder {
        id: FolderId,
        folder: Folder,
        index: u64,
    },
    RemoveFolder {
        id: FolderId,
        folder: Folder,
        index: u64,
    },
    ModifyFolder {
        id: FolderId,
        before: Folder,
        after: Folder,
    },
}

/// The current version of every op in schema version 1.
pub const OP_VERSION: u64 = 1;

// ---------------------------------------------------------------------------------------------
// model → record

impl From<&m::Color> for Color {
    fn from(c: &m::Color) -> Self {
        match *c {
            m::Color::ByLayer => Color::ByLayer,
            m::Color::ByBlock => Color::ByBlock,
            m::Color::Index(i) => Color::Index(i),
            m::Color::Rgb(v) => Color::Rgb(v),
        }
    }
}

impl From<&m::Style> for Style {
    fn from(s: &m::Style) -> Self {
        Style { layer: s.layer, color: (&s.color).into(), linetype: s.linetype.clone(), lineweight: s.lineweight }
    }
}

impl From<&m::Primitive> for Primitive {
    fn from(p: &m::Primitive) -> Self {
        match p {
            m::Primitive::Line { a, b } => Primitive::Line { a: pt(*a), b: pt(*b) },
            m::Primitive::Polyline { vertices, closed } => {
                Primitive::Polyline { vertices: vertices.iter().copied().map(pt).collect(), closed: *closed }
            }
            m::Primitive::Arc { center, radius, start, end } => Primitive::Arc { center: pt(*center), radius: *radius, start: *start, end: *end },
            m::Primitive::Circle { center, radius } => Primitive::Circle { center: pt(*center), radius: *radius },
            m::Primitive::Text { at, height, rotation, value } => {
                Primitive::Text { at: pt(*at), height: *height, rotation: *rotation, value: value.clone() }
            }
            m::Primitive::Point { at } => Primitive::Point { at: pt(*at) },
        }
    }
}

impl From<&m::Item> for Item {
    fn from(i: &m::Item) -> Self {
        let kind = match &i.kind {
            m::ItemKind::Primitive(p) => ItemKind::Primitive(p.into()),
            m::ItemKind::Symbol(s) => ItemKind::Symbol(SymbolInstance {
                definition: s.definition,
                placement: Placement {
                    origin: pt(s.placement.origin),
                    rotation: s.placement.rotation,
                    mirror: s.placement.mirror,
                    scale: s.placement.scale,
                },
            }),
        };
        Item { page: i.page, style: (&i.style).into(), kind }
    }
}

impl From<&m::Component> for Component {
    fn from(c: &m::Component) -> Self {
        Component { properties: c.properties.clone(), definition: c.definition }
    }
}

impl From<&m::Terminal> for Terminal {
    fn from(t: &m::Terminal) -> Self {
        Terminal { component: t.component, label: t.label.clone(), key: t.key.clone() }
    }
}

impl From<&m::Representation> for Representation {
    fn from(r: &m::Representation) -> Self {
        Representation {
            subject: match r.subject {
                m::Subject::Component(c) => Subject::Component(c),
                m::Subject::Conductor(c) => Subject::Conductor(c),
                m::Subject::Cable(c) => Subject::Cable(c),
            },
            page: r.page,
            form: match r.form {
                m::Form::Drawn => Form::Drawn,
                m::Form::Symbol => Form::Symbol,
            },
            role: r.role.clone(),
            items: r.items.clone(),
            connection_points: r.connection_points.iter().map(|(k, v)| (*k, pt(*v))).collect(),
        }
    }
}

fn end(e: m::End) -> End {
    match e {
        m::End::A => End::A,
        m::End::B => End::B,
    }
}

impl From<&m::Conductor> for Conductor {
    fn from(c: &m::Conductor) -> Self {
        Conductor { properties: c.properties.clone(), ends: Ends { a: c.end(m::End::A), b: c.end(m::End::B) } }
    }
}

impl From<&m::CableMember> for CableMember {
    fn from(c: &m::CableMember) -> Self {
        CableMember { conductor: c.conductor, core: c.core.clone() }
    }
}

impl From<&m::Cable> for Cable {
    fn from(c: &m::Cable) -> Self {
        Cable { properties: c.properties.clone(), members: c.members.iter().map(Into::into).collect() }
    }
}

impl From<&m::Definition> for Definition {
    fn from(d: &m::Definition) -> Self {
        Definition {
            name: d.name.clone(),
            origin: d.origin.as_ref().map(|o| Origin {
                library: o.library,
                definition: o.definition,
                library_seq: o.library_seq,
                content_hash: o.content_hash,
            }),
            graphics: d.graphics.iter().map(Into::into).collect(),
            terminals: d.terminals.iter().map(|t| TerminalDef { key: t.key.clone(), label: t.label.clone(), at: pt(t.at) }).collect(),
            bridges: d.bridges.clone(),
        }
    }
}

impl From<&m::Page> for Page {
    fn from(p: &m::Page) -> Self {
        Page { title: p.title.clone(), properties: p.properties.clone() }
    }
}

impl From<Page> for m::Page {
    fn from(p: Page) -> Self {
        m::Page { title: p.title, properties: p.properties }
    }
}

impl From<&m::Folder> for Folder {
    fn from(f: &m::Folder) -> Self {
        Folder { name: f.name.clone(), parent: f.parent }
    }
}

impl From<Folder> for m::Folder {
    fn from(f: Folder) -> Self {
        m::Folder { name: f.name, parent: f.parent }
    }
}

impl From<&m::ProjectSettings> for ProjectSettings {
    fn from(s: &m::ProjectSettings) -> Self {
        ProjectSettings {
            length_unit: match s.length_unit {
                m::LengthUnit::Millimetre => LengthUnit::Millimetre,
                m::LengthUnit::Inch => LengthUnit::Inch,
            },
            tag_padding: s.tag_padding,
            tag_prefixes: s.tag_prefixes.clone(),
            wiring: WiringSettings::from(&s.wiring),
            sheet: (&s.sheet).into(),
            page_number_step: s.page_number_step,
            reference_style: match s.reference_style {
                m::ReferenceStyle::PageColumnRow => ReferenceStyle::PageColumnRow,
                m::ReferenceStyle::PageColumn => ReferenceStyle::PageColumn,
            },
        }
    }
}

fn object(o: m::ObjectRef) -> ObjectRef {
    match o {
        m::ObjectRef::Component(c) => ObjectRef::Component(c),
        m::ObjectRef::Conductor(c) => ObjectRef::Conductor(c),
        m::ObjectRef::Cable(c) => ObjectRef::Cable(c),
        m::ObjectRef::Page(p) => ObjectRef::Page(p),
        m::ObjectRef::Project => ObjectRef::Project,
    }
}

impl From<&m::Op> for Op {
    fn from(op: &m::Op) -> Self {
        let ix = |i: usize| i as u64;
        match op {
            m::Op::AddPage { id, page, index } => Op::AddPage { id: *id, page: page.into(), index: ix(*index) },
            m::Op::RemovePage { id, page, index } => Op::RemovePage { id: *id, page: page.into(), index: ix(*index) },
            m::Op::AddLayer { id, layer } => Op::AddLayer { id: *id, layer: Layer { name: layer.name.clone() } },
            m::Op::RemoveLayer { id, layer } => Op::RemoveLayer { id: *id, layer: Layer { name: layer.name.clone() } },
            m::Op::AddItem { id, item } => Op::AddItem { id: *id, item: item.into() },
            m::Op::ModifyItem { id, before, after } => Op::ModifyItem { id: *id, before: before.into(), after: after.into() },
            m::Op::RemoveItem { id, item } => Op::RemoveItem { id: *id, item: item.into() },
            m::Op::CreateComponent { id, component } => Op::CreateComponent { id: *id, component: component.into() },
            m::Op::DeleteComponent { id, component } => Op::DeleteComponent { id: *id, component: component.into() },
            m::Op::AddTerminal { id, terminal } => Op::AddTerminal { id: *id, terminal: terminal.into() },
            m::Op::RemoveTerminal { id, terminal } => Op::RemoveTerminal { id: *id, terminal: terminal.into() },
            m::Op::LinkRepresentation { id, rep } => Op::LinkRepresentation { id: *id, rep: rep.into() },
            m::Op::ModifyRepresentation { id, before, after } => Op::ModifyRepresentation { id: *id, before: before.into(), after: after.into() },
            m::Op::UnlinkRepresentation { id, rep } => Op::UnlinkRepresentation { id: *id, rep: rep.into() },
            m::Op::CreateConductor { id, conductor } => Op::CreateConductor { id: *id, conductor: conductor.into() },
            m::Op::DeleteConductor { id, conductor } => Op::DeleteConductor { id: *id, conductor: conductor.into() },
            m::Op::AttachEnd { conductor, end: e, before, after } => {
                Op::AttachEnd { conductor: *conductor, end: end(*e), before: *before, after: *after }
            }
            m::Op::CreateCable { id, cable } => Op::CreateCable { id: *id, cable: cable.into() },
            m::Op::DeleteCable { id, cable } => Op::DeleteCable { id: *id, cable: cable.into() },
            m::Op::AddCableMember { cable, index, member } => Op::AddCableMember { cable: *cable, index: ix(*index), member: member.into() },
            m::Op::RemoveCableMember { cable, index, member } => Op::RemoveCableMember { cable: *cable, index: ix(*index), member: member.into() },
            m::Op::SetProperty { object: o, key, before, after } => {
                Op::SetProperty { object: object(*o), key: key.clone(), before: before.clone(), after: after.clone() }
            }
            m::Op::EmbedDefinition { id, definition } => Op::EmbedDefinition { id: *id, definition: definition.into() },
            m::Op::RemoveDefinition { id, definition } => Op::RemoveDefinition { id: *id, definition: definition.into() },
            m::Op::SetProjectSettings { before, after } => Op::SetProjectSettings { before: before.into(), after: after.into() },
            m::Op::NameVersion { name, at, description } => Op::NameVersion { name: name.clone(), at: *at, description: description.clone() },
            m::Op::UnnameVersion { name, at, description } => Op::UnnameVersion { name: name.clone(), at: *at, description: description.clone() },
            m::Op::AddFolder { id, folder, index } => Op::AddFolder { id: *id, folder: folder.into(), index: ix(*index) },
            m::Op::RemoveFolder { id, folder, index } => Op::RemoveFolder { id: *id, folder: folder.into(), index: ix(*index) },
            m::Op::ModifyFolder { id, before, after } => Op::ModifyFolder { id: *id, before: before.into(), after: after.into() },
        }
    }
}

impl From<&m::Author> for Author {
    fn from(a: &m::Author) -> Self {
        match a {
            m::Author::User => Author::User,
            m::Author::Agent(n) => Author::Agent(n.clone()),
        }
    }
}

impl From<&m::TxKind> for TxKind {
    fn from(k: &m::TxKind) -> Self {
        match *k {
            m::TxKind::Edit => TxKind::Edit,
            m::TxKind::Undo { of } => TxKind::Undo(of),
            m::TxKind::Redo { of } => TxKind::Redo(of),
            m::TxKind::Merge { source, source_head, ancestor } => TxKind::Merge(MergeKind {
                source: Hex(source),
                source_head: MergeStep { seq: source_head.seq, chain: Hex(source_head.chain) },
                ancestor: MergeAncestor { branch: Hex(ancestor.branch), seq: ancestor.seq, chain: Hex(ancestor.chain) },
            }),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// record → model

/// A record value the model cannot hold.
pub type ConvertError = String;

fn index(i: u64) -> Result<usize, ConvertError> {
    usize::try_from(i).map_err(|_| format!("index {i} is out of range"))
}

impl From<Color> for m::Color {
    fn from(c: Color) -> Self {
        match c {
            Color::ByLayer => m::Color::ByLayer,
            Color::ByBlock => m::Color::ByBlock,
            Color::Index(i) => m::Color::Index(i),
            Color::Rgb(v) => m::Color::Rgb(v),
        }
    }
}

impl From<Primitive> for m::Primitive {
    fn from(p: Primitive) -> Self {
        match p {
            Primitive::Line { a, b } => m::Primitive::Line { a: mpt(a), b: mpt(b) },
            Primitive::Polyline { vertices, closed } => m::Primitive::Polyline { vertices: vertices.into_iter().map(mpt).collect(), closed },
            Primitive::Arc { center, radius, start, end } => m::Primitive::Arc { center: mpt(center), radius, start, end },
            Primitive::Circle { center, radius } => m::Primitive::Circle { center: mpt(center), radius },
            Primitive::Text { at, height, rotation, value } => m::Primitive::Text { at: mpt(at), height, rotation, value },
            Primitive::Point { at } => m::Primitive::Point { at: mpt(at) },
        }
    }
}

impl From<Item> for m::Item {
    fn from(i: Item) -> Self {
        let kind = match i.kind {
            ItemKind::Primitive(p) => m::ItemKind::Primitive(p.into()),
            ItemKind::Symbol(s) => m::ItemKind::Symbol(m::SymbolInstance {
                definition: s.definition,
                placement: m::Placement {
                    origin: mpt(s.placement.origin),
                    rotation: s.placement.rotation,
                    mirror: s.placement.mirror,
                    scale: s.placement.scale,
                },
            }),
        };
        m::Item {
            page: i.page,
            style: m::Style { layer: i.style.layer, color: i.style.color.into(), linetype: i.style.linetype, lineweight: i.style.lineweight },
            kind,
        }
    }
}

impl From<Component> for m::Component {
    fn from(c: Component) -> Self {
        m::Component { properties: c.properties, definition: c.definition }
    }
}

impl From<Terminal> for m::Terminal {
    fn from(t: Terminal) -> Self {
        m::Terminal { component: t.component, label: t.label, key: t.key }
    }
}

impl From<Representation> for m::Representation {
    fn from(r: Representation) -> Self {
        m::Representation {
            subject: match r.subject {
                Subject::Component(c) => m::Subject::Component(c),
                Subject::Conductor(c) => m::Subject::Conductor(c),
                Subject::Cable(c) => m::Subject::Cable(c),
            },
            page: r.page,
            form: match r.form {
                Form::Drawn => m::Form::Drawn,
                Form::Symbol => m::Form::Symbol,
            },
            role: r.role,
            items: r.items,
            connection_points: r.connection_points.into_iter().map(|(k, v)| (k, mpt(v))).collect(),
        }
    }
}

fn mend(e: End) -> m::End {
    match e {
        End::A => m::End::A,
        End::B => m::End::B,
    }
}

impl From<Conductor> for m::Conductor {
    fn from(c: Conductor) -> Self {
        m::Conductor { properties: c.properties, ends: [c.ends.a, c.ends.b] }
    }
}

impl From<CableMember> for m::CableMember {
    fn from(c: CableMember) -> Self {
        m::CableMember { conductor: c.conductor, core: c.core }
    }
}

impl From<Cable> for m::Cable {
    fn from(c: Cable) -> Self {
        m::Cable { properties: c.properties, members: c.members.into_iter().map(Into::into).collect() }
    }
}

impl From<Definition> for m::Definition {
    fn from(d: Definition) -> Self {
        m::Definition {
            name: d.name,
            origin: d.origin.map(|o| m::Origin {
                library: o.library,
                definition: o.definition,
                library_seq: o.library_seq,
                content_hash: o.content_hash,
            }),
            graphics: d.graphics.into_iter().map(Into::into).collect(),
            terminals: d.terminals.into_iter().map(|t| m::TerminalDef { key: t.key, label: t.label, at: mpt(t.at) }).collect(),
            bridges: d.bridges,
        }
    }
}

impl From<ProjectSettings> for m::ProjectSettings {
    fn from(s: ProjectSettings) -> Self {
        m::ProjectSettings {
            length_unit: match s.length_unit {
                LengthUnit::Millimetre => m::LengthUnit::Millimetre,
                LengthUnit::Inch => m::LengthUnit::Inch,
            },
            tag_padding: s.tag_padding,
            tag_prefixes: s.tag_prefixes,
            wiring: s.wiring.into(),
            sheet: s.sheet.into(),
            page_number_step: s.page_number_step,
            reference_style: match s.reference_style {
                ReferenceStyle::PageColumnRow => m::ReferenceStyle::PageColumnRow,
                ReferenceStyle::PageColumn => m::ReferenceStyle::PageColumn,
            },
        }
    }
}

fn mobject(o: ObjectRef) -> m::ObjectRef {
    match o {
        ObjectRef::Component(c) => m::ObjectRef::Component(c),
        ObjectRef::Conductor(c) => m::ObjectRef::Conductor(c),
        ObjectRef::Cable(c) => m::ObjectRef::Cable(c),
        ObjectRef::Page(p) => m::ObjectRef::Page(p),
        ObjectRef::Project => m::ObjectRef::Project,
    }
}

impl TryFrom<Op> for m::Op {
    type Error = ConvertError;
    fn try_from(op: Op) -> Result<Self, ConvertError> {
        Ok(match op {
            Op::AddPage { id, page, index: i } => m::Op::AddPage { id, page: page.into(), index: index(i)? },
            Op::RemovePage { id, page, index: i } => m::Op::RemovePage { id, page: page.into(), index: index(i)? },
            Op::AddLayer { id, layer } => m::Op::AddLayer { id, layer: m::Layer { name: layer.name } },
            Op::RemoveLayer { id, layer } => m::Op::RemoveLayer { id, layer: m::Layer { name: layer.name } },
            Op::AddItem { id, item } => m::Op::AddItem { id, item: item.into() },
            Op::ModifyItem { id, before, after } => m::Op::ModifyItem { id, before: before.into(), after: after.into() },
            Op::RemoveItem { id, item } => m::Op::RemoveItem { id, item: item.into() },
            Op::CreateComponent { id, component } => m::Op::CreateComponent { id, component: component.into() },
            Op::DeleteComponent { id, component } => m::Op::DeleteComponent { id, component: component.into() },
            Op::AddTerminal { id, terminal } => m::Op::AddTerminal { id, terminal: terminal.into() },
            Op::RemoveTerminal { id, terminal } => m::Op::RemoveTerminal { id, terminal: terminal.into() },
            Op::LinkRepresentation { id, rep } => m::Op::LinkRepresentation { id, rep: rep.into() },
            Op::ModifyRepresentation { id, before, after } => m::Op::ModifyRepresentation { id, before: before.into(), after: after.into() },
            Op::UnlinkRepresentation { id, rep } => m::Op::UnlinkRepresentation { id, rep: rep.into() },
            Op::CreateConductor { id, conductor } => m::Op::CreateConductor { id, conductor: conductor.into() },
            Op::DeleteConductor { id, conductor } => m::Op::DeleteConductor { id, conductor: conductor.into() },
            Op::AttachEnd { conductor, end, before, after } => m::Op::AttachEnd { conductor, end: mend(end), before, after },
            Op::CreateCable { id, cable } => m::Op::CreateCable { id, cable: cable.into() },
            Op::DeleteCable { id, cable } => m::Op::DeleteCable { id, cable: cable.into() },
            Op::AddCableMember { cable, index: i, member } => m::Op::AddCableMember { cable, index: index(i)?, member: member.into() },
            Op::RemoveCableMember { cable, index: i, member } => m::Op::RemoveCableMember { cable, index: index(i)?, member: member.into() },
            Op::SetProperty { object, key, before, after } => m::Op::SetProperty { object: mobject(object), key, before, after },
            Op::EmbedDefinition { id, definition } => m::Op::EmbedDefinition { id, definition: definition.into() },
            Op::RemoveDefinition { id, definition } => m::Op::RemoveDefinition { id, definition: definition.into() },
            Op::SetProjectSettings { before, after } => m::Op::SetProjectSettings { before: before.into(), after: after.into() },
            Op::NameVersion { name, at, description } => m::Op::NameVersion { name, at, description },
            Op::UnnameVersion { name, at, description } => m::Op::UnnameVersion { name, at, description },
            Op::AddFolder { id, folder, index: i } => m::Op::AddFolder { id, folder: folder.into(), index: index(i)? },
            Op::RemoveFolder { id, folder, index: i } => m::Op::RemoveFolder { id, folder: folder.into(), index: index(i)? },
            Op::ModifyFolder { id, before, after } => m::Op::ModifyFolder { id, before: before.into(), after: after.into() },
        })
    }
}

impl From<Author> for m::Author {
    fn from(a: Author) -> Self {
        match a {
            Author::User => m::Author::User,
            Author::Agent(n) => m::Author::Agent(n),
        }
    }
}

impl From<TxKind> for m::TxKind {
    fn from(k: TxKind) -> Self {
        match k {
            TxKind::Edit => m::TxKind::Edit,
            TxKind::Undo(of) => m::TxKind::Undo { of },
            TxKind::Redo(of) => m::TxKind::Redo { of },
            TxKind::Merge(k) => m::TxKind::Merge {
                source: k.source.0,
                source_head: m::StepPoint { seq: k.source_head.seq, chain: k.source_head.chain.0 },
                ancestor: m::BranchPoint { branch: k.ancestor.branch.0, seq: k.ancestor.seq, chain: k.ancestor.chain.0 },
            },
        }
    }
}
