//! The document: every map of objects, its indexes and its checks.

use std::collections::BTreeMap;

use crate::document::index::{Index, referrers};

use serde::{Deserialize, Serialize};

use crate::error::{ModelError, Result};
use crate::naming::id::{CableId, ConnectionPointId, DefinitionId, DeviceId, FolderId, ItemId, LayerId, PageId, RepresentationId, RouteId, WireId};

use super::{Properties, cable::*, definition::*, device::*, drawing::*, settings::*};
use crate::objects::wire::{End, Wire};

/// The authoritative document: everything stored, nothing derived.
///
/// # Semantics
/// Maps are ordered by ID so iteration, serialisation and comparison are deterministic.
/// [`Document::check`] verifies every invariant and [`Document::check_touched`] those a change
/// could have affected; [`crate::Ledger`] runs the scoped check after each transaction so
/// an invalid state is never committed (see [`crate::document::invariant`]).
///
/// The document also keeps reverse indexes (the private `index` module) for queries such as
/// [`Document::cables_of`]. They are derived: not serialised, not compared by `==`, and kept up
/// to date by [`crate::Op::apply`]. The maps are read through accessors (`doc.devices()`); the file reader builds a
/// document with [`Document::from_parts`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(from = "DocumentData")]
pub struct Document {
    pub(crate) pages: BTreeMap<PageId, Page>,
    pub(crate) page_order: Vec<PageId>,
    pub(crate) layers: BTreeMap<LayerId, Layer>,
    pub(crate) items: BTreeMap<ItemId, Item>,
    pub(crate) devices: BTreeMap<DeviceId, Device>,
    pub(crate) connection_points: BTreeMap<ConnectionPointId, ConnectionPoint>,
    pub(crate) representations: BTreeMap<RepresentationId, Representation>,
    pub(crate) wires: BTreeMap<WireId, Wire>,
    pub(crate) cables: BTreeMap<CableId, Cable>,
    /// The lines drawn on pages (design 072 F31).
    pub(crate) routes: BTreeMap<RouteId, crate::objects::route::Route>,
    /// The drawing aids: continuations and location boxes (design 072 §6.2).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) aids: BTreeMap<crate::naming::id::AidId, crate::objects::aid::Aid>,
    /// The loose parts: panel-only material on the BOM (design 072 §6.4).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) loose_parts: BTreeMap<crate::naming::id::LoosePartId, crate::objects::loose_part::LoosePart>,
    /// The typed links between devices (design 072 §2.6).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) device_links: BTreeMap<crate::naming::id::LinkId, crate::objects::user::DeviceLink>,
    pub(crate) definitions: BTreeMap<DefinitionId, Definition>,
    pub(crate) settings: ProjectSettings,
    /// Named versions (bookmarks): name → the `seq` it names (§6.1). History, never pruned.
    #[serde(with = "crate::document::rev::as_points")]
    pub(crate) versions: BTreeMap<crate::PointName, u64>,
    /// The project's own display bag (brief 019, decision FP1): title, drawing number, customer,
    /// status, default function and location, revisions (`rev.<n>.<field>`) and free ones.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) display: Properties,
    /// The project's own data: the location and function lists (design 072 §5.3).
    #[serde(skip_serializing_if = "crate::document::project::Project::is_empty")]
    pub(crate) project: crate::document::project::Project,
    /// Folders of pages (brief 019, decision FP3).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) folders: BTreeMap<FolderId, Folder>,
    /// Every folder once; siblings are ordered as they appear here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) folder_order: Vec<FolderId>,
    /// The project's standard (brief 050a, FP1); `None` is IEC (built in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) standard: Option<crate::objects::styles::Standard>,
    /// The project layer of style records (brief 050a, FP2).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) styles: crate::objects::styles::StyleCatalogues,
    /// Derived reverse indexes; never serialised.
    #[serde(skip)]
    pub(crate) index: Index,
    /// Derived nets (brief 022), kept by [`crate::Op::apply`]; never serialised.
    #[serde(skip)]
    pub(crate) conn: crate::connectivity::Connectivity,
}

