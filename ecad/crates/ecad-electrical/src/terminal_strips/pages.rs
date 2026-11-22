//! The graphic terminal-strip pages' kept layout data (brief 046, design 046): each strip's position
//! block widths and sheet count, the pages' style and their sheets and titles. What
//! `crate::strip_page` draws from them is the engine's.
//!
//! # No update map
//! [`StripPages`] is kept in [`Outputs`] beside the strips: per strip, its position block widths
//! and its sheet count at the project's sheet, and two indexes, `by_cable` (cables with a core on
//! the strip) and `by_far` (far devices named in a cable box). A full build takes 0.5 ms on the
//! large project (end-gate review, 2026-11-21), under the ~5 ms bar, so [`StripPages::update`]
//! builds it again from scratch after every commit, right after the strips' own update.
//!
//! Which strips' drawing changed (for the redraw) is still worked out from what the effects name
//! ([`Named`]): the strips the strips' update changed, every strip when the layout changed, the
//! strips of a cable changed in place (`by_cable`, box texts) and of a far device changed
//! (`by_far`). The project's `terminal.plan.style` sets `style_changed`.
//!
//! A page's own style (`SetProperty(Page P, terminal.plan.style)`) is P's own property, which
//! [`crate::frames::touches`] already redraws; its sheet count reads the kept counts.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::effect::{Effect, Effects};
use ecad_model::sheet::{MARGIN_MM, SheetSpec};
use ecad_model::{CableId, DeviceId, Document, ObjectRef, PageId};

use crate::accessories::Acc;
use crate::defaults::strip_pages as entries;
use crate::outputs::kept::Outputs;
use crate::strips::{PlanItem, StripData, Strips, TermRow};

/// Gap between the tag box and the first column.
pub const TAG_GAP_MM: f64 = 2.0;

/// The default title of a graphic page (New page ▸ Terminal strips).
pub const TITLE: &str = "Terminal strips";

/// The words of an empty page (028's).
pub const NO_STRIPS: &str = "No terminal strips yet.";

/// A `terminals` page's style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Table,
    Graphic,
}

/// The style of `page` (registry `terminal.plan.style`: page, project, built-in `table`).
pub fn style_of(doc: &Document, page: PageId) -> Style {
    match crate::defaults::resolve(doc, &entries::STYLE, crate::defaults::Scope::page(page)).word() {
        entries::GRAPHIC => Style::Graphic,
        _ => Style::Table,
    }
}

/// Whether `page` is a `terminals` page drawn in the graphic style.
pub fn is_graphic(doc: &Document, page: PageId) -> bool {
    crate::generated::kind_of(doc, page) == Some(ecad_model::GenKind::Terminals) && style_of(doc, page) == Style::Graphic
}

/// The strip layout lengths (design 046 §7.2), paper millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub pitch: f64,
    pub body: f64,
    pub lead: f64,
    pub band_y: f64,
    pub tagbox: f64,
    pub bar: f64,
    pub margin: f64,
    pub tag_scale: f64,
}

impl Default for Geometry {
    /// The built-ins.
    fn default() -> Self {
        let n = |e: &crate::defaults::Entry| match e.builtin() {
            crate::defaults::Value::Length(x) | crate::defaults::Value::Number(x) => x,
            _ => 0.0,
        };
        Geometry {
            pitch: n(&entries::PITCH),
            body: n(&entries::BODY),
            lead: n(&entries::LEAD),
            band_y: n(&entries::BAND_Y),
            tagbox: n(&entries::TAGBOX),
            bar: n(&entries::BAR),
            margin: n(&entries::MARGIN),
            tag_scale: n(&entries::TAG_SCALE),
        }
    }
}

impl Geometry {
    /// The project's values (the registry keeps each in its range).
    pub fn of(doc: &Document) -> Geometry {
        let n = |e: &crate::defaults::Entry| crate::defaults::resolve(doc, e, crate::defaults::Scope::project()).number();
        Geometry {
            pitch: n(&entries::PITCH),
            body: n(&entries::BODY),
            lead: n(&entries::LEAD),
            band_y: n(&entries::BAND_Y),
            tagbox: n(&entries::TAGBOX),
            bar: n(&entries::BAR),
            margin: n(&entries::MARGIN),
            tag_scale: n(&entries::TAG_SCALE),
        }
    }

