//! The ECAD section of CADCraft's Properties palette (brief 019, design 019 §8): what it shows for
//! the selection, built as a plain copy the UI draws. Edits go through the commands' JSON forms
//! (`setprop`, `pageprops`, `projectprops`), never through here.
//!
//! # Update map (design 019 §9, `ecad/CLAUDE.md` "Derived views")
//! A [`PropsView`] names the objects its rows read ([`PropsView::deps`]). It is rebuilt when the
//! selection or the active page changes, and after a change whose [`touched`] set meets those
//! objects (or that could touch anything, `None`); every other change leaves it as it is. A build
//! reads the selected objects, their pages and the project's properties only: O(selected).
//!
//! Excluded: editing several kinds at once, cable members and cores (read only here), free
//! project properties (Project Properties holds them).

use ecad_model::Word;
use ecad_model::keys::PropsExt;
use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::doc::Handle;
use ecad_model::book::{Book, FUNCTION_KEY, LOCATION_KEY, NUMBER_KEY};
use ecad_model::{CableId, DeviceId, Document, End, ObjectRef, Op, PageId, Subject, WireId};

use crate::project::{Mapped, Project};
pub use crate::words::Row;

/// What a row edits, as the command that writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Page(PageId),
    Project,
    Devices(Vec<DeviceId>),
    Wire(WireId),
    Cable(CableId),
}

/// One section: `ECAD · PAGE 105`, its rows, a note and the buttons it offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    /// `page`, `project`, `device`, `wire`, `cable`, `reference`, `objects` (element names).
    pub kind: &'static str,
    pub target: Option<Target>,
    pub rows: Vec<Row>,
    pub note: Option<String>,
    /// `goto:<reference hex>` (a reference).
    pub buttons: Vec<String>,
}

/// The whole ECAD section for one selection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropsView {
    pub sections: Vec<Section>,
    /// The objects its rows read: a change touching none of them leaves it as it is.
    pub deps: BTreeSet<ObjectRef>,
}

/// The objects whose shown properties `ops` (about to be applied to `doc`) could change; `None`
/// when that could be anything (settings, page order, folders). `ObjectRef::Page(p)` means page
/// `p`'s own properties (its number, title, sheet…), not the objects drawn on it.
pub fn touched(doc: &Document, ops: &[Op]) -> Option<BTreeSet<ObjectRef>> {
    let mut out = BTreeSet::new();
    let subject = |s: Subject, out: &mut BTreeSet<ObjectRef>| {
        out.insert(match s {
            Subject::Device(c) => ObjectRef::Device(c),
            Subject::Wire(c) => ObjectRef::Wire(c),
            Subject::Cable(c) => ObjectRef::Cable(c),
        });
    };
    for op in ops {
        match op {
            Op::ModifyPage { id, .. } => {
                out.insert(ObjectRef::Page(*id));
            }
            Op::SetProperty { object, .. } => {
                out.insert(*object);
                // A location box's own location is what its page's devices inherit (brief 024).
                if let ObjectRef::Device(c) = object
                    && crate::locations::is_box(doc, *c)
                {
                    out.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| ObjectRef::Page(r.page)));
                }
            }
            Op::CreateDevice { id, .. } | Op::DeleteDevice { id, .. } | Op::ModifyDevice { id, .. } | Op::SetDeviceDefinition { id, .. } => {
                out.insert(ObjectRef::Device(*id));
            }
            Op::AddTerminal { terminal, .. } | Op::RemoveTerminal { terminal, .. } => {
                out.insert(ObjectRef::Device(terminal.component));
            }
            Op::ModifyWire { id, .. } => {
                out.insert(ObjectRef::Wire(*id));
            }
            Op::CreateWire { id, conductor } | Op::DeleteWire { id, conductor } => {
                out.insert(ObjectRef::Wire(*id));
                // The devices at its ends: a wire's net (brief 022) is shown through them.
                out.extend(conductor.ends.iter().flatten().filter_map(|t| doc.terminals.get(t)).map(|t| ObjectRef::Device(t.component)));
            }
            Op::AttachEnd { conductor: id, before, after, .. } => {
                out.insert(ObjectRef::Wire(*id));
                out.extend([before, after].into_iter().flatten().filter_map(|t| doc.terminals.get(t)).map(|t| ObjectRef::Device(t.component)));
            }
            Op::CreateCable { id, .. } | Op::DeleteCable { id, .. } => {
                out.insert(ObjectRef::Cable(*id));
            }
            Op::AddCableMember { cable, member, .. } | Op::RemoveCableMember { cable, member, .. } => {
                out.insert(ObjectRef::Cable(*cable));
                out.insert(ObjectRef::Wire(member.conductor));
            }
            // A new or removed item is shown through its representation (linked or unlinked in the
            // same transaction); a moved one changes its subject's place.
            Op::AddItem { .. } => {}
            Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    subject(r.subject, &mut out);
                    if matches!(r.subject, Subject::Device(c) if crate::locations::is_box(doc, c)) {
                        out.insert(ObjectRef::Page(r.page));
                    }
                }
            }
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => {
                subject(rep.subject, &mut out);
                // A location box (brief 024) changes the location its page's devices inherit.
                if matches!(rep.subject, Subject::Device(c) if crate::locations::is_box(doc, c) || rep.role == crate::locations::ROLE) {
                    out.insert(ObjectRef::Page(rep.page));
                }
            }
            Op::ModifyRepresentation { before, after, .. } => {
                subject(before.subject, &mut out);
                subject(after.subject, &mut out);
            }
            Op::NameVersion { .. } | Op::UnnameVersion { .. } => {
                out.insert(ObjectRef::Project);
            }
            // Brief 050a: styles and the standard show in the property views of the project (the
            // inspector's style rows re-read through it).
            Op::SetStyle { .. } | Op::SetStandard { .. } => {
                out.insert(ObjectRef::Project);
            }
            // Brief 048 (design §8.4): a project symbol's new or removed version changes whether
            // the devices showing its lineage read "update available".
            Op::EmbedDefinition { definition, .. } | Op::RemoveDefinition { definition, .. } => {
                if let Some(o) = definition.origin.as_ref().filter(|o| o.library == crate::symbols::PROJECT_SYMBOLS) {
                    for v in crate::symbols::versions_of(doc, o.definition) {
                        for item in doc.items_using(v.id) {
                            if let Some((_, r)) = doc.representation_of_item(item) {
                                subject(r.subject, &mut out);
                            }
                        }
                    }
                }
            }
            Op::AddLayer { .. } | Op::RemoveLayer { .. } => {}
            Op::AddPage { .. }
            | Op::RemovePage { .. }
            | Op::AddFolder { .. }
            | Op::RemoveFolder { .. }
            | Op::ModifyFolder { .. }
            | Op::SetProjectSettings { .. }
            // Design 072 §5.3: a code renamed in the project's list shows on every object storing it.
            | Op::ModifyProject { .. } => return None,
        }
    }
    Some(out)
}

