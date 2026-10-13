//! The document's objects and their relationships (proposal §3 and §4).
//!
//! Stored intent (components, terminals, conductors, cables, definitions) and drawing content
//! (pages, layers, items, representations) live here. Derived results do not (see [`crate::derive`]).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::error::{ModelError, Result};
use crate::geom::{Placement, Pt};
use crate::id::{
    CableId, ComponentId, ConductorId, DefinitionId, ItemId, LayerId, PageId, RepresentationId, TerminalId,
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

    fn is_finite(&self) -> bool {
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
    /// Drawn form only: where each terminal sits on the page.
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

/// Where an embedded definition came from (LIB-01).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Origin {
    pub library: String,
    pub definition: String,
    pub version: u32,
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
/// [`Document::check`] verifies every invariant; [`crate::ledger::Ledger`] runs it after each
/// transaction so an invalid state is never committed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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
}

impl Document {
    /// The representation an item belongs to, if any (DOC-05 allows at most one).
    pub fn representation_of_item(&self, item: ItemId) -> Option<(RepresentationId, &Representation)> {
        self.representations.iter().find(|(_, r)| r.items.contains(&item)).map(|(id, r)| (*id, r))
    }

    /// Representations of `subject`.
    pub fn representations_of(&self, subject: Subject) -> impl Iterator<Item = (RepresentationId, &Representation)> {
        self.representations.iter().filter(move |(_, r)| r.subject == subject).map(|(id, r)| (*id, r))
    }

    /// Terminals of `component`, in ID order.
    pub fn terminals_of(&self, component: ComponentId) -> impl Iterator<Item = (TerminalId, &Terminal)> {
        self.terminals.iter().filter(move |(_, t)| t.component == component).map(|(id, t)| (*id, t))
    }

    /// Conductor ends attached to `terminal`.
    pub fn ends_at(&self, terminal: TerminalId) -> Vec<(ConductorId, End)> {
        let mut out = Vec::new();
        for (id, c) in &self.conductors {
            for end in End::BOTH {
                if c.end(end) == Some(terminal) {
                    out.push((*id, end));
                }
            }
        }
        out
    }

    /// Cables `conductor` is a member of.
    pub fn cables_of(&self, conductor: ConductorId) -> Vec<CableId> {
        self.cables
            .iter()
            .filter(|(_, c)| c.members.iter().any(|m| m.conductor == conductor))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Verify every invariant.
    ///
    /// # Errors
    /// [`ModelError::Invariant`] naming the first violation found.
    pub fn check(&self) -> Result<()> {
        let bad = |m: String| Err(ModelError::Invariant(m));
        let order: BTreeSet<_> = self.page_order.iter().collect();
        if order.len() != self.page_order.len()
            || order.len() != self.pages.len()
            || !self.pages.keys().all(|p| order.contains(p))
        {
            return bad("page order does not list each page exactly once".into());
        }
        for (id, item) in &self.items {
            if !self.pages.contains_key(&item.page) {
                return bad(format!("item {id} is on a missing page"));
            }
            if !self.layers.contains_key(&item.style.layer) {
                return bad(format!("item {id} uses a missing layer"));
            }
            match &item.kind {
                ItemKind::Primitive(p) if !p.is_finite() => return bad(format!("item {id} has non-finite geometry")),
                ItemKind::Symbol(s) if !self.definitions.contains_key(&s.definition) => {
                    return bad(format!("item {id} places a missing definition"));
                }
                ItemKind::Symbol(s) if !s.placement.is_valid() => {
                    return bad(format!("item {id} has an invalid placement"));
                }
                _ => {}
            }
        }
        for (id, d) in &self.definitions {
            if !d.graphics.iter().all(Primitive::is_finite) || !d.terminals.iter().all(|t| t.at.is_finite()) {
                return bad(format!("definition {id} has non-finite geometry"));
            }
            let keys: BTreeSet<&str> = d.terminals.iter().map(|t| t.key.as_str()).collect();
            if keys.len() != d.terminals.len() {
                return bad(format!("definition {id} repeats a terminal key"));
            }
        }
        for (id, t) in &self.terminals {
            if !self.components.contains_key(&t.component) {
                return bad(format!("terminal {id} belongs to a missing component"));
            }
        }
        for (id, c) in &self.components {
            if let Some(d) = c.definition
                && !self.definitions.contains_key(&d)
            {
                return bad(format!("component {id} uses a missing definition"));
            }
        }
        let mut owner: BTreeMap<ItemId, RepresentationId> = BTreeMap::new();
        for (id, r) in &self.representations {
            let subject_exists = match r.subject {
                Subject::Component(c) => self.components.contains_key(&c),
                Subject::Conductor(c) => self.conductors.contains_key(&c),
                Subject::Cable(c) => self.cables.contains_key(&c),
            };
            if !subject_exists {
                return bad(format!("representation {id} depicts a missing subject"));
            }
            if !self.pages.contains_key(&r.page) {
                return bad(format!("representation {id} is on a missing page"));
            }
            for item in &r.items {
                let Some(it) = self.items.get(item) else {
                    return bad(format!("representation {id} lists missing item {item}"));
                };
                if it.page != r.page {
                    return bad(format!("representation {id} lists item {item} from another page"));
                }
                if let Some(other) = owner.insert(*item, *id) {
                    return bad(format!("item {item} belongs to representations {other} and {id} (DOC-05)"));
                }
            }
            match r.form {
                Form::Symbol => {
                    let instance = match r.items.as_slice() {
                        [only] if r.connection_points.is_empty() => match self.items.get(only).map(|i| &i.kind) {
                            Some(ItemKind::Symbol(inst)) => Some(inst),
                            _ => None,
                        },
                        _ => None,
                    };
                    let Some(inst) = instance else {
                        return bad(format!("symbol representation {id} must hold exactly one symbol instance"));
                    };
                    // The symbol shown must be the component's own definition.
                    let Subject::Component(c) = r.subject else {
                        return bad(format!("symbol representation {id} must depict a component"));
                    };
                    if self.components.get(&c).and_then(|c| c.definition) != Some(inst.definition) {
                        return bad(format!(
                            "symbol representation {id} shows a definition its component does not use"
                        ));
                    }
                }
                Form::Drawn => {
                    if r.items.iter().filter_map(|i| self.items.get(i)).any(|i| matches!(i.kind, ItemKind::Symbol(_))) {
                        return bad(format!("drawn representation {id} holds a symbol instance"));
                    }
                }
            }
            for (t, p) in &r.connection_points {
                let Subject::Component(c) = r.subject else {
                    return bad(format!("representation {id} of a non-component has connection points"));
                };
                if self.terminals.get(t).map(|t| t.component) != Some(c) {
                    return bad(format!("representation {id} places terminal {t} of another component"));
                }
                if !p.is_finite() {
                    return bad(format!("representation {id} has a non-finite connection point"));
                }
            }
        }
        for (id, c) in &self.conductors {
            for t in c.ends.iter().flatten() {
                if !self.terminals.contains_key(t) {
                    return bad(format!("conductor {id} is attached to missing terminal {t}"));
                }
            }
        }
        for (id, c) in &self.cables {
            let mut seen = BTreeSet::new();
            for m in &c.members {
                if !self.conductors.contains_key(&m.conductor) {
                    return bad(format!("cable {id} lists missing conductor {}", m.conductor));
                }
                if !seen.insert(m.conductor) {
                    return bad(format!("cable {id} lists conductor {} twice", m.conductor));
                }
            }
        }
        Ok(())
    }
}
