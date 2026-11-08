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

use std::collections::BTreeSet;

use cadcraft_engine::doc::Handle;
use ecad_model::book::{Book, FUNCTION_KEY, LOCATION_KEY, NUMBER_KEY};
use ecad_model::{CableId, ComponentId, ConductorId, Document, End, ObjectRef, Op, PageId, Subject};

use crate::project::{Mapped, Project};

/// What a row edits, as the command that writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Page(PageId),
    Project,
    Devices(Vec<ComponentId>),
    Conductor(ConductorId),
    Cable(CableId),
}

/// One row: a label and a value box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The property key it edits (`number`, `title`, `description`…).
    pub key: String,
    pub label: String,
    /// The value shown.
    pub value: String,
    pub editable: bool,
    /// The value comes from the project (a page's Function or Location): shown faint.
    pub inherited: bool,
    /// Several selected objects differ: `*varies*`.
    pub varies: bool,
    /// Suggestions (a combo): Status, Sheet.
    pub choices: Vec<String>,
}

impl Row {
    fn ro(key: &str, label: &str, value: String) -> Row {
        Row {
            key: key.into(),
            label: label.into(),
            value,
            editable: false,
            inherited: false,
            varies: false,
            choices: Vec::new(),
        }
    }
    fn rw(key: &str, label: &str, value: String) -> Row {
        Row { editable: true, ..Row::ro(key, label, value) }
    }
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
            Subject::Component(c) => ObjectRef::Component(c),
            Subject::Conductor(c) => ObjectRef::Conductor(c),
            Subject::Cable(c) => ObjectRef::Cable(c),
        });
    };
    for op in ops {
        match op {
            Op::SetProperty { object, .. } => {
                out.insert(*object);
            }
            Op::CreateComponent { id, .. } | Op::DeleteComponent { id, .. } => {
                out.insert(ObjectRef::Component(*id));
            }
            Op::AddTerminal { terminal, .. } | Op::RemoveTerminal { terminal, .. } => {
                out.insert(ObjectRef::Component(terminal.component));
            }
            Op::CreateConductor { id, .. } | Op::DeleteConductor { id, .. } | Op::AttachEnd { conductor: id, .. } => {
                out.insert(ObjectRef::Conductor(*id));
            }
            Op::CreateCable { id, .. } | Op::DeleteCable { id, .. } => {
                out.insert(ObjectRef::Cable(*id));
            }
            Op::AddCableMember { cable, member, .. } | Op::RemoveCableMember { cable, member, .. } => {
                out.insert(ObjectRef::Cable(*cable));
                out.insert(ObjectRef::Conductor(member.conductor));
            }
            // A new or removed item is shown through its representation (linked or unlinked in the
            // same transaction); a moved one changes its subject's place.
            Op::AddItem { .. } => {}
            Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    subject(r.subject, &mut out);
                }
            }
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => subject(rep.subject, &mut out),
            Op::ModifyRepresentation { before, after, .. } => {
                subject(before.subject, &mut out);
                subject(after.subject, &mut out);
            }
            Op::NameVersion { .. } | Op::UnnameVersion { .. } => {
                out.insert(ObjectRef::Project);
            }
            Op::EmbedDefinition { .. } | Op::RemoveDefinition { .. } | Op::AddLayer { .. } | Op::RemoveLayer { .. } => {
            }
            Op::AddPage { .. }
            | Op::RemovePage { .. }
            | Op::AddFolder { .. }
            | Op::RemoveFolder { .. }
            | Op::ModifyFolder { .. }
            | Op::SetProjectSettings { .. } => return None,
        }
    }
    Some(out)
}