/// What the selection is, by kind.
#[derive(Debug, Default)]
struct Picked {
    devices: BTreeSet<DeviceId>,
    symbol: Option<ecad_model::RepresentationId>,
    wires: BTreeSet<WireId>,
    cables: BTreeSet<CableId>,
    references: BTreeSet<ecad_model::RepresentationId>,
    /// Brief 030: link lines picked: (strip, link).
    links: BTreeSet<(DeviceId, u32)>,
    plain: usize,
}

/// Most selected objects looked at (a bound on the work per build).
pub const MAX_LOOKED_AT: usize = 10_000;

fn pick(p: &Project, page: PageId, selection: &[Handle]) -> Picked {
    let doc = p.ledger.document();
    let mut out = Picked::default();
    let Some(map) = p.pages.get(&page).map(|v| &v.map) else { return out };
    for h in selection.iter().take(MAX_LOOKED_AT) {
        let rep = match map.get(*h) {
            Some(Mapped::Item(i)) => doc.representation_of_item(i).map(|(id, _)| id),
            Some(Mapped::Marker(r, _) | Mapped::Tag(r) | Mapped::RefText(r) | Mapped::Xref(r)) => Some(r),
            // Brief 030: a link line shows its strip (Properties ▸ Link).
            Some(Mapped::Link(c, n)) => {
                out.links.insert((c, n));
                None
            }
            Some(Mapped::Fan(k)) => {
                match map.reference_of(*h) {
                    Some(r) => {
                        out.references.insert(r);
                    }
                    None => {
                        out.cables.insert(k);
                    }
                }
                continue;
            }
            Some(Mapped::Frame(_)) => continue,
            // A conductor label (brief 022) stands for its wire.
            Some(Mapped::Label(w)) => {
                out.wires.insert(w);
                continue;
            }
            None => None,
        };
        let Some((rid, r)) = rep.and_then(|id| doc.representations.get(&id).map(|r| (id, r))) else {
            out.plain += 1;
            continue;
        };
        match r.subject {
            _ if crate::references::is_reference(r) => {
                out.references.insert(rid);
            }
            Subject::Device(c) => {
                out.devices.insert(c);
                out.symbol.get_or_insert(rid);
            }
            Subject::Wire(w) => {
                out.wires.insert(w);
            }
            Subject::Cable(k) => {
                out.cables.insert(k);
            }
        }
    }
    out
}

/// The devices, cables and wires `selection` on `page` stands for, as hex IDs (brief 026: the
/// Renumber dialog's Sel scope).
pub fn selected_objects(p: &Project, page: PageId, selection: &[Handle]) -> Vec<String> {
    let k = pick(p, page, selection);
    k.devices.iter().map(|c| c.hex()).chain(k.cables.iter().map(|c| c.hex())).chain(k.wires.iter().map(|w| w.hex())).collect()
}

/// The ECAD section for `selection` on the active page `page` of `p`.
pub fn view(p: &Project, page: Option<PageId>, selection: &[Handle]) -> PropsView {
    let doc = p.ledger.document();
    let mut v = PropsView::default();
    let Some(page) = page.filter(|pg| doc.pages.contains_key(pg)) else { return v };
    if selection.is_empty() {
        // Project-level things are popouts of the Project panel (owner, fix round of 2026-11-08):
        // with nothing selected, Properties shows the page. Its Function and Location show the
        // project's defaults when the page has none, so the project is a dependency too.
        v.sections.push(page_section(doc, page));
        v.deps.insert(ObjectRef::Page(page));
        v.deps.insert(ObjectRef::Project);
        return v;
    }
    let k = pick(p, page, selection);
    // Brief 030: a link line alone selects its link.
    if k.devices.is_empty()
        && k.wires.is_empty()
        && k.cables.is_empty()
        && k.references.is_empty()
        && let Some((c, n)) = k.links.first().copied()
    {
        v.deps.insert(ObjectRef::Device(c));
        v.sections.push(link_section(p, c, n));
        return v;
    }
    let kinds = [!k.devices.is_empty(), !k.wires.is_empty(), !k.cables.is_empty(), !k.references.is_empty()];
    let n_kinds = kinds.iter().filter(|x| **x).count();
    if n_kinds == 0 {
        return v;
    }
    if n_kinds > 1 {
        let mut words = Vec::new();
        for (n, one, many) in [
            (k.devices.len(), "device", "devices"),
            (k.wires.len(), "wire", "wires"),
            (k.cables.len(), "cable", "cables"),
            (k.references.len(), "reference", "references"),
        ] {
            if n > 0 {
                words.push(format!("{n} {}", if n == 1 { one } else { many }));
            }
        }
        let total = k.devices.len() + k.wires.len() + k.cables.len() + k.references.len();
        v.sections.push(Section {
            title: format!("ECAD · {total} OBJECTS"),
            kind: "objects",
            target: None,
            rows: Vec::new(),
            note: Some(format!("{}. Select one kind to edit.", words.join(", "))),
            buttons: Vec::new(),
        });
        return v;
    }
    if !k.devices.is_empty() {
        v.deps.extend(k.devices.iter().map(|c| ObjectRef::Device(*c)));
        v.deps.insert(ObjectRef::Page(page));
        // Brief 025: the Coil and Contacts rows name places on the device's pages.
        for c in &k.devices {
            v.deps.extend(doc.representations_of(Subject::Device(*c)).map(|(_, r)| ObjectRef::Page(r.page)));
            // Brief 049: an arrow's Other ends row names its partners' places.
            if let Some(n) = crate::continuations::name_of(doc, *c) {
                for o in doc.continuations_named(&n) {
                    v.deps.insert(ObjectRef::Device(o));
                    v.deps.extend(doc.representations_of(Subject::Device(o)).map(|(_, r)| ObjectRef::Page(r.page)));
                }
            }
        }
        // Brief 052: codes come from the project's list, and an unset value's proposal from the
        // selected symbol's page (its boxes and values) and the project's.
        v.deps.insert(ObjectRef::Project);
        // Brief 045: a card's Rack row shows its rack's tag.
        v.deps.extend(k.devices.iter().filter_map(|c| crate::plc::props_deps(doc, *c)));
        if let Some(r) = k.symbol.and_then(|s| doc.representations.get(&s)) {
            v.deps.insert(ObjectRef::Page(r.page));
        }
        match k.devices.iter().copied().collect::<Vec<_>>().as_slice() {
            [b] if crate::locations::is_box(doc, *b) => v.sections.push(box_section(doc, *b)),
            _ => v.sections.push(device_section(doc, p.outputs.as_ref().map(|o| &o.plc), &k.devices, k.symbol)),
        }
    } else if let [w] = k.wires.iter().copied().collect::<Vec<_>>().as_slice() {
        v.deps.insert(ObjectRef::Wire(*w));
        v.deps.extend(doc.cables_of(*w).into_iter().map(ObjectRef::Cable));
        // From and To name each end's device and connection point: a renamed device, or a
        // connection point added or removed, changes them.
        let ends = doc.wires.get(w).map(|c| c.ends).unwrap_or([None, None]);
        v.deps.extend(ends.iter().flatten().filter_map(|t| doc.terminals.get(t)).map(|t| ObjectRef::Device(t.component)));
        // Brief 022: the number, potential and Net block read the whole net: its conductors and
        // the devices (junctions and continuations included) at its connection points.
        let conn = doc.connectivity();
        if let Some(net) = conn.net_of_conductor(*w).and_then(|id| conn.net(id)) {
            v.deps.extend(net.wires.iter().map(|c| ObjectRef::Wire(*c)));
            v.deps.extend(net.terminals.iter().filter_map(|t| doc.terminals.get(t)).map(|t| ObjectRef::Device(t.component)));
        }
        v.sections.push(wire_section(doc, *w));
    } else if let [c] = k.cables.iter().copied().collect::<Vec<_>>().as_slice() {
        v.deps.insert(ObjectRef::Cable(*c));
        v.sections.push(cable_section(doc, *c));
    } else if let [r] = k.references.iter().copied().collect::<Vec<_>>().as_slice() {
        if let Some(Subject::Cable(c)) = doc.representations.get(r).map(|x| x.subject) {
            v.deps.insert(ObjectRef::Cable(c));
            // "Other ends" name each partner's page number and its sheet's zones.
            let refs = ecad_model::derive::cable_references_of(doc, &BTreeSet::from([c]));
            v.deps.extend(refs.iter().map(|x| ObjectRef::Page(x.page)));
        }
        v.deps.insert(ObjectRef::Page(page));
        v.sections.push(reference_section(doc, *r));
    } else {
        let n = k.wires.len() + k.cables.len() + k.references.len();
        v.sections.push(Section {
            title: format!("ECAD · {n} OBJECTS"),
            kind: "objects",
            target: None,
            rows: Vec::new(),
            note: Some("Select one wire, cable or reference to see its properties.".into()),
            buttons: Vec::new(),
        });
    }
    v
}

