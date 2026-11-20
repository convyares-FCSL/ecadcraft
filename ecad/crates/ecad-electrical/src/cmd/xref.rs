//! The rules of the cross-reference commands (brief 025, design 025 §4.4, §5.3, §5.4): where
//! `refgoto` goes, and the `mirror` value `mirrormove` writes. The listings (`xrefs`,
//! `contacts`) are [`crate::xref`]'s own; the engine crate's shims open pages and answer in JSON.

use ecad_model::{DeviceId, Document, ItemId, Op, PageId, Pt, RepresentationId, Subject};

use crate::error::CmdError;
use crate::traits::DeviceKind;
use crate::xref::{self, Placement, RefIndex};

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
/// repeat (after `ring`, the place the last go-to ringed), wrapping (brief 049, design 049 §5.3).
///
/// # Errors
/// No such reference, one with no other end, or an other end that cannot be located.
pub fn goto(
    doc: &Document, idx: &RefIndex, source: RepresentationId, target: Option<RepresentationId>, ring: Option<(PageId, Pt)>,
) -> Result<Jump, CmdError> {
    let r = doc.representations.get(&source).ok_or(CmdError::refused("no such reference"))?;
    let arrow = matches!(idx.member(source), Some((xref::Group::Continuation(_), _)));
    let none = match r.subject {
        // Brief 049: a lonely continuation arrow.
        Subject::Device(_) if arrow || crate::continuations::arrow(doc, source).is_some() => "This continuation has no other end.",
        Subject::Device(_) => "This symbol shows no reference: its device has no coil or no contacts.",
        Subject::Cable(_) => "this reference has no other end",
        Subject::Wire(_) => "That is not a reference.",
    };
    if idx.member(source).is_none() {
        return Err(CmdError::refused(none));
    }
    let t = if arrow { crate::continuations::goto(doc, source, target, ring) } else { xref::goto_target(doc, idx, source, target) };
    let t = t.ok_or(CmdError::refused(none))?;
    const LOST: &str = "the other end cannot be located";
    let tr = doc.representations.get(&t).ok_or(CmdError::refused(LOST))?;
    let at = xref::anchor(doc, tr)
        .or_else(|| {
            tr.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
                ecad_model::ItemKind::Primitive(ecad_model::Primitive::Point { at }) if at.is_finite() => Some(*at),
                _ => None,
            })
        })
        .ok_or(CmdError::refused(LOST))?;
    let mut places = xref::Places::new(doc);
    let place = places.text(tr.page, Some(at));
    let what = match tr.subject {
        Subject::Device(c) if crate::continuations::is_continuation(doc, c) => {
            format!(
                "continuation {}",
                doc.devices.get(&c).and_then(|x| x.properties.get(ecad_model::connectivity::NAME)).cloned().unwrap_or_default()
            )
        }
        Subject::Device(c) => match xref::device(doc, c).head {
            Some(h) if h == t => format!("{}'s coil", crate::devices::tag_words(doc, c)),
            _ => format!("{}'s contact", crate::devices::tag_words(doc, c)),
        },
        Subject::Cable(k) => format!("the other end of cable {}", crate::cables::cable_title(doc, k)),
        Subject::Wire(_) => "the reference".into(),
    };
    let message = format!("{}, {what} ({place}).", crate::references::page_words(doc, tr.page).replacen("page", "Page", 1));
    Ok(Jump { page: tr.page, at, item: tr.items.first().copied(), target: t, message })
}

/// The `mirror` value `mirrormove` writes on device `c`: a point `at` (page coordinates) for the
/// contact table, or a `placement` word (auto, below, right, off). `None`: the project default.
///
/// # Errors
/// Neither `at` nor `placement`, both, an unreadable word, or a point too far from the coil.
pub fn mirror_for(doc: &Document, c: DeviceId, at: Option<Pt>, placement: Option<&str>) -> Result<Option<String>, CmdError> {
    let placement = match (at, placement) {
        (Some(_), Some(_)) => return Err(CmdError::input("give `at` or `placement`, not both")),
        (None, None) => return Err(CmdError::input("`at` ([x, y]) or `placement` (auto, below, right, off) is required")),
        (None, Some(w)) => match xref::parse_mirror(w)? {
            Placement::At(_) => return Err(CmdError::refused("`placement` must be auto, below, right or off; use `at` for a point")),
            p => p,
        },
        (Some(p), None) => {
            let dev = xref::device(doc, c);
            let head = dev
                .head
                .and_then(|h| doc.representations.get(&h))
                .ok_or(CmdError::refused("This device has no coil (or other head symbol) to put a table under."))?;
            let origin = xref::anchor(doc, head).ok_or(CmdError::refused("This device's coil cannot be located."))?;
            let k = match doc.settings.length_unit {
                ecad_model::LengthUnit::Millimetre => 1.0,
                ecad_model::LengthUnit::Inch => 25.4,
            };
            let d = Pt::new((p.x - origin.x) * k, (p.y - origin.y) * k);
            if !(d.x.abs() <= xref::MAX_OFFSET_MM && d.y.abs() <= xref::MAX_OFFSET_MM) {
                return Err(CmdError::refused(format!("Put the table within {} mm of its coil.", xref::MAX_OFFSET_MM)));
            }
            Placement::At(d)
        }
    };
    Ok(match placement {
        // The project default is `auto` unless the project says otherwise: `auto` writes `auto`
        // only when the project's default differs.
        Placement::Auto if doc.settings.contact_mirror == ecad_model::ContactMirror::Auto => None,
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
pub fn plan_mirrormove(doc: &Document, c: DeviceId, at: Option<Pt>, placement: Option<&str>) -> Result<Mirrored, CmdError> {
    let comp = doc.devices.get(&c).ok_or(CmdError::refused("no such component"))?;
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
    let message = format!("{}'s contact table {words}.", crate::devices::tag_words(doc, c));
    let who = crate::devices::tag_words(doc, c);
    let table = xref::table_of_typed(after.as_deref())?;
    let ops = vec![Op::ModifyDevice { id: c, before: comp.clone(), after: xref::with_table(comp, &who, table)? }];
    Ok(Mirrored { ops, mirror: after, message })
}