/// The serialised form of [`Document`]: its stored fields, without the derived index.
#[derive(Deserialize)]
pub(super) struct DocumentData {
    pages: BTreeMap<PageId, Page>,
    page_order: Vec<PageId>,
    layers: BTreeMap<LayerId, Layer>,
    items: BTreeMap<ItemId, Item>,
    devices: BTreeMap<DeviceId, Device>,
    connection_points: BTreeMap<ConnectionPointId, ConnectionPoint>,
    representations: BTreeMap<RepresentationId, Representation>,
    wires: BTreeMap<WireId, Wire>,
    cables: BTreeMap<CableId, Cable>,
    #[serde(default)]
    routes: BTreeMap<RouteId, crate::objects::route::Route>,
    #[serde(default)]
    aids: BTreeMap<crate::naming::id::AidId, crate::objects::aid::Aid>,
    #[serde(default)]
    loose_parts: BTreeMap<crate::naming::id::LoosePartId, crate::objects::loose_part::LoosePart>,
    #[serde(default)]
    device_links: BTreeMap<crate::naming::id::LinkId, crate::objects::user::DeviceLink>,
    definitions: BTreeMap<DefinitionId, Definition>,
    settings: ProjectSettings,
    #[serde(with = "crate::document::rev::as_points")]
    versions: BTreeMap<crate::PointName, u64>,
    #[serde(default)]
    display: Properties,
    #[serde(default)]
    project: crate::document::project::Project,
    #[serde(default)]
    folders: BTreeMap<FolderId, Folder>,
    #[serde(default)]
    folder_order: Vec<FolderId>,
    #[serde(default)]
    standard: Option<crate::objects::styles::Standard>,
    #[serde(default)]
    styles: crate::objects::styles::StyleCatalogues,
}