    /// The width of a gap column (design 046 §5.3).
    fn gap_width(acc: Acc) -> f64 {
        match acc {
            Acc::EndStop => 3.0,
            Acc::EndPlate => 1.5,
            Acc::Partition => 2.0,
            Acc::StripLabel | Acc::Marker => 8.0,
        }
    }

    /// The width columns may take on a sheet `spec`: the drawing area less the tag box and the
    /// right margin (at least 1 mm).
    pub fn available(&self, spec: &SheetSpec) -> f64 {
        let area = f64::from(spec.width) - 2.0 * (MARGIN_MM + 5.0);
        (area - self.tagbox - TAG_GAP_MM - self.margin).max(1.0)
    }
}

/// The project's sheet (a page with no sheet of its own).
fn project_sheet(doc: &Document) -> SheetSpec {
    doc.settings.sheet()
}

/// One column of a strip's layout.
#[derive(Debug, Clone, Copy)]
pub enum Col<'a> {
    /// A plan row (one level of a position): its index in `StripData::rows`.
    Term(usize, &'a TermRow),
    /// A gap accessory.
    Gap(Acc),
}

/// The columns of strip `s` in plan order with their widths, and the position blocks: (first
/// column, end column, width). A block is a position (all its levels) and the gap columns after
/// it; the gap columns before the first position belong to the first block. O(rows).
pub fn columns<'a>(s: &'a StripData, g: &Geometry) -> (Vec<(Col<'a>, f64)>, Vec<(usize, usize, f64)>) {
    let mut cols: Vec<(Col<'a>, f64)> = Vec::new();
    let mut blocks: Vec<(usize, usize, f64)> = Vec::new();
    let mut open: Option<(usize, f64, bool)> = None;
    for item in s.plan(true) {
        let (col, w, starts) = match item {
            PlanItem::Term(i, r) => (Col::Term(i, r), g.pitch, s.first_of_position(i)),
            PlanItem::Gap { acc, .. } => (Col::Gap(acc), Geometry::gap_width(acc), false),
        };
        let at = cols.len();
        match open.as_mut() {
            Some((_, width, has_term)) if !(starts && *has_term) => {
                *width += w;
                *has_term |= matches!(col, Col::Term(..));
            }
            _ => {
                if let Some((first, width, _)) = open.take() {
                    blocks.push((first, at, width));
                }
                open = Some((at, w, matches!(col, Col::Term(..))));
            }
        }
        cols.push((col, w));
    }
    if let Some((first, width, _)) = open {
        blocks.push((first, cols.len(), width));
    }
    (cols, blocks)
}

/// Sheets from block widths: (first block, end block) per sheet. A block is never split; a block
/// wider than `avail` gets a sheet of its own. At least one sheet. O(blocks).
pub fn paginate(widths: &[f64], avail: f64) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let (mut start, mut used) = (0, 0.0);
    for (i, w) in widths.iter().enumerate() {
        if i > start && used + w > avail + 1e-9 {
            out.push((start, i));
            start = i;
            used = 0.0;
        }
        used += w;
    }
    out.push((start, widths.len()));
    out
}

/// What is kept per strip.
#[derive(Debug, Clone, Default, PartialEq)]
struct Kept {
    /// Position block widths, in plan order.
    blocks: Vec<f64>,
    /// Sheets at the project's sheet.
    sheets: usize,
    cables: BTreeSet<CableId>,
    far: BTreeSet<DeviceId>,
}

impl Kept {
    fn of(s: &StripData, g: &Geometry, avail: f64) -> Kept {
        let (_, blocks) = columns(s, g);
        let blocks: Vec<f64> = blocks.into_iter().map(|(_, _, w)| w).collect();
        let sheets = paginate(&blocks, avail).len();
        let mut cables = BTreeSet::new();
        let mut far = BTreeSet::new();
        for r in &s.rows {
            for c in &r.outside {
                if let Some((k, _)) = &c.cable {
                    cables.insert(*k);
                    far.extend(c.far);
                }
            }
            for c in &r.inside {
                if let Some((k, _)) = &c.cable {
                    cables.insert(*k);
                }
            }
        }
        Kept { blocks, sheets, cables, far }
    }
}

/// What a transaction's effects name for the strip pages' redraw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Named {
    /// A layout setting (`strippage.*`) or the project's sheet changed.
    pub layout: bool,
    /// The project's `terminal.plan.style` changed.
    pub style: bool,
    /// Cables changed in place (their strips' box texts).
    pub cables: BTreeSet<CableId>,
    /// Devices changed in place or given a property (as far devices in a cable box).
    pub devices: BTreeSet<DeviceId>,
}

