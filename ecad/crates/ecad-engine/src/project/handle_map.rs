//! ECAD ID ⇄ CADCraft handle for one page (session only).

use super::*;

/// What a projected handle stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapped {
    Item(ecad_model::ItemId),
    /// A terminal marker of a drawn representation.
    Marker(RepresentationId, ConnectionPointId),
    /// A line of a cable's fan view (brief 005): computed, not an item. Edits of it are refused.
    Fan(CableId),
    /// The derived text of a cable reference (brief 010), by its reference representation:
    /// computed, not an item. Erasing it erases its reference; other edits are refused.
    RefText(RepresentationId),
    /// A device's derived tag text or terminal label beside one of its symbols (brief 018), by
    /// the symbol representation: computed, not an item. Erasing it erases its representation;
    /// other edits of it alone are refused.
    Tag(RepresentationId),
    /// A conductor's derived label (brief 022): computed, not an item. Left out of an edit that
    /// also touches other objects; an edit of it alone is refused.
    Label(WireId),
    /// A line or text of the page's sheet frame or title block (brief 019), by its index among
    /// the frame's parts: computed, not an item. Every edit of it is refused.
    Frame(u32),
    /// A device cross-reference (brief 025): the text under a contact (by the contact's
    /// representation) or a part of the contact table under a head (by the head's): computed,
    /// not an item. Left out of an edit that also touches other objects; an edit of it alone is
    /// refused.
    DeviceReference(RepresentationId),
    /// A segment of a strip's link line (brief 030, design 030 §5.3), by strip and link number:
    /// computed, not an item. Selecting it selects the link; erasing it deletes the link; other
    /// edits of it alone are refused, and it is left out of an edit with other objects.
    Link(DeviceId, u32),
    /// A ring of a T's dot (design 072 §4.4), by the route that ends on another route: computed,
    /// not an item. Left out of an edit that also touches other objects; an edit of it alone is
    /// refused.
    Tee(ecad_model::RouteId),
}

/// ECAD ID ⇄ CADCraft handle for one page. Session-only, never saved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HandleMap {
    by_handle: HashMap<Handle, Mapped>,
    items: BTreeMap<ecad_model::ItemId, Handle>,
    markers: BTreeMap<(RepresentationId, ConnectionPointId), Handle>,
    /// Computed cable drawing (fans, core labels, reference flags; briefs 005, 009): (cable,
    /// index of the part) → handle.
    fans: BTreeMap<(CableId, u32), Handle>,
    /// The cables drawn as fans on this page.
    fan_cables: BTreeSet<CableId>,
    /// The member and the reference end each computed part belongs to, where it has one.
    parts: HashMap<Handle, (Option<WireId>, Option<RepresentationId>)>,
    /// Loose wire ends at a cable end that stops at a reference (design 009 §7.3): no loose-end
    /// square is shown there.
    pub(super) quiet: Vec<Pt>,
    /// Brief 032 §2.6: open ends lying on another wire (the yellow looks-connected ring).
    pub(super) looks: Vec<Pt>,
    /// Reference texts (brief 010): reference representation → handle.
    refs: BTreeMap<RepresentationId, Handle>,
    /// Tag texts and terminal labels (brief 018): (symbol representation, which text) → handle.
    tags: BTreeMap<(RepresentationId, crate::devices::TextOf), Handle>,
    /// Wire labels (brief 022): route representation → (conductor, handle).
    labels: BTreeMap<RepresentationId, (WireId, Handle)>,
    /// The sheet frame's parts (brief 019): index → handle.
    frame: BTreeMap<u32, Handle>,
    /// Device cross-references (brief 025): (contact or head representation, part index) → handle.
    xrefs: BTreeMap<(RepresentationId, u32), Handle>,
    /// The part each cross-reference handle is.
    device_reference_parts: HashMap<Handle, (RepresentationId, u32)>,
    /// Brief 030: link line segments: (strip, link, index) → handle.
    links: BTreeMap<(DeviceId, u32, u32), Handle>,
    /// Brief 040: a two-colour wire's dashes, (route representation, index) → handle, mapped to
    /// the conductor as its label is (derived: left out of an edit with other objects, refused
    /// alone), and each dash's route handle.
    dashes: BTreeMap<(RepresentationId, u32), Handle>,
    dash_routes: HashMap<Handle, Handle>,
    /// Brief 040: the route handles drawn in a wire colour.
    coloured: BTreeSet<Handle>,
    /// Lane 072 D: the rings of the T dots, (route, end index, ring) → handle.
    tees: BTreeMap<(ecad_model::RouteId, u8, u8), Handle>,
}