impl From<DocumentData> for Document {
    fn from(d: DocumentData) -> Self {
        let DocumentData {
            pages,
            page_order,
            layers,
            items,
            devices,
            connection_points,
            representations,
            wires,
            cables,
            routes,
            aids,
            loose_parts,
            device_links,
            definitions,
            settings,
            versions,
            display,
            project,
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
            devices,
            connection_points,
            representations,
            wires,
            cables,
            routes,
            aids,
            loose_parts,
            device_links,
            definitions,
            settings,
            versions,
            display,
            project,
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
            devices,
            connection_points,
            representations,
            wires,
            cables,
            routes,
            aids,
            loose_parts,
            device_links,
            definitions,
            settings,
            versions,
            display,
            project,
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
            && *devices == other.devices
            && *connection_points == other.connection_points
            && *representations == other.representations
            && *wires == other.wires
            && *cables == other.cables
            && *routes == other.routes
            && *aids == other.aids
            && *loose_parts == other.loose_parts
            && *device_links == other.device_links
            && *definitions == other.definitions
            && *settings == other.settings
            && *versions == other.versions
            && *display == other.display
            && *project == other.project
            && *folders == other.folders
            && *folder_order == other.folder_order
            && *standard == other.standard
            && *styles == other.styles
    }
}

/// The stored objects of a [`Document`], taken apart: what the file reader builds a document
/// from ([`Document::from_parts`]) and what fixtures change ([`Document::edit_parts`]).
#[derive(Debug, Clone, Default)]
pub struct DocumentParts {
    pub pages: BTreeMap<PageId, Page>,
    pub page_order: Vec<PageId>,
    pub layers: BTreeMap<LayerId, Layer>,
    pub items: BTreeMap<ItemId, Item>,
    pub devices: BTreeMap<DeviceId, Device>,
    pub connection_points: BTreeMap<ConnectionPointId, ConnectionPoint>,
    pub representations: BTreeMap<RepresentationId, Representation>,
    pub wires: BTreeMap<WireId, Wire>,
    pub cables: BTreeMap<CableId, Cable>,
    /// The lines drawn on pages (design 072 F31).
    pub routes: BTreeMap<RouteId, crate::objects::route::Route>,
    /// The drawing aids: continuations and location boxes (design 072 §6.2).
    pub aids: BTreeMap<crate::naming::id::AidId, crate::objects::aid::Aid>,
    /// The loose parts: panel-only material on the BOM (design 072 §6.4).
    pub loose_parts: BTreeMap<crate::naming::id::LoosePartId, crate::objects::loose_part::LoosePart>,
    /// The typed links between devices (design 072 §2.6).
    pub device_links: BTreeMap<crate::naming::id::LinkId, crate::objects::user::DeviceLink>,
    pub definitions: BTreeMap<DefinitionId, Definition>,
    pub settings: ProjectSettings,
    /// Named versions (bookmarks): name → the `seq` it names (§6.1). History, never pruned.
    pub versions: BTreeMap<crate::PointName, u64>,
    /// The project's own display bag (brief 019, decision FP1): title, drawing number, customer,
    /// status, default function and location, revisions (`rev.<n>.<field>`) and free ones.
    pub display: Properties,
    /// The project's own data: the location and function lists (design 072 §5.3).
    pub project: crate::document::project::Project,
    /// Folders of pages (brief 019, decision FP3).
    pub folders: BTreeMap<FolderId, Folder>,
    /// Every folder once; siblings are ordered as they appear here.
    pub folder_order: Vec<FolderId>,
    /// The project's standard (brief 050a, FP1); `None` is IEC (built in).
    pub standard: Option<crate::objects::styles::Standard>,
    /// The project layer of style records (brief 050a, FP2).
    pub styles: crate::objects::styles::StyleCatalogues,
}

impl Document {
    /// Build a document from its stored objects and recompute its indexes.
    ///
    /// # Semantics
    /// Builds the indexes only, not [`Document::check`], so a file with a broken state can still be
    /// loaded and reported. The document it returns has current indexes, as every document outside
    /// this crate does: nothing else needs to rebuild them.
    #[must_use]
    pub fn from_parts(parts: DocumentParts) -> Document {
        let DocumentParts {
            pages,
            page_order,
            layers,
            items,
            devices,
            connection_points,
            representations,
            wires,
            cables,
            routes,
            aids,
            loose_parts,
            device_links,
            definitions,
            settings,
            versions,
            display,
            project,
            folders,
            folder_order,
            standard,
            styles,
        } = parts;
        let mut doc = Document {
            pages,
            page_order,
            layers,
            items,
            devices,
            connection_points,
            representations,
            wires,
            cables,
            routes,
            aids,
            loose_parts,
            device_links,
            definitions,
            settings,
            versions,
            display,
            project,
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

    /// Take the document apart into its stored objects.
    #[must_use]
    pub fn into_parts(self) -> DocumentParts {
        let Document {
            pages,
            page_order,
            layers,
            items,
            devices,
            connection_points,
            representations,
            wires,
            cables,
            routes,
            aids,
            loose_parts,
            device_links,
            definitions,
            settings,
            versions,
            display,
            project,
            folders,
            folder_order,
            standard,
            styles,
            index: _,
            conn: _,
        } = self;
        DocumentParts {
            pages,
            page_order,
            layers,
            items,
            devices,
            connection_points,
            representations,
            wires,
            cables,
            routes,
            aids,
            loose_parts,
            device_links,
            definitions,
            settings,
            versions,
            display,
            project,
            folders,
            folder_order,
            standard,
            styles,
        }
    }

    /// Change the stored objects directly, then recompute the indexes.
    ///
    /// For test fixtures and the file reader. Production changes go through ops and the ledger,
    /// which keep the history, the checks and the indexes.
    ///
    /// Call it once per fixture: each call rebuilds every index.
    pub fn edit_parts<R>(&mut self, edit: impl FnOnce(&mut DocumentParts) -> R) -> R {
        let mut parts = std::mem::take(self).into_parts();
        let out = edit(&mut parts);
        *self = Document::from_parts(parts);
        out
    }

    /// The `pages` of the document.
    #[must_use]
    pub fn pages(&self) -> &BTreeMap<PageId, Page> {
        &self.pages
    }

    /// The `page_order` of the document.
    #[must_use]
    pub fn page_order(&self) -> &[PageId] {
        &self.page_order
    }

    /// The `layers` of the document.
    #[must_use]
    pub fn layers(&self) -> &BTreeMap<LayerId, Layer> {
        &self.layers
    }

    /// The `items` of the document.
    #[must_use]
    pub fn items(&self) -> &BTreeMap<ItemId, Item> {
        &self.items
    }

    /// The `devices` of the document.
    #[must_use]
    pub fn devices(&self) -> &BTreeMap<DeviceId, Device> {
        &self.devices
    }

    /// The `connection_points` of the document.
    #[must_use]
    pub fn connection_points(&self) -> &BTreeMap<ConnectionPointId, ConnectionPoint> {
        &self.connection_points
    }

    /// The `representations` of the document.
    #[must_use]
    pub fn representations(&self) -> &BTreeMap<RepresentationId, Representation> {
        &self.representations
    }

    /// The `wires` of the document.
    #[must_use]
    pub fn wires(&self) -> &BTreeMap<WireId, Wire> {
        &self.wires
    }

    /// The `cables` of the document.
    #[must_use]
    pub fn cables(&self) -> &BTreeMap<CableId, Cable> {
        &self.cables
    }

    /// The `routes` of the document.
    #[must_use]
    pub fn routes(&self) -> &BTreeMap<RouteId, crate::objects::route::Route> {
        &self.routes
    }

    /// The `aids` of the document.
    #[must_use]
    pub fn aids(&self) -> &BTreeMap<crate::naming::id::AidId, crate::objects::aid::Aid> {
        &self.aids
    }

    /// The `loose_parts` of the document.
    #[must_use]
    pub fn loose_parts(&self) -> &BTreeMap<crate::naming::id::LoosePartId, crate::objects::loose_part::LoosePart> {
        &self.loose_parts
    }

    /// The `device_links` of the document.
    #[must_use]
    pub fn device_links(&self) -> &BTreeMap<crate::naming::id::LinkId, crate::objects::user::DeviceLink> {
        &self.device_links
    }

    /// The `definitions` of the document.
    #[must_use]
    pub fn definitions(&self) -> &BTreeMap<DefinitionId, Definition> {
        &self.definitions
    }

    /// The `settings` of the document.
    #[must_use]
    pub fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    /// The `versions` of the document.
    #[must_use]
    pub fn versions(&self) -> &BTreeMap<crate::PointName, u64> {
        &self.versions
    }

    /// The `display` of the document.
    #[must_use]
    pub fn display(&self) -> &Properties {
        &self.display
    }

    /// The `project` of the document.
    #[must_use]
    pub fn project(&self) -> &crate::document::project::Project {
        &self.project
    }

    /// The `folders` of the document.
    #[must_use]
    pub fn folders(&self) -> &BTreeMap<FolderId, Folder> {
        &self.folders
    }

    /// The `folder_order` of the document.
    #[must_use]
    pub fn folder_order(&self) -> &[FolderId] {
        &self.folder_order
    }

    /// The `standard` of the document.
    #[must_use]
    pub fn standard(&self) -> Option<&crate::objects::styles::Standard> {
        self.standard.as_ref()
    }

    /// The `styles` of the document.
    #[must_use]
    pub fn styles(&self) -> &crate::objects::styles::StyleCatalogues {
        &self.styles
    }

    /// Recompute the reverse indexes and the nets from the stored objects.
    ///
    /// # Semantics
    /// Needed only where this crate changes the stored maps directly: building a document
    /// ([`Document::from_parts`], [`Document::edit_parts`], reading one) and a merge's working
    /// copy. [`crate::Op::apply`] keeps the indexes current on its own, so a document outside this
    /// crate is always indexed.
    pub(crate) fn rebuild_index(&mut self) {
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

    /// Continuations (drawing aids) whose name has the key `key`
    /// ([`crate::connectivity::name_key`]), in ID order, through the name index.
    pub fn continuations_named(&self, key: &str) -> Vec<crate::naming::id::AidId> {
        referrers(&self.index.continuations, &key.to_string()).collect()
    }

    /// Every continuation name key and its continuations, through the name index.
    pub fn continuation_names(&self) -> impl Iterator<Item = (&str, crate::naming::id::AidId)> {
        self.index.continuations.iter().flat_map(|(k, ids)| ids.keys().map(move |c| (k.as_str(), *c)))
    }

    /// Components made from `definition`, in ID order.
    pub fn components_using(&self, definition: DefinitionId) -> Vec<DeviceId> {
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

    /// Items on `page`, in ID order (through the page index, without going through the whole
    /// document).
    pub fn items_on(&self, page: PageId) -> impl Iterator<Item = (ItemId, &Item)> {
        referrers(&self.index.page_items, &page).filter_map(|id| self.items.get(&id).map(|i| (id, i)))
    }

    /// Representations that show `connection_point` through an explicit connection point, in ID order
    /// (brief 036: a level terminal is always shown that way).
    pub fn representations_showing(&self, connection_point: ConnectionPointId) -> impl Iterator<Item = (RepresentationId, &Representation)> {
        referrers(&self.index.connection_point_reps, &connection_point).filter_map(|id| self.representations.get(&id).map(|r| (id, r)))
    }

    /// The definition a symbol representation places (its one symbol instance), if any.
    pub fn rep_definition(&self, r: &Representation) -> Option<&Definition> {
        r.items.iter().find_map(|i| match &self.items.get(i)?.kind {
            ItemKind::Symbol(s) => self.definitions.get(&s.definition),
            ItemKind::Primitive(_) => None,
        })
    }

    /// Connection points of `owner` (a device or a drawing aid), in ID order.
    pub fn connection_points_of(&self, owner: impl Into<ConnectionPointOwner>) -> impl Iterator<Item = (ConnectionPointId, &ConnectionPoint)> {
        referrers(&self.index.component_connection_points, &owner.into()).filter_map(|id| self.connection_points.get(&id).map(|t| (id, t)))
    }

    /// Whether any conductor end is attached to `connection_point`, through the index, without listing them.
    pub fn has_ends(&self, connection_point: ConnectionPointId) -> bool {
        self.index.connection_point_ends.get(&connection_point).is_some_and(|m| !m.is_empty())
    }

    /// Wire ends attached to `connection_point`, in conductor ID order, end A before end B.
    pub fn ends_at(&self, connection_point: ConnectionPointId) -> Vec<(WireId, End)> {
        referrers(&self.index.connection_point_ends, &connection_point).collect()
    }

    /// The wires of net `net`, in ID order, through the index.
    pub fn wires_of_net(&self, net: crate::naming::id::NetId) -> impl Iterator<Item = WireId> + '_ {
        referrers(&self.index.net_wires, &net)
    }

    /// The routes with an end on connection point `t`, in ID order, through the index.
    pub fn routes_at(&self, t: ConnectionPointId) -> impl Iterator<Item = crate::naming::id::RouteId> + '_ {
        referrers(&self.index.connection_point_routes, &t)
    }

    /// The routes ending on the middle of route `r` (its T-joins), in ID order, through the index.
    pub fn tees_of(&self, r: crate::naming::id::RouteId) -> impl Iterator<Item = crate::naming::id::RouteId> + '_ {
        referrers(&self.index.route_tees, &r)
    }

    /// The routes drawn on page `page`, in ID order, through the index.
    pub fn routes_on(&self, page: PageId) -> impl Iterator<Item = crate::naming::id::RouteId> + '_ {
        referrers(&self.index.page_routes, &page)
    }

