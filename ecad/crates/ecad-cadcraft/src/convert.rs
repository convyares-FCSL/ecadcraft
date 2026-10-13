//! Conversion between ECAD items and CADCraft entities.
//!
//! Every conversion is checked by converting back: an entity is only accepted when the ECAD item
//! reproduces it exactly (H2, projection fidelity). Anything else is reported as unsupported
//! rather than silently simplified.

use cadcraft_engine::doc::{
    Arc as CArc, Circle, Common, Entity, EntityKind, Handle, Insert, Line, Lineweight, LwPolyline, Point as CPoint,
    Text,
};
use cadcraft_engine::geom::{PolyVertex, Vec2, Vec3};
use ecad_model::{
    Color, DefinitionId, Document, Item, ItemKind, LayerId, PageId, Placement, Primitive, Pt, Style, SymbolInstance,
};

/// Layer that carries terminal markers in projected drawings. Reserved: users' edits on it are refused.
pub const MARKER_LAYER: &str = "ECAD-TERMINALS";
/// Prefix of projected block and group names. Reserved.
pub const ECAD_PREFIX: &str = "ECAD$";

/// Block name for a definition.
pub fn block_name(def: DefinitionId) -> String {
    format!("{ECAD_PREFIX}{:032x}", def.0)
}

/// Definition for a block name, if it is one of ours.
pub fn definition_of_block(name: &str) -> Option<DefinitionId> {
    name.strip_prefix(ECAD_PREFIX).and_then(|h| u128::from_str_radix(h, 16).ok()).map(DefinitionId)
}

fn v3(p: Pt) -> Vec3 {
    Vec3::new(p.x, p.y, 0.0)
}
fn pt(v: Vec3) -> Pt {
    Pt::new(v.x, v.y)
}

fn color_to(c: Color) -> cadcraft_engine::doc::color::Color {
    use cadcraft_engine::doc::color::Color as C;
    match c {
        Color::ByLayer => C::ByLayer,
        Color::ByBlock => C::ByBlock,
        Color::Index(i) => C::Index(i),
        Color::Rgb(v) => C::True(cadcraft_engine::doc::color::Rgb::from_u32(v)),
    }
}

fn color_from(c: cadcraft_engine::doc::color::Color) -> Color {
    use cadcraft_engine::doc::color::Color as C;
    match c {
        C::ByLayer => Color::ByLayer,
        C::ByBlock => Color::ByBlock,
        C::Index(i) => Color::Index(i),
        C::True(rgb) => Color::Rgb(rgb.to_u32()),
    }
}

/// CADCraft common properties for an ECAD style.
pub fn common_for(style: &Style, layer_name: &str) -> Common {
    Common {
        layer: layer_name.to_string(),
        color: color_to(style.color),
        linetype: style.linetype.clone(),
        lineweight: Lineweight::from_dxf(style.lineweight),
        ..Common::default()
    }
}

/// The CADCraft kind for a primitive.
pub fn kind_for_primitive(p: &Primitive) -> EntityKind {
    match p {
        Primitive::Line { a, b } => EntityKind::Line(Line { a: v3(*a), b: v3(*b) }),
        Primitive::Polyline { vertices, closed } => EntityKind::LwPolyline(LwPolyline {
            vertices: vertices
                .iter()
                .map(|p| PolyVertex { p: Vec2::new(p.x, p.y), bulge: 0.0, start_width: 0.0, end_width: 0.0 })
                .collect(),
            closed: *closed,
            const_width: 0.0,
            elevation: 0.0,
            plinegen: false,
        }),
        Primitive::Arc { center, radius, start, end } => {
            EntityKind::Arc(CArc { center: v3(*center), radius: *radius, start: *start, end: *end })
        }
        Primitive::Circle { center, radius } => EntityKind::Circle(Circle { center: v3(*center), radius: *radius }),
        Primitive::Text { at, height, rotation, value } => EntityKind::Text(Text {
            insert: v3(*at),
            align_pt: None,
            height: *height,
            value: value.clone(),
            rotation: *rotation,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: Default::default(),
            valign: Default::default(),
        }),
        Primitive::Point { at } => EntityKind::Point(CPoint { p: v3(*at), angle: 0.0 }),
    }
}

fn kind_for_symbol(s: &SymbolInstance) -> EntityKind {
    let sx = if s.placement.mirror { -s.placement.scale } else { s.placement.scale };
    EntityKind::Insert(Insert {
        block: block_name(s.definition),
        insert: v3(s.placement.origin),
        scale: Vec3::new(sx, s.placement.scale, 1.0),
        rotation: s.placement.rotation,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
    })
}