impl HandleMap {
    pub fn get(&self, h: Handle) -> Option<Mapped> {
        self.by_handle.get(&h).copied()
    }
    pub fn item(&self, id: ecad_model::ItemId) -> Option<Handle> {
        self.items.get(&id).copied()
    }
    pub fn marker(&self, rep: RepresentationId, t: ConnectionPointId) -> Option<Handle> {
        self.markers.get(&(rep, t)).copied()
    }
    pub fn insert_item(&mut self, id: ecad_model::ItemId, h: Handle) {
        self.items.insert(id, h);
        self.by_handle.insert(h, Mapped::Item(id));
    }
    pub fn insert_marker(&mut self, rep: RepresentationId, t: ConnectionPointId, h: Handle) {
        self.markers.insert((rep, t), h);
        self.by_handle.insert(h, Mapped::Marker(rep, t));
    }
    pub fn fan(&self, cable: CableId, index: u32) -> Option<Handle> {
        self.fans.get(&(cable, index)).copied()
    }
    pub fn insert_fan(&mut self, cable: CableId, index: u32, h: Handle) {
        self.fans.insert((cable, index), h);
        self.by_handle.insert(h, Mapped::Fan(cable));
    }
    /// A computed part of `cable`'s drawing (brief 009): `fan` when the cable is drawn as a fan on
    /// this page; the member (`core`) and the reference end it belongs to.
    pub fn insert_part(&mut self, cable: CableId, index: u32, h: Handle, fan: bool, core: Option<WireId>, reference: Option<RepresentationId>) {
        self.insert_fan(cable, index, h);
        if fan {
            self.fan_cables.insert(cable);
        }
        if core.is_some() || reference.is_some() {
            self.parts.insert(h, (core, reference));
        }
    }
    /// Loose wire ends that show no loose-end square (design 009 §7.3).
    pub fn quiet_ends(&self) -> &[Pt] {
        &self.quiet
    }
    /// Open ends that look connected (design 032 §2.6), computed with the page's projection.
    pub fn looks_connected(&self) -> &[Pt] {
        &self.looks
    }
    /// Whether `cable` is drawn as a fan on this page.
    pub fn is_fan(&self, cable: CableId) -> bool {
        self.fan_cables.contains(&cable)
    }
    /// The member a computed part belongs to (a bend, a tail, a core label, a wire number).
    pub fn core_of(&self, h: Handle) -> Option<WireId> {
        self.parts.get(&h).and_then(|(c, _)| *c)
    }
    /// The reference end a computed part belongs to (its flag or text).
    pub fn reference_of(&self, h: Handle) -> Option<RepresentationId> {
        self.parts.get(&h).and_then(|(_, r)| *r)
    }
    /// Every handle of a fan view line.
    pub fn fan_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.fans.values().copied()
    }
    /// The handle of a reference's derived text.
    pub fn ref_text(&self, reference: RepresentationId) -> Option<Handle> {
        self.refs.get(&reference).copied()
    }
    pub fn insert_ref_text(&mut self, reference: RepresentationId, h: Handle) {
        self.refs.insert(reference, h);
        self.by_handle.insert(h, Mapped::RefText(reference));
    }
    /// The handle of a derived tag text or terminal label.
    pub fn tag_text(&self, rep: RepresentationId, of: crate::devices::TextOf) -> Option<Handle> {
        self.tags.get(&(rep, of)).copied()
    }
    pub fn insert_tag_text(&mut self, rep: RepresentationId, of: crate::devices::TextOf, h: Handle) {
        self.tags.insert((rep, of), h);
        self.by_handle.insert(h, Mapped::Tag(rep));
    }
    /// The derived text a handle draws (brief 031: Show/Hide Text and Move Text pick by it).
    pub fn tag_text_of(&self, h: Handle) -> Option<(RepresentationId, crate::devices::TextOf)> {
        self.tags.iter().find(|(_, x)| **x == h).map(|(k, _)| *k)
    }
    /// Every handle of a tag text or terminal label.
    pub fn tag_text_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.tags.values().copied()
    }
    /// The handle of a conductor label, by its route representation.
    pub fn label(&self, rep: RepresentationId) -> Option<Handle> {
        self.labels.get(&rep).map(|(_, h)| *h)
    }
    pub fn insert_label(&mut self, rep: RepresentationId, conductor: WireId, h: Handle) {
        self.labels.insert(rep, (conductor, h));
        self.by_handle.insert(h, Mapped::Label(conductor));
    }
    /// Every handle of a conductor label.
    pub fn label_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.labels.values().map(|(_, h)| *h)
    }
    /// The handle of frame part `index`.
    pub fn frame_part(&self, index: u32) -> Option<Handle> {
        self.frame.get(&index).copied()
    }
    pub fn insert_frame(&mut self, index: u32, h: Handle) {
        self.frame.insert(index, h);
        self.by_handle.insert(h, Mapped::Frame(index));
    }
    /// The handle of cross-reference part `index` of `rep` (brief 025).
    pub fn xref(&self, rep: RepresentationId, index: u32) -> Option<Handle> {
        self.xrefs.get(&(rep, index)).copied()
    }
    /// Brief 030: the handle of segment `index` of link `n` of `strip`.
    pub fn link(&self, strip: DeviceId, n: u32, index: u32) -> Option<Handle> {
        self.links.get(&(strip, n, index)).copied()
    }
    pub fn insert_link(&mut self, strip: DeviceId, n: u32, index: u32, h: Handle) {
        self.links.insert((strip, n, index), h);
        self.by_handle.insert(h, Mapped::Link(strip, n));
    }
    /// Lane 072 D: the handle of ring `ring` of the T dot at end `end` of route `r`.
    pub fn tee(&self, r: ecad_model::RouteId, end: u8, ring: u8) -> Option<Handle> {
        self.tees.get(&(r, end, ring)).copied()
    }
    pub fn insert_tee(&mut self, r: ecad_model::RouteId, end: u8, ring: u8, h: Handle) {
        self.tees.insert((r, end, ring), h);
        self.by_handle.insert(h, Mapped::Tee(r));
    }
    /// Every handle of a T dot's ring.
    pub fn tee_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.tees.values().copied()
    }
    /// Brief 040: the handle of dash `index` of route `rep`.
    pub fn dash(&self, rep: RepresentationId, index: u32) -> Option<Handle> {
        self.dashes.get(&(rep, index)).copied()
    }
    pub fn insert_dash(&mut self, rep: RepresentationId, conductor: WireId, index: u32, h: Handle, route: Handle) {
        self.dashes.insert((rep, index), h);
        self.dash_routes.insert(h, route);
        self.by_handle.insert(h, Mapped::Label(conductor));
    }
    /// The route a dash is laid over (picking a dash picks the wire).
    pub fn route_of_dash(&self, h: Handle) -> Option<Handle> {
        self.dash_routes.get(&h).copied()
    }
    /// Every handle of a dash.
    pub fn dash_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.dashes.values().copied()
    }
    /// Mark route handle `h` as drawn in a wire colour.
    pub fn insert_coloured(&mut self, h: Handle) {
        self.coloured.insert(h);
    }
    /// Whether route handle `h` is drawn in a wire colour.
    pub fn is_coloured(&self, h: Handle) -> bool {
        self.coloured.contains(&h)
    }
    pub fn insert_device_reference(&mut self, rep: RepresentationId, index: u32, h: Handle) {
        self.xrefs.insert((rep, index), h);
        self.device_reference_parts.insert(h, (rep, index));
        self.by_handle.insert(h, Mapped::DeviceReference(rep));
    }
    /// Which cross-reference part a handle is: its representation and index.
    pub fn device_reference_part(&self, h: Handle) -> Option<(RepresentationId, u32)> {
        self.device_reference_parts.get(&h).copied()
    }
    /// Every handle of a cross-reference part.
    pub fn device_reference_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.xrefs.values().copied()
    }
    /// Every handle of the sheet frame.
    pub fn frame_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.frame.values().copied()
    }
    /// Every handle of a reference text.
    pub fn ref_text_handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.refs.values().copied()
    }
    pub fn len(&self) -> usize {
        self.by_handle.len()
    }
    pub fn is_empty(&self) -> bool {
        self.by_handle.is_empty()
    }
    /// Every mapped handle.
    pub fn handles(&self) -> impl Iterator<Item = Handle> + '_ {
        self.by_handle.keys().copied()
    }
    /// Whether the map is one-to-one in both directions.
    pub fn is_consistent(&self) -> bool {
        self.by_handle.len()
            == self.items.len()
                + self.markers.len()
                + self.fans.len()
                + self.refs.len()
                + self.tags.len()
                + self.labels.len()
                + self.frame.len()
                + self.xrefs.len()
                + self.links.len()
                + self.dashes.len()
                + self.tees.len()
            && self.dash_routes.len() == self.dashes.len()
            && self.dashes.values().all(|h| matches!(self.by_handle.get(h), Some(Mapped::Label(_))))
            && self.links.iter().all(|((c, n, _), h)| self.by_handle.get(h) == Some(&Mapped::Link(*c, *n)))
            && self.device_reference_parts.len() == self.xrefs.len()
            && self.xrefs.iter().all(|(k, h)| self.by_handle.get(h) == Some(&Mapped::DeviceReference(k.0)) && self.device_reference_parts.get(h) == Some(k))
            && self.items.iter().all(|(id, h)| self.by_handle.get(h) == Some(&Mapped::Item(*id)))
            && self.markers.iter().all(|((r, t), h)| self.by_handle.get(h) == Some(&Mapped::Marker(*r, *t)))
            && self.fans.iter().all(|((k, _), h)| self.by_handle.get(h) == Some(&Mapped::Fan(*k)))
            && self.refs.iter().all(|(r, h)| self.by_handle.get(h) == Some(&Mapped::RefText(*r)))
            && self.tags.iter().all(|((r, _), h)| self.by_handle.get(h) == Some(&Mapped::Tag(*r)))
            && self.labels.values().all(|(c, h)| self.by_handle.get(h) == Some(&Mapped::Label(*c)))
            && self.frame.iter().all(|(i, h)| self.by_handle.get(h) == Some(&Mapped::Frame(*i)))
            && self.tees.iter().all(|((r, _, _), h)| self.by_handle.get(h) == Some(&Mapped::Tee(*r)))
    }
}
