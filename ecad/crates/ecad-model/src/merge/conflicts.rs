//! Finding conflicts: per object and per property, the three-way pick.

use std::collections::BTreeSet;

use crate::naming::id::{DeviceId, FolderId};
use crate::naming::tag::{self};
use crate::objects::route::Route;
use crate::objects::styles::{Standard, StyleCatalogues, StyleLayer, StyleRecord, StyleRef};
use crate::{Cable, ConnectionPoint, Core, CorePosition, Device, Document, Folder, Item, Page, Properties, Representation, Subject, Wire};

use super::state::*;

/// Merge property maps: per key.
pub(super) fn merge_props(
    ctx: &mut Ctx<'_, '_>, obj: Obj, what: &str, a: &Properties, t: &Properties, s: &Properties, keep: &dyn Fn(&str) -> bool,
) -> Properties {
    let mut out = t.clone();
    let keys: BTreeSet<&String> = a.keys().chain(t.keys()).chain(s.keys()).collect();
    for k in keys {
        if !keep(k) {
            continue;
        }
        let v = match pick(a.get(k), t.get(k), s.get(k)) {
            Some(v) => v,
            None => {
                let c = Conflict {
                    id: format!("{}/prop.{k}", obj.hex()),
                    kind: ConflictKind::Value,
                    object: obj,
                    what: format!("{what} · {}", key_words(k)),
                    // A value naming a component shows its tag.
                    target: shown(t.get(k).map(|v| value_words(&[ctx.inp.target, ctx.inp.source, ctx.inp.ancestor], v)).as_ref()),
                    source: shown(s.get(k).map(|v| value_words(&[ctx.inp.source, ctx.inp.target, ctx.inp.ancestor], v)).as_ref()),
                    choices: vec![Choice::Target, Choice::Source, Choice::Value(String::new())],
                    chosen: None,
                };
                match ctx.ask(c) {
                    Some(Choice::Source) => s.get(k).cloned(),
                    Some(Choice::Value(x)) => match (ctx.inp.edit_rule)(obj, k, &x) {
                        Ok(v) => Some(v),
                        Err(e) => {
                            ctx.problems.push(format!("{what} · {}: {e}", key_words(k)));
                            t.get(k).cloned()
                        }
                    },
                    _ => t.get(k).cloned(),
                }
            }
        };
        match v {
            Some(v) => out.insert(k.clone(), v),
            None => out.remove(k),
        };
    }
    out
}

/// Merge one field as a whole value.
#[allow(clippy::too_many_arguments)]
/// A tag slot as shown in a conflict: the tag as drawn, or `(none)`.
pub(super) fn tag_text(v: &tag::TagSlot) -> String {
    v.shown().unwrap_or_else(|| "(none)".into())
}

/// A location or function as shown in a conflict: its short id, with `(proposed)` when it was.
pub(super) fn place_text<I: Copy>(v: &crate::document::book::Place<I>, short: fn(I) -> String) -> String {
    match v {
        crate::document::book::Place::Unset => "(none)".to_string(),
        crate::document::book::Place::Manual(i) => short(*i),
        crate::document::book::Place::Proposed(i) => format!("{} (proposed)", short(*i)),
    }
}

pub(super) fn field<T: PartialEq + Clone>(
    ctx: &mut Ctx<'_, '_>, obj: Obj, name: &str, what: String, a: &T, t: &T, s: &T, show: &dyn Fn(&T) -> String,
) -> T {
    match pick(Some(a), Some(t), Some(s)) {
        Some(Some(v)) => v,
        _ => {
            let c = Conflict {
                id: format!("{}/{name}", obj.hex()),
                kind: ConflictKind::Value,
                object: obj,
                what,
                target: show(t),
                source: show(s),
                choices: vec![Choice::Target, Choice::Source],
                chosen: None,
            };
            match ctx.ask(c) {
                Some(Choice::Source) => s.clone(),
                _ => t.clone(),
            }
        }
    }
}

/// Merge a device's type: a PLC station's and module's fields one by one when all three keep the
/// same group (a station is shown by its tag); otherwise the type whole.
pub(super) fn merge_device_type(
    ctx: &mut Ctx<'_, '_>, obj: Obj, what: &str, docs: &[&Document], a: &crate::DeviceType, t: &crate::DeviceType, s: &crate::DeviceType,
) -> crate::DeviceType {
    use crate::DeviceType as T;
    let words = |v: &dyn Fn() -> Option<String>| v().unwrap_or_else(|| "(none)".into());
    match (a, t, s) {
        (T::PlcModule(a), T::PlcModule(t), T::PlcModule(s)) => {
            let rack_name = |r: &Option<DeviceId>| r.map_or_else(|| "no rack".into(), |r| name_of(docs, Obj::Device(r)));
            T::PlcModule(crate::objects::plc::PlcModule {
                rack: field(ctx, obj, "plc.rack", format!("{what} · rack"), &a.rack, &t.rack, &s.rack, &rack_name),
                slot: field(ctx, obj, "plc.slot", format!("{what} · slot"), &a.slot, &t.slot, &s.slot, &|x| words(&|| x.map(|n| n.to_string()))),
                function: field(ctx, obj, "plc.function", format!("{what} · function and channels"), &a.function, &t.function, &s.function, &|f| {
                    f.word().replace('_', " ")
                }),
                attributes: field(ctx, obj, "plc.attributes", format!("{what} · attributes"), &a.attributes, &t.attributes, &s.attributes, &|x| {
                    format!("{x:?}")
                }),
                address: field(ctx, obj, "plc.addr", format!("{what} · pinned address"), &a.address, &t.address, &s.address, &|x| {
                    words(&|| x.start.map(|n| n.to_string()))
                }),
                supply: field(ctx, obj, "plc.supply", format!("{what} · supply"), &a.supply, &t.supply, &s.supply, &|x| {
                    words(&|| x.map(|v| v.to_string()))
                }),
            })
        }
        (T::PlcRack(a), T::PlcRack(t), T::PlcRack(s)) => T::PlcRack(crate::objects::plc::PlcRack {
            network: field(ctx, obj, "plc.network", format!("{what} · network"), &a.network, &t.network, &s.network, &|x| {
                words(&|| x.as_ref().map(ToString::to_string))
            }),
            address: field(ctx, obj, "plc.network.address", format!("{what} · network address"), &a.address, &t.address, &s.address, &|x| {
                words(&|| x.map(|v| v.to_string()))
            }),
            head: field(ctx, obj, "plc.head", format!("{what} · head"), &a.head, &t.head, &s.head, &|h| format!("{h:?}")),
            address_style: field(
                ctx,
                obj,
                "plc.address.style",
                format!("{what} · address style"),
                &a.address_style,
                &t.address_style,
                &s.address_style,
                &|x| words(&|| x.map(|v| crate::Word::word(v).to_string())),
            ),
        }),
        _ => field(ctx, obj, "device_type", format!("{what} · type"), a, t, s, &|d| d.group().label().into()),
    }
}

