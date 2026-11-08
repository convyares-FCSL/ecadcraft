//! Schema version 1 records (proposal §20.2-§20.3): the file format's own types.
//!
//! These mirror `ecad-model` deliberately: the model may change shape freely, while these types
//! change only with a schema version and an upcast. Every struct refuses unknown fields (E5).
//! IDs use the model's ID types, whose serialised form (32 hex digits) is itself part of this
//! schema and fixed by the golden-file test.

use std::collections::BTreeMap;

use ecad_model as m;
use ecad_model::{
    CableId, ComponentId, ConductorId, DefinitionId, ItemId, LayerId, LibraryId, PageId, RepresentationId, TerminalId,
    TxId,
};
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
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    Millimetre,
    Inch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSettings {
    pub length_unit: LengthUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ObjectRef {
    Component(ComponentId),
    Conductor(ConductorId),
    Cable(CableId),
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
}

/// An op, version 1 of every op. In the file each op is written flat:
/// `{"op": "<Name>", "v": 1, …fields…}` (see [`crate::record`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Op {
    AddPage { id: PageId, page: Page, index: u64 },
    RemovePage { id: PageId, page: Page, index: u64 },
    AddLayer { id: LayerId, layer: Layer },
    RemoveLayer { id: LayerId, layer: Layer },
    AddItem { id: ItemId, item: Item },
    ModifyItem { id: ItemId, before: Item, after: Item },
    RemoveItem { id: ItemId, item: Item },
    CreateComponent { id: ComponentId, component: Component },
    DeleteComponent { id: ComponentId, component: Component },
    AddTerminal { id: TerminalId, terminal: Terminal },
    RemoveTerminal { id: TerminalId, terminal: Terminal },
    LinkRepresentation { id: RepresentationId, rep: Representation },
    ModifyRepresentation { id: RepresentationId, before: Representation, after: Representation },
    UnlinkRepresentation { id: RepresentationId, rep: Representation },
    CreateConductor { id: ConductorId, conductor: Conductor },
    DeleteConductor { id: ConductorId, conductor: Conductor },
    AttachEnd { conductor: ConductorId, end: End, before: Option<TerminalId>, after: Option<TerminalId> },
    CreateCable { id: CableId, cable: Cable },
    DeleteCable { id: CableId, cable: Cable },
    AddCableMember { cable: CableId, index: u64, member: CableMember },
    RemoveCableMember { cable: CableId, index: u64, member: CableMember },
    SetProperty { object: ObjectRef, key: String, before: Option<String>, after: Option<String> },
    EmbedDefinition { id: DefinitionId, definition: Definition },
    RemoveDefinition { id: DefinitionId, definition: Definition },
    SetProjectSettings { before: ProjectSettings, after: ProjectSettings },
    NameVersion { name: String, at: u64 },
    UnnameVersion { name: String, at: u64 },
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
            m::Primitive::Arc { center, radius, start, end } => {
                Primitive::Arc { center: pt(*center), radius: *radius, start: *start, end: *end }
            }
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
            terminals: d
                .terminals
                .iter()
                .map(|t| TerminalDef { key: t.key.clone(), label: t.label.clone(), at: pt(t.at) })
                .collect(),
        }
    }
}

impl From<&m::ProjectSettings> for ProjectSettings {
    fn from(s: &m::ProjectSettings) -> Self {
        ProjectSettings {
            length_unit: match s.length_unit {
                m::LengthUnit::Millimetre => LengthUnit::Millimetre,
                m::LengthUnit::Inch => LengthUnit::Inch,
            },
        }
    }
}

fn object(o: m::ObjectRef) -> ObjectRef {
    match o {
        m::ObjectRef::Component(c) => ObjectRef::Component(c),
        m::ObjectRef::Conductor(c) => ObjectRef::Conductor(c),
        m::ObjectRef::Cable(c) => ObjectRef::Cable(c),
    }
}