impl Named {
    /// What `effects` name.
    pub fn of(effects: &Effects<'_>) -> Named {
        let mut n = Named::default();
        for effect in effects {
            n.note(effect);
        }
        n
    }

    /// Note what one effect names.
    pub fn note(&mut self, effect: &Effect<'_>) {
        match effect {
            Effect::Prop { object: ObjectRef::Project, key, .. } if entries::is_layout_key(key.raw) => self.layout = true,
            Effect::Prop { object: ObjectRef::Project, key, .. } if key.raw == entries::STYLE.key => self.style = true,
            Effect::Settings { before, after } if before.sheet != after.sheet => self.layout = true,
            Effect::Cable { id, before: Some(_), after: Some(_) } => {
                self.cables.insert(*id);
            }
            // A far device's tag or description (design 072: its fields) shows in the strip's rows.
            Effect::Prop { object: ObjectRef::Device(d), .. } | Effect::Device { id: d, before: Some(_), after: Some(_) } => {
                self.devices.insert(*d);
            }
            Effect::Prop { .. }
            | Effect::Settings { .. }
            | Effect::Cable { .. }
            | Effect::Device { .. }
            | Effect::Page { .. }
            | Effect::Layer { .. }
            | Effect::Item { .. }
            | Effect::DeviceDefinition { .. }
            | Effect::Terminal { .. }
            | Effect::Rep { .. }
            | Effect::Wire { .. }
            | Effect::Route { .. }
            | Effect::Definition { .. }
            | Effect::Version { .. }
            | Effect::Folder { .. }
            | Effect::Style { .. }
            | Effect::Standard { .. }
            | Effect::Project { .. }
            | Effect::Aid { .. }
            | Effect::LoosePart { .. }
            | Effect::DeviceLink { .. } => {}
        }
    }
}

/// The graphic pages' kept layout data, built from scratch after every commit.
#[derive(Debug, Clone, PartialEq)]
pub struct StripPages {
    kept: BTreeMap<DeviceId, Kept>,
    by_cable: BTreeMap<CableId, BTreeSet<DeviceId>>,
    by_far: BTreeMap<DeviceId, BTreeSet<DeviceId>>,
    pub geometry: Geometry,
    /// The column width of the project's sheet the kept counts are for.
    avail: f64,
    /// Every strip's sheets at the project's sheet.
    pub sheets_total: usize,
    /// The strips whose drawing the last update changed.
    pub last_changed: BTreeSet<DeviceId>,
    /// The last update changed the layout settings (every graphic page redraws).
    pub layout_changed: bool,
    /// The last update changed the project's `terminal.plan.style`.
    pub style_changed: bool,
}

impl Default for StripPages {
    fn default() -> Self {
        let geometry = Geometry::default();
        StripPages {
            kept: BTreeMap::new(),
            by_cable: BTreeMap::new(),
            by_far: BTreeMap::new(),
            avail: geometry.available(&SheetSpec::default()),
            geometry,
            sheets_total: 0,
            last_changed: BTreeSet::new(),
            layout_changed: false,
            style_changed: false,
        }
    }
}

fn index_add<K: Ord + Copy>(map: &mut BTreeMap<K, BTreeSet<DeviceId>>, keys: &BTreeSet<K>, c: DeviceId) {
    for k in keys {
        map.entry(*k).or_default().insert(c);
    }
}

impl StripPages {
    /// Every strip, from scratch (open, reload, first show).
    pub fn build(doc: &Document, strips: &Strips) -> StripPages {
        let geometry = Geometry::of(doc);
        let mut p = StripPages { avail: geometry.available(&project_sheet(doc)), geometry, ..StripPages::default() };
        for (c, s) in &strips.strips {
            p.add(*c, s);
        }
        p
    }

    fn add(&mut self, c: DeviceId, s: &StripData) {
        let k = Kept::of(s, &self.geometry, self.avail);
        self.sheets_total += k.sheets;
        index_add(&mut self.by_cable, &k.cables, c);
        index_add(&mut self.by_far, &k.far, c);
        self.kept.insert(c, k);
    }