/// Merge one object: whole when it resolves, else by `fields` when both sides kept it, else a
/// removed conflict.
pub(super) fn object<T: PartialEq + Clone>(
    ctx: &mut Ctx<'_, '_>, obj: Obj, what: &str, a: Option<&T>, t: Option<&T>, s: Option<&T>, fields: impl FnOnce(&mut Ctx<'_, '_>, &T, &T, &T) -> T,
) -> Option<T> {
    if let Some(v) = pick(a, t, s) {
        return v;
    }
    match (t, s) {
        (Some(tv), Some(sv)) => Some(fields(ctx, a.unwrap_or(tv), tv, sv)),
        (None, Some(sv)) => {
            let c = Conflict {
                id: format!("{}/removed", obj.hex()),
                kind: ConflictKind::Removed,
                object: obj,
                what: format!("{what} · removed on {}; changed on {}", ctx.t(), ctx.s()),
                target: "removed".into(),
                source: "changed".into(),
                choices: vec![Choice::Target, Choice::Source],
                chosen: None,
            };
            match ctx.ask(c) {
                Some(Choice::Source) => Some(sv.clone()),
                _ => None,
            }
        }
        (Some(tv), None) => {
            let c = Conflict {
                id: format!("{}/removed", obj.hex()),
                kind: ConflictKind::Removed,
                object: obj,
                what: format!("{what} · changed on {}; removed on {}", ctx.t(), ctx.s()),
                target: "changed".into(),
                source: "removed".into(),
                choices: vec![Choice::Target, Choice::Source],
                chosen: None,
            };
            match ctx.ask(c) {
                Some(Choice::Source) => None,
                _ => Some(tv.clone()),
            }
        }
        (None, None) => None,
    }
}

/// A device's text fields as a bag keyed by their `setprop` keys, so they merge as its properties
/// do (one conflict per key, `<id>/prop.<key>`, with an edited value offered).
pub(super) fn text_bag(d: &Device) -> Properties {
    crate::TextField::ALL.into_iter().filter_map(|f| d.text(f).map(|v| (f.key().to_string(), v.to_string()))).collect()
}

/// `d` with the text fields of the merged bag `texts`; a value the field can't hold keeps the
/// target's and is a problem.
pub(super) fn with_texts(ctx: &mut Ctx<'_, '_>, what: &str, mut d: Device, t: &Device, texts: &Properties) -> Device {
    for f in crate::TextField::ALL {
        if let Err(e) = d.set_text(f, texts.get(f.key()).map(String::as_str)) {
            ctx.problems.push(format!("{what} · {}: {e}", key_words(f.key())));
            let _ = d.set_text(f, t.text(f));
        }
    }
    d
}

