//! The interactive forms of brief 018 (design 018 §5.3, §6.2, §6.3): Place Symbol with starter
//! symbols and devices, Device Properties and Delete Device. Each commits through its JSON form.
//!
//! A duplicate tag is never a dead end (owner, 2026-11-08): a tag that exists offers to add the
//! symbol to that device (placing) or to make the device part of it (renaming), with "Choose
//! another" as the other answer.

use cadcraft_engine::doc::{Entity, EntityKind};
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Accept, Input, Interactive, Prompt, Session, Step};
use ecad_model::{ComponentId, Placement, Subject};
use serde_json::{Value, json};

use super::forms::{R, Safe, commit, other, paper_guard, safely};
use super::{
    OUT_OF_RANGE, TERMINAL_ACI, TERMINAL_PX, can_start, in_range, pick, pixel, preview_entity, pt, read, ring_kind,
};
use crate::devices::{self, TextOf};
use crate::project::Mapped;
use crate::symbols::{self, Starter};

/// A prompt that takes a typed text or a click.
const TEXT_OR_POINT: Accept = Accept { point: true, number: false, text: true, select: false, enter: true };

fn prompt(message: impl Into<String>, accept: Accept, keywords: &[&str], default: Option<String>) -> Prompt {
    let mut p = Prompt::new(message, accept).kw(keywords);
    p.default = default;
    p
}