/// The Sheet combo's choices for a page: the project's sheet, then the presets.
pub fn sheet_choices(doc: &Document) -> Vec<String> {
    let mut out = vec![format!("Project ({})", doc.settings.sheet().summary())];
    out.extend(ecad_model::sheet::PRESETS.iter().map(|p| p.0.to_string()));
    out
}

fn page_section(doc: &Document, page: PageId) -> Section {
    let book = Book::of(doc);
    let pg = doc.pages.get(&page);
    let number = book.number(doc, page);
    let mut rows =
        vec![Row::rw(NUMBER_KEY, "Number", number.clone()), Row::rw("title", "Title", pg.map(|x| x.title.to_string()).unwrap_or_default())];
    let folder = ecad_model::book::folder_of(doc, page).filter(|f| doc.folders.contains_key(f));
    let path = crate::revisions::folder_path(doc, folder);
    rows.push(Row::ro("folder", "Folder", if path.is_empty() { "(top level)".into() } else { path }));
    for (key, label) in [(FUNCTION_KEY, "Function"), (LOCATION_KEY, "Location")] {
        // Brief 052: stored as entry keys, shown as codes (design 072 §5.3: page and project fields).
        let shown = |v: Option<String>| v.map(|v| crate::designations::display(doc, key, &v));
        let a = crate::designations::Aspect::of_property(key);
        let own = a.and_then(|a| pg.and_then(|x| crate::designations::page_key(x, a)));
        let mut r = match shown(own) {
            Some(v) => Row::rw(key, label, v),
            None => {
                let inherited = shown(a.and_then(|a| crate::designations::default_key(&doc.project, a)));
                let mut r = Row::rw(key, label, inherited.as_ref().map(|v| format!("{v} (project)")).unwrap_or_default());
                r.inherited = inherited.is_some();
                r
            }
        };
        r.choices = codes(doc, key);
        rows.push(r);
    }
    let mut sheet = Row::rw(
        ecad_model::sheet::SHEET_KEY,
        "Sheet",
        match pg.and_then(|x| x.sheet) {
            Some(t) => t.text(),
            None => format!("Project ({})", doc.settings.sheet().summary()),
        },
    );
    sheet.choices = sheet_choices(doc);
    rows.push(sheet);
    // Brief 031 (design §5.6): the page's Texts group.
    text_rows(doc, TextsAt::Page(page), &mut rows);
    Section { title: format!("ECAD · PAGE {number}"), kind: "page", target: Some(Target::Page(page)), rows, note: None, buttons: Vec::new() }
}

/// The codes of `key`'s aspect in the project's list, offered as completions (design 052 §9.1).
fn codes(doc: &Document, key: &str) -> Vec<String> {
    crate::designations::Aspect::of_property(key)
        .map(|a| crate::designations::entries(doc, a).into_iter().map(|e| e.code).collect())
        .unwrap_or_default()
}

/// A location box (design 024 §3.5, design 052 §5.8): Function and Location (editable, either may
/// be empty), Vertices and Devices inside.
fn box_section(doc: &Document, b: DeviceId) -> Section {
    let shape = doc.representations_of(Subject::Device(b)).find_map(|(rid, r)| crate::locations::box_of_rep(doc, rid, r));
    let shown =
        |key: &str| doc.devices.get(&b).and_then(|c| c.properties.get(key)).map(|v| crate::designations::display(doc, key, v)).unwrap_or_default();
    let location = shown(LOCATION_KEY);
    let mut rows = Vec::new();
    for (key, label) in [(FUNCTION_KEY, "Function"), (LOCATION_KEY, "Location")] {
        let mut r = Row::rw(key, label, shown(key));
        r.choices = codes(doc, key);
        rows.push(r);
    }
    let location = shape.as_ref().map_or(location, |s| crate::locations::label_of(s).0);
    if let Some(s) = &shape {
        rows.push(Row::ro("vertices", "Vertices", s.vertices.len().to_string()));
        let (n, keeps) = crate::locations::inside_counts(doc, s);
        let words = if keeps == 0 { n.to_string() } else { format!("{n}, {keeps} keep their own location") };
        rows.push(Row::ro("inside", "Devices inside", words));
    }
    Section {
        title: format!("ECAD · LOCATION BOX {location}"),
        kind: "box",
        target: Some(Target::Devices(vec![b])),
        rows,
        note: None,
        buttons: Vec::new(),
    }
}