/// Merge every object of the scope into `m` (a copy of the target).
pub(super) fn merge_objects(ctx: &mut Ctx<'_, '_>, m: &mut Document, objects: &[Obj]) {
    let (a, t, s) = (ctx.inp.ancestor, ctx.inp.target, ctx.inp.source);
    let docs = [t, s, a];
    let all = |_: &str| true;
    for &obj in objects {
        let what = name_of(&docs, obj);
        match obj {
            Obj::Device(id) => {
                let v = object(ctx, obj, &what, a.devices.get(&id), t.devices.get(&id), s.devices.get(&id), |ctx, a, t, s| {
                    let texts = merge_props(ctx, obj, &what, &text_bag(a), &text_bag(t), &text_bag(s), &all);
                    let d = Device {
                        tag: field(ctx, obj, tag::TAG_KEY, format!("{what} · tag"), &a.tag, &t.tag, &s.tag, &|v| tag_text(v)),
                        location: field(
                            ctx,
                            obj,
                            crate::document::book::LOCATION_KEY,
                            format!("{what} · location"),
                            &a.location,
                            &t.location,
                            &s.location,
                            &|x| place_text(x, crate::naming::id::LocationId::short),
                        ),
                        function: field(
                            ctx,
                            obj,
                            crate::document::book::FUNCTION_KEY,
                            format!("{what} · function"),
                            &a.function,
                            &t.function,
                            &s.function,
                            &|x| place_text(x, crate::naming::id::FunctionId::short),
                        ),
                        properties: merge_props(ctx, obj, &what, &a.properties, &t.properties, &s.properties, &all),
                        definition: field(ctx, obj, "definition", format!("{what} · symbol"), &a.definition, &t.definition, &s.definition, &|d| {
                            d.map_or_else(|| "(none)".into(), |d| name_of(&docs, Obj::Definition(d)))
                        }),
                        device_type: merge_device_type(ctx, obj, &what, &docs, &a.device_type, &t.device_type, &s.device_type),
                        ..Device::default()
                    };
                    with_texts(ctx, &what, d, t, &texts)
                });
                set(&mut m.devices, id, v);
            }
            Obj::Wire(id) => {
                let v = object(ctx, obj, &what, a.wires.get(&id), t.wires.get(&id), s.wires.get(&id), |ctx, a, t, s| {
                    let opt = |x: Option<String>| x.unwrap_or_else(|| "(none)".into());
                    Wire {
                        net: field(ctx, obj, "net", format!("{what} · net"), &a.net, &t.net, &s.net, &|x| x.to_string()),
                        conductor: t.conductor,
                        net_order: field(
                            ctx,
                            obj,
                            "net_order",
                            format!("{what} · place in its net"),
                            &a.net_order,
                            &t.net_order,
                            &s.net_order,
                            &|x| x.to_string(),
                        ),
                        ends: field(ctx, obj, "ends", format!("{what} · connection points"), &a.ends, &t.ends, &s.ends, &|e| {
                            format!("{} – {}", name_of(&docs, Obj::ConnectionPoint(e[0])), name_of(&docs, Obj::ConnectionPoint(e[1])))
                        }),
                        order_fixed: field(
                            ctx,
                            obj,
                            "order_fixed",
                            format!("{what} · order fixed"),
                            &a.order_fixed,
                            &t.order_fixed,
                            &s.order_fixed,
                            &|x| {
                                if *x { "yes".into() } else { "no".into() }
                            },
                        ),
                        wire_type: field(ctx, obj, "wire_type", format!("{what} · wire type"), &a.wire_type, &t.wire_type, &s.wire_type, &|x| {
                            opt(x.as_ref().map(StyleRef::text))
                        }),
                        size: field(ctx, obj, "size", format!("{what} · size"), &a.size, &t.size, &s.size, &|x| opt(x.map(|v| v.to_string()))),
                        colours: field(ctx, obj, "colours", format!("{what} · colours"), &a.colours, &t.colours, &s.colours, &|x| {
                            opt(x.as_ref().map(|v| v.to_string()))
                        }),
                        number: field(ctx, obj, "number", format!("{what} · wire number"), &a.number, &t.number, &s.number, &|x| {
                            opt(x.as_ref().map(|n| if n.fixed { format!("{} (fixed)", n.text) } else { n.text.to_string() }))
                        }),
                        potential: field(ctx, obj, "potential", format!("{what} · potential"), &a.potential, &t.potential, &s.potential, &|x| {
                            opt(x.as_ref().map(|v| v.to_string()))
                        }),
                        rating: field(ctx, obj, "rating", format!("{what} · rating inputs"), &a.rating, &t.rating, &s.rating, &|x| format!("{x:?}")),
                        load: field(ctx, obj, "load", format!("{what} · load"), &a.load, &t.load, &s.load, &|x| opt(x.map(|v| v.to_string()))),
                        protected_by: field(
                            ctx,
                            obj,
                            "protected_by",
                            format!("{what} · protected by"),
                            &a.protected_by,
                            &t.protected_by,
                            &s.protected_by,
                            &|x| opt(x.map(|p| name_of(&docs, Obj::Device(p.device)))),
                        ),
                        display: merge_props(ctx, obj, &what, &a.display, &t.display, &s.display, &all),
                    }
                });
                set(&mut m.wires, id, v);
            }
            Obj::Cable(id) => {
                let v = object(ctx, obj, &what, a.cables.get(&id), t.cables.get(&id), s.cables.get(&id), |ctx, a, t, s| Cable {
                    tag: field(ctx, obj, tag::TAG_KEY, format!("{what} · tag"), &a.tag, &t.tag, &s.tag, &|v| tag_text(v)),
                    description: field(
                        ctx,
                        obj,
                        crate::objects::devices::DESCRIPTION_KEY,
                        format!("{what} · description"),
                        &a.description,
                        &t.description,
                        &s.description,
                        &|v| v.as_ref().map_or_else(|| "(none)".into(), |t| t.to_string()),
                    ),
                    properties: merge_props(ctx, obj, &what, &a.properties, &t.properties, &s.properties, &all),
                    cores: merge_members(&a.cores, &t.cores, &s.cores),
                    cable_type: field(ctx, obj, "cable_type", format!("{what} · cable type"), &a.cable_type, &t.cable_type, &s.cable_type, &|x| {
                        x.as_ref().map_or_else(|| "(none)".to_string(), StyleRef::text)
                    }),
                    shield: field(ctx, obj, "shield", format!("{what} · shield"), &a.shield, &t.shield, &s.shield, &|x| {
                        x.map_or_else(|| "(none)".to_string(), |v| crate::naming::words::Word::word(v).to_string())
                    }),
                    rating: field(ctx, obj, "rating", format!("{what} · rating inputs"), &a.rating, &t.rating, &s.rating, &|x| format!("{x:?}")),
                    load: field(ctx, obj, "load", format!("{what} · load"), &a.load, &t.load, &s.load, &|x| {
                        x.map_or_else(|| "(none)".to_string(), |v| v.to_string())
                    }),
                    protected_by: field(
                        ctx,
                        obj,
                        "protected_by",
                        format!("{what} · protected by"),
                        &a.protected_by,
                        &t.protected_by,
                        &s.protected_by,
                        &|x| x.map_or_else(|| "(none)".to_string(), |p| name_of(&docs, Obj::Device(p.device))),
                    ),
                });
                set(&mut m.cables, id, v);
            }
            Obj::Aid(id) => {
                let v = object(ctx, obj, &what, a.aids.get(&id), t.aids.get(&id), s.aids.get(&id), |ctx, a, t, s| {
                    field(ctx, obj, "aid", format!("{what} · changed on both"), a, t, s, &|x| format!("{x:?}"))
                });
                set(&mut m.aids, id, v);
            }
            Obj::LoosePart(id) => {
                let v = object(ctx, obj, &what, a.loose_parts.get(&id), t.loose_parts.get(&id), s.loose_parts.get(&id), |ctx, a, t, s| {
                    field(ctx, obj, "loose_part", format!("{what} · changed on both"), a, t, s, &|x| format!("{x:?}"))
                });
                set(&mut m.loose_parts, id, v);
            }
            Obj::Route(id) => {
                let v = object(ctx, obj, &what, a.routes.get(&id), t.routes.get(&id), s.routes.get(&id), |ctx, a, t, s| Route {
                    page: field(ctx, obj, "page", format!("{what} · page"), &a.page, &t.page, &s.page, &|p| name_of(&docs, Obj::Page(*p))),
                    points: field(ctx, obj, "points", format!("{what} · drawn differently on both"), &a.points, &t.points, &s.points, &|p| {
                        format!("{} points", p.len())
                    }),
                    ends: field(ctx, obj, "ends", format!("{what} · ends"), &a.ends, &t.ends, &s.ends, &|e| format!("{e:?}")),
                });
                set(&mut m.routes, id, v);
            }
            Obj::Page(id) => {
                let v = object(ctx, obj, &what, a.pages.get(&id), t.pages.get(&id), s.pages.get(&id), |ctx, a, t, s| Page {
                    title: field(ctx, obj, "title", format!("{what} · title"), &a.title, &t.title, &s.title, &|x| format!("\"{x}\"")),
                    number: field(ctx, obj, "number", format!("{what} · number"), &a.number, &t.number, &s.number, &|x| shown_value(x.as_ref())),
                    folder: field(ctx, obj, "folder", format!("{what} · folder"), &a.folder, &t.folder, &s.folder, &|x| {
                        x.map_or_else(|| "(top level)".to_string(), FolderId::short)
                    }),
                    kind: field(ctx, obj, "kind", format!("{what} · kind"), &a.kind, &t.kind, &s.kind, &|x| {
                        x.generated_kind().map_or("schematic", crate::Word::word).to_string()
                    }),
                    page_type: field(ctx, obj, "page_type", format!("{what} · page type"), &a.page_type, &t.page_type, &s.page_type, &|x| {
                        shown_value(x.as_ref())
                    }),
                    sheet: field(ctx, obj, "sheet", format!("{what} · sheet"), &a.sheet, &t.sheet, &s.sheet, &|x| {
                        x.map_or_else(|| "(project's)".to_string(), |s| s.text())
                    }),
                    title_block: field(
                        ctx,
                        obj,
                        "title_block",
                        format!("{what} · title block"),
                        &a.title_block,
                        &t.title_block,
                        &s.title_block,
                        &|x| shown_value(x.as_ref()),
                    ),
                    location: field(ctx, obj, "location", format!("{what} · location"), &a.location, &t.location, &s.location, &|x| {
                        x.map_or_else(|| "(project's)".to_string(), crate::naming::id::LocationId::short)
                    }),
                    function: field(ctx, obj, "function", format!("{what} · function"), &a.function, &t.function, &s.function, &|x| {
                        x.map_or_else(|| "(project's)".to_string(), crate::naming::id::FunctionId::short)
                    }),
                    display: merge_props(ctx, obj, &what, &a.display, &t.display, &s.display, &all),
                });
                set(&mut m.pages, id, v);
            }
            Obj::Folder(id) => {
                let v = object(ctx, obj, &what, a.folders.get(&id), t.folders.get(&id), s.folders.get(&id), |ctx, a, t, s| Folder {
                    name: field(ctx, obj, "name", format!("{what} · name"), &a.name, &t.name, &s.name, &|x| format!("\"{x}\"")),
                    parent: field(ctx, obj, "parent", format!("{what} · placed in"), &a.parent, &t.parent, &s.parent, &|x| {
                        x.map_or_else(|| "the top level".into(), |f| name_of(&docs, Obj::Folder(f)))
                    }),
                    start: field(ctx, obj, "start", format!("{what} · start number"), &a.start, &t.start, &s.start, &|x| shown_value(x.as_ref())),
                });
                set(&mut m.folders, id, v);
            }
            Obj::Representation(id) => {
                let v =
                    object(ctx, obj, &what, a.representations.get(&id), t.representations.get(&id), s.representations.get(&id), |ctx, a, t, s| {
                        let show = |r: &Representation| format!("on {}", name_of(&docs, Obj::Page(r.page)).to_lowercase());
                        field(ctx, obj, "placement", format!("{what} · placed differently on both"), a, t, s, &show)
                    });
                set(&mut m.representations, id, v);
            }
            Obj::Item(id) => {
                let v = object(ctx, obj, &what, a.items.get(&id), t.items.get(&id), s.items.get(&id), |ctx, a, t, s| {
                    field(ctx, obj, "item", format!("{what} · changed on both"), a, t, s, &|_: &Item| "its drawing".into())
                });
                set(&mut m.items, id, v);
            }
            Obj::ConnectionPoint(id) => {
                let v = object(
                    ctx,
                    obj,
                    &what,
                    a.connection_points.get(&id),
                    t.connection_points.get(&id),
                    s.connection_points.get(&id),
                    |ctx, a, t, s| {
                        let show = |x: &ConnectionPoint| {
                            x.label.clone().or_else(|| x.key.as_ref().map(ToString::to_string)).unwrap_or_else(|| "(no label)".into())
                        };
                        field(ctx, obj, "record", format!("{what} · changed on both"), a, t, s, &show)
                    },
                );
                set(&mut m.connection_points, id, v);
            }
            Obj::Layer(id) => {
                let v = object(ctx, obj, &what, a.layers.get(&id), t.layers.get(&id), s.layers.get(&id), |ctx, a, t, s| {
                    field(ctx, obj, "layer", format!("{what} · changed on both"), a, t, s, &|l| format!("\"{}\"", l.name))
                });
                set(&mut m.layers, id, v);
            }
            Obj::Definition(id) => {
                let v = object(ctx, obj, &what, a.definitions.get(&id), t.definitions.get(&id), s.definitions.get(&id), |ctx, a, t, s| {
                    field(ctx, obj, "definition", format!("{what} · changed on both"), a, t, s, &|d| format!("\"{}\"", d.name))
                });
                set(&mut m.definitions, id, v);
            }
            Obj::Project => {
                m.display = merge_props(ctx, obj, "Project", &a.display, &t.display, &s.display, ctx.inp.keep_project_key);
                let clash = revision_clash(ctx, obj);
                m.project = merge_project(ctx, obj, &a.project, &t.project, &s.project, clash);
                m.device_links =
                    field(ctx, obj, "device_links", "Project · device links".into(), &a.device_links, &t.device_links, &s.device_links, &|x| {
                        format!("{} device links", x.len())
                    });
                m.settings =
                    field(ctx, obj, "settings", "Project settings · changed on both".into(), &a.settings, &t.settings, &s.settings, &|_| {
                        "its settings".into()
                    });
                merge_standard(ctx, m, obj);
            }
        }
    }
}