    /// Devices whose real (not provisional) tag is the same tag as `tag` (equal
    /// [`crate::naming::tag::key`]), in ID order, through the tag index: the cost does not grow with
    /// the project.
    pub fn components_tagged(&self, tag: &str) -> Vec<DeviceId> {
        self.components_with(&crate::naming::tag::Key::real(tag))
    }

    /// Devices holding tag identity `key` (text and state), in ID order, through the tag index.
    pub fn components_with(&self, key: &crate::naming::tag::Key) -> Vec<DeviceId> {
        referrers(&self.index.tags, key).collect()
    }

    /// Every tagged device through the tag index (brief 018), by tag key, then ID: no scan of the
    /// devices.
    pub fn tagged(&self) -> impl Iterator<Item = (&crate::naming::tag::Key, DeviceId)> {
        self.index.tags.iter().flat_map(|(k, ids)| ids.keys().map(move |c| (k, *c)))
    }

    /// Cables whose real tag is the same tag as `tag` (brief 026), in ID order, through the tag index.
    pub fn cables_tagged(&self, tag: &str) -> Vec<CableId> {
        self.cables_with(&crate::naming::tag::Key::real(tag))
    }

    /// Cables holding tag identity `key`, in ID order.
    pub fn cables_with(&self, key: &crate::naming::tag::Key) -> Vec<CableId> {
        referrers(&self.index.cable_tags, key).collect()
    }