/// What the selection is, by kind.
#[derive(Debug, Default)]
struct Picked {
    devices: BTreeSet<ComponentId>,
    symbol: Option<ecad_model::RepresentationId>,
    wires: BTreeSet<ConductorId>,
    cables: BTreeSet<CableId>,
    references: BTreeSet<ecad_model::RepresentationId>,
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
            Some(Mapped::Marker(r, _) | Mapped::Tag(r) | Mapped::RefText(r)) => Some(r),
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
            Subject::Component(c) => {
                out.devices.insert(c);
                out.symbol.get_or_insert(rid);
            }
            Subject::Conductor(w) => {
                out.wires.insert(w);
            }
            Subject::Cable(k) => {
                out.cables.insert(k);
            }
        }
    }
    out
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
        v.deps.extend(k.devices.iter().map(|c| ObjectRef::Component(*c)));
        v.deps.insert(ObjectRef::Page(page));
        v.sections.push(device_section(doc, &k.devices, k.symbol));
    } else if let [w] = k.wires.iter().copied().collect::<Vec<_>>().as_slice() {
        v.deps.insert(ObjectRef::Conductor(*w));
        v.deps.extend(doc.cables_of(*w).into_iter().map(ObjectRef::Cable));
        v.sections.push(wire_section(doc, *w));
    } else if let [c] = k.cables.iter().copied().collect::<Vec<_>>().as_slice() {
        v.deps.insert(ObjectRef::Cable(*c));
        v.sections.push(cable_section(doc, *c));
    } else if let [r] = k.references.iter().copied().collect::<Vec<_>>().as_slice() {
        if let Some(Subject::Cable(c)) = doc.representations.get(r).map(|x| x.subject) {
            v.deps.insert(ObjectRef::Cable(c));
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
    let mut out = vec![format!("Project ({})", doc.settings.sheet.summary())];
    out.extend(ecad_model::sheet::PRESETS.iter().map(|p| p.0.to_string()));
    out
}

fn page_section(doc: &Document, page: PageId) -> Section {
    let book = Book::of(doc);
    let pg = doc.pages.get(&page);
    let prop = |k: &str| pg.and_then(|x| x.properties.get(k)).cloned();
    let number = book.number(doc, page);
    let mut rows = vec![
        Row::rw(NUMBER_KEY, "Number", number.clone()),
        Row::rw("title", "Title", pg.map(|x| x.title.clone()).unwrap_or_default()),
    ];
    let folder = ecad_model::book::folder_of(doc, page).filter(|f| doc.folders.contains_key(f));
    let path = crate::sheet_cmds::folder_path(doc, folder);
    rows.push(Row::ro("folder", "Folder", if path.is_empty() { "(top level)".into() } else { path }));
    for (key, label) in [(FUNCTION_KEY, "Function"), (LOCATION_KEY, "Location")] {
        let mut r = match prop(key) {
            Some(v) => Row::rw(key, label, v),
            None => {
                let inherited = doc.properties.get(key).cloned();
                let mut r =
                    Row::rw(key, label, inherited.as_ref().map(|v| format!("{v} (project)")).unwrap_or_default());
                r.inherited = inherited.is_some();
                r
            }
        };
        r.choices.clear();
        rows.push(r);
    }
    let mut sheet = Row::rw(
        ecad_model::sheet::SHEET_KEY,
        "Sheet",
        match prop(ecad_model::sheet::SHEET_KEY) {
            Some(t) => t,
            None => format!("Project ({})", doc.settings.sheet.summary()),
        },
    );
    sheet.choices = sheet_choices(doc);
    rows.push(sheet);
    Section {
        title: format!("ECAD · PAGE {number}"),
        kind: "page",
        target: Some(Target::Page(page)),
        rows,
        note: None,
        buttons: Vec::new(),
    }
}

fn device_section(
    doc: &Document,
    devices: &BTreeSet<ComponentId>,
    symbol: Option<ecad_model::RepresentationId>,
) -> Section {
    let one = devices.len() == 1;
    let mut rows = Vec::new();
    for (key, label) in crate::devices::PROPERTIES {
        let values: BTreeSet<Option<&String>> =
            devices.iter().map(|c| doc.components.get(c).and_then(|x| x.properties.get(key))).collect();
        let varies = values.len() > 1;
        let value =
            if varies { "*varies*".into() } else { values.into_iter().next().flatten().cloned().unwrap_or_default() };
        let mut r = if key == "tag" && !one { Row::ro(key, label, value) } else { Row::rw(key, label, value) };
        r.varies = varies;
        rows.push(r);
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
            let labels: Vec<String> = shown.iter().map(|t| crate::interact::terminal_name(doc, *t)).collect();
            let mut this = name;
            if !labels.is_empty() {
                this.push(' ');
                this.push_str(&labels.join("/"));
            }
            rows.push(Row::ro("symbol", "This symbol", this));
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
                .map_or_else(|| "?".into(), |pl| pl.text(doc.settings.reference_style));
            rows.push(Row::ro("where", "Where", place));
        }
        format!("ECAD · DEVICE {tag}")
    } else {
        format!("ECAD · {} DEVICES", devices.len())
    };
    Section {
        title,
        kind: "device",
        target: Some(Target::Devices(devices.iter().copied().collect())),
        rows,
        note: if one { None } else { Some("Tags are one per device.".into()) },
        buttons: Vec::new(),
    }
}