fn device_section(
    doc: &Document, plc: Option<&crate::plc::kept::Plc>, devices: &BTreeSet<DeviceId>, symbol: Option<ecad_model::RepresentationId>,
) -> Section {
    let one = devices.len() == 1;
    let mut rows = Vec::new();
    for (key, label) in crate::devices::PROPERTIES {
        let values: BTreeSet<Option<&String>> = devices.iter().map(|c| doc.devices.get(c).and_then(|x| x.properties.get(key))).collect();
        let varies = values.len() > 1;
        let value = if varies {
            "*varies*".into()
        } else {
            // Brief 052: a function or location shows its code (`?` for a key with no entry).
            values.into_iter().next().flatten().map(|v| crate::designations::display(doc, key, v)).unwrap_or_default()
        };
        let mut r = if key == "tag" && !one { Row::ro(key, label, value) } else { Row::rw(key, label, value) };
        r.varies = varies;
        // Brief 052 (design 052 §9.1): an unset function or location is `?`, with the selected
        // symbol's context offered as **Use +CP1**. Nothing is inherited.
        if let Some(a) = crate::designations::Aspect::of_property(key) {
            r.choices = codes(doc, key);
            if !varies && r.value.is_empty() {
                r.proposal = proposal(doc, a, symbol);
            }
        }
        rows.push(r);
        // The overrides (design 052 §5.2, §9.1): a connector's halves; a non-strip device's
        // terminal symbols.
        if key == LOCATION_KEY
            && one
            && let Some(c) = devices.iter().next().copied()
        {
            override_rows(doc, c, &mut rows);
        }
        // Brief 026 (design §4, §9): Fixed beside Tag; greyed for a provisional tag.
        if key == "tag" && one {
            let c = devices.iter().next().copied();
            let props = c.and_then(|c| doc.devices.get(&c)).map(|x| &x.properties);
            rows.push(fixed_row(props));
        }
    }
    let title = if one {
        let c = devices.iter().next().copied();
        let tag = c.and_then(|c| crate::devices::tag_of(doc, c)).unwrap_or("(no tag)").to_string();
        if let Some(r) = symbol.and_then(|s| doc.representations.get(&s)) {
            let name = crate::devices::rep_starter(doc, r).map_or_else(|| "drawn".to_string(), |s| s.name.to_string());
            // The terminals this symbol shows: its connection points, else the device's own.
            let shown: Vec<ecad_model::TerminalId> = if r.connection_points.is_empty() {
                c.map(|c| doc.terminals_of(c).map(|(t, _)| t).collect()).unwrap_or_default()
            } else {
                r.connection_points.keys().copied().collect()
            };
            let labels: Vec<String> = shown.iter().map(|t| crate::terminal_types::terminal_name(doc, *t)).collect();
            let mut this = name;
            if !labels.is_empty() {
                this.push(' ');
                this.push_str(&labels.join("/"));
            }
            rows.push(Row::ro("symbol", "This symbol", this));
            // Brief 052 (design 052 §3.5): the symbol's type, from its definition.
            rows.push(Row::ro("rep_type", "Symbol", crate::reps::rep_type(doc, r).word().to_string()));
            let book = Book::of(doc);
            let grid = ecad_model::sheet::Grid::of(doc, r.page);
            let at = r
                .items
                .first()
                .and_then(|i| doc.items.get(i))
                .and_then(|i| match &i.kind {
                    ecad_model::ItemKind::Symbol(s) => Some(s.placement.origin),
                    ecad_model::ItemKind::Primitive(_) => None,
                })
                .or_else(|| r.connection_points.values().next().copied());
            let place = at
                .and_then(|p| ecad_model::derive::place_on(&grid, &book.number(doc, r.page), p))
                .map_or_else(|| "?".into(), |pl| pl.text(doc.settings.reference_style()));
            rows.push(Row::ro("where", "Where", place));
        }
        format!("ECAD · DEVICE {tag}")
    } else {
        format!("ECAD · {} DEVICES", devices.len())
    };
    // Brief 031 (design §5.6): the Texts group of one device, and of one symbol when one is picked.
    if let (true, Some(c)) = (one, devices.iter().next().copied()) {
        text_rows(doc, TextsAt::Device(c), &mut rows);
        if let Some(r) = symbol.filter(|r| doc.representations.get(r).is_some_and(|x| x.form == ecad_model::Form::Symbol)) {
            text_rows(doc, TextsAt::Symbol(r, c), &mut rows);
        }
    }
    let mut buttons = Vec::new();
    match (one, devices.iter().next().copied()) {
        // Brief 028 (design 028 §7.2): a strip's offer, its counts and the PE row's part.
        (true, Some(c)) if crate::strips::is_strip(doc, c) => strip_rows(doc, c, &mut rows, &mut buttons),
        // Brief 049 (design 049 §5.2): a continuation's Caption and Other ends.
        (true, Some(c)) if crate::continuations::is_continuation(doc, c) => continuation_rows(doc, devices, symbol, &mut rows, &mut buttons),
        (true, Some(c)) => {
            xref_rows(doc, c, symbol, &mut rows, &mut buttons);
            // Brief 045 (design 045 §2.6): Properties ▸ Card and ▸ Rack.
            crate::plc::props_rows(doc, c, &mut rows);
            crate::plc::channel_rows(doc, plc, c, symbol, &mut rows);
        }
        (false, _) if devices.iter().all(|c| crate::continuations::is_continuation(doc, *c)) => {
            continuation_rows(doc, devices, None, &mut rows, &mut buttons);
        }
        _ => {}
    }
    Section {
        title,
        kind: "device",
        target: Some(Target::Devices(devices.iter().copied().collect())),
        rows,
        note: if one { None } else { Some("Tags are one per device.".into()) },
        buttons,
    }
}

/// The proposal for an unset aspect `a`: the selected symbol's context (design 052 §9.1).
fn proposal(doc: &Document, a: crate::designations::Aspect, symbol: Option<ecad_model::RepresentationId>) -> Option<(String, String)> {
    let r = symbol.and_then(|s| doc.representations.get(&s))?;
    let ctx = crate::locations::rep_context_in(doc, a, r, &mut crate::locations::BoxCache::default());
    let code = ctx.value?;
    let from = match ctx.source {
        crate::locations::Source::Box { page, .. } => format!("From the box on {}", ecad_model::book::page_number(doc, page)),
        crate::locations::Source::Page(p) => format!("From page {}", ecad_model::book::page_number(doc, p)),
        _ => "From the project".to_string(),
    };
    Some((code, from))
}

/// The override rows of device `c` (design 052 §9.1): **Location (socket)** and **Location
/// (plug)** for a connector; **Terminals in** for a device other than a strip with terminal
/// symbols. An unset override shows the device's value faint, `(device)`.
fn override_rows(doc: &Document, c: DeviceId, rows: &mut Vec<Row>) {
    let props = doc.devices.get(&c).map(|x| &x.properties);
    let device = crate::locations::resolve(doc, c).value.unwrap_or_else(|| "?".into());
    let mut row = |key: &str, label: &str| {
        let mut r = match props.and_then(|p| p.get(key)) {
            Some(v) => Row::rw(key, label, crate::designations::display(doc, key, v)),
            None => Row { inherited: true, ..Row::rw(key, label, format!("{device} (device)")) },
        };
        r.choices = codes(doc, LOCATION_KEY);
        rows.push(r);
    };
    if crate::connectors::is_connector(doc, c) {
        row("location.socket", "Location (socket)");
        row("location.plug", "Location (plug)");
    } else if !crate::strips::is_strip(doc, c)
        && doc.representations_of(Subject::Device(c)).any(|(_, r)| crate::reps::rep_type(doc, r) == crate::reps::RepType::Terminal)
    {
        row("location.terminal", "Terminals in");
    }
}

