//! The engine half of [`crate::wires`]: wire routes in their colours, line types and dashes,
//! conductor labels as text entities in the page's text styles, the label layout measured with
//! CADCraft's fonts, and the quiet ends of the cables drawn on a page.

use cadcraft_engine::doc::{Common, Entity, EntityKind, HAlign, Handle, Text, VAlign};
use cadcraft_engine::geom::Vec3;
use ecad_model::{Document, Form, ItemId, PageId, Pt, Subject};

use crate::styles::draw::{DrawApply as _, LOOK};
use {ecad_electrical::wires::DASH_LAYER, ecad_electrical::wires::LABEL_LAYER, ecad_electrical::wires::labels::WireLabel};

/// [`labels_on_page`] without the routes whose items are `hidden` (a cable drawn as a fan: its
/// members' routes and their labels are not shown), measured with
/// CADCraft's fonts ([`ecad_electrical::wires::labels::labels_measured`]).
pub fn labels_on_page_with(doc: &Document, page: PageId, hidden: &std::collections::BTreeSet<ItemId>) -> Vec<WireLabel> {
    ecad_electrical::wires::labels::labels_measured(doc, page, hidden, &LOOK)
}

/// Draw the page's wires and cores in their colour codes: the route entity
/// takes the first colour, and a two-colour code adds the second colour's 2 mm on / 2 mm off
/// dashes over it on [`DASH_LAYER`], mapped to the conductor like its label (picking a dash
/// picks the wire, [`crate::cables::draw::pick_routes`]). Colours are those of `sheet`, the sheet the
/// page is projected for. An unknown code leaves the route in ink.
/// Derived on every projection of the page, never stored.
pub(crate) fn colour_routes(
    doc: &Document, page: PageId, d: &mut cadcraft_engine::doc::Drawing, ents: &mut Vec<Entity>, map: &mut crate::project::HandleMap,
    prev: &crate::project::HandleMap, sheet: &ecad_model::palette::Sheet,
) {
    use cadcraft_engine::doc::color::Color;
    let routes = crate::cables::draw::coloured_routes(doc, page, Some(map), sheet);
    if routes.is_empty() {
        return;
    }
    if d.layer(DASH_LAYER).is_none() {
        d.layers.push(cadcraft_engine::doc::Layer::new(DASH_LAYER));
    }
    let index: std::collections::HashMap<Handle, usize> = ents.iter().enumerate().map(|(i, e)| (e.handle, i)).collect();
    // The page's dash budget: past it a two-colour wire keeps its first colour only (Checks says).
    let budget = ecad_electrical::cables::words::dash_budget(doc, page);
    let mut dashes = Vec::new();
    for r in routes {
        let Some(route) = r.handle else { continue };
        let Some(e) = index.get(&route).and_then(|i| ents.get_mut(*i)) else { continue };
        e.common.color = Color::True(crate::cables::draw::rgb(r.colours.solid));
        map.insert_coloured(route);
        if !budget.dashed.contains(&r.rep) {
            continue;
        }
        let common = e.common.clone();
        for (i, mut dash) in route_dashes(&r, &common).into_iter().enumerate() {
            let Ok(i) = u32::try_from(i) else { break };
            dash.handle = prev.dash(r.rep, i).unwrap_or_else(|| d.new_handle());
            map.insert_dash(r.rep, r.conductor, i, dash.handle, route);
            dashes.push(dash);
        }
    }
    ents.extend(dashes);
}

/// Draw `page`'s wire routes in their category's line type and
/// weight where these differ from the built-in (continuous, 0.25 mm), on screen and in print (a
/// mono print keeps them: they are not colour). `item_handle` finds a route item's entity.
pub(crate) fn style_routes(doc: &Document, page: PageId, ents: &mut [Entity], item_handle: impl Fn(ItemId) -> Option<Handle>) {
    let draws = ecad_electrical::styles::wire::WireDraws::new(doc);
    let index: std::collections::HashMap<Handle, usize> = ents.iter().enumerate().map(|(i, e)| (e.handle, i)).collect();
    for (_, r) in doc.representations_on(page) {
        let Subject::Route(x) = r.subject else { continue };
        if r.form != Form::Drawn {
            continue;
        }
        // A line looks as the wire it stands for.
        let Some(c) = ecad_electrical::wires::routes::wire_of_route(doc, x) else { continue };
        let draw = draws.draw(doc, c);
        let weight = (draw.line_weight * 100.0).round().clamp(0.0, 211.0) as u16;
        if draw.line_type == "continuous" && weight == ecad_electrical::cables::words::WIRE_WEIGHT {
            continue;
        }
        let Some(e) = r.items.first().and_then(|i| item_handle(*i)).and_then(|h| index.get(&h)).and_then(|i| ents.get_mut(*i)) else { continue };
        if draw.line_type != "continuous" {
            e.common.linetype = ecad_electrical::styles::wire::linetype(draw.line_type).into();
        }
        e.common.lineweight = cadcraft_engine::doc::Lineweight::Mm100(weight);
    }
}

