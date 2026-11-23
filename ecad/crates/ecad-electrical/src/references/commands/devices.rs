//! The rules of the cross-reference commands: where
//! `refgoto` goes, and the `mirror` value `mirrormove` writes. The listings (`references`,
//! `contacts`) are [`crate::references::devices`]'s own; the engine crate's shims open pages and answer in JSON.

use ecad_model::{CableId, DeviceId, Document, ItemId, Op, PageId, Pt, RepresentationId, Subject};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::commands::input::Point;
use crate::commands::run::{Access, Cx, Generic, Outcome, Pages};
use crate::devices::traits::DeviceKind;
use crate::error::CmdError;
use crate::references::devices;
use crate::references::devices::index::RefIndex;
use crate::references::devices::parse::Placement;

/// Where `refgoto` goes: the page, the point to centre, the item to select, and the words.
#[derive(Debug, Clone, PartialEq)]
pub struct Jump {
    pub page: PageId,
    pub at: Pt,
    pub item: Option<ItemId>,
    pub target: RepresentationId,
    pub message: String,
}

/// `refgoto {source, target?}`: the place reference `source` (a cable reference end, a contact, a
/// coil or a continuation arrow) shows. An arrow goes to its first other end, then the next on
/// repeat (after `ring`, the place the last go-to ringed), wrapping.
///
/// # Errors
/// No such reference, one with no other end, or an other end that cannot be located.
pub fn goto(
    doc: &Document, idx: &RefIndex, source: RepresentationId, target: Option<RepresentationId>, ring: Option<(PageId, Pt)>,
) -> Result<Jump, CmdError> {
    let r = doc.representations().get(&source).ok_or(CmdError::refused("no such reference"))?;
    let arrow = matches!(idx.member(source), Some((devices::classify::Group::Continuation(_), _)));
    let none = match r.subject {
        // A lonely continuation arrow.
        Subject::Device(_) if arrow || crate::continuations::arrow(doc, source).is_some() => "This continuation has no other end.",
        Subject::Device(_) => "This symbol shows no reference: its device has no coil or no contacts.",
        Subject::Cable(_) => "this reference has no other end",
        Subject::Wire(_) | Subject::Route(_) => "That is not a reference.",
        Subject::Aid(a) => match doc.aids().get(&a).map(ecad_model::Aid::kind) {
            Some(ecad_model::AidKind::Continuation) => "This continuation has no other end.",
            Some(ecad_model::AidKind::Box) | None => "That is not a reference.",
        },
    };
    if idx.member(source).is_none() {
        return Err(CmdError::refused(none));
    }
    let t = if arrow { crate::continuations::goto(doc, source, target, ring) } else { devices::read::goto_target(doc, idx, source, target) };
    let t = t.ok_or(CmdError::refused(none))?;
    const LOST: &str = "the other end cannot be located";
    let tr = doc.representations().get(&t).ok_or(CmdError::refused(LOST))?;
    let at = devices::classify::anchor(doc, tr)
        .or_else(|| {
            tr.items.first().and_then(|i| doc.items().get(i)).and_then(|i| match &i.kind {
                ecad_model::ItemKind::Primitive(ecad_model::Primitive::Point { at }) if at.is_finite() => Some(*at),
                _ => None,
            })
        })
        .ok_or(CmdError::refused(LOST))?;
    let mut places = devices::places::Places::new(doc);
    let place = places.text(tr.page, Some(at));
    let what = match tr.subject {
        Subject::Device(c) => match devices::classify::device(doc, c).head {
            Some(h) if h == t => format!("{}'s coil", crate::devices::tags::tag_words(doc, c)),
            _ => format!("{}'s contact", crate::devices::tags::tag_words(doc, c)),
        },
        Subject::Cable(k) => format!("the other end of cable {}", crate::cables::cable_title(doc, k)),
        Subject::Wire(_) | Subject::Route(_) => "the reference".into(),
        Subject::Aid(a) => match doc.aids().get(&a) {
            Some(ecad_model::Aid::Continuation { name, .. }) => format!("continuation {}", name.as_ref().map_or("", |n| n.as_str())),
            Some(ecad_model::Aid::Box(_)) | None => "the reference".into(),
        },
    };
    let message = format!("{}, {what} ({place}).", crate::references::cables::page_words(doc, tr.page).replacen("page", "Page", 1));
    Ok(Jump { page: tr.page, at, item: tr.items.first().copied(), target: t, message })
}