    /// Build the kept data again from `strips` (just updated) on `doc`, the state after, and work
    /// out which strips' drawing changed from what the effects named (`named`, see the module).
    pub fn update(&mut self, doc: &Document, named: Named, strips: &Strips) {
        let rebuilt = StripPages::build(doc, strips);
        let layout_changed = rebuilt.geometry != self.geometry || (rebuilt.avail - self.avail).abs() > 1e-9;
        let mut last_changed: BTreeSet<DeviceId> = if layout_changed { strips.strips.keys().copied().collect() } else { strips.last_changed.clone() };
        for k in &named.cables {
            for idx in [&self.by_cable, &rebuilt.by_cable] {
                last_changed.extend(idx.get(k).into_iter().flatten().copied());
            }
        }
        for d in &named.devices {
            for idx in [&self.by_far, &rebuilt.by_far] {
                last_changed.extend(idx.get(d).into_iter().flatten().copied());
            }
        }
        *self = StripPages { last_changed, layout_changed, style_changed: named.style, ..rebuilt };
    }

    /// Take the kept data of `rebuilt` (verify mode put right), keeping what the last update
    /// changed.
    pub fn adopt(&mut self, rebuilt: StripPages) {
        let (last_changed, layout_changed, style_changed) = (std::mem::take(&mut self.last_changed), self.layout_changed, self.style_changed);
        *self = StripPages { last_changed, layout_changed, style_changed, ..rebuilt };
    }

    /// The sheets of strip `c` at column width `avail`: kept at the project's sheet, else counted
    /// from the kept block widths (O(positions), for a page with a sheet of its own).
    fn sheets_of(&self, c: DeviceId, avail: f64) -> usize {
        match self.kept.get(&c) {
            None => 1,
            Some(k) if (avail - self.avail).abs() < 1e-9 => k.sheets,
            Some(k) => paginate(&k.blocks, avail).len(),
        }
    }
}

/// The column width of `page`'s sheet.
pub fn page_avail(doc: &Document, page: PageId, o: &Outputs) -> f64 {
    o.strip_pages.geometry.available(&ecad_model::sheet::sheet_of(doc, page))
}

/// How many sheets a graphic `terminals` page needs: each strip from a new sheet, its kept
/// count (O(strips) with no row walk at the project's sheet).
pub fn sheets(doc: &Document, page: PageId, o: &Outputs) -> usize {
    let sp = &o.strip_pages;
    let avail = page_avail(doc, page, o);
    let n = match crate::generated::strip_of(doc, page) {
        Some(c) => sp.sheets_of(c, avail),
        None if (avail - sp.avail).abs() < 1e-9 => sp.sheets_total,
        None => o.strips.strips.keys().map(|c| sp.sheets_of(*c, avail)).sum(),
    };
    n.max(1)
}

/// What sheet `i` of a graphic page shows: the strip (if any; `Err` when the page's `strip`
/// names no strip), its sheet within the strip and how many it has.
#[allow(clippy::result_unit_err)]
pub fn sheet_at<'o>(doc: &Document, page: PageId, o: &'o Outputs, i: usize) -> Result<Option<(&'o StripData, usize, usize)>, ()> {
    let avail = page_avail(doc, page, o);
    let shown: Vec<&StripData> = match crate::generated::strip_of(doc, page) {
        Some(c) => match o.strips.get(c) {
            Some(s) => vec![s],
            None => return Err(()),
        },
        None => o.sorted_strips(),
    };
    let mut left = i;
    for s in shown {
        let n = o.strip_pages.sheets_of(s.component, avail).max(1);
        if left < n {
            return Ok(Some((s, left, n)));
        }
        left -= n;
    }
    Ok(None)
}

/// The words after a graphic sheet's page title (design 046 §6.3): `-X01 Interface (2/3)`.
pub fn sheet_title(doc: &Document, page: PageId, o: &Outputs, i: usize) -> Option<String> {
    let (s, j, n) = sheet_at(doc, page, o, i).ok().flatten()?;
    let mut t = s.tag.clone();
    if let Some(d) = s.description.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        t.push(' ');
        t.push_str(d);
    }
    if n > 1 {
        t.push_str(&format!(" ({}/{n})", j + 1));
    }
    Some(t)
}