/// Brief 025's rows of the device section (design 025 §5.2): Contacts offered, Contact table,
/// and read-only Coil (a contact selected) or Contacts (its coil selected), each with Go to
/// (`refgoto:<source>:<target>:<label>` buttons).
fn xref_rows(doc: &Document, c: DeviceId, symbol: Option<ecad_model::RepresentationId>, rows: &mut Vec<Row>, buttons: &mut Vec<String>) {
    use crate::xref;
    let props = doc.devices.get(&c).map(|x| &x.properties);
    rows.push(Row::rw(xref::CONTACTS_KEY, "Contacts offered", props.and_then(|p| p.get(xref::CONTACTS_KEY)).cloned().unwrap_or_default()));
    let dev = xref::device(doc, c);
    if dev.head.is_some() {
        let project = doc.settings.contact_mirror.word();
        let default = format!("Project default ({project})");
        let value = match props.and_then(|p| p.get(xref::MIRROR_KEY)) {
            None => default.clone(),
            Some(w) => match xref::parse_mirror(w) {
                Ok(xref::Placement::At(_)) => "Moved".into(),
                Ok(p) => {
                    let w = p.word();
                    let mut ch = w.chars();
                    ch.next().map(|f| f.to_uppercase().chain(ch).collect()).unwrap_or_default()
                }
                Err(_) => default.clone(),
            },
        };
        let mut r = Row::rw(xref::MIRROR_KEY, "Contact table", value);
        r.choices = vec![default, "Auto".into(), "Below".into(), "Right".into(), "Off".into()];
        rows.push(r);
    }
    let Some(sym) = symbol else { return };
    let mut places = xref::Places::new(doc);
    if dev.head == Some(sym) {
        let offer = xref::offer_of(doc, c);
        let list = xref::rows(&mut places, &dev, offer.as_ref().map(|(_, o)| o.as_slice()));
        let text: Vec<String> = list
            .iter()
            .map(|r| {
                let state = match r.state {
                    xref::State::Drawn => "",
                    xref::State::Spare => " (spare)",
                    xref::State::Over => " (beyond the part)",
                };
                format!("{} {} {}{state}", r.label_pair(), r.kind.word(), r.place)
            })
            .collect();
        rows.push(Row::ro("contactrows", "Contacts", if text.is_empty() { "none drawn".into() } else { text.join("; ") }));
        for r in list.iter().filter(|r| r.state != xref::State::Spare) {
            if let Some(t) = r.rep {
                buttons.push(format!("refgoto:{}:{}:Go to {}", sym.hex(), t.hex(), r.label_pair()));
            }
        }
    } else if dev.children.contains(&sym) {
        let value = match dev.head.and_then(|h| doc.representations.get(&h)) {
            Some(h) => places.text(h.page, xref::anchor(doc, h)),
            None => "not drawn".into(),
        };
        rows.push(Row::ro("coil", "Coil", value));
        if dev.head.is_some() {
            buttons.push(format!("refgoto:{}::Go to coil", sym.hex()));
        }
    }
}

/// Brief 049's rows of continuation arrows (design 049 §5.2): **Caption** (blank when they
/// differ), and for one arrow its **Other ends** with Go to (`refgoto:<source>:<target>:<label>`).
fn continuation_rows(
    doc: &Document, arrows: &BTreeSet<DeviceId>, symbol: Option<ecad_model::RepresentationId>, rows: &mut Vec<Row>, buttons: &mut Vec<String>,
) {
    use crate::continuations as k;
    let captions: BTreeSet<Option<&str>> = arrows.iter().map(|c| k::caption_of(doc, *c)).collect();
    let mut r = Row::rw(
        k::CAPTION_KEY,
        "Caption",
        if captions.len() == 1 { captions.into_iter().next().flatten().unwrap_or_default().to_string() } else { String::new() },
    );
    r.varies = arrows.iter().map(|c| k::caption_of(doc, *c)).collect::<BTreeSet<_>>().len() > 1;
    rows.push(r);
    let Some(sym) = symbol.filter(|s| k::arrow(doc, *s).is_some()) else { return };
    let ends = k::other_ends(doc, sym);
    let words: Vec<String> = ends.iter().map(|e| k::end_words(doc, e)).collect();
    rows.push(Row::ro("otherends", "Other ends", if words.is_empty() { "none".into() } else { words.join("; ") }));
    for e in &ends {
        buttons.push(format!("refgoto:{}:{}:Go to {}", sym.hex(), e.rep.hex(), e.text));
    }
}

/// Brief 028's rows of a strip (design 028 §7.2): Terminals offered, the read-only counts with
/// **Show plan** (`showplan:<strip>`), and the PE terminals' part number and maker.
fn strip_rows(doc: &Document, c: DeviceId, rows: &mut Vec<Row>, buttons: &mut Vec<String>) {
    use crate::strips;
    let props = doc.devices.get(&c).map(|x| &x.properties);
    let get = |k: &str| props.and_then(|p| p.get(k)).cloned().unwrap_or_default();
    rows.push(Row::rw(strips::OFFER_KEY, "Terminals offered", get(strips::OFFER_KEY)));
    let drawn = strips::drawn_terminals(doc, c).len();
    let value = match strips::offer_of(doc, c) {
        Some(o) => {
            let labels: BTreeSet<String> = doc.terminals_of(c).filter_map(|(_, t)| t.label.clone()).collect();
            let spare = (1..=o.positions).filter(|n| !labels.contains(&n.to_string())).count();
            format!("{drawn} drawn, {spare} spare")
        }
        None => format!("{drawn} drawn"),
    };
    rows.push(Row::ro("terminalcounts", "Terminals", value));
    rows.push(Row::rw(strips::PE_PART_KEY, "PE part number", get(strips::PE_PART_KEY)));
    rows.push(Row::rw(strips::PE_MAKER_KEY, "PE maker", get(strips::PE_MAKER_KEY)));
    // Brief 030 (design 030 §2.5): what is placed, and the parts (the project's value shown
    // faint where the strip has none).
    rows.push(Row::ro("acc.placed", "Accessories placed", placed_words(props)));
    for item in crate::accessories::PART_ITEMS {
        let words = item.replace('_', " ");
        for (field, label) in [("part_number", "part number"), ("maker", "maker")] {
            rows.push(part_row(doc, props, &format!("{item}.{field}"), &format!("{words} {label}")));
        }
    }
    rows.push(part_row(doc, props, crate::accessories::PER_CARD_KEY, "markers per card"));
    rows.push(part_row(doc, props, crate::accessories::BAR_KEY, "jumper bar size"));
    buttons.push(format!("showplan:{}", c.hex()));
}