/// The `mirror` value `mirrormove` writes on device `c`: a point `at` (page coordinates) for the
/// contact table, or a `placement` word (auto, below, right, off). `None`: the project default.
///
/// # Errors
/// Neither `at` nor `placement`, both, an unreadable word, or a point too far from the coil.
pub(crate) fn mirror_for(doc: &Document, c: DeviceId, at: Option<Pt>, placement: Option<&str>) -> Result<Option<String>, CmdError> {
    let placement = match (at, placement) {
        (Some(_), Some(_)) => return Err(CmdError::input("give `at` or `placement`, not both")),
        (None, None) => return Err(CmdError::input("`at` ([x, y]) or `placement` (auto, below, right, off) is required")),
        (None, Some(w)) => match devices::parse::parse_mirror(w)? {
            Placement::At(_) => return Err(CmdError::refused("`placement` must be auto, below, right or off; use `at` for a point")),
            p => p,
        },
        (Some(p), None) => {
            let dev = devices::classify::device(doc, c);
            let head = dev
                .head
                .and_then(|h| doc.representations().get(&h))
                .ok_or(CmdError::refused("This device has no coil (or other head symbol) to put a table under."))?;
            let origin = devices::classify::anchor(doc, head).ok_or(CmdError::refused("This device's coil cannot be located."))?;
            let k = match doc.settings().length_unit {
                ecad_model::LengthUnit::Millimetre => 1.0,
                ecad_model::LengthUnit::Inch => 25.4,
            };
            let d = Pt::new((p.x - origin.x) * k, (p.y - origin.y) * k);
            if !(d.x.abs() <= ecad_model::objects::devices::contacts::MAX_OFFSET_MM
                && d.y.abs() <= ecad_model::objects::devices::contacts::MAX_OFFSET_MM)
            {
                return Err(CmdError::refused(format!(
                    "Put the table within {} mm of its coil.",
                    ecad_model::objects::devices::contacts::MAX_OFFSET_MM
                )));
            }
            Placement::At(d)
        }
    };
    Ok(match placement {
        // The project default is `auto` unless the project says otherwise: `auto` writes `auto`
        // only when the project's default differs.
        Placement::Auto if doc.settings().contact_mirror == ecad_model::ContactMirror::Auto => None,
        p => Some(p.word()),
    })
}

/// What `mirrormove` records and says.
#[derive(Debug, Clone, PartialEq)]
pub struct Mirrored {
    /// Empty when the device already has that value.
    pub ops: Vec<Op>,
    /// The value written.
    pub mirror: Option<String>,
    /// What happened, in words.
    pub message: String,
}

/// `mirrormove {component, at? | placement?}`: where device `c`'s contact table goes, as one
/// `ModifyDevice` of its `contact_table`.
///
/// # Errors
/// No such component, or [`mirror_for`]'s refusals.
pub(crate) fn plan_mirrormove(doc: &Document, c: DeviceId, at: Option<Pt>, placement: Option<&str>) -> Result<Mirrored, CmdError> {
    let comp = doc.devices().get(&c).ok_or(CmdError::refused("no such component"))?;
    let after = mirror_for(doc, c, at, placement)?;
    let before = comp.device_type.as_contacts().and_then(|x| x.contact_table()).map(|t| Placement::of_table(t).word());
    if before == after {
        return Ok(Mirrored { ops: Vec::new(), mirror: after, message: String::new() });
    }
    let words = match after.as_deref() {
        None => "follows the project default".to_string(),
        Some("off") => "is hidden".to_string(),
        Some(w) if w.starts_with("at ") => "is moved".to_string(),
        Some(w) => format!("is placed {w}"),
    };
    let message = format!("{}'s contact table {words}.", crate::devices::tags::tag_words(doc, c));
    let who = crate::devices::tags::tag_words(doc, c);
    let table = devices::parse::table_of_typed(after.as_deref())?;
    let ops = vec![Op::ModifyDevice { id: c, before: comp.clone(), after: devices::parse::with_table(comp, &who, table)? }];
    Ok(Mirrored { ops, mirror: after, message })
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run). `refgoto` opens a page, zooms and selects, and stays
// the engine's.