/// The revisions both sides issued since the ancestor under one name: the same rev with a
/// different `at`. Asked as one conflict that names them, never picked; `Some(true)` when the
/// answer takes the source's revision rows, `Some(false)` for the target's, `None` when there is
/// no clash. Version names are branch-local history and never come across a merge, so two
/// branches may each have a version of one name; a revision is project data and does.
pub(super) fn revision_clash(ctx: &mut Ctx<'_, '_>, obj: Obj) -> Option<bool> {
    let (a, t, s) = (ctx.inp.ancestor, ctx.inp.target, ctx.inp.source);
    let new =
        |d: &Document, k: &crate::PointName| matches!(k, crate::PointName::Revision(_)) && d.versions.contains_key(k) && !a.versions.contains_key(k);
    let clashes: Vec<crate::Rev> =
        t.versions.iter().filter(|(k, at)| new(t, k) && new(s, k) && s.versions.get(*k) != Some(*at)).filter_map(|(k, _)| k.rev()).collect();
    if clashes.is_empty() {
        return None;
    }
    let scheme = &t.project.rev_scheme;
    let names = clashes.iter().map(|r| scheme.show(*r)).collect::<Vec<_>>().join(", ");
    let of = |d: &Document| {
        let at: Vec<String> = clashes.iter().filter_map(|r| d.versions.get(&crate::PointName::Revision(*r))).map(|n| format!("step {n}")).collect();
        format!("issued at {}", at.join(", "))
    };
    let c = Conflict {
        id: format!("{}/revision.issued", obj.hex()),
        kind: ConflictKind::Value,
        object: obj,
        what: format!("Revision {names} · issued on both branches"),
        target: of(t),
        source: of(s),
        choices: vec![Choice::Target, Choice::Source],
        chosen: None,
    };
    Some(matches!(ctx.ask(c), Some(Choice::Source)))
}

