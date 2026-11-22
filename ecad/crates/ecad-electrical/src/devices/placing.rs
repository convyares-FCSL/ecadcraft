//! Joining labels (§3.1) and placing a symbol.

use super::*;

/// The terminal labels of component `c`.
pub fn labels_of(doc: &Document, c: DeviceId) -> BTreeSet<String> {
    doc.connection_points_of(c).filter_map(|(_, t)| t.label.clone()).collect()
}

/// The labels a symbol `s` gets when it joins `c`, in its definition's terminal order; `extra`
/// are labels already given in the same transaction.
///
/// # Errors
/// Design 018 §3.1's refusals (T3).
pub fn join_labels(doc: &Document, c: DeviceId, s: &Starter, extra: &BTreeSet<String>) -> Result<Vec<String>, CmdError> {
    let mut used = labels_of(doc, c);
    used.extend(extra.iter().cloned());
    let who = tag_words(doc, c);
    // Brief 044: a connector holds connector halves and pins only.
    if crate::connectors::connector_of(doc, c).is_some_and(|x| x.mating) && !matches!(s.kind, Kind::ConnectorHalf { .. }) {
        return Err(CmdError::refused(format!("{who} is a connector; place a connector half or pin on it, or Enter for a new device.")));
    }
    match s.kind {
        Kind::Contact { make } => {
            // Brief 025 (owner answer Q4): the part's next free offered contact of this kind.
            if let Some(labels) = crate::references::devices::table::offer_labels(doc, c, make, &used) {
                return Ok(labels);
            }
            // Brief 034: the digit rule lives in the catalogue (a fuse bank's outputs are not contacts).
            symbols::rules::sequence_labels(&used, &symbols::rules::bank_outputs(doc, c), if make { &['3', '4'] } else { &['1', '2'] }, &who)
        }
        Kind::TerminalBlock => {
            let highest = used.iter().filter_map(|l| l.parse::<u64>().ok()).filter(|n| *n < 1_000_000).max().unwrap_or(0);
            Ok(vec![(highest + 1).to_string()])
        }
        // Brief 045 (FP7): a channel joins a card as its next free channel; its labels are pins.
        Kind::Channel(_) => crate::plc::channels::join_labels(doc, c, s, extra),
        Kind::Tagless => Err(CmdError::refused(format!("A {} is not part of a device; place it on its own.", s.name.to_lowercase()))),
        // Brief 034: the label rule lives in the catalogue.
        Kind::ChangeOver => symbols::rules::sequence_labels(&used, &symbols::rules::bank_outputs(doc, c), &['1', '2', '4'], &who),
        // Brief 044: a connector half joins by its own rules (design 044 §4.4), through `place`.
        Kind::ConnectorHalf { .. } => Err(CmdError::refused(format!(
            "A {} joins a connector only: place it with Place ▸ Connector, or name a connector.",
            s.name.to_lowercase()
        ))),
        Kind::Fixed { exclusive } => {
            // Design 052 §3.4: the device's head by type, whichever symbol was placed first.
            if exclusive && let Some(h) = crate::symbols::reps::head(doc, c) {
                return Err(CmdError::refused(format!("{who} already has a {}.", head_noun(doc, h))));
            }
            let labels = s.labels();
            if let Some(l) = labels.iter().find(|l| used.contains(*l)) {
                return Err(CmdError::refused(format!("{who} already has connection point {l}.")));
            }
            Ok(labels)
        }
    }
}

// ---------------------------------------------------------------- placing

/// What a placement makes: a new device, or another symbol of an existing one.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A new device, with this tag (read once, [`tag::enter`]) or the next automatic one.
    New(Option<tag::Entry>),
    /// Another symbol of this device.
    Join(DeviceId),
}

/// A planned `place` transaction and what it reports.
#[derive(Debug, Clone)]
pub struct Placed {
    pub ops: Vec<Op>,
    /// The transaction label.
    pub label: String,
    pub component: DeviceId,
    pub representation: RepresentationId,
    pub item: ItemId,
    pub definition: DefinitionId,
    pub tag: String,
    pub joined: bool,
    /// (definition key, label, terminal), in definition order.
    pub connection_points: Vec<(PointKey, String, ConnectionPointId)>,
}