fn end_words(doc: &Document, w: ConductorId, end: End) -> String {
    match doc.conductors.get(&w).and_then(|c| c.end(end)) {
        Some(t) => {
            let comp = doc.terminals.get(&t).map(|x| x.component);
            let tag = comp
                .and_then(|c| crate::devices::tag_of(doc, c).map(str::to_string))
                .unwrap_or_else(|| comp.map(|c| c.short()).unwrap_or_default());
            format!("{tag}:{}", crate::interact::terminal_name(doc, t))
        }
        None => "not connected".into(),
    }
}

fn free_rows(props: Option<&ecad_model::Properties>, skip: &[&str]) -> Vec<Row> {
    props
        .into_iter()
        .flatten()
        .filter(|(k, _)| !skip.contains(&k.as_str()))
        .map(|(k, v)| Row::rw(k, k, v.clone()))
        .collect()
}

fn wire_section(doc: &Document, w: ConductorId) -> Section {
    let props = doc.conductors.get(&w).map(|c| &c.properties);
    let mut rows =
        vec![Row::ro("from", "From", end_words(doc, w, End::A)), Row::ro("to", "To", end_words(doc, w, End::B))];
    let cables: Vec<String> = doc.cables_of(w).into_iter().map(|k| crate::views::cable_title(doc, k)).collect();
    rows.push(Row::ro("cables", "Cables", if cables.is_empty() { "none".into() } else { cables.join(", ") }));
    rows.push(Row::rw("tag", "Tag", props.and_then(|p| p.get("tag")).cloned().unwrap_or_default()));
    rows.extend(free_rows(props, &["tag"]));
    Section {
        title: "ECAD · WIRE".into(),
        kind: "wire",
        target: Some(Target::Conductor(w)),
        rows,
        note: None,
        buttons: Vec::new(),
    }
}

fn cable_section(doc: &Document, k: CableId) -> Section {
    let cable = doc.cables.get(&k);
    let props = cable.map(|c| &c.properties);
    let mut rows = vec![Row::rw("tag", "Tag", props.and_then(|p| p.get("tag")).cloned().unwrap_or_default())];
    rows.extend(free_rows(props, &["tag"]));
    rows.push(Row::ro("cores", "Cores", cable.map_or(0, |c| c.members.len()).to_string()));
    Section {
        title: format!("ECAD · CABLE {}", crate::views::cable_title(doc, k)),
        kind: "cable",
        target: Some(Target::Cable(k)),
        rows,
        note: None,
        buttons: Vec::new(),
    }
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
        Row::ro("cable", "Cable", crate::views::cable_title(doc, k)),
        Row::ro("this", "This end", this.map_or_else(|| "?".into(), |x| x.locator())),
        Row::ro("others", "Other ends", if others.is_empty() { "none".into() } else { others.join(", ") }),
    ];
    Section {
        title: "ECAD · CABLE REFERENCE".into(),
        kind: "reference",
        target: None,
        rows,
        note: None,
        buttons: vec![format!("goto:{}", r.hex())],
    }
}

/// The command line (JSON form) that writes `value` to row `key` of `target`; `None` for `null`
/// (clear). Several devices are written in one `setprop` with `components`.
pub fn edit_line(target: &Target, key: &str, value: Option<&str>) -> String {
    let v = value.map_or(serde_json::Value::Null, |s| serde_json::Value::String(s.to_string()));
    let j = match target {
        Target::Page(p) => {
            let mut m = serde_json::Map::new();
            m.insert("page".into(), p.hex().into());
            let v = if key == ecad_model::sheet::SHEET_KEY
                && value.is_some_and(|s| s.starts_with("Project (") || s.eq_ignore_ascii_case("project"))
            {
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
        Target::Devices(cs) if cs.len() == 1 => {
            ("setprop", serde_json::json!({"component": cs.first().map(|c| c.hex()), "key": key, "value": v}))
        }
        Target::Devices(cs) => (
            "setprop",
            serde_json::json!({"components": cs.iter().map(|c| c.hex()).collect::<Vec<_>>(), "key": key, "value": v}),
        ),
        Target::Conductor(w) => ("setprop", serde_json::json!({"conductor": w.hex(), "key": key, "value": v})),
        Target::Cable(k) => ("setprop", serde_json::json!({"cable": k.hex(), "key": key, "value": v})),
    };
    format!("{} {}", j.0, j.1)
}