/// The project's own data: each list merges entry by entry (an entry changed on
/// both sides differently is a conflict), the defaults field by field, the revision rows whole.
pub(super) fn merge_project(
    ctx: &mut Ctx<'_, '_>, obj: Obj, a: &crate::Project, t: &crate::Project, s: &crate::Project, clash: Option<bool>,
) -> crate::Project {
    // The info merges field by field, like the project properties it was.
    let mut info = t.info.clone();
    for f in <crate::document::project::InfoField as crate::Word>::ALL {
        let get = |p: &crate::Project| p.info.get(*f).map(str::to_string);
        let what = format!("Project · {}", crate::Word::word(*f));
        let v = field(ctx, obj, crate::Word::word(*f), what, &get(a), &get(t), &get(s), &|x| shown_value(x.as_ref()));
        info = info.with(*f, v.as_deref()).unwrap_or(info);
    }
    crate::Project {
        info,
        locations: merge_list(ctx, obj, "location", &a.locations, &t.locations, &s.locations),
        functions: merge_list(ctx, obj, "function", &a.functions, &t.functions, &s.functions),
        location: field(ctx, obj, "location", "Project · location".into(), &a.location, &t.location, &s.location, &|x| {
            x.map_or_else(|| "(none)".to_string(), crate::naming::id::LocationId::short)
        }),
        function: field(ctx, obj, "function", "Project · function".into(), &a.function, &t.function, &s.function, &|x| {
            x.map_or_else(|| "(none)".to_string(), crate::naming::id::FunctionId::short)
        }),
        rev_scheme: field(ctx, obj, "rev_scheme", "Project · revision scheme".into(), &a.rev_scheme, &t.rev_scheme, &s.rev_scheme, &|x| x.example()),
        // Revision rows are merged only where the filter allows (never into the release line; an
        // update brings them unchanged), as one unit.
        user_fields: field(ctx, obj, "user_fields", "Project · user fields".into(), &a.user_fields, &t.user_fields, &s.user_fields, &|x| {
            format!("{} user fields", x.len())
        }),
        revisions: if let Some(take_source) = clash {
            if take_source { s.revisions.clone() } else { t.revisions.clone() }
        } else if (ctx.inp.keep_project_key)(crate::document::project::REVISIONS_KEY) {
            field(ctx, obj, "revisions", "Project · revisions".into(), &a.revisions, &t.revisions, &s.revisions, &|x| {
                format!("{} revisions", x.len())
            })
        } else {
            t.revisions.clone()
        },
    }
}