/// Plan placing starter symbol `s` at `placement` on `page` (design 018 §10).
///
/// # Errors
/// The refusals of §3.1 (joining), an unknown device, no tag number left.
#[allow(clippy::too_many_arguments)]
pub fn plan_place(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, page: PageId, style: Style, s: &Starter, placement: Placement, target: Target,
) -> Result<Placed, CmdError> {
    let doc = ledger.document();
    let mut ops = Vec::new();
    let definition = match symbols::catalogue::embedded(doc, s) {
        Some(d) => d,
        None => {
            let d: DefinitionId = ids.fresh();
            ops.push(Op::EmbedDefinition { id: d, definition: s.definition()?.clone() });
            d
        }
    };
    let def = s.definition()?;
    // A typed tag a device already holds is that device (a tag is an identity).
    let target = match target {
        Target::New(Some(tag::Entry::Tag(g))) => match holder_of(doc, &g.key(), None) {
            Some(c) => Target::Join(c),
            // Devices and cables share one namespace (brief 026).
            None if !doc.cables_with(&g.key()).is_empty() => {
                return Err(CmdError::refused(format!("Tag {} is a cable's; choose another tag for a device.", g.shown())));
            }
            None => Target::New(Some(tag::Entry::Tag(g))),
        },
        other => other,
    };
    let (item, representation): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
    let instance = Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition, placement }) };
    let mut connection_points = Vec::new();
    match target {
        Target::New(t) => {
            // `-K?` typed: the next provisional number (brief 026).
            let t = high.given(ledger, t, &prefix_for(doc, s))?;
            let c: DeviceId = ids.fresh();
            ops.push(Op::CreateDevice {
                id: c,
                device: crate::devices::with_given(
                    Device {
                        description: description_of(s.name),
                        definition: Some(definition),
                        device_type: crate::plc::types::new_type(doc, &s.device_type()),
                        ..Device::default()
                    },
                    &t,
                )?,
            });
            for td in &def.connection_points {
                let tid: ConnectionPointId = ids.fresh();
                ops.push(Op::AddConnectionPoint {
                    id: tid,
                    connection_point: ConnectionPoint { owner: c.into(), label: td.label.clone(), key: Some(td.key.clone()), accepts: td.accepts },
                });
                connection_points.push((td.key.clone(), td.label.clone().unwrap_or_else(|| td.key.to_string()), tid));
            }
            ops.push(Op::AddItem { id: item, item: instance });
            ops.push(Op::LinkRepresentation {
                id: representation,
                rep: Representation {
                    subject: Subject::Device(c),
                    page,
                    form: Form::Symbol,
                    role: ecad_model::Role::Part(s.part_role()),
                    items: vec![item],
                    connection_points: BTreeMap::new(),
                },
            });
            Ok(Placed {
                ops,
                label: "Place symbol".into(),
                component: c,
                representation,
                item,
                definition,
                tag: t.shown(),
                joined: false,
                connection_points,
            })
        }
        Target::Join(c) => {
            if !doc.devices().contains_key(&c) {
                return Err(CmdError::NotFound(Noun::Device));
            }
            let labels = join_labels(doc, c, s, &BTreeSet::new())?;
            let mut cps = BTreeMap::new();
            for (td, label) in def.connection_points.iter().zip(&labels) {
                let tid: ConnectionPointId = ids.fresh();
                ops.push(Op::AddConnectionPoint {
                    id: tid,
                    connection_point: ConnectionPoint {
                        owner: c.into(),
                        label: Some(label.clone()),
                        key: Some(PointKey::Def(label.clone())),
                        accepts: td.accepts,
                    },
                });
                cps.insert(tid, td.at);
                connection_points.push((td.key.clone(), label.clone(), tid));
            }
            ops.push(Op::AddItem { id: item, item: instance });
            ops.push(Op::LinkRepresentation {
                id: representation,
                rep: Representation {
                    subject: Subject::Device(c),
                    page,
                    form: Form::Symbol,
                    role: ecad_model::Role::Part(s.part_role()),
                    items: vec![item],
                    connection_points: cps,
                },
            });
            // Design 072 §3: a PLC card's body is its symbol (a card a channel cell made had the
            // cell's), so its points are its symbol's.
            if crate::plc::rack::CardType::of_symbol(s).is_some()
                && let Some(d) = doc.devices().get(&c)
                && matches!(d.device_type, ecad_model::DeviceType::PlcModule(_) | ecad_model::DeviceType::PlcRack(_))
                && d.definition != Some(definition)
            {
                ops.push(Op::SetDeviceDefinition { id: c, before: d.definition, after: Some(definition) });
            }
            let t = tag_words(doc, c);
            Ok(Placed {
                ops,
                label: format!("Add symbol to {t}"),
                component: c,
                representation,
                item,
                definition,
                tag: t,
                joined: true,
                connection_points,
            })
        }
    }
}