fn yes(i: &Input) -> Option<bool> {
    match i {
        Input::Keyword(k) | Input::Text(k) => {
            let k = k.trim().to_ascii_lowercase();
            if k.starts_with('y') {
                Some(true)
            } else if k.starts_with('c') || k.starts_with('n') {
                Some(false)
            } else {
                None
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------- Place Symbol (§5.3)

/// `place`: choose a symbol (typed search, or a click for the default), then click it onto the
/// page as a new device or as another symbol of one; repeat until Enter.
pub(crate) fn place(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    let default = read(s, |p, _| Ok(p.last_placed.as_ref().map(|(k, _)| k.clone()))).ok().flatten();
    let default = default.and_then(|k| symbols::by_key(&k)).unwrap_or(&symbols::STARTERS[0]);
    Ok(Box::new(Safe(Place {
        stage: Stage::Symbol,
        default,
        symbol: None,
        rotation: 0,
        mirror: false,
        join: None,
        tag: None,
        last: None,
    })))
}

#[derive(Debug, Clone, PartialEq)]
enum Stage {
    /// Step 1: which symbol.
    Symbol,
    /// §5.2: which symbol to add to this device.
    DeviceSymbol(ComponentId),
    /// Step 2: where.
    Point,
    /// Step 2d: which device to join.
    Device,
    /// The tag of the next new device.
    Tag,
    /// The typed tag exists: add the symbol to that device?
    ConfirmJoin(ComponentId, String),
    /// The typed device does not exist: make it a new device with that tag?
    ConfirmNew(String),
}

struct Place {
    stage: Stage,
    default: &'static Starter,
    symbol: Option<&'static Starter>,
    /// Degrees counter-clockwise: 0, 90, 180 or 270.
    rotation: u16,
    mirror: bool,
    /// The device the next symbol joins.
    join: Option<ComponentId>,
    /// The typed tag of the next new device.
    tag: Option<String>,
    /// The device of the most recent placement in this command.
    last: Option<ComponentId>,
}

impl Place {
    fn symbol(&self) -> &'static Starter {
        self.symbol.unwrap_or(self.default)
    }

    fn placement(&self, at: Vec2) -> Placement {
        Placement { origin: pt(at), rotation: f64::from(self.rotation).to_radians(), mirror: self.mirror, scale: 1.0 }
    }

    /// What the next click makes: `new device -K03` or `joins -K01 as 13/14`, with the tag and
    /// labels it will draw.
    fn outcome(&self, s: &Session) -> Option<(String, String, Vec<String>)> {
        let sym = self.symbol();
        read(s, |p, _| {
            let doc = p.ledger.document();
            match self.join {
                Some(c) => {
                    let labels = devices::join_labels(doc, c, sym, &Default::default())?;
                    let t = devices::tag_words(doc, c);
                    Ok((format!("joins {t} as {}", labels.join("/")), t, labels))
                }
                None => {
                    let t = match &self.tag {
                        Some(t) => t.clone(),
                        None => p.tag_high.clone().next(&p.ledger, &devices::prefix_for(doc, sym))?,
                    };
                    Ok((format!("new device {t}"), t, sym.labels()))
                }
            }
        })
        .ok()
    }

    /// Resolve a typed symbol (design 018 §5.3 step 1): one match is taken; a device tag goes to
    /// §5.2.
    fn choose(&mut self, s: &mut Session, text: &str) -> R<Step> {
        let t = text.trim();
        // A typed point that could not be used (not finite, out of range) is no symbol name.
        if t.contains(',') || t.contains('<') {
            return Err(super::forms::not_a_point(t));
        }
        let device = read(s, |p, _| Ok(devices::holder_of(p.ledger.document(), t, None))).ok().flatten();
        if t.starts_with('-')
            && let Some(c) = device
        {
            self.stage = Stage::DeviceSymbol(c);
            return Ok(Step::Continue);
        }
        let found = match symbols::by_key(t) {
            Some(exact) => vec![exact],
            None => symbols::search(t),
        };
        match found.as_slice() {
            [one] => {
                self.symbol = Some(one);
                Ok(Step::Continue)
            }
            [] => match device {
                Some(c) => {
                    self.stage = Stage::DeviceSymbol(c);
                    Ok(Step::Continue)
                }
                None => Err(other(format!("No symbol \"{t}\". Type List to see them."))),
            },
            many => {
                let keys: Vec<&str> = many.iter().map(|s| s.key).collect();
                s.echo(format!("\"{t}\" matches {} symbols: {}.", many.len(), keys.join(", ")));
                Ok(Step::Continue)
            }
        }
    }

    fn list(s: &mut Session) {
        for x in symbols::STARTERS {
            s.echo(format!("{:<9} {:<28} -{}  {}", x.key, x.name, x.prefix, x.labels().join("/")));
        }
    }

    fn place_at(&mut self, s: &mut Session, at: Vec2) -> R<Step> {
        if !in_range(at) {
            return Err(other(OUT_OF_RANGE));
        }
        let sym = self.symbol();
        let mut params = json!({
            "symbol": sym.key, "at": [at.x, at.y], "rotation": f64::from(self.rotation), "mirror": self.mirror,
        });
        if let Some(c) = self.join {
            params["device"] = Value::String(c.hex());
        } else if let Some(t) = &self.tag {
            params["tag"] = Value::String(t.clone());
        }
        let r = commit(s, "place", &params)?;
        let tag = r.get("tag").and_then(Value::as_str).unwrap_or_default().to_string();
        let labels: Vec<&str> = r
            .get("terminals")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|t| t.get("label").and_then(Value::as_str))
            .collect();
        let component = r
            .get("component")
            .and_then(Value::as_str)
            .and_then(|h| u128::from_str_radix(h, 16).ok())
            .map(ComponentId::from);
        if r.get("joined").and_then(Value::as_bool) == Some(true) {
            let pages: Vec<String> = r
                .get("pages")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .map(|n| n.to_string())
                .collect();
            let pages = if pages.len() == 1 {
                format!("page {}", pages.join(", "))
            } else {
                format!("pages {}", pages.join(", "))
            };
            s.echo(format!("Added {} {} to {tag} (now on {pages}).", sym.name, labels.join("/")));
            self.join = component;
        } else {
            s.echo(format!("Placed {tag} ({}) with connection points {}.", sym.name, labels.join(", ")));
        }
        // A typed tag names one device; the next click makes a new one with the next number.
        self.tag = None;
        self.last = component;
        Ok(Step::Continue)
    }

    /// The default answer at "Enter device to join": this command's last device, else the last
    /// device placed with this prefix, else New.
    fn default_device(&self, s: &Session) -> Option<ComponentId> {
        if self.last.is_some() {
            return self.last;
        }
        let prefix = self.symbol().prefix;
        read(s, |p, _| {
            let doc = p.ledger.document();
            Ok(p.last_placed.as_ref().map(|(_, c)| *c).filter(|c| {
                devices::tag_of(doc, *c).and_then(ecad_model::tag::parse).is_some_and(|(px, _)| px == prefix)
            }))
        })
        .ok()
        .flatten()
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        match self.stage.clone() {
            Stage::Symbol | Stage::DeviceSymbol(_) => {
                let device = match &self.stage {
                    Stage::DeviceSymbol(c) => Some(*c),
                    _ => None,
                };
                match i {
                    Input::Keyword(k) if k == "List" => {
                        Self::list(s);
                        Ok(Step::Continue)
                    }
                    // Rotate and Mirror take the default symbol and turn it (Place Symbol, R, as
                    // before brief 018).
                    Input::Keyword(k) if device.is_none() && (k == "Rotate" || k == "Mirror") => {
                        if k == "Rotate" {
                            self.rotation = (self.rotation + 90) % 360;
                        } else {
                            self.mirror = !self.mirror;
                        }
                        self.symbol = Some(self.default);
                        self.stage = Stage::Point;
                        Ok(Step::Continue)
                    }
                    Input::Enter => {
                        self.symbol = Some(self.default);
                        self.join = device;
                        self.stage = Stage::Point;
                        Ok(Step::Continue)
                    }
                    Input::Text(t) => {
                        self.choose(s, &t)?;
                        if self.symbol.is_some() && matches!(self.stage, Stage::Symbol | Stage::DeviceSymbol(_)) {
                            if let Some(c) = device {
                                read(s, |p, _| {
                                    devices::join_labels(p.ledger.document(), c, self.symbol(), &Default::default())
                                })
                                .map_err(other)?;
                                self.join = Some(c);
                            }
                            self.stage = Stage::Point;
                        }
                        Ok(Step::Continue)
                    }
                    // A click takes the default symbol and places it there (click-first, design 008).
                    Input::Point(p) if device.is_none() => {
                        self.symbol = Some(self.default);
                        self.stage = Stage::Point;
                        self.place_at(s, p)
                    }
                    _ => Ok(Step::Continue),
                }
            }
            Stage::Point => match i {
                Input::Keyword(k) if k == "Rotate" => {
                    self.rotation = (self.rotation + 90) % 360;
                    Ok(Step::Continue)
                }
                Input::Keyword(k) if k == "Mirror" => {
                    self.mirror = !self.mirror;
                    Ok(Step::Continue)
                }
                Input::Keyword(k) if k == "Device" => {
                    self.stage = Stage::Device;
                    Ok(Step::Continue)
                }
                Input::Keyword(k) if k == "Tag" => {
                    self.stage = Stage::Tag;
                    Ok(Step::Continue)
                }
                Input::Point(p) => self.place_at(s, p),
                Input::Enter => Ok(Step::Done),
                Input::Text(t) => Err(super::forms::not_a_point(&t)),
                _ => Ok(Step::Continue),
            },
            Stage::Device => match i {
                Input::Keyword(k) if k == "New" => {
                    self.join = None;
                    self.stage = Stage::Point;
                    Ok(Step::Continue)
                }
                // The default device's keyword, clicked or typed: the same as typing its tag.
                Input::Keyword(k) => self.step(s, Input::Text(k)),
                Input::Enter => {
                    self.join = self.default_device(s);
                    self.stage = Stage::Point;
                    Ok(Step::Continue)
                }
                Input::Text(t) => {
                    let sym = self.symbol();
                    let (holder, normal) = read(s, |p, _| {
                        let doc = p.ledger.document();
                        let normal = ecad_model::tag::normalise(&t, devices::padding(doc))?;
                        let holder = devices::holder_of(doc, &normal, None);
                        if let Some(c) = holder {
                            devices::join_labels(doc, c, sym, &Default::default())?;
                        }
                        Ok((holder, normal))
                    })
                    .map_err(other)?;
                    match holder {
                        Some(c) => {
                            self.join = Some(c);
                            self.stage = Stage::Point;
                        }
                        None => self.stage = Stage::ConfirmNew(normal),
                    }
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            },
            Stage::Tag => match i {
                Input::Enter => {
                    self.stage = Stage::Point;
                    Ok(Step::Continue)
                }
                Input::Text(t) => {
                    let (holder, normal) = read(s, |p, _| {
                        let doc = p.ledger.document();
                        let normal = ecad_model::tag::normalise(&t, devices::padding(doc))?;
                        Ok((devices::holder_of(doc, &normal, None), normal))
                    })
                    .map_err(other)?;
                    match holder {
                        Some(c) => self.stage = Stage::ConfirmJoin(c, normal),
                        None => {
                            self.tag = Some(normal);
                            self.join = None;
                            self.stage = Stage::Point;
                        }
                    }
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            },
            Stage::ConfirmJoin(c, _) => match (yes(&i), &i) {
                (Some(true), _) | (None, Input::Enter) => {
                    read(s, |p, _| devices::join_labels(p.ledger.document(), c, self.symbol(), &Default::default()))
                        .map_err(|e| {
                            self.stage = Stage::Tag;
                            other(e)
                        })?;
                    self.join = Some(c);
                    self.tag = None;
                    self.stage = Stage::Point;
                    Ok(Step::Continue)
                }
                (Some(false), _) => {
                    self.stage = Stage::Tag;
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            },
            Stage::ConfirmNew(t) => match (yes(&i), &i) {
                (Some(true), _) | (None, Input::Enter) => {
                    self.tag = Some(t);
                    self.join = None;
                    self.stage = Stage::Point;
                    Ok(Step::Continue)
                }
                (Some(false), _) => {
                    self.stage = Stage::Device;
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            },
        }
    }
}

impl Interactive for Place {
    fn name(&self) -> &'static str {
        "PLACE"
    }
    fn prompt(&self, s: &Session) -> Prompt {
        match &self.stage {
            Stage::Symbol => {
                prompt("Enter symbol", TEXT_OR_POINT, &["List", "Rotate", "Mirror"], Some(self.default.key.into()))
            }
            Stage::DeviceSymbol(c) => {
                let t = read(s, |p, _| Ok(devices::tag_words(p.ledger.document(), *c))).unwrap_or_default();
                prompt(format!("Enter symbol to add to {t}"), Accept::TEXT, &["List"], Some(self.default.key.into()))
            }
            Stage::Point => {
                let rot = format!("rotation {}{}", self.rotation, if self.mirror { ", mirrored" } else { "" });
                let what = match self.outcome(s) {
                    Some((w, _, _)) => format!("{w}, {rot}"),
                    None => rot,
                };
                prompt(
                    format!("Specify insertion point ({what})"),
                    Accept::POINT,
                    &["Rotate", "Mirror", "Device", "Tag"],
                    None,
                )
            }
            Stage::Device => {
                let default = self
                    .default_device(s)
                    .and_then(|c| read(s, |p, _| Ok(devices::tag_words(p.ledger.document(), c))).ok())
                    .unwrap_or_else(|| "New".into());
                // The default device is a keyword too, so a click on it answers (owner's trial,
                // 2026-11-08: no Enter needed after a click).
                let keywords: Vec<&str> = if default == "New" { vec!["New"] } else { vec![default.as_str(), "New"] };
                prompt("Enter device to join", Accept::TEXT, &keywords, Some(default.clone()))
            }
            Stage::Tag => {
                let auto = self.outcome(s).map(|(_, t, _)| t).filter(|_| self.join.is_none());
                prompt("Enter tag for the new device", Accept::TEXT, &[], auto)
            }
            Stage::ConfirmJoin(c, t) => {
                let holder = read(s, |p, _| Ok(devices::holder_words(p.ledger.document(), *c))).unwrap_or_default();
                let name = self.symbol().name;
                prompt(
                    format!("{t} exists: {holder}. Add this {name} to it?"),
                    Accept::TEXT,
                    &["Yes", "Choose another"],
                    Some("Yes".into()),
                )
            }
            Stage::ConfirmNew(t) => prompt(
                format!("No device {t}. Make it a new device with that tag?"),
                Accept::TEXT,
                &["Yes", "Choose another"],
                Some("Yes".into()),
            ),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "place", |s| self.step(s, i))
    }
    /// The symbol at the cursor, as it would be placed.
    fn preview(&self, s: &Session, cursor: Vec2) -> Vec<EntityKind> {
        if !matches!(self.stage, Stage::Point | Stage::Symbol) || !super::model_space(s) || !in_range(cursor) {
            return Vec::new();
        }
        let pl = self.placement(cursor);
        self.symbol()
            .definition()
            .graphics
            .iter()
            .filter_map(|g| super::forms::placed(g, &pl))
            .map(|g| crate::convert::kind_for_primitive(&g))
            .collect()
    }
    /// Its terminal rings, tag and terminal labels at their would-be positions.
    fn preview_objects(&self, s: &Session, cursor: Vec2) -> Vec<Entity> {
        if !matches!(self.stage, Stage::Point | Stage::Symbol) {
            return Vec::new();
        }
        let Some(px) = pixel(s).filter(|_| in_range(cursor)) else { return Vec::new() };
        let pl = self.placement(cursor);
        let def = self.symbol().definition();
        let mut out: Vec<Entity> = def
            .terminals
            .iter()
            .map(|t| preview_entity(s, ring_kind(pl.apply(t.at), TERMINAL_PX, px), TERMINAL_ACI))
            .collect();
        if let Some((_, tag, labels)) = self.outcome(s) {
            let at: Vec<(TextOf, String, ecad_model::Pt)> =
                def.terminals.iter().zip(labels).map(|(t, l)| (TextOf::Tag, l, pl.apply(t.at))).collect();
            for (_, value, p, h, ha, va) in devices::texts_for(&def, &pl, Some(&tag), &at) {
                let e = devices::text_entity_with(&value, p, h, ha, va, cadcraft_engine::doc::Handle(0), "0");
                out.push(preview_entity(s, e.kind, TERMINAL_ACI));
            }
        }
        out
    }
}

// ---------------------------------------------------------------- picking a device

const NOT_A_DEVICE: &str = "That is not a device.";
const NOTHING_THERE: &str = "Nothing there. Click a device.";

/// The device an object names: any item, tag text, terminal label or marker of a component.
fn device_of(s: &Session, h: cadcraft_engine::doc::Handle) -> Result<ComponentId, String> {
    read(s, |p, page| {
        let map = &p.pages.get(&page).ok_or(NOTHING_THERE)?.map;
        let doc = p.ledger.document();
        let rep = match map.get(h) {
            Some(Mapped::Item(i)) => doc.representation_of_item(i).map(|(_, r)| r),
            Some(Mapped::Tag(r) | Mapped::Marker(r, _)) => doc.representations.get(&r),
            _ => None,
        };
        match rep.map(|r| r.subject) {
            Some(Subject::Component(c)) => Ok(c),
            _ => Err(NOT_A_DEVICE.into()),
        }
    })
}

fn picked(s: &Session, i: &Input) -> Option<Result<ComponentId, String>> {
    match i {
        Input::Point(p) => Some(match pick(s, *p) {
            Some(h) => device_of(s, h),
            None => Err(NOTHING_THERE.into()),
        }),
        Input::Pick(hs) => Some(hs.first().map_or_else(|| Err(NOTHING_THERE.into()), |h| device_of(s, *h))),
        _ => None,
    }
}

fn from_selection(s: &Session) -> Option<ComponentId> {
    let sel = s.selection();
    let first = sel.first()?;
    device_of(s, *first).ok()
}

// ---------------------------------------------------------------- Device Properties (§6.2)

/// `deviceprops`: pick a device, then change its tag, description, maker, part number, location
/// or function at the command line.
pub(crate) fn deviceprops(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Props { device: None, key: None, merge: None })))
}

struct Props {
    device: Option<ComponentId>,
    /// The property being changed.
    key: Option<&'static str>,
    /// A typed tag another device holds: (that device, the tag), waiting for Yes / Choose another.
    merge: Option<(ComponentId, String)>,
}

const KEYWORDS: [(&str, &str); 6] = [
    ("Tag", "tag"),
    ("Description", "description"),
    ("Maker", "maker"),
    ("Part", "part_number"),
    ("Location", "location"),
    ("Function", "function"),
];

impl Props {
    fn show(s: &mut Session, c: ComponentId) {
        let lines = read(s, |p, _| {
            let doc = p.ledger.document();
            let props = doc.components.get(&c).map(|x| x.properties.clone()).unwrap_or_default();
            Ok(devices::PROPERTIES
                .iter()
                .map(|(k, label)| format!("{label}: {}", props.get(*k).map_or("?", String::as_str)))
                .collect::<Vec<_>>())
        })
        .unwrap_or_default();
        for l in lines {
            s.echo(l);
        }
    }

    fn select(&mut self, s: &mut Session, c: ComponentId) {
        self.device = Some(c);
        Self::show(s, c);
    }

    fn set(&mut self, s: &mut Session, c: ComponentId, key: &str, value: &str, merge: bool) -> R<Step> {
        let before = read(s, |p, _| Ok(devices::tag_words(p.ledger.document(), c))).unwrap_or_default();
        let mut params = json!({"component": c.hex(), "key": key, "value": value});
        if merge {
            params["merge"] = Value::Bool(true);
        }
        let r = commit(s, "setprop", &params)?;
        if let Some(m) = r.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        if let Some(into) = r.get("component").and_then(Value::as_str).and_then(|h| u128::from_str_radix(h, 16).ok()) {
            self.device = Some(ComponentId::from(into));
        }
        let device = self.device.unwrap_or(c);
        let words = read(s, |p, _| {
            let doc = p.ledger.document();
            let n = doc.representations_of(Subject::Component(device)).count();
            Ok((devices::tag_words(doc, device), n, devices::pages_words(doc, device)))
        });
        if !merge && let Ok((tag, n, pages)) = words {
            if key == "tag" {
                s.echo(format!("{before} is now {tag} ({n} symbol{} on {pages}).", if n == 1 { "" } else { "s" }));
            } else {
                let label = devices::PROPERTIES.iter().find(|(k, _)| *k == key).map_or(key, |(_, l)| *l);
                let shown = r.get("value").and_then(Value::as_str).unwrap_or("");
                s.echo(format!("{label} of {tag} set to \"{shown}\"."));
            }
        }
        self.key = None;
        self.merge = None;
        Ok(Step::Continue)
    }

    fn step(&mut self, s: &mut Session, i: Input) -> R<Step> {
        let Some(c) = self.device else {
            return match picked(s, &i) {
                Some(Ok(c)) => {
                    self.select(s, c);
                    Ok(Step::Continue)
                }
                Some(Err(m)) => {
                    s.echo(m);
                    Ok(Step::Continue)
                }
                None if i == Input::Enter => Ok(Step::Done),
                None => Ok(Step::Continue),
            };
        };
        if let Some((holder, t)) = self.merge.clone() {
            return match (yes(&i), &i) {
                (Some(true), _) => self.set(s, c, "tag", &t, true),
                (Some(false), _) | (None, Input::Enter) => {
                    self.merge = None;
                    Ok(Step::Continue)
                }
                _ => {
                    let _ = holder;
                    Ok(Step::Continue)
                }
            };
        }
        match self.key {
            None => match i {
                Input::Keyword(k) => {
                    self.key = KEYWORDS.iter().find(|(kw, _)| *kw == k).map(|(_, key)| *key);
                    Ok(Step::Continue)
                }
                Input::Enter => Ok(Step::Done),
                _ => Ok(Step::Continue),
            },
            Some(key) => match i {
                Input::Enter => {
                    self.key = None;
                    Ok(Step::Continue)
                }
                Input::Text(t) | Input::Keyword(t) => {
                    if key == "tag" {
                        let holder = read(s, |p, _| {
                            let doc = p.ledger.document();
                            let normal = ecad_model::tag::normalise(&t, devices::padding(doc))?;
                            Ok(devices::holder_of(doc, &normal, Some(c)).map(|h| (h, normal)))
                        })
                        .map_err(other)?;
                        if let Some(found) = holder {
                            self.merge = Some(found);
                            return Ok(Step::Continue);
                        }
                    }
                    self.set(s, c, key, &t, false)
                }
                _ => Ok(Step::Continue),
            },
        }
    }
}

impl Interactive for Props {
    fn name(&self) -> &'static str {
        "DEVICEPROPS"
    }
    fn begin(&mut self, s: &mut Session) -> R<Step> {
        safely(s, "deviceprops", |s| {
            if let Some(c) = from_selection(s) {
                self.select(s, c);
            }
            Ok(Step::Continue)
        })
    }
    fn prompt(&self, s: &Session) -> Prompt {
        let Some(c) = self.device else { return Prompt::new("Select a device", Accept::SELECT) };
        if let Some((holder, t)) = &self.merge {
            let words = read(s, |p, _| Ok(devices::holder_words(p.ledger.document(), *holder))).unwrap_or_default();
            return prompt(
                format!("{t} exists: {words}. Make this the same device?"),
                Accept::TEXT,
                &["Yes", "Choose another"],
                Some("Choose another".into()),
            );
        }
        match self.key {
            None => prompt(
                "Enter property to change",
                Accept::TEXT,
                &KEYWORDS.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                None,
            ),
            Some(key) => {
                let current = read(s, |p, _| {
                    Ok(p.ledger.document().components.get(&c).and_then(|x| x.properties.get(key).cloned()))
                })
                .ok()
                .flatten();
                let label = devices::PROPERTIES.iter().find(|(k, _)| *k == key).map_or(key, |(_, l)| *l);
                prompt(format!("New {}", label.to_lowercase()), Accept::TEXT, &[], current)
            }
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "deviceprops", |s| self.step(s, i))
    }
}

// ---------------------------------------------------------------- Delete Device (§6.3)

/// `devicedelete`: pick a device, confirm, and it goes with every symbol on every page.
pub(crate) fn devicedelete(s: &Session) -> R<Box<dyn Interactive>> {
    can_start(s)?;
    Ok(Box::new(Safe(Delete { device: None })))
}

struct Delete {
    device: Option<ComponentId>,
}

impl Interactive for Delete {
    fn name(&self) -> &'static str {
        "DEVICEDELETE"
    }
    fn begin(&mut self, s: &mut Session) -> R<Step> {
        self.device = from_selection(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, s: &Session) -> Prompt {
        let Some(c) = self.device else { return Prompt::new("Select a device", Accept::SELECT) };
        let (tag, n, pages) = read(s, |p, _| {
            let doc = p.ledger.document();
            Ok((
                devices::tag_words(doc, c),
                doc.representations_of(Subject::Component(c)).count(),
                devices::pages_words(doc, c),
            ))
        })
        .unwrap_or_default();
        prompt(
            format!("Delete {tag} and its {n} symbol{} on {pages}?", if n == 1 { "" } else { "s" }),
            Accept::TEXT,
            &["Yes", "No"],
            Some("No".into()),
        )
    }
    fn input(&mut self, s: &mut Session, i: Input) -> R<Step> {
        paper_guard(s, &i)?;
        safely(s, "devicedelete", |s| {
            let Some(c) = self.device else {
                return match picked(s, &i) {
                    Some(Ok(c)) => {
                        self.device = Some(c);
                        Ok(Step::Continue)
                    }
                    Some(Err(m)) => {
                        s.echo(m);
                        Ok(Step::Continue)
                    }
                    None if i == Input::Enter => Ok(Step::Done),
                    None => Ok(Step::Continue),
                };
            };
            if yes(&i) == Some(true) {
                let r = commit(s, "devicedelete", &json!({"component": c.hex()}))?;
                s.echo(r.get("message").and_then(Value::as_str).unwrap_or("Deleted.").to_string());
            } else {
                s.echo("Nothing deleted.");
            }
            Ok(Step::Done)
        })
    }
}