/// One list of the project, merged by entry id: the target's order, then the source's new entries.
pub(super) fn merge_list<I: Copy + Ord + std::fmt::Display>(
    ctx: &mut Ctx<'_, '_>, obj: Obj, what: &str, a: &[crate::ListEntry<I>], t: &[crate::ListEntry<I>], s: &[crate::ListEntry<I>],
) -> Vec<crate::ListEntry<I>> {
    let find = |l: &[crate::ListEntry<I>], id: I| l.iter().find(|e| e.id == id).cloned();
    let mut ids: Vec<I> = t.iter().map(|e| e.id).collect();
    ids.extend(s.iter().map(|e| e.id).filter(|id| !t.iter().any(|e| e.id == *id)));
    ids.extend(a.iter().map(|e| e.id).filter(|id| !t.iter().chain(s).any(|e| e.id == *id)));
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let v = field(ctx, obj, &format!("{what}.{id}"), format!("Project · {what} {id}"), &find(a, id), &find(t, id), &find(s, id), &|x| {
            x.as_ref().map_or_else(|| "(removed)".to_string(), |e| format!("\"{}\"", e.code))
        });
        out.extend(v);
    }
    out
}

/// The standard and the style records: the standard's header is
/// one unit, its values merge per key like project properties, and each style record
/// `(layer, catalogue, id)` is one unit. A standard removed on one side and changed in any part
/// on the other is one removed conflict; removed on one side and untouched on the other, it goes.
pub(super) fn merge_standard(ctx: &mut Ctx<'_, '_>, m: &mut Document, obj: Obj) {
    let (a, t, s) = (ctx.inp.ancestor, ctx.inp.target, ctx.inp.source);
    // Risk 20: a standard removed on one side and changed on the other (its header, a value or a
    // standard-layer style) is one conflict, keep it with those changes or remove it; merged per
    // part, the header alone would decide and the other side's changes would vanish.
    let removed = |d: &Document| a.standard.is_some() && d.standard.is_none();
    let changed = |d: &Document| d.standard.is_some() && d.standard != a.standard;
    let one_sided = match (removed(t), removed(s)) {
        (true, false) if changed(s) => Some(false),
        (false, true) if changed(t) => Some(true),
        _ => None,
    };
    if let Some(target_keeps) = one_sided {
        let name = a.standard.as_ref().map_or_else(String::new, |x| x.header.name.to_string());
        let (on_changed, on_removed) = if target_keeps { (ctx.t(), ctx.s()) } else { (ctx.s(), ctx.t()) };
        let word = |keeps: bool| if keeps { "changed" } else { "removed" };
        let c = Conflict {
            id: format!("{}/standard", obj.hex()),
            kind: ConflictKind::Removed,
            object: obj,
            what: format!("Standard \"{name}\" · removed on {on_removed}; changed on {on_changed}"),
            target: word(target_keeps).into(),
            source: word(!target_keeps).into(),
            choices: vec![Choice::Target, Choice::Source],
            chosen: None,
        };
        let keep = match ctx.ask(c) {
            Some(Choice::Source) => !target_keeps,
            _ => target_keeps,
        };
        let keeper = if target_keeps { t } else { s };
        m.styles = merge_style_layer(ctx, StyleLayer::Project, &a.styles, &t.styles, &s.styles);
        m.standard = if keep { keeper.standard.clone() } else { None };
        return;
    }
    let all = |_: &str| true;
    let header = |d: &Document| d.standard.as_ref().map(|x| x.header.clone());
    let head = field(ctx, obj, "standard", "Standard · changed on both".into(), &header(a), &header(t), &header(s), &|h| {
        h.as_ref().map_or_else(|| "IEC (built in)".into(), |h| format!("\"{}\"", h.name))
    });
    let empty = Properties::new();
    fn values<'d>(d: &'d Document, empty: &'d Properties) -> &'d Properties {
        d.standard.as_ref().map_or(empty, |x| &x.values)
    }
    let merged_values = merge_props(ctx, obj, "Standard", values(a, &empty), values(t, &empty), values(s, &empty), &all);
    let no_cats = StyleCatalogues::new();
    fn std_cats<'d>(d: &'d Document, none: &'d StyleCatalogues) -> &'d StyleCatalogues {
        d.standard.as_ref().map_or(none, |x| &x.styles)
    }
    let std_styles = merge_style_layer(ctx, StyleLayer::Standard, std_cats(a, &no_cats), std_cats(t, &no_cats), std_cats(s, &no_cats));
    m.styles = merge_style_layer(ctx, StyleLayer::Project, &a.styles, &t.styles, &s.styles);
    m.standard = head.map(|header| Standard { header, values: merged_values, styles: std_styles });
}