/// The entity for an item, with the given handle.
pub fn entity_for(doc: &Document, item: &Item, handle: Handle) -> Option<Entity> {
    let layer = doc.layers.get(&item.style.layer)?;
    let kind = match &item.kind {
        ItemKind::Primitive(p) => kind_for_primitive(p),
        ItemKind::Symbol(s) => kind_for_symbol(s),
    };
    Some(Entity { handle, common: common_for(&item.style, &layer.name), kind })
}

/// A terminal marker entity.
pub fn marker_entity(at: Pt, handle: Handle) -> Entity {
    Entity {
        handle,
        common: Common { layer: MARKER_LAYER.into(), ..Common::default() },
        kind: EntityKind::Point(CPoint { p: v3(at), angle: 0.0 }),
    }
}

/// Position of a marker entity, if it is one.
pub fn marker_position(e: &Entity) -> Option<Pt> {
    match &e.kind {
        EntityKind::Point(p) if e.common.layer == MARKER_LAYER => Some(pt(p.p)),
        _ => None,
    }
}

fn primitive_from(kind: &EntityKind) -> Option<Primitive> {
    Some(match kind {
        EntityKind::Line(l) => Primitive::Line { a: pt(l.a), b: pt(l.b) },
        EntityKind::LwPolyline(p) => Primitive::Polyline {
            vertices: p.vertices.iter().map(|v| Pt::new(v.p.x, v.p.y)).collect(),
            closed: p.closed,
        },
        EntityKind::Arc(a) => Primitive::Arc { center: pt(a.center), radius: a.radius, start: a.start, end: a.end },
        EntityKind::Circle(c) => Primitive::Circle { center: pt(c.center), radius: c.radius },
        EntityKind::Text(t) => {
            Primitive::Text { at: pt(t.insert), height: t.height, rotation: t.rotation, value: t.value.clone() }
        }
        EntityKind::Point(p) => Primitive::Point { at: pt(p.p) },
        _ => return None,
    })
}

fn symbol_from(kind: &EntityKind) -> Option<SymbolInstance> {
    let EntityKind::Insert(i) = kind else { return None };
    let definition = definition_of_block(&i.block)?;
    let scale = i.scale.y;
    Some(SymbolInstance {
        definition,
        placement: Placement { origin: pt(i.insert), rotation: i.rotation, mirror: i.scale.x < 0.0, scale },
    })
}

/// Why an entity cannot be recorded.
pub fn kind_name(kind: &EntityKind) -> &'static str {
    match kind {
        EntityKind::Line(_) => "line",
        EntityKind::LwPolyline(_) => "polyline",
        EntityKind::Arc(_) => "arc",
        EntityKind::Circle(_) => "circle",
        EntityKind::Text(_) => "text",
        EntityKind::Point(_) => "point",
        EntityKind::Insert(_) => "block reference",
        EntityKind::Hatch(_) => "hatch",
        EntityKind::Dimension(_) => "dimension",
        EntityKind::MText(_) => "multiline text",
        _ => "object of this kind",
    }
}

/// The ECAD item for an entity on `page`, or why it cannot be recorded.
///
/// # Semantics
/// The entity is accepted only if [`entity_for`] reproduces it exactly from the item.
///
/// # Errors
/// A message naming what ECAD pages do not support yet.
pub fn item_from(doc: &Document, page: PageId, e: &Entity) -> Result<Item, String> {
    if e.common.layer == MARKER_LAYER {
        return Err(format!("the {MARKER_LAYER} layer is reserved for terminal markers"));
    }
    let layer: LayerId =
        doc.layers.iter().find(|(_, l)| l.name == e.common.layer).map(|(id, _)| *id).ok_or_else(|| {
            format!("layer {} is not part of the project (layer edits are not supported yet)", e.common.layer)
        })?;
    let style = Style {
        layer,
        color: color_from(e.common.color),
        linetype: e.common.linetype.clone(),
        lineweight: e.common.lineweight.to_dxf(),
    };
    let kind = match (primitive_from(&e.kind), symbol_from(&e.kind)) {
        (Some(p), _) => ItemKind::Primitive(p),
        (_, Some(s)) => ItemKind::Symbol(s),
        _ => return Err(format!("{}s are not supported on ECAD pages yet", kind_name(&e.kind))),
    };
    let item = Item { page, style, kind };
    match entity_for(doc, &item, e.handle) {
        Some(back) if back == *e => Ok(item),
        _ => Err(format!("this {} has properties ECAD pages do not record yet", kind_name(&e.kind))),
    }
}