    /// Every tagged cable through the tag index (brief 026), by tag key, then ID.
    pub fn tagged_cables(&self) -> impl Iterator<Item = (&crate::naming::tag::Key, CableId)> {
        self.index.cable_tags.iter().flat_map(|(k, ids)| ids.keys().map(move |c| (k, *c)))
    }

    /// Real tag keys starting with `start` held by devices or cables (brief 026), through a range of
    /// the tag indexes: the cost follows the keys with that start, not the project. Provisional tags
    /// are not among them (S6, #3).
    pub(crate) fn tag_keys_starting(&self, start: &str) -> impl Iterator<Item = &str> {
        let from = crate::naming::tag::Key { provisional: false, text: start.to_string() };
        let a = self.index.tags.range(from.clone()..).map(|(k, _)| k);
        let b = self.index.cable_tags.range(from..).map(|(k, _)| k);
        let (s1, s2) = (start.to_string(), start.to_string());
        a.take_while(move |k| !k.provisional && k.text.starts_with(s1.as_str()))
            .chain(b.take_while(move |k| !k.provisional && k.text.starts_with(s2.as_str())))
            .map(|k| k.text.as_str())
    }

    /// The provisional tag keys of `prefix` (upper case, without `-`) held by devices or cables:
    /// the numbers a new provisional tag must skip.
    pub fn provisional_keys(&self, prefix: &str) -> impl Iterator<Item = &str> {
        let from = crate::naming::tag::Key { provisional: true, text: prefix.to_string() };
        let a = self.index.tags.range(from.clone()..).map(|(k, _)| k);
        let b = self.index.cable_tags.range(from..).map(|(k, _)| k);
        let (s1, s2) = (prefix.to_string(), prefix.to_string());
        a.take_while(move |k| k.provisional && k.text.starts_with(s1.as_str()))
            .chain(b.take_while(move |k| k.provisional && k.text.starts_with(s2.as_str())))
            .map(|k| k.text.as_str())
    }