impl From<&m::Op> for Op {
    fn from(op: &m::Op) -> Self {
        let ix = |i: usize| i as u64;
        match op {
            m::Op::AddPage { id, page, index } => {
                Op::AddPage { id: *id, page: Page { title: page.title.clone() }, index: ix(*index) }
            }
            m::Op::RemovePage { id, page, index } => {
                Op::RemovePage { id: *id, page: Page { title: page.title.clone() }, index: ix(*index) }
            }
            m::Op::AddLayer { id, layer } => Op::AddLayer { id: *id, layer: Layer { name: layer.name.clone() } },
            m::Op::RemoveLayer { id, layer } => Op::RemoveLayer { id: *id, layer: Layer { name: layer.name.clone() } },
            m::Op::AddItem { id, item } => Op::AddItem { id: *id, item: item.into() },
            m::Op::ModifyItem { id, before, after } => {
                Op::ModifyItem { id: *id, before: before.into(), after: after.into() }
            }
            m::Op::RemoveItem { id, item } => Op::RemoveItem { id: *id, item: item.into() },
            m::Op::CreateComponent { id, component } => Op::CreateComponent { id: *id, component: component.into() },
            m::Op::DeleteComponent { id, component } => Op::DeleteComponent { id: *id, component: component.into() },
            m::Op::AddTerminal { id, terminal } => Op::AddTerminal { id: *id, terminal: terminal.into() },
            m::Op::RemoveTerminal { id, terminal } => Op::RemoveTerminal { id: *id, terminal: terminal.into() },
            m::Op::LinkRepresentation { id, rep } => Op::LinkRepresentation { id: *id, rep: rep.into() },
            m::Op::ModifyRepresentation { id, before, after } => {
                Op::ModifyRepresentation { id: *id, before: before.into(), after: after.into() }
            }
            m::Op::UnlinkRepresentation { id, rep } => Op::UnlinkRepresentation { id: *id, rep: rep.into() },
            m::Op::CreateConductor { id, conductor } => Op::CreateConductor { id: *id, conductor: conductor.into() },
            m::Op::DeleteConductor { id, conductor } => Op::DeleteConductor { id: *id, conductor: conductor.into() },
            m::Op::AttachEnd { conductor, end: e, before, after } => {
                Op::AttachEnd { conductor: *conductor, end: end(*e), before: *before, after: *after }
            }
            m::Op::CreateCable { id, cable } => Op::CreateCable { id: *id, cable: cable.into() },
            m::Op::DeleteCable { id, cable } => Op::DeleteCable { id: *id, cable: cable.into() },
            m::Op::AddCableMember { cable, index, member } => {
                Op::AddCableMember { cable: *cable, index: ix(*index), member: member.into() }
            }
            m::Op::RemoveCableMember { cable, index, member } => {
                Op::RemoveCableMember { cable: *cable, index: ix(*index), member: member.into() }
            }
            m::Op::SetProperty { object: o, key, before, after } => {
                Op::SetProperty { object: object(*o), key: key.clone(), before: before.clone(), after: after.clone() }
            }
            m::Op::EmbedDefinition { id, definition } => Op::EmbedDefinition { id: *id, definition: definition.into() },
            m::Op::RemoveDefinition { id, definition } => {
                Op::RemoveDefinition { id: *id, definition: definition.into() }
            }
            m::Op::SetProjectSettings { before, after } => {
                Op::SetProjectSettings { before: before.into(), after: after.into() }
            }
            m::Op::NameVersion { name, at } => Op::NameVersion { name: name.clone(), at: *at },
            m::Op::UnnameVersion { name, at } => Op::UnnameVersion { name: name.clone(), at: *at },
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
            Primitive::Polyline { vertices, closed } => {
                m::Primitive::Polyline { vertices: vertices.into_iter().map(mpt).collect(), closed }
            }
            Primitive::Arc { center, radius, start, end } => {
                m::Primitive::Arc { center: mpt(center), radius, start, end }
            }
            Primitive::Circle { center, radius } => m::Primitive::Circle { center: mpt(center), radius },
            Primitive::Text { at, height, rotation, value } => {
                m::Primitive::Text { at: mpt(at), height, rotation, value }
            }
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
            style: m::Style {
                layer: i.style.layer,
                color: i.style.color.into(),
                linetype: i.style.linetype,
                lineweight: i.style.lineweight,
            },
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
            terminals: d
                .terminals
                .into_iter()
                .map(|t| m::TerminalDef { key: t.key, label: t.label, at: mpt(t.at) })
                .collect(),
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
        }
    }
}