/// The dash entities of a two-colour route (handles unset): the second colour's 2 mm on / 2 mm
/// off lines on [`DASH_LAYER`], with the route entity's `common` otherwise (its lineweight).
/// None for a one-colour route. Also what the page check expects ([`crate::dev::verify`]).
pub(crate) fn route_dashes(r: &crate::cables::draw::ColouredRoute, route: &Common) -> Vec<Entity> {
    use cadcraft_engine::doc::color::Color;
    let Some(second) = r.colours.dashes else { return Vec::new() };
    let (on, off) = (f64::from(ecad_model::palette::wire::DASH_ON_MM), f64::from(ecad_model::palette::wire::DASH_OFF_MM));
    let common = Common { layer: DASH_LAYER.into(), color: Color::True(crate::cables::draw::rgb(second)), ..route.clone() };
    ecad_electrical::cables::words::dash_segments(&r.points, on, off)
        .into_iter()
        .map(|(a, b)| Entity {
            handle: Handle::default(),
            common: common.clone(),
            kind: EntityKind::Line(cadcraft_engine::doc::Line { a: Vec3::new(a.x, a.y, 0.0), b: Vec3::new(b.x, b.y, 0.0) }),
        })
        .collect()
}

/// Every conductor label on `page`: one per route, on its longest segment.
pub fn labels_on_page(doc: &Document, page: PageId) -> Vec<WireLabel> {
    labels_on_page_with(doc, page, &std::collections::BTreeSet::new())
}

/// A label as an entity on [`LABEL_LAYER`].
///
/// Its style (the wire-label kind's font, weight, and bold lineweight in the stroke font) comes
/// from the page's resolved `draw` of the page ([`crate::styles::draw::DrawStyles::apply`]).
pub fn label_entity(draw: &crate::styles::draw::DrawStyles, l: &WireLabel, handle: Handle) -> Entity {
    let p = Vec3::new(l.at.x, l.at.y, 0.0);
    let color = if l.error {
        cadcraft_engine::doc::color::Color::Index(ecad_electrical::references::cables::ORPHAN_ACI)
    } else if l.provisional {
        cadcraft_engine::doc::color::Color::Index(ecad_electrical::devices::PROVISIONAL_ACI)
    } else {
        cadcraft_engine::doc::color::Color::ByLayer
    };
    let mut e = Entity {
        handle,
        common: Common { layer: LABEL_LAYER.into(), color, ..Common::default() },
        kind: EntityKind::Text(Text {
            insert: p,
            align_pt: Some(p),
            height: l.height,
            value: l.text.clone(),
            rotation: l.rotation,
            width_factor: 1.0,
            oblique: 0.0,
            style: ecad_drawing_style::text::PLACEHOLDER_STYLE.into(),
            halign: HAlign::Center,
            valign: VAlign::Bottom,
        }),
    };
    match &l.own {
        Some(o) => draw.apply_own(&mut e, ecad_model::look::TextKind::WireLabel, o),
        None => draw.apply(&mut e, ecad_model::look::TextKind::WireLabel),
    }
    // An error or provisional label keeps its own colour over the style's.
    if l.error || l.provisional {
        e.common.color = color;
    }
    e
}

/// The quiet wire ends on `page` (cable reference ends): the ends a new wire
/// never joins and that show no loose-end square. Derived from the document and the cables shown
/// as fans (`fan`), never from what is on screen. Only a page with a
/// cable reference has any, so other pages cost nothing.
pub fn quiet_ends_of(doc: &Document, page: PageId, fan: &std::collections::BTreeSet<ecad_model::CableId>, tol: f64) -> Vec<Pt> {
    if ecad_electrical::references::cables::markers_on_page(doc, page, &crate::styles::draw::LOOK).is_empty() {
        return Vec::new();
    }
    crate::cables::views::drawings_on_page(doc, page, fan, tol).into_values().flat_map(|d| d.quiet).collect()
}