    /// Whether a device or a cable holds the real tag `tag` (brief 026: one namespace).
    pub fn tag_taken(&self, tag: &str) -> bool {
        let k = crate::naming::tag::Key::real(tag);
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
    pub fn style_users(&self, key: &str, value: &str) -> impl Iterator<Item = crate::document::op::ObjectRef> + '_ {
        referrers(&self.index.style_users, &(value.trim().to_string(), key.to_string()))
    }

    /// Every style choice an object holds, through the index (brief 050a): (stored style ID,
    /// choice key, object), by ID then key.
    pub fn style_choices(&self) -> impl Iterator<Item = (&str, &str, crate::document::op::ObjectRef)> + '_ {
        self.index.style_users.iter().flat_map(|((v, k), objs)| objs.keys().map(move |o| (v.as_str(), k.as_str(), *o)))
    }

    /// Every (stored style ID, choice key) with at least one explicit user, and its users, from
    /// `value` on (brief 050a), through the index.
    pub fn style_users_of(&self, value: &str) -> impl Iterator<Item = (&str, crate::document::op::ObjectRef)> + '_ {
        let v = value.trim().to_string();
        self.index
            .style_users
            .range((v.clone(), String::new())..)
            .take_while(move |((sv, _), _)| *sv == v)
            .flat_map(|((_, k), objs)| objs.keys().map(move |o| (k.as_str(), *o)))
    }

    /// Cables `conductor` is a member of, in ID order.
    pub fn cables_of(&self, conductor: WireId) -> Vec<CableId> {
        referrers(&self.index.conductor_cables, &conductor).collect()
    }

    /// The cables with a core on route `r` (a line with no wire yet).
    pub fn cables_on_route(&self, r: RouteId) -> Vec<CableId> {
        referrers(&self.index.route_cables, &r).collect()
    }

    /// The cable and core index whose named point `t` is (a point of a cable reference, design 072
    /// §4.4, F4), through the index.
    pub fn cable_point(&self, t: ConnectionPointId) -> Option<(CableId, usize)> {
        let cable = referrers(&self.index.cable_points, &t).next()?;
        let index = self.cables.get(&cable)?.cores.iter().position(|c| c.through.contains(&t))?;
        Some((cable, index))
    }

    /// Whether `t` is a connection point: a component's terminal or a cable reference's point.
    pub(crate) fn has_point(&self, t: ConnectionPointId) -> bool {
        self.connection_points.contains_key(&t) || self.cable_point(t).is_some()
    }

    /// The named points joined to `t` (`t` included), when `t` is one (design 072 §4.4, F4, F5): a
    /// cable reference's point joins the core's points on the cable's other references; a
    /// continuation's point joins the points of every continuation with the same name (an unnamed
    /// continuation's, its own). `None` for a device's own point. Named points are aids: a wire
    /// runs through them, never to them, so it runs device point to device point across pages.
    pub fn named_points(&self, t: ConnectionPointId) -> Option<Vec<ConnectionPointId>> {
        if let Some((cable, i)) = self.cable_point(t) {
            return self.cables.get(&cable)?.cores.get(i).map(|c| c.through.clone());
        }
        let aid = self.connection_points.get(&t)?.owner.aid()?;
        Some(self.continuation_points(aid, self.aids.get(&aid)?))
    }

    /// The points of continuation `id` (as `aid`, before or after an edit) and of its partners by
    /// name, in ID order.
    pub(crate) fn continuation_points(&self, id: crate::naming::id::AidId, aid: &crate::objects::aid::Aid) -> Vec<ConnectionPointId> {
        let aids = match crate::connectivity::continuation_key(aid) {
            Some(k) => self.continuations_named(&k),
            None => Vec::new(),
        };
        let aids = aids.into_iter().chain(std::iter::once(id));
        let mut out: Vec<ConnectionPointId> = aids.into_iter().flat_map(|a| self.connection_points_of(a).map(|(t, _)| t)).collect();
        out.sort();
        out.dedup();
        out
    }
}
