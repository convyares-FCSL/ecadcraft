//! The engine half of [`crate::devices`]: derived texts as CADCraft text entities in the page's
//! text styles, the rules' text layout measured with CADCraft's fonts, and the tag preview read
//! from a live [`crate::Project`].

use cadcraft_engine::doc::{Common, Entity, EntityKind, HAlign as EHAlign, Handle, Text, VAlign as EVAlign};
use cadcraft_engine::geom::Vec3;
use ecad_model::{ComponentId, Document, PageId, Pt, RepresentationId};

use crate::devices::{DerivedText, PageText, TAG_LAYER, TextOf, kind_of, prefix_for};
use crate::draw::text_style::{EngineMetrics, LookApply as _};
use crate::symbols::Starter;

/// [`all_texts_on_page`], reading PLC addresses from the kept model `plc` when there is one
/// (brief 045, review of #84, 3); measured with CADCraft's fonts
/// ([`crate::devices::all_texts_measured`]).
pub fn all_texts_on_page_with(doc: &Document, page: PageId, plc: Option<&crate::plc::kept::Plc>) -> Vec<PageText> {
    crate::devices::all_texts_measured(doc, page, plc, &EngineMetrics)
}

/// Every derived text shown on `page` (what the page draws and prints), measured with CADCraft's
/// fonts ([`crate::devices::texts_on_page_measured`]).
pub fn texts_on_page(doc: &Document, page: PageId) -> Vec<DerivedText> {
    crate::devices::texts_on_page_measured(doc, page, &EngineMetrics)
}

/// The lens of device `c` hovered through its representation `here` (design 018 §6.4), measured
/// with CADCraft's fonts ([`crate::devices::hover_lines_measured`]).
pub fn hover_lines(doc: &Document, c: ComponentId, here: RepresentationId) -> Vec<String> {
    crate::devices::hover_lines_measured(doc, c, here, &EngineMetrics)
}

/// Every derived text on `page`, shown or hidden: for each symbol representation of a component,
/// its device's tag block (the tag, then its description and part number when they have values)
/// and its terminals' labels; a continuation's name and other places. Visibility follows design
/// 031 §5.2 (symbol, device, page, project, built-in) and a tag set to once per run is shown on
/// the first symbol of its run only (§5.3). Hostile values (non-finite placements) give nothing
/// for that representation.
pub fn all_texts_on_page(doc: &Document, page: PageId) -> Vec<PageText> {
    all_texts_on_page_with(doc, page, None)
}

/// A derived text as an entity on [`TAG_LAYER`], in its kind's style from `look` (design 031 §3).
pub fn text_entity(look: &crate::text_style::Look, t: &DerivedText, handle: Handle) -> Entity {
    let mut e = text_entity_with(
        &t.value,
        t.at,
        t.height,
        crate::convert::engine_halign(t.halign),
        crate::convert::engine_valign(t.valign),
        handle,
        TAG_LAYER,
    );
    match &t.own {
        // Brief 050a: the device's own text style (its height is already the text's).
        Some(s) => look.apply_own(&mut e, kind_of(t.of), s),
        None => look.apply(&mut e, kind_of(t.of)),
    }
    // Brief 045 (design 045 §7.1): a channel description reads upwards under its cell.
    if let (TextOf::ChannelText { rotated: true }, EntityKind::Text(x)) = (t.of, &mut e.kind) {
        x.rotation = std::f64::consts::FRAC_PI_2;
    }
    if let Some(aci) = t.aci {
        e.common.color = cadcraft_engine::doc::color::Color::Index(aci);
    }
    e
}

/// A justified text entity.
pub fn text_entity_with(value: &str, at: Pt, height: f64, halign: EHAlign, valign: EVAlign, handle: Handle, layer: &str) -> Entity {
    let p = Vec3::new(at.x, at.y, 0.0);
    let justified = !(halign == EHAlign::Left && valign == EVAlign::Baseline);
    Entity {
        handle,
        common: Common { layer: layer.into(), ..Common::default() },
        kind: EntityKind::Text(Text {
            insert: p,
            align_pt: justified.then_some(p),
            height,
            value: value.to_string(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign,
            valign,
        }),
    }
}

/// The tag the next new device of symbol `s` would get in project `p`, without using it up (for
/// prompts and pick lists).
pub fn next_tag_preview(p: &crate::project::Project, s: &Starter) -> Option<String> {
    let doc = p.ledger.document();
    p.tag_high.peek_auto(doc, &prefix_for(doc, s)).ok()
}
