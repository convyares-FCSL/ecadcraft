//! The cross-reference commands (brief 025, design 025 §4.4, §5.3, §5.4): `xrefs` and `contacts`
//! (read only), `refgoto` (opens a page, records nothing) and `mirrormove` (one `setprop` of the
//! device's `mirror`). Their interactive forms are in [`crate::interact`].

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{CableId, ComponentId, ObjectRef, Op, PageId, Pt, RepresentationId, Subject};
use serde_json::{Value, json};

use crate::commands::{err, id, opt_id, point, with_any_page, with_project};
use crate::xref::{self, Placement};

/// Most references `xrefs` returns per call by default, and at most.
pub const DEFAULT_LIMIT: usize = 200;
pub const MAX_LIMIT: usize = 2000;

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("xrefs", "List References", run_xrefs)
            .params("{component?: id, cable?: id, page?: id, limit?: n (200, max 2000), after?: n} (cable and device references; read only)")
            .noundo(),
        CommandSpec::new("contacts", "Device Contacts", run_contacts)
            .params("{component: id} (the device's contact table: offer, head, placement, rows; read only)")
            .noundo(),
        CommandSpec::new("refgoto", "Go to Reference", run_refgoto)
            .params("{source: id, target?: id} (a cable reference end, a contact, a coil or a continuation arrow: opens the place it shows; records nothing)")
            .noundo(),
        CommandSpec::new("mirrormove", "Move Mirror Table", run_mirrormove)
            .params("{component: id, at?: [x,y], placement?: \"auto\"|\"below\"|\"right\"|\"off\"} (where the device's contact table goes)")
            .interactive(crate::interact::mirrormove)
            .noundo(),
    ]
}

fn run_xrefs(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let component: Option<ComponentId> = opt_id(params, "component").map_err(err)?;
    let cable: Option<CableId> = opt_id(params, "cable").map_err(err)?;
    let page: Option<PageId> = opt_id(params, "page").map_err(err)?;
    let limit = match params.get("limit") {
        None | Some(Value::Null) => DEFAULT_LIMIT,
        Some(v) => v.as_u64().map(|n| usize::try_from(n).unwrap_or(MAX_LIMIT).clamp(1, MAX_LIMIT)).ok_or_else(|| err("`limit` must be a number"))?,
    };
    let after = match params.get("after") {
        None | Some(Value::Null) => 0,
        Some(v) => v.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| err("`after` must be a number"))?,
    };
    with_any_page(s, |p, _| xref::xrefs_json(p.ledger.document(), &p.xref, component, cable, page, limit, after))
}

fn run_contacts(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let c: ComponentId = id(params, "component").map_err(err)?;
    with_any_page(s, |p, _| xref::contacts_json(p.ledger.document(), c))
}

/// Where `refgoto` goes: the page, the point to centre, the item to select, and the words.
struct Jump {
    page: PageId,
    at: Pt,
    item: Option<ecad_model::ItemId>,
    target: RepresentationId,
    message: String,
}

