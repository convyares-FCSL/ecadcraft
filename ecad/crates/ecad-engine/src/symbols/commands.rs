//! The commands of the symbol builder: `symbolnew`, `symboledit`,
//! `symbolimpact` (read only), `symbolupdate` and `makesymbol`, and `place` of a project symbol.
//!
//! Every write is one transaction, committed through [`crate::Project::apply_edit`]; nothing here opens a
//! dialog. The builder saves through these JSON forms.
//!
//! Each command is a thin shim: its params struct reads the JSON, the rules in
//! [`ecad_electrical::symbols::commands`] (content, impact, updates, promotion) decide, and the shim
//! finds the selection on the page, commits the ops and answers in JSON.
//!
//! Not here: the builder's drawing, drafts and the review drawer; "update available" in
//! Properties and Devices.

use cadcraft_engine::doc::Handle;
use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::symbols::commands::not_plain;
use ecad_electrical::symbols::commands::{self as rules, MakeAt};
use ecad_model::geometry::units::Radians;
use ecad_model::{DeviceId, Placement};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::{MAX_HANDLES, err, on_project, plain_style};
use crate::project::Mapped;
use ecad_electrical::commands::input::{Bounded, Point};
use ecad_electrical::symbols;

/// The commands of this module with a typed input.
pub(crate) const TYPED: &[Typed] = &[Typed::of::<MakeParams>("makesymbol")];

/// The commands this module adds (registered in `lanes.rs`).
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("makesymbol", "Make Symbol", crate::commands::input::typed::<MakeParams>).noundo()]
}

// ---------------------------------------------------------------- content

use ecad_electrical::symbols::builder::{Bridges, ContentFields, Points, Shapes, Slots};

// ---------------------------------------------------------------- make symbol

/// A symbol from plain geometry (`handles`, default the selection), or a drawn component promoted (`component`),
/// keeping its IDs.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MakeParams {
    /// The shapes to make it of (hex handles; default: the selection).
    #[serde(default)]
    handles: Option<Bounded<String, 0, { MAX_HANDLES }>>,
    /// A drawn component to promote instead.
    #[serde(default)]
    component: Option<DeviceId>,
    /// The symbol's name.
    #[serde(default)]
    name: Option<String>,
    /// Its key.
    #[serde(default)]
    key: Option<String>,
    /// The drawing as edited in the builder, in the symbol's frame (points and slots then in that
    /// frame too; default: the page geometry).
    #[serde(default)]
    graphics: Option<Shapes>,
    /// Its connection points (page points, unless `graphics` is given).
    #[serde(default)]
    points: Option<Points>,
    /// Where its device texts go (page points, unless `graphics` is given).
    #[serde(default)]
    slots: Option<Slots>,
    /// Its defaults.
    #[serde(default)]
    defaults: Option<std::collections::BTreeMap<String, String>>,
    /// Groups of point keys bridged inside the device.
    #[serde(default)]
    bridges: Option<Bridges>,
    /// Replace the drawing with the new symbol (default true).
    #[serde(default)]
    replace: Option<bool>,
    /// The page point of the symbol's origin (default: the drawing's centre on the point grid).
    #[serde(default)]
    at: Option<Point>,
}