/// A style record's name in texts: `Text style "Tag large"` (`Wire category "24 V DC"`).
pub(crate) fn style_words(catalogue: crate::objects::styles::StyleCatalogue, id: &StyleRef, record: Option<&StyleRecord>) -> String {
    use crate::objects::styles::StyleCatalogue as C;
    let kind = match catalogue {
        C::Text => "Text style",
        C::Location => "Location box style",
        C::Frame => "Frame style",
        C::Wire => "Wire type",
        C::Page | C::Cable | C::RatingTable => "Style",
    };
    match record.and_then(|r| r.name.as_ref()).map(|n| n.as_str()) {
        Some(n) => format!("{kind} \"{n}\""),
        None => format!("{kind} {id}"),
    }
}

/// Merge one layer's style records, each record whole.
pub(super) fn merge_style_layer(
    ctx: &mut Ctx<'_, '_>, layer: StyleLayer, a: &StyleCatalogues, t: &StyleCatalogues, s: &StyleCatalogues,
) -> StyleCatalogues {
    let mut out = t.clone();
    let cats: BTreeSet<crate::objects::styles::StyleCatalogue> = a.keys().chain(t.keys()).chain(s.keys()).copied().collect();
    for cat in cats {
        let get = |c: &'_ StyleCatalogues| c.get(&cat).cloned().unwrap_or_default();
        let (ra, rt, rs) = (get(a), get(t), get(s));
        let ids: BTreeSet<&StyleRef> = ra.keys().chain(rt.keys()).chain(rs.keys()).collect();
        for id in ids {
            let (x, y, z) = (ra.get(id), rt.get(id), rs.get(id));
            let v = match pick(x, y, z) {
                Some(v) => v,
                None => {
                    let what = style_words(cat, id, y.or(z).or(x));
                    let show = |r: Option<&StyleRecord>| match r {
                        None => "removed".to_string(),
                        Some(r) => {
                            let f: Vec<String> = r.body.entries().iter().map(|(k, v)| format!("{k} {v}")).collect();
                            if f.is_empty() { "no changes".into() } else { f.join(", ") }
                        }
                    };
                    let c = Conflict {
                        id: format!("project/style.{}.{cat}.{id}", layer.word()),
                        kind: ConflictKind::Value,
                        object: Obj::Project,
                        what: format!("{what} · changed on both"),
                        target: show(y),
                        source: show(z),
                        choices: vec![Choice::Target, Choice::Source],
                        chosen: None,
                    };
                    match ctx.ask(c) {
                        Some(Choice::Source) => z.cloned(),
                        _ => y.cloned(),
                    }
                }
            };
            crate::objects::styles::put(&mut out, cat, id, v.as_ref());
        }
    }
    out
}

/// The object with ID `hex` in any of `docs`.
pub(super) fn obj_by_hex(docs: &[&Document], hex: &str) -> Option<Obj> {
    let id = u128::from_str_radix(hex, 16).ok()?;
    docs.iter().find_map(|d| {
        use crate::naming::id::*;
        [
            Obj::Device(DeviceId(id)),
            Obj::ConnectionPoint(ConnectionPointId(id)),
            Obj::Wire(WireId(id)),
            Obj::Cable(CableId(id)),
            Obj::Page(PageId(id)),
            Obj::Representation(RepresentationId(id)),
            Obj::Item(ItemId(id)),
            Obj::Folder(FolderId(id)),
            Obj::Layer(LayerId(id)),
            Obj::Definition(DefinitionId(id)),
        ]
        .into_iter()
        .find(|o| holds(d, *o))
    })
}

/// What a device kept from the source brings back with it (it is re-added with the
/// branch's values and whatever it needs): its connection points and its symbols (with their
/// drawing items) that the source has and the result lacks.
pub(super) fn device_needs(m: &Document, s: &Document, c: DeviceId) -> Vec<Obj> {
    let mut out: Vec<Obj> = s
        .connection_points
        .iter()
        .filter(|(k, t)| t.device() == Some(c) && !m.connection_points.contains_key(k))
        .map(|(k, _)| Obj::ConnectionPoint(*k))
        .collect();
    for (r, rep) in s.representations.iter().filter(|(_, r)| r.subject == Subject::Device(c)) {
        if !m.representations.contains_key(r) {
            out.push(Obj::Representation(*r));
            out.extend(rep.items.iter().filter(|i| !m.items.contains_key(i)).map(|i| Obj::Item(*i)));
        }
    }
    out
}