fn run_refgoto(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let source: RepresentationId = id(params, "source").map_err(err)?;
    let target: Option<RepresentationId> = opt_id(params, "target").map_err(err)?;
    let (uid, jump, opened) = with_any_page(s, |p, pg| {
        let doc = p.ledger.document();
        let r = doc.representations.get(&source).ok_or("no such reference")?;
        let arrow = matches!(p.xref.member(source), Some((xref::Group::Continuation(_), _)));
        let none = match r.subject {
            // Brief 049: a lonely continuation arrow.
            Subject::Component(_) if arrow || crate::continuations::arrow(doc, source).is_some() => "This continuation has no other end.",
            Subject::Component(_) => "This symbol shows no reference: its device has no coil or no contacts.",
            Subject::Cable(_) => "this reference has no other end",
            Subject::Conductor(_) => "That is not a reference.",
        };
        if p.xref.member(source).is_none() {
            return Err(none.into());
        }
        // Brief 049 (design 049 §5.3): an arrow goes to its first other end, then the next on
        // repeat (after the place the last go-to ringed), wrapping.
        let t = if arrow {
            crate::continuations::goto(doc, source, target, p.goto_ring.map(|(page, at, _)| (page, at))).ok_or(none)?
        } else {
            xref::goto_target(doc, &p.xref, source, target).ok_or(none)?
        };
        let tr = doc.representations.get(&t).ok_or("the other end cannot be located")?;
        let at = xref::anchor(doc, tr)
            .or_else(|| {
                tr.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
                    ecad_model::ItemKind::Primitive(ecad_model::Primitive::Point { at }) if at.is_finite() => Some(*at),
                    _ => None,
                })
            })
            .ok_or("the other end cannot be located")?;
        let mut places = xref::Places::new(doc);
        let place = places.text(tr.page, Some(at));
        let what = match tr.subject {
            Subject::Component(c) if crate::continuations::is_continuation(doc, c) => {
                format!(
                    "continuation {}",
                    doc.components.get(&c).and_then(|x| x.properties.get(ecad_model::connectivity::NAME)).cloned().unwrap_or_default()
                )
            }
            Subject::Component(c) => match xref::device(doc, c).head {
                Some(h) if h == t => format!("{}'s coil", crate::devices::tag_words(doc, c)),
                _ => format!("{}'s contact", crate::devices::tag_words(doc, c)),
            },
            Subject::Cable(k) => format!("the other end of cable {}", crate::views::cable_title(doc, k)),
            Subject::Conductor(_) => "the reference".into(),
        };
        let message = format!("{}, {what} ({place}).", crate::references::page_words(doc, tr.page).replacen("page", "Page", 1));
        let jump = Jump { page: tr.page, at, item: tr.items.first().copied(), target: t, message };
        let opened = p.pages.get(&jump.page).and_then(|v| v.uid).is_none();
        let uid = p.open_page(pg, jump.page)?;
        // The go-to ring (display only, until the next change), as the Checks tab's.
        p.goto_ring = Some((jump.page, jump.at, crate::checks::Severity::Info));
        Ok((uid, jump, opened))
    })?;
    if opened {
        let _ = s.zoom_extents();
    }
    if let Ok(st) = s.state_mut() {
        let height = st.view().height;
        let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(jump.at.x, jump.at.y), height, twist: 0.0 });
    }
    let page = jump.page;
    let item = jump.item;
    let handle = with_any_page(s, |p, _| Ok(item.and_then(|i| p.pages.get(&page).and_then(|v| v.map.item(i)))))?;
    if let Some(h) = handle {
        s.set_selection(vec![h]);
    }
    Ok(json!({
        "page": jump.page.hex(),
        "document": uid,
        "target": jump.target.hex(),
        "item": jump.item.map(|i| i.hex()),
        "message": jump.message,
    }))
}

/// The `mirror` value `mirrormove` writes for `params` on device `c` of `doc`.
///
/// # Errors
/// Neither `at` nor `placement`, both, an unreadable word, or a point too far from the coil.
pub(crate) fn mirror_for(doc: &ecad_model::Document, c: ComponentId, params: &Value) -> Result<Option<String>, String> {
    let at = match params.get("at") {
        None | Some(Value::Null) => None,
        Some(v) => Some(point(v).ok_or("`at` must be [x, y] with finite numbers")?),
    };
    let word = params.get("placement").filter(|v| !v.is_null());
    let placement = match (at, word) {
        (Some(_), Some(_)) => return Err("give `at` or `placement`, not both".into()),
        (None, None) => return Err("`at` ([x, y]) or `placement` (auto, below, right, off) is required".into()),
        (None, Some(w)) => {
            let w = w.as_str().ok_or("`placement` must be auto, below, right or off")?;
            match xref::parse_mirror(w)? {
                Placement::At(_) => return Err("`placement` must be auto, below, right or off; use `at` for a point".into()),
                p => p,
            }
        }
        (Some(p), None) => {
            let dev = xref::device(doc, c);
            let head =
                dev.head.and_then(|h| doc.representations.get(&h)).ok_or("This device has no coil (or other main symbol) to put a table under.")?;
            let origin = xref::anchor(doc, head).ok_or("This device's coil cannot be located.")?;
            let k = match doc.settings.length_unit {
                ecad_model::LengthUnit::Millimetre => 1.0,
                ecad_model::LengthUnit::Inch => 25.4,
            };
            let d = Pt::new((p.x - origin.x) * k, (p.y - origin.y) * k);
            if !(d.x.abs() <= xref::MAX_OFFSET_MM && d.y.abs() <= xref::MAX_OFFSET_MM) {
                return Err(format!("Put the table within {} mm of its coil.", xref::MAX_OFFSET_MM));
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

fn run_mirrormove(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let c: ComponentId = id(params, "component").map_err(err)?;
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let comp = doc.components.get(&c).ok_or("no such component")?;
        let after = mirror_for(doc, c, params)?;
        let before = comp.properties.get(xref::MIRROR_KEY).cloned();
        let tag = crate::devices::tag_words(doc, c);
        if before == after {
            return Ok(json!({"changed": false, "mirror": after}));
        }
        p.apply_edit(
            pg,
            "mirrormove",
            "Move contact table",
            vec![Op::SetProperty { object: ObjectRef::Component(c), key: xref::MIRROR_KEY.into(), before, after: after.clone() }],
        )?;
        let words = match after.as_deref() {
            None => "follows the project default".to_string(),
            Some("off") => "is hidden".to_string(),
            Some(w) if w.starts_with("at ") => "is moved".to_string(),
            Some(w) => format!("is placed {w}"),
        };
        Ok(json!({"changed": true, "mirror": after, "message": format!("{tag}'s contact table {words}.")}))
    })
}