impl Command for MakeParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        // A promoted component's drawing is its own; the selection is not read.
        let hs: Vec<Handle> = match (&self.component, self.handles) {
            (Some(_), _) => Vec::new(),
            (None, None) => s.selection(),
            (None, Some(b)) => {
                b.0.iter()
                    .map(|h| Handle::parse_hex(h).ok_or_else(|| err(format!("`{}` is not a valid handle", json!(h)))))
                    .collect::<Result<_, _>>()?
            }
        };
        let fields = ContentFields {
            name: self.name,
            key: self.key,
            graphics: self.graphics,
            points: self.points,
            slots: self.slots,
            defaults: self.defaults,
            bridges: self.bridges,
        };
        let content = fields.content().map_err(|e| err(e.to_string()))?;
        let replace = self.replace.unwrap_or(true);
        let at = self.at.map(|a| a.0);
        on_project(s, |p, pg, page| {
            if let Some(c) = self.component {
                let style = plain_style(p);
                let made = rules::plan_promote(p.ledger.document(), &mut p.ids, c, content, style)?;
                p.apply_edit(pg, "makesymbol", &made.plan.label, made.plan.ops)?;
                let mut v = rules::result(p.ledger.document(), made.plan.id);
                v["component"] = json!(c.hex());
                v["representation"] = json!(made.representation.hex());
                return Ok(v);
            }
            let map = &p.pages.get(&page).ok_or(CmdError::refused("page view not found"))?.map;
            let doc = p.ledger.document();
            let mut items = Vec::new();
            for h in hs {
                match map.get(h) {
                    Some(Mapped::Item(i)) if doc.representation_of_item(i).is_none() => items.push(i),
                    Some(Mapped::Item(i)) => return Err(CmdError::refused(not_plain(doc, i))),
                    Some(_) => return Err(CmdError::refused("Make symbol takes lines, arcs and text; part of the selection already has a meaning.")),
                    None => return Err(CmdError::refused("The selection holds an object this page does not know.")),
                }
            }
            let style = plain_style(p);
            let made = rules::plan_make(&p.ledger, &mut p.ids, &mut p.indexes.tag_high, &items, content, MakeAt { page, style, at, replace })?;
            p.apply_edit(pg, "makesymbol", &made.plan.label, made.plan.ops)?;
            let mut v = rules::result(p.ledger.document(), made.plan.id);
            if let Some((c, r, t)) = made.placed {
                v["component"] = json!(c.hex());
                v["representation"] = json!(r.hex());
                v["tag"] = json!(t);
            }
            Ok(v)
        })
    }
}

// ---------------------------------------------------------------- placing

/// `place {symbol: <project key>, at, rotation?, mirror?, device?, tag?}`: the
/// newest version of that project symbol. `None` when `key` names no project symbol.
pub(crate) fn place_project_key(
    s: &mut Session, params: &crate::devices::commands::PlaceParams, key: &str,
) -> cadcraft_engine::Result<Option<Value>> {
    let at = params.at.0;
    if !(at.x.abs() <= ecad_electrical::commands::input::MAX_COORD && at.y.abs() <= ecad_electrical::commands::input::MAX_COORD) {
        return Err(err(ecad_electrical::commands::input::OUT_OF_RANGE));
    }
    let degrees = params.rotation.unwrap_or_default();
    let mirror = params.mirror.unwrap_or(false);
    let device = params.device_id()?;
    // Pack re-run, bug 2: a typed tag names the new device, as for a built-in symbol.
    let tag = params.tag.clone();
    ecad_electrical::devices::commands::place::one_of(device.is_some(), tag.is_some(), "device").map_err(|e| err(e.to_string()))?;
    let modes = params.modes()?;
    let key = key.to_string();
    on_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let Some(v) = symbols::project::project_by_key(doc, &key) else { return Ok(None) };
        let (id, def) = (v.id, v.def.clone());
        // A typed tag a device already holds is that device (a tag is an identity), as `place`
        // does for built-in symbols; devices and cables share one namespace.
        let (device, tag) = ecad_electrical::devices::commands::place::project_target(doc, device, tag.as_deref())?;
        let pl = Placement { origin: at, rotation: Radians::from(degrees), mirror, scale: 1.0 };
        let style = plain_style(p);
        let mut plan = rules::plan_place(&p.ledger, &mut p.ids, &mut p.indexes.tag_high, page, style, id, &def, pl, device, tag)?;
        let label = if device.is_some() { format!("Add symbol to {}", plan.tag) } else { "Place symbol".to_string() };
        // The place proposes.
        let proposed = crate::symbols::draw::plan_place(p, &plan.ops, modes, plan.component, page, at, None).map_err(CmdError::refused)?;
        plan.ops.extend(proposed.ops.iter().cloned());
        p.apply_edit(pg, "place", &label, plan.ops)?;
        let doc = p.ledger.document();
        let connection_points: Vec<Value> =
            doc.connection_points_of(plan.component).map(|(t, x)| json!({"id": t.hex(), "key": x.key, "label": x.label})).collect();
        let mut v = json!({
            "component": plan.component.hex(), "representation": plan.representation.hex(), "definition": id.hex(),
            "tag": plan.tag, "joined": device.is_some(), "connection_points": connection_points,
        });
        crate::symbols::draw::finish_place(p, pg, plan.component, device.is_some(), &proposed, &mut v);
        Ok(Some(v))
    })
}
