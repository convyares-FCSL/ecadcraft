//! The cross-reference commands (brief 025, design 025 §4.4, §5.3, §5.4): `xrefs` and `contacts`
//! (read only), `refgoto` (opens a page, records nothing) and `mirrormove` (one `setprop` of the
//! device's `mirror`). Their interactive forms are in [`crate::interact`].
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::xref`] decide, and the shim opens the page or commits the ops and
//! answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::xref as rules;
use ecad_model::{CableId, DeviceId, PageId, RepresentationId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Command, Point, Typed};
use crate::cmd::{on_any_page, on_project};
use crate::xref;

/// Most references `xrefs` returns per call by default, and at most.
pub const DEFAULT_LIMIT: usize = 200;
pub const MAX_LIMIT: usize = 2000;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<XrefsParams>("xrefs"),
    Typed::of::<ContactsParams>("contacts"),
    Typed::of::<RefgotoParams>("refgoto"),
    Typed::of::<MirrormoveParams>("mirrormove"),
];

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("xrefs", "List References", crate::cmd::input::typed::<XrefsParams>)
            .params("{component?: id, cable?: id, page?: id, limit?: n (200, max 2000), after?: n} (cable and device references; read only)")
            .noundo(),
        CommandSpec::new("contacts", "Device Contacts", crate::cmd::input::typed::<ContactsParams>)
            .params("{component: id} (the device's contact table: offer, head, placement, rows; read only)")
            .noundo(),
        CommandSpec::new("refgoto", "Go to Reference", crate::cmd::input::typed::<RefgotoParams>)
            .params("{source: id, target?: id} (a cable reference end, a contact, a coil or a continuation arrow: opens the place it shows; records nothing)")
            .noundo(),
        CommandSpec::new("mirrormove", "Move Mirror Table", crate::cmd::input::typed::<MirrormoveParams>)
            .params("{component: id, at?: [x,y], placement?: \"auto\"|\"below\"|\"right\"|\"off\"} (where the device's contact table goes)")
            .interactive(crate::interact::mirrormove)
            .noundo(),
    ]
}

/// `xrefs {component?, cable?, page?, limit?, after?}` (read only): cable and device references,
/// a page at a time.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct XrefsParams {
    /// Only this device's references.
    #[serde(default)]
    component: Option<DeviceId>,
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

impl Command for XrefsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let limit = self.limit.map_or(DEFAULT_LIMIT, |n| usize::try_from(n).unwrap_or(MAX_LIMIT).clamp(1, MAX_LIMIT));
        let after = match self.after {
            None => 0,
            Some(n) => usize::try_from(n).map_err(|_| crate::cmd::err("`after` must be a number"))?,
        };
        let (component, cable, page) = (self.component, self.cable, self.page);
        on_any_page(s, |p, _| xref::xrefs_json(p.ledger.document(), &p.xref, component, cable, page, limit, after))
    }
}

/// `contacts {component}` (read only): the device's contact table: offer, head, placement, rows.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContactsParams {
    /// The device.
    component: DeviceId,
}

impl Command for ContactsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, _| xref::contacts_json(p.ledger.document(), self.component).map_err(CmdError::refused))
    }
}

/// `refgoto {source, target?}`: opens the place a cable reference end, a contact, a coil or a
/// continuation arrow shows; records nothing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RefgotoParams {
    /// The reference clicked (a representation).
    source: RepresentationId,
    /// Which of its other ends (default: the first, or the next after the last go-to).
    #[serde(default)]
    target: Option<RepresentationId>,
}

impl Command for RefgotoParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let (uid, jump, opened) = on_any_page(s, |p, pg| {
            let ring = p.goto_ring.map(|(page, at, _)| (page, at));
            let jump = rules::goto(p.ledger.document(), &p.xref, self.source, self.target, ring)?;
            let opened = p.pages.get(&jump.page).and_then(|v| v.uid).is_none();
            let uid = p.open_page(pg, jump.page).map_err(CmdError::refused)?;
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
        let handle = on_any_page(s, |p, _| Ok(item.and_then(|i| p.pages.get(&page).and_then(|v| v.map.item(i)))))?;
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
}

/// `mirrormove {component, at? | placement?}`: where the device's contact table goes.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MirrormoveParams {
    /// The device.
    component: DeviceId,
    /// A page point for the table.
    #[serde(default)]
    at: Option<Point>,
    /// Or a placement: auto, below, right or off.
    #[serde(default)]
    placement: Option<String>,
}

impl Command for MirrormoveParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let at = self.at.map(|a| a.0);
        on_project(s, |p, pg, _| {
            let m = rules::plan_mirrormove(p.ledger.document(), self.component, at, self.placement.as_deref())?;
            if m.ops.is_empty() {
                return Ok(json!({"changed": false, "mirror": m.mirror}));
            }
            p.apply_edit(pg, "mirrormove", "Move contact table", m.ops)?;
            Ok(json!({"changed": true, "mirror": m.mirror, "message": m.message}))
        })
    }
}