/// Rows one `references` answers by default.
pub const DEFAULT_LIMIT: usize = 200;
/// Most rows one `references` answers.
pub const MAX_LIMIT: usize = 2000;

/// `references {component?, cable?, page?, limit?, after?}` (read only): cable and device references,
/// a page at a time.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceReferencesParams {
    /// Only this device's references.
    #[serde(default)]
    component: Option<DeviceId>,
    /// Only this continuation arrow's other ends.
    #[serde(default)]
    aid: Option<ecad_model::AidId>,
    /// Only this cable's references.
    #[serde(default)]
    cable: Option<CableId>,
    /// Only references on this page.
    #[serde(default)]
    page: Option<PageId>,
    /// At most this many (default 200, at most 2000).
    #[serde(default)]
    limit: Option<u64>,
    /// Skip this many (the `next` of the last call).
    #[serde(default)]
    after: Option<u64>,
}

/// `contacts {component}` (read only): the device's contact table: offer, head, placement, rows.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactsParams {
    /// The device.
    component: DeviceId,
}

/// `mirrormove {component, at? | placement?}`: where the device's contact table goes.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MirrormoveParams {
    /// The device.
    component: DeviceId,
    /// A page point for the table.
    #[serde(default)]
    at: Option<Point>,
    /// Or a placement: auto, below, right or off.
    #[serde(default)]
    placement: Option<String>,
}

impl Generic for DeviceReferencesParams {
    const ID: &'static str = "references";
    const TITLE: &'static str = "List References";
    const HELP: &'static str = "{component?: id, aid?: id, cable?: id, page?: id, limit?: n (200, max 2000), after?: n} (cable, device and continuation references; read only)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn check(&self, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        self.after.map_or(Ok(0), |n| usize::try_from(n).map_err(|_| CmdError::refused("`after` must be a number"))).map(|_| ())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let limit = self.limit.map_or(DEFAULT_LIMIT, |n| usize::try_from(n).unwrap_or(MAX_LIMIT).clamp(1, MAX_LIMIT));
        let after = self.after.map_or(Ok(0), |n| usize::try_from(n).map_err(|_| CmdError::refused("`after` must be a number")))?;
        let doc = cx.doc;
        let v = devices::read::device_references_json(doc, cx.derived.references(), self.component, self.aid, self.cable, self.page, limit, after)?;
        Ok(Outcome::answer(v))
    }
}

impl Generic for ContactsParams {
    const ID: &'static str = "contacts";
    const TITLE: &'static str = "Device Contacts";
    const HELP: &'static str = "{component: id} (the device's contact table: offer, head, placement, rows; read only)";
    const PAGES: Pages = Pages::Any;
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        Ok(Outcome::answer(devices::read::contacts_json_measured(cx.doc, self.component, cx.look)?))
    }
}

impl Generic for MirrormoveParams {
    const ID: &'static str = "mirrormove";
    const TITLE: &'static str = "Move Mirror Table";
    const HELP: &'static str =
        "{component: id, at?: [x,y], placement?: \"auto\"|\"below\"|\"right\"|\"off\"} (where the device's contact table goes)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let at = self.at.map(|a| a.0);
        let m = plan_mirrormove(cx.doc, self.component, at, self.placement.as_deref())?;
        if m.ops.is_empty() {
            return Ok(Outcome::answer(json!({"changed": false, "mirror": m.mirror})));
        }
        Ok(Outcome::edit("Move contact table", m.ops, json!({"changed": true, "mirror": m.mirror, "message": m.message})))
    }
}