/// Devices kept from the source by a removed conflict's answer bring back what they need, listed
/// in the conflict's row. A connection point or symbol another answer
/// removes is a problem with a choice, never a device merged silently without it. Returns what
/// was brought back.
pub(super) fn keep_devices(ctx: &mut Ctx<'_, '_>, m: &mut Document) -> BTreeSet<Obj> {
    let (t, s) = (ctx.inp.target, ctx.inp.source);
    let docs = [s, t, ctx.inp.ancestor];
    let mut out = BTreeSet::new();
    let removed: Vec<(usize, DeviceId)> = ctx
        .conflicts
        .iter()
        .enumerate()
        .filter_map(|(i, k)| match k.object {
            Obj::Device(c) if k.kind == ConflictKind::Removed && !t.devices.contains_key(&c) && s.devices.contains_key(&c) => Some((i, c)),
            _ => None,
        })
        .collect();
    for (i, c) in removed {
        // What keeping it brings back, listed in the row whatever the answer.
        let needs = device_needs(t, s, c);
        let connection_points = needs.iter().filter(|o| matches!(o, Obj::ConnectionPoint(_))).count();
        let symbols = needs.iter().filter(|o| matches!(o, Obj::Representation(_))).count();
        if let Some(k) = ctx.conflicts.get_mut(i) {
            let mut parts = Vec::new();
            if connection_points > 0 {
                let names: Vec<String> = needs.iter().filter(|o| matches!(o, Obj::ConnectionPoint(_))).take(4).map(|o| name_of(&docs, *o)).collect();
                parts.push(format!("connection point{} {}", if connection_points == 1 { "" } else { "s" }, names.join(", ")));
            }
            if symbols > 0 {
                parts.push(format!("{symbols} symbol{}", if symbols == 1 { "" } else { "s" }));
            }
            if !parts.is_empty() {
                k.source = format!("{} (brings back {})", k.source, parts.join(" and "));
            }
        }
        if !m.devices.contains_key(&c) {
            continue;
        }
        for o in device_needs(m, s, c) {
            // Another answer removed it: say so with the way through.
            let answered_out = ctx.conflicts.iter().any(|k| k.object == o && k.kind == ConflictKind::Removed && k.chosen == Some(Choice::Target));
            if answered_out {
                ctx.problems.push(format!(
                    "Keeping {} needs {}: keep {} too, or keep {} removed, first.",
                    name_of(&docs, Obj::Device(c)),
                    name_of(&docs, o),
                    name_of(&docs, o),
                    name_of(&docs, Obj::Device(c))
                ));
                continue;
            }
            copy(m, s, o);
            out.insert(o);
        }
    }
    out
}

/// The cores on a line that went: each stays unplaced or goes ([`Core::off_line`]), in place.
pub(super) fn off_line(cores: &mut Vec<Core>, gone: impl Fn(&Core) -> bool) {
    let mut out: Vec<Core> = Vec::with_capacity(cores.len());
    for c in cores.drain(..) {
        if !gone(&c) {
            out.push(c);
        } else if let Some(k) = c.off_line(&out) {
            out.push(k);
        }
    }
    *cores = out;
}

/// Cable cores: adds and removes from both sides combine. A core is the same core
/// on both sides when it is on the same line, or, unplaced, has the same label. Each of its fields
/// is the side that changed it (the target's when both did).
pub(super) fn merge_members(a: &[Core], t: &[Core], s: &[Core]) -> Vec<Core> {
    fn has<'a>(l: &'a [Core], c: &Core) -> Option<&'a Core> {
        l.iter().find(|m| m.position == c.position && (c.position != CorePosition::Unplaced || m.label == c.label))
    }
    fn field<T: PartialEq + Clone>(am: Option<&Core>, m: &Core, sm: Option<&Core>, f: fn(&Core) -> &T) -> T {
        pick(am.map(f), Some(f(m)), sm.map(f)).flatten().unwrap_or_else(|| f(m).clone())
    }
    let mut out: Vec<Core> = Vec::new();
    for m in t {
        let (am, sm) = (has(a, m), has(s, m));
        if am.is_some() && sm.is_none() {
            continue; // removed by the source
        }
        out.push(Core {
            label: field(am, m, sm, |x| &x.label),
            colours: field(am, m, sm, |x| &x.colours),
            purpose: field(am, m, sm, |x| &x.purpose),
            position: m.position,
            through: field(am, m, sm, |x| &x.through),
        });
    }
    for m in s {
        if has(t, m).is_none() && has(a, m).is_none() {
            out.push(m.clone()); // added by the source
        }
    }
    out
}

/// A three-way merge of an order (pages or folders) over the objects `keep` holds.
pub(super) fn merge_order<T: Copy + Ord>(ctx: &mut Ctx<'_, '_>, id: &str, what: &str, a: &[T], t: &[T], s: &[T], keep: &dyn Fn(T) -> bool) -> Vec<T> {
    let (sa, st, ss): (BTreeSet<T>, BTreeSet<T>, BTreeSet<T>) =
        (a.iter().copied().collect(), t.iter().copied().collect(), s.iter().copied().collect());
    let common = |l: &[T]| -> Vec<T> { l.iter().copied().filter(|x| sa.contains(x) && st.contains(x) && ss.contains(x)).collect() };
    let (fa, ft, fs) = (common(a), common(t), common(s));
    let base: &[T] = if ft == fa {
        s
    } else if fs == fa || fs == ft {
        t
    } else {
        let c = Conflict {
            id: id.into(),
            kind: ConflictKind::Order,
            object: Obj::Project,
            what: what.into(),
            target: format!("as on {}", ctx.t()),
            source: format!("as on {}", ctx.s()),
            choices: vec![Choice::Target, Choice::Source],
            chosen: None,
        };
        match ctx.ask(c) {
            Some(Choice::Source) => s,
            _ => t,
        }
    };
    let mut out: Vec<T> = base.iter().copied().filter(|x| keep(*x)).collect();
    let mut placed: BTreeSet<T> = out.iter().copied().collect();
    for list in [t, s] {
        for (i, x) in list.iter().enumerate() {
            if placed.contains(x) || !keep(*x) {
                continue;
            }
            // After its nearest predecessor already placed, else first.
            let at = list.get(..i).unwrap_or_default().iter().rev().find_map(|p| out.iter().position(|q| q == p)).map_or(0, |p| p + 1);
            out.insert(at, *x);
            placed.insert(*x);
        }
    }
    out
}