fn mobject(o: ObjectRef) -> m::ObjectRef {
    match o {
        ObjectRef::Component(c) => m::ObjectRef::Component(c),
        ObjectRef::Conductor(c) => m::ObjectRef::Conductor(c),
        ObjectRef::Cable(c) => m::ObjectRef::Cable(c),
    }
}

impl TryFrom<Op> for m::Op {
    type Error = ConvertError;
    fn try_from(op: Op) -> Result<Self, ConvertError> {
        Ok(match op {
            Op::AddPage { id, page, index: i } => {
                m::Op::AddPage { id, page: m::Page { title: page.title }, index: index(i)? }
            }
            Op::RemovePage { id, page, index: i } => {
                m::Op::RemovePage { id, page: m::Page { title: page.title }, index: index(i)? }
            }
            Op::AddLayer { id, layer } => m::Op::AddLayer { id, layer: m::Layer { name: layer.name } },
            Op::RemoveLayer { id, layer } => m::Op::RemoveLayer { id, layer: m::Layer { name: layer.name } },
            Op::AddItem { id, item } => m::Op::AddItem { id, item: item.into() },
            Op::ModifyItem { id, before, after } => {
                m::Op::ModifyItem { id, before: before.into(), after: after.into() }
            }
            Op::RemoveItem { id, item } => m::Op::RemoveItem { id, item: item.into() },
            Op::CreateComponent { id, component } => m::Op::CreateComponent { id, component: component.into() },
            Op::DeleteComponent { id, component } => m::Op::DeleteComponent { id, component: component.into() },
            Op::AddTerminal { id, terminal } => m::Op::AddTerminal { id, terminal: terminal.into() },
            Op::RemoveTerminal { id, terminal } => m::Op::RemoveTerminal { id, terminal: terminal.into() },
            Op::LinkRepresentation { id, rep } => m::Op::LinkRepresentation { id, rep: rep.into() },
            Op::ModifyRepresentation { id, before, after } => {
                m::Op::ModifyRepresentation { id, before: before.into(), after: after.into() }
            }
            Op::UnlinkRepresentation { id, rep } => m::Op::UnlinkRepresentation { id, rep: rep.into() },
            Op::CreateConductor { id, conductor } => m::Op::CreateConductor { id, conductor: conductor.into() },
            Op::DeleteConductor { id, conductor } => m::Op::DeleteConductor { id, conductor: conductor.into() },
            Op::AttachEnd { conductor, end, before, after } => {
                m::Op::AttachEnd { conductor, end: mend(end), before, after }
            }
            Op::CreateCable { id, cable } => m::Op::CreateCable { id, cable: cable.into() },
            Op::DeleteCable { id, cable } => m::Op::DeleteCable { id, cable: cable.into() },
            Op::AddCableMember { cable, index: i, member } => {
                m::Op::AddCableMember { cable, index: index(i)?, member: member.into() }
            }
            Op::RemoveCableMember { cable, index: i, member } => {
                m::Op::RemoveCableMember { cable, index: index(i)?, member: member.into() }
            }
            Op::SetProperty { object, key, before, after } => {
                m::Op::SetProperty { object: mobject(object), key, before, after }
            }
            Op::EmbedDefinition { id, definition } => m::Op::EmbedDefinition { id, definition: definition.into() },
            Op::RemoveDefinition { id, definition } => m::Op::RemoveDefinition { id, definition: definition.into() },
            Op::SetProjectSettings { before, after } => {
                m::Op::SetProjectSettings { before: before.into(), after: after.into() }
            }
            Op::NameVersion { name, at } => m::Op::NameVersion { name, at },
            Op::UnnameVersion { name, at } => m::Op::UnnameVersion { name, at },
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
        }
    }
}