/// A strip part row: its own value, else the project's shown faint.
fn part_row(doc: &Document, props: Option<&ecad_model::Properties>, key: &str, label: &str) -> Row {
    let own = props.and_then(|p| p.get(key)).cloned();
    let mut label: String = label.to_string();
    if let Some(f) = label.get_mut(0..1) {
        f.make_ascii_uppercase();
    }
    match own {
        Some(v) => Row::rw(key, &label, v),
        None => {
            let from = doc.properties.get(&crate::accessories::project_key(key)).cloned().unwrap_or_default();
            Row { inherited: !from.is_empty(), ..Row::rw(key, &label, from) }
        }
    }
}

/// `start: end stop · after 4: partition · end: end plate, end stop · markers: 12`.
fn placed_words(props: Option<&ecad_model::Properties>) -> String {
    let Some(props) = props else { return String::new() };
    let mut parts = Vec::new();
    let mut markers = 0;
    for (k, v) in props.range(crate::accessories::ACC_PREFIX.to_string()..).take_while(|(k, _)| k.starts_with(crate::accessories::ACC_PREFIX)) {
        match crate::accessories::spot_of(k) {
            Some(crate::accessories::Spot::On(_)) => markers += 1,
            Some(s) => {
                let at = match &s {
                    crate::accessories::Spot::Start => "start".to_string(),
                    crate::accessories::Spot::End => "end".to_string(),
                    other => format!("after {}", other.gap()),
                };
                parts.push(format!("{at}: {}", v.replace(',', ", ").replace('_', " ")));
            }
            None => {}
        }
    }
    if markers > 0 {
        parts.push(format!("markers: {markers}"));
    }
    if parts.is_empty() { "No accessories placed.".into() } else { parts.join(" \u{b7} ") }
}

/// Properties ▸ Link (design 030 §5.5): positions, part, bars.
fn link_section(p: &Project, c: DeviceId, n: u32) -> Section {
    let doc = p.ledger.document();
    let key = ecad_model::links::key(n);
    let props = doc.devices.get(&c).map(|x| &x.properties);
    let get = |k: &str| props.and_then(|p| p.get(k)).cloned().unwrap_or_default();
    let info = p.outputs.as_ref().and_then(|o| o.strips.get(c)).and_then(|d| d.acc.links.iter().find(|l| l.n == n).cloned());
    let rows = vec![
        Row::rw(&key, "Positions", get(&key)),
        Row::rw(&format!("{key}.part_number"), "Part number", get(&format!("{key}.part_number"))),
        Row::rw(&format!("{key}.maker"), "Maker", get(&format!("{key}.maker"))),
        Row::ro("link.bars", "Bars", info.map_or_else(String::new, |l| l.bars.to_string())),
    ];
    Section {
        title: format!("ECAD \u{b7} LINK {} {}", crate::devices::tag_words(doc, c), get(&key)),
        kind: "link",
        target: Some(Target::Devices(vec![c])),
        rows,
        note: None,
        buttons: vec![format!("link:{}:{n}", c.hex())],
    }
}

fn end_words(doc: &Document, w: WireId, end: End) -> String {
    match doc.wires.get(&w).and_then(|c| c.end(end)) {
        // `-K01:A1` (design 019 §8.2); a junction or continuation by what it is (brief 022).
        Some(t) => crate::wires::point_words(doc, t),
        None => "not connected".into(),
    }
}

fn free_rows(props: Option<&ecad_model::Properties>, skip: &[&str]) -> Vec<Row> {
    props.into_iter().flatten().filter(|(k, _)| !skip.contains(&k.as_str())).map(|(k, v)| Row::rw(k, k, v.clone())).collect()
}

/// Brief 022's rows of the wire section (design 022 §9): Wire number, Fixed, Potential,
/// Cross-section, Colour, and the read-only Net block.
fn wiring_rows(doc: &Document, w: WireId) -> (Vec<Row>, Vec<Row>) {
    use ecad_model::wiring;
    let conn = doc.connectivity();
    let Some(net) = conn.net_of_conductor(w).and_then(|id| conn.net(id)) else { return (Vec::new(), Vec::new()) };
    let pot = wiring::net_potential(doc, net);
    let mut top = vec![
        Row::rw(wiring::WIRE_NUMBER_KEY, "Wire number", wiring::net_number(doc, net).unwrap_or_else(|| "?".into())),
        Row {
            choices: vec!["yes".into(), "no".into()],
            ..Row::rw(ecad_model::connectivity::WIRE_NUMBER_FIXED, "Fixed", if wiring::net_fixed(net) { "yes" } else { "no" }.into())
        },
    ];
    let mut potential = Row::rw(wiring::POTENTIAL_KEY, "Potential", pot.name.clone().unwrap_or_default());
    potential.choices = doc.settings.wiring.potentials.iter().map(|p| p.name.clone()).collect();
    if pot.from_continuation {
        potential.value = format!("{} (continuation)", pot.name.clone().unwrap_or_default());
        potential.inherited = true;
    }
    top.push(potential);
    // Brief 050b: what the wire has none of its own comes from its category.
    let look = crate::styles::wire::wire_look(doc, w);
    let cat_value =
        |v: &Option<(String, crate::defaults::Source)>| v.as_ref().filter(|x| x.1 != crate::defaults::Source::Object).map(|x| x.0.clone());
    let cat_name = look.category.as_ref().map(|id| crate::styles::name(doc, ecad_model::StyleCatalogue::Wire, id));
    let (cs_default, colour_default) = (cat_value(&look.cross_section), cat_value(&look.colour));
    let wire = doc.wires.get(&w);
    let own_size = wire.and_then(|x| x.size).map(|s| s.to_string());
    let own_colours = wire.and_then(|x| x.colours.as_ref()).map(|c| c.shown(doc.settings.wiring.colour_style().words()));
    let rows = [
        (ecad_electrical::cmd::props::d::SIZE, "Cross-section", own_size, &cs_default),
        (ecad_electrical::cmd::props::d::COLOURS, "Colour", own_colours, &colour_default),
    ];
    for (key, label, stored, default) in rows {
        let shown = |v: &str| if key == ecad_electrical::cmd::props::d::COLOURS { doc.settings.wiring.colour(v) } else { v.to_string() };
        let mut row = Row::rw(key, label, stored.clone().unwrap_or_default());
        if key == ecad_electrical::cmd::props::d::COLOURS {
            row.choices =
                ecad_model::colour::ConductorColour::ALL.iter().map(|c| c.shown(doc.settings.wiring.colour_style().words()).to_string()).collect();
        }
        if stored.is_none()
            && let (Some(d), Some(n)) = (default, &cat_name)
        {
            row.value = format!("{} ({n})", shown(d));
            row.inherited = true;
        }
        top.push(row);
    }
    let mut pages: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    for page in &doc.page_order {
        let on = net.wires.iter().any(|c| doc.representations_of(Subject::Wire(*c)).any(|(_, r)| r.page == *page));
        if on && seen.insert(*page) {
            pages.push(ecad_model::book::page_number(doc, *page));
        }
    }
    let points = crate::wires::net_points(doc, net);
    let n = net.wires.len();
    let summary = format!(
        "{} \u{b7} {} connection point{} \u{b7} page{} {}",
        if n == 1 { "1 wire".to_string() } else { format!("{n} wires") },
        points.len(),
        if points.len() == 1 { "" } else { "s" },
        if pages.len() == 1 { "" } else { "s" },
        pages.join(" ")
    );
    let mut names: BTreeMap<String, usize> = BTreeMap::new();
    for t in &net.terminals {
        if let Some(x) = doc.terminals.get(t)
            && crate::wires::kind_of(doc, x.component) == Some(ecad_model::connectivity::CONTINUATION)
            && let Some(name) = doc.devices.get(&x.component).and_then(|c| c.properties.get(ecad_model::connectivity::NAME))
        {
            *names.entry(name.clone()).or_default() += 1;
        }
    }
    let mut block = vec![
        Row::ro("net", "Net", summary),
        Row::ro("points", "Points", if points.is_empty() { "none".into() } else { crate::wires::first_words(&points, 8) }),
    ];
    if !names.is_empty() {
        let words: Vec<String> = names.iter().map(|(k, v)| format!("{k} \u{d7}{v}")).collect();
        block.push(Row::ro("continuations", "Continuations", words.join(", ")));
    }
    (top, block)
}

