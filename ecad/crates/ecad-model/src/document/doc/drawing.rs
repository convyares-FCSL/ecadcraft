//! The drawing content: pages, folders, layers, items and representations.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::document::book::PageNo;
use crate::document::page::{PageKind, PageTitle};
use crate::document::sheet::SheetSpec;
use crate::geometry::geom::{Placement, Pt};
use crate::geometry::units::Radians;
use crate::naming::id::{CableId, ConnectionPointId, DefinitionId, DeviceId, FolderId, ItemId, LayerId, PageId, RouteId, WireId};

use super::Properties;
use crate::objects::role::Role;
use crate::objects::styles::StyleRef;
use crate::objects::wire::End;

/// A page of the project (design 072 §5.1).
///
/// What the page is ([`PageKind`]), its place in the book (`number`, `folder`) and its look
/// (`page_type`, `sheet`, `title_block`) are fields; `display` holds only presentation choices and
/// the defaults registry's page-level values. Every optional field is left out of records while
/// unset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub title: PageTitle,
    /// The designer's page number (`100`, `105a`); `None` shows the book position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<PageNo>,
    /// The folder it is in; `None` at the top level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<FolderId>,
    /// What the page is; set when it is made.
    #[serde(default, skip_serializing_if = "PageKind::is_schematic")]
    pub kind: PageKind,
    /// Its page type (a style of catalogue `page`, brief 054); `None` takes the type its kind
    /// implies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_type: Option<StyleRef>,
    /// Its own sheet (brief 019); `None` takes its page type's, else the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<SheetSpec>,
    /// The frame style whose title block it borrows (brief 055); `None` takes its page type's,
    /// else its frame's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_block: Option<StyleRef>,
    /// Its location (an entry of the project's list); `None` takes the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<crate::naming::id::LocationId>,
    /// Its function (an entry of the project's list); `None` takes the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<crate::naming::id::FunctionId>,
    /// Presentation choices and registry values at page level.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub display: Properties,
}

impl Page {
    /// A schematic page titled `title`, with nothing else set.
    pub fn new(title: PageTitle) -> Self {
        Page {
            title,
            number: None,
            folder: None,
            kind: PageKind::Schematic,
            page_type: None,
            sheet: None,
            title_block: None,
            location: None,
            function: None,
            display: Properties::new(),
        }
    }

    /// What differs between `self` (before) and `after`: the fields one by one, and the display
    /// keys whose value differs. The update maps read a `ModifyPage` through it.
    pub fn change<'a>(&'a self, after: &'a Page) -> PageChange<'a> {
        let display = self
            .display
            .keys()
            .chain(after.display.keys().filter(|k| !self.display.contains_key(*k)))
            .filter(|k| self.display.get(*k) != after.display.get(*k))
            .map(String::as_str)
            .collect();
        PageChange {
            title: self.title != after.title,
            number: self.number != after.number,
            folder: self.folder != after.folder,
            page_type: self.page_type != after.page_type,
            sheet: self.sheet != after.sheet,
            title_block: self.title_block != after.title_block,
            location: self.location != after.location,
            function: self.function != after.function,
            display,
        }
    }

    /// A schematic page titled `title`, if it is a valid title.
    ///
    /// # Errors
    /// Why the title was refused.
    pub fn titled(title: impl AsRef<str>) -> std::result::Result<Self, crate::naming::text::TextError> {
        PageTitle::new(title.as_ref()).map(Page::new)
    }
}

/// What a `ModifyPage` changed ([`Page::change`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageChange<'a> {
    pub title: bool,
    pub number: bool,
    pub folder: bool,
    pub page_type: bool,
    pub sheet: bool,
    pub title_block: bool,
    pub location: bool,
    pub function: bool,
    /// The display keys whose value changed.
    pub display: Vec<&'a str>,
}

/// A folder of pages (brief 019, decision FP3): a name and a place in the tree. Pages name their
/// folder in their `folder` field; sibling order is the relative order in
/// [`Document::folder_order`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub name: String,
    /// The folder it is in; `None` at the top level.
    pub parent: Option<FolderId>,
    /// The number its pages are renumbered from (brief 054, FP7; design 072 §5.2); `None` keeps
    /// the run of the folder above. Left out of records while unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<PageNo>,
}

impl Folder {
    /// A folder named `name` in `parent`, with no start number.
    pub fn new(name: impl Into<String>, parent: Option<FolderId>) -> Folder {
        Folder { name: name.into(), parent, start: None }
    }
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
        rotation: Radians,
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

pub(super) fn is_default<T: Default + PartialEq>(t: &T) -> bool {
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

/// What a representation depicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Subject {
    Device(DeviceId),
    Wire(WireId),
    Cable(CableId),
    /// A route's drawing (design 072 F31).
    Route(RouteId),
    /// A drawing aid's drawing (design 072 §6.2).
    Aid(crate::naming::id::AidId),
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
    /// What it is for its subject (design 072 §4.4, F3′): a device symbol's part, or a fixed role.
    pub role: Role,
    pub items: Vec<ItemId>,
    /// Drawn form: where each terminal sits on the page. Symbol form (brief 018, decision on
    /// I14): empty for a symbol of the component's own definition, whose terminals are matched to
    /// the definition by key; otherwise where each terminal sits in the **definition's local
    /// frame**, so the terminals follow the instance's placement.
    pub connection_points: BTreeMap<ConnectionPointId, Pt>,
}