fn wire_section(doc: &Document, w: WireId) -> Section {
    let props = doc.wires.get(&w).map(|c| &c.display);
    let (wiring, net) = wiring_rows(doc, w);
    let mut rows = wiring;
    rows.push(Row::ro("from", "From", end_words(doc, w, End::A)));
    rows.push(Row::ro("to", "To", end_words(doc, w, End::B)));
    let cables: Vec<String> = doc.cables_of(w).into_iter().map(|k| crate::cables::cable_title(doc, k)).collect();
    rows.push(Row::ro("cables", "Cables", if cables.is_empty() { "none".into() } else { cables.join(", ") }));
    rows.extend(free_rows(props, &[]));
    rows.extend(net);
    Section { title: "ECAD · WIRE".into(), kind: "wire", target: Some(Target::Wire(w)), rows, note: None, buttons: Vec::new() }
}

fn cable_section(doc: &Document, k: CableId) -> Section {
    let cable = doc.cables.get(&k);
    let props = cable.map(|c| &c.properties);
    let mut rows = vec![Row::rw("tag", "Tag", props.and_then(|p| p.text(ecad_model::keys::TAG)).unwrap_or_default().to_string())];
    // Brief 026: Fixed beside Tag.
    rows.push(fixed_row(props));
    rows.extend(free_rows(props, &["tag", ecad_model::tag::FIXED_KEY, crate::strips::CORES_KEY]));
    // Brief 028 (design 028 §7.2): Cores (its own, else read from the type, shown faint) and the
    // read-only Used / Spare with Show cores.
    let count = crate::strips::cable_cores(doc, k).unwrap_or_default();
    let own = props.and_then(|p| p.get(crate::strips::CORES_KEY)).cloned();
    let mut cores = Row::rw(crate::strips::CORES_KEY, "Cores", own.clone().unwrap_or_default());
    if own.is_none()
        && let Some(t) = &count.offer_text
    {
        cores.value = format!("from type: {t}");
        cores.inherited = true;
    }
    rows.push(cores);
    let members = cable.map_or(0, |c| c.members.len());
    let used = match count.spare_text() {
        Some(s) => format!("{} / {s}", count.used()),
        None => format!("{members} / ? (set the cable's cores or type to count spares)"),
    };
    rows.push(Row::ro("usedspare", "Used / Spare", used));
    Section {
        title: format!("ECAD · CABLE {}", crate::cables::cable_title(doc, k)),
        kind: "cable",
        target: Some(Target::Cable(k)),
        rows,
        note: None,
        buttons: vec![format!("showcores:{}", k.hex())],
    }
}

/// The Fixed row of a device or cable (brief 026): `yes` or `no`, read-only for a provisional tag
/// or none.
fn fixed_row(props: Option<&ecad_model::Properties>) -> Row {
    let t = props.and_then(|p| p.get(ecad_model::tag::TAG_KEY));
    let fixed = props.is_some_and(ecad_model::numbering::is_fixed);
    let value = if fixed { "yes" } else { "no" }.to_string();
    let mut r = if t.is_some_and(|t| !ecad_model::tag::is_provisional(t)) {
        Row::rw(ecad_model::tag::FIXED_KEY, "Fixed", value)
    } else {
        Row::ro(ecad_model::tag::FIXED_KEY, "Fixed", value)
    };
    r.choices = vec!["yes".into(), "no".into()];
    r
}

/// The key a clash box's button edits (`tag#swap`, `tag#merge`, `tag#next`): the tag, with that
/// way forward (brief 026, design §7).
pub fn conflict_key(way: &str) -> String {
    format!("{}#{way}", ecad_model::tag::TAG_KEY)
}

fn reference_section(doc: &Document, r: ecad_model::RepresentationId) -> Section {
    let Some(Subject::Cable(k)) = doc.representations.get(&r).map(|x| x.subject) else {
        return Section {
            title: "ECAD · CABLE REFERENCE".into(),
            kind: "reference",
            target: None,
            rows: Vec::new(),
            note: None,
            buttons: Vec::new(),
        };
    };
    let refs = ecad_model::derive::cable_references_of(doc, &BTreeSet::from([k]));
    let this = refs.iter().find(|x| x.reference == r);
    let others: Vec<String> = refs.iter().filter(|x| x.reference != r).map(|x| x.locator()).collect();
    let rows = vec![
        Row::ro("cable", "Cable", crate::cables::cable_title(doc, k)),
        Row::ro("this", "This end", this.map_or_else(|| "?".into(), |x| x.locator())),
        Row::ro("others", "Other ends", if others.is_empty() { "none".into() } else { others.join(", ") }),
    ];
    Section { title: "ECAD · CABLE REFERENCE".into(), kind: "reference", target: None, rows, note: None, buttons: vec![format!("goto:{}", r.hex())] }
}

// ---------------------------------------------------------------- Texts (brief 031, design §5.6)

/// Where a Texts group writes.
#[derive(Debug, Clone, Copy)]
enum TextsAt {
    Device(DeviceId),
    Symbol(ecad_model::RepresentationId, DeviceId),
    Page(PageId),
}

/// The words a Texts row shows for a stored visibility.
fn shown_words(w: &str) -> &'static str {
    <crate::text_style::Shown as ecad_model::Word>::parse(w).unwrap_or_default().label()
}

/// The Texts rows of a device, a symbol or a page: one per kind of §5.1, `Default (…)` when the
/// level has no value of its own. Row keys are `text.<kind>.show` (device, page) and
/// `text.<rep>.<kind>.show` (symbol); [`edit_line`] writes them with `showtext`.
fn text_rows(doc: &Document, at: TextsAt, rows: &mut Vec<Row>) {
    use crate::text_style::TextKind;
    let kinds: &[TextKind] = match at {
        TextsAt::Page(_) => &[TextKind::Tag, TextKind::PointLabel, TextKind::Description, TextKind::PartNumber, TextKind::ContinuationName],
        _ => &[TextKind::Tag, TextKind::PointLabel, TextKind::Description, TextKind::PartNumber],
    };
    let group = match at {
        TextsAt::Symbol(..) => "Texts on this symbol",
        _ => "Texts",
    };
    for kind in kinds {
        let Some(entry) = kind.show_entry() else { continue };
        let (key, own, inherited) = match at {
            TextsAt::Device(c) => {
                let own = doc.devices.get(&c).and_then(|x| x.properties.get(entry.key)).cloned();
                (entry.key.to_string(), own, crate::defaults::resolve(doc, entry, crate::defaults::Scope::project()).word())
            }
            TextsAt::Symbol(r, c) => {
                let key = crate::defaults::symbol_key(entry.key, r);
                let own = doc.devices.get(&c).and_then(|x| x.properties.get(&key)).cloned();
                let page = doc.representations.get(&r).map(|x| x.page);
                let scope = crate::defaults::Scope { page, component: Some(c), ..crate::defaults::Scope::project() };
                (key, own, crate::defaults::resolve(doc, entry, scope).word())
            }
            TextsAt::Page(p) => {
                let own = doc.pages.get(&p).and_then(|x| x.display.get(entry.key)).cloned();
                (entry.key.to_string(), own, crate::defaults::resolve(doc, entry, crate::defaults::Scope::project()).word())
            }
        };
        let own = own.filter(|v| entry.parse(v).is_some());
        let default = format!("Default ({})", shown_words(inherited).to_ascii_lowercase());
        let value = own.as_deref().map_or_else(|| default.clone(), |w| shown_words(w).to_string());
        let label = format!("{group}: {}", crate::cmd::text::kind_words(*kind));
        let mut r = Row::rw(&key, &label, value);
        r.inherited = own.is_none();
        r.choices = vec![default, "Shown".into(), "Hidden".into()];
        if *kind == TextKind::Tag {
            r.choices.push("Once per run".into());
        }
        rows.push(r);
    }
}

/// The `showtext` line of a Texts row edit, when `key` is a Texts row's key.
fn showtext_line(target: &Target, key: &str, value: Option<&str>) -> Option<String> {
    let rest = key.strip_prefix("text.")?.strip_suffix(".show")?;
    let (rep, kind) = match rest.split_once('.') {
        Some((rep, kind)) if rep.len() == 32 => (ecad_model::RepresentationId::parse(rep), kind),
        _ => (None, rest),
    };
    <crate::text_style::TextKind as ecad_model::Word>::parse(kind)?;
    let show = value.and_then(crate::text_style::Shown::from_input).map_or(serde_json::Value::Null, |s| serde_json::json!(s.word()));
    let mut j = serde_json::json!({"kind": kind, "show": show});
    match (rep, target) {
        (Some(r), _) => j["representation"] = r.hex().into(),
        (None, Target::Devices(cs)) => j["component"] = cs.first().map(|c| c.hex()).into(),
        (None, Target::Page(p)) => j["page"] = p.hex().into(),
        (None, _) => {}
    }
    Some(format!("showtext {j}"))
}

/// The command line (JSON form) that writes `value` to row `key` of `target`; `None` for `null`
/// (clear). Several devices are written in one `setprop` with `devices`.
pub fn edit_line(target: &Target, key: &str, value: Option<&str>) -> String {
    // Brief 031: a Texts row writes its visibility with `showtext`.
    if let Some(line) = showtext_line(target, key, value) {
        return line;
    }
    // Brief 045: a card's Rack and Slot rows write `plcslot`.
    if let Target::Devices(cs) = target
        && let [c] = cs.as_slice()
        && let Some(line) = crate::plc::edit_line(*c, key, value)
    {
        return line;
    }
    let v = value.map_or(serde_json::Value::Null, |s| serde_json::Value::String(s.to_string()));
    // Brief 026: a clash box's choice (`tag#swap`…), and Fixed `no` releasing the flag.
    if let Some(way) = key.strip_prefix("tag#") {
        let mut j = match target {
            Target::Devices(cs) => serde_json::json!({"component": cs.first().map(|c| c.hex())}),
            Target::Cable(k) => serde_json::json!({"cable": k.hex()}),
            _ => serde_json::json!({}),
        };
        j["key"] = "tag".into();
        j["value"] = v;
        j["conflict"] = way.into();
        return format!("setprop {j}");
    }
    let v = if key == ecad_model::tag::FIXED_KEY && value.and_then(ecad_model::words::flag) == Some(false) { serde_json::Value::Null } else { v };
    let j = match target {
        Target::Page(p) => {
            let mut m = serde_json::Map::new();
            m.insert("page".into(), p.hex().into());
            let v = if key == ecad_model::sheet::SHEET_KEY && value.is_some_and(|s| s.starts_with("Project (") || s.eq_ignore_ascii_case("project")) {
                serde_json::Value::Null
            } else {
                v
            };
            m.insert(key.into(), v);
            ("pageprops", serde_json::Value::Object(m))
        }
        Target::Project => {
            let mut m = serde_json::Map::new();
            m.insert(key.into(), v);
            ("projectprops", serde_json::Value::Object(m))
        }
        Target::Devices(cs) if cs.len() == 1 => ("setprop", serde_json::json!({"component": cs.first().map(|c| c.hex()), "key": key, "value": v})),
        Target::Devices(cs) => ("setprop", serde_json::json!({"devices": cs.iter().map(|c| c.hex()).collect::<Vec<_>>(), "key": key, "value": v})),
        // Brief 022: the number and the potential belong to the whole net.
        Target::Wire(w) if key == ecad_model::wiring::WIRE_NUMBER_KEY => ("wirenumber", serde_json::json!({"conductor": w.hex(), "value": v})),
        Target::Wire(w) if key == ecad_model::connectivity::WIRE_NUMBER_FIXED => {
            let fixed = value.and_then(ecad_model::words::flag) == Some(true);
            ("wirenumber", serde_json::json!({"conductor": w.hex(), "fixed": fixed}))
        }
        Target::Wire(w) if key == ecad_model::wiring::POTENTIAL_KEY => {
            let v = value
                .map(|s| s.trim().trim_end_matches("(continuation)").trim().to_string())
                .filter(|s| !s.is_empty())
                .map_or(serde_json::Value::Null, serde_json::Value::String);
            ("potential", serde_json::json!({"conductor": w.hex(), "value": v}))
        }
        Target::Wire(w) => ("setprop", serde_json::json!({"conductor": w.hex(), "key": key, "value": v})),
        Target::Cable(k) => ("setprop", serde_json::json!({"cable": k.hex(), "key": key, "value": v})),
    };
    format!("{} {}", j.0, j.1)
}
