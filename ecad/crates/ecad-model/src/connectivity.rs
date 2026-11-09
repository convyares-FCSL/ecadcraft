//! Nets, kept up to date with every op (brief 022, design 022 §2 and §4).
//!
//! A **net** is a connected set of connection points (terminals), conductors and continuation
//! name groups, joined only by stored references (DOC-03: geometry is never consulted):
//! 1. a conductor end attached to a terminal;
//! 2. a continuation (a component with `kind = continuation` and a `name`) joins its terminals to
//!    every other continuation with the same name key ([`name_key`]): the name group is a node;
//! 3. a definition's bridges ([`crate::Definition::bridges`]) join a component's terminals whose
//!    keys are in one group.
//!
//! Nothing else joins: crossings, T-junctions without a junction device and loose ends lying on
//! another route are separate nets.
//!
//! # Semantics
//! - [`Connectivity`] is derived: never serialised, never compared by the document's `==`.
//!   [`Connectivity::build`] computes it from scratch; [`crate::Op::apply`] keeps it current.
//! - **Joins** (an end attached, a node added, a continuation named into a group, a bridge
//!   embedded) union two nets at once: the smaller net's members are relabelled into the larger.
//! - **Cuts** (an end detached, a node removed, a continuation renamed out of a group, a bridge
//!   removed) only mark the net *suspect*, with the nodes on either side of each cut (its
//!   *frontier*). Once per transaction ([`Connectivity::settle`], run by the ledger after a
//!   transaction's ops, by [`crate::Op::apply`] after a single op) a search runs from each frontier
//!   node of a suspect net, one step at a time, the smallest search first; searches that meet join.
//!   When at most one search is still running, every finished search is a part that fell off: it
//!   becomes a new net, and the net keeps its identity for the rest. So a cut costs about twice the
//!   smaller part when the net splits and the net's size when it does not; never the project.
//! - Every op not listed in [`Connectivity::on_op`]'s map changes nothing here.
//! - Per net, the counted wire numbers, fixed numbers and potentials of its conductors are kept
//!   ([`NetData`]), and project-wide, which nets carry each number ([`Connectivity::nets_numbered`])
//!   and the highest sequence used per number prefix ([`Connectivity::next_in`]).
//! - Net IDs ([`NetId`]) are not stable across a rebuild: compare nets by their members.
//!
//! This module reads the document and its indexes; it holds no I/O.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use crate::doc::{Document, Properties};
use crate::id::{ComponentId, ConductorId, TerminalId};
use crate::op::{ObjectRef, Op};

/// Conductor property: the wire number its net carries (design 022 §5.1).
pub const WIRE_NUMBER: &str = "wire_number";
/// Conductor property: `yes` when the wire number was fixed by hand.
pub const WIRE_NUMBER_FIXED: &str = "wire_number_fixed";
/// Conductor property: the potential (`24V`, `L1`…) the conductor carries.
pub const POTENTIAL: &str = "potential";
/// Component property naming what a tagless component is (`junction`, `continuation`).
pub const KIND: &str = "kind";
/// The [`KIND`] of a continuation.
pub const CONTINUATION: &str = "continuation";
/// The [`KIND`] of a junction.
pub const JUNCTION: &str = "junction";
/// Component property: a continuation's name.
pub const NAME: &str = "name";

/// The key two continuation names are compared by: trimmed, upper case.
pub fn name_key(name: &str) -> String {
    name.trim().to_uppercase()
}

/// The name key of a component with these properties, when it is a named continuation.
pub fn continuation_key(props: &Properties) -> Option<String> {
    if props.get(KIND).map(String::as_str) != Some(CONTINUATION) {
        return None;
    }
    let k = name_key(props.get(NAME)?);
    (!k.is_empty()).then_some(k)
}

/// A wire number split into its prefix and trailing sequence number (`24V-07` → `("24V-", 7)`,
/// `24V-105.03` → `("24V-105.", 3)`); `None` without trailing digits, or past `u64`.
pub fn split_number(number: &str) -> Option<(&str, u64)> {
    let digits = number.bytes().rev().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let at = number.len().checked_sub(digits)?;
    let (prefix, n) = (number.get(..at)?, number.get(at..)?);
    Some((prefix, n.parse().ok()?))
}

/// A node of the connectivity graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Node {
    Terminal(TerminalId),
    Conductor(ConductorId),
    /// A continuation name group, by [`name_key`].
    Name(String),
}

/// A net's identity in one [`Connectivity`] (not stable across rebuilds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetId(pub u64);

/// What one conductor contributes to its net's counted values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Contribution {
    number: Option<String>,
    fixed: bool,
    potential: Option<String>,
}

impl Contribution {
    pub(crate) fn of(props: &Properties) -> Self {
        Self {
            number: props.get(WIRE_NUMBER).cloned(),
            fixed: props.get(WIRE_NUMBER_FIXED).is_some_and(|v| v == "yes"),
            potential: props.get(POTENTIAL).cloned(),
        }
    }
}

/// One net: its members and the counted values its conductors carry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetData {
    pub terminals: BTreeSet<TerminalId>,
    pub conductors: BTreeSet<ConductorId>,
    /// Continuation name keys joined into it.
    pub names: BTreeSet<String>,
    /// Wire number → how many of its conductors carry it.
    pub numbers: BTreeMap<String, u32>,
    /// Fixed wire number → how many of its conductors carry it fixed.
    pub fixed: BTreeMap<String, u32>,
    /// Potential → how many of its conductors carry it.
    pub potentials: BTreeMap<String, u32>,
}

impl NetData {
    fn len(&self) -> usize {
        self.terminals.len() + self.conductors.len() + self.names.len()
    }

    fn nodes(&self) -> Vec<Node> {
        let mut out: Vec<Node> = Vec::with_capacity(self.len());
        out.extend(self.terminals.iter().map(|t| Node::Terminal(*t)));
        out.extend(self.conductors.iter().map(|c| Node::Conductor(*c)));
        out.extend(self.names.iter().map(|n| Node::Name(n.clone())));
        out
    }

    /// The members, sorted, as a comparable value.
    fn members(&self) -> (Vec<TerminalId>, Vec<ConductorId>, Vec<String>) {
        (self.terminals.iter().copied().collect(), self.conductors.iter().copied().collect(), self.names.iter().cloned().collect())
    }

    /// The conductor with the lowest ID (nets are ordered by it).
    pub fn first_conductor(&self) -> Option<ConductorId> {
        self.conductors.first().copied()
    }
}

/// `map[key] += 1`.
fn count_up(map: &mut BTreeMap<String, u32>, key: &str) {
    let n = map.entry(key.to_string()).or_insert(0);
    *n = n.saturating_add(1);
}

/// `map[key] -= 1`, removing it at zero.
fn count_down(map: &mut BTreeMap<String, u32>, key: &str) {
    if let Some(n) = map.get_mut(key) {
        *n = n.saturating_sub(1);
        if *n == 0 {
            map.remove(key);
        }
    }
}

/// The nets of a document, kept incrementally (see the module docs).
///
/// Its `Debug` form is canonical (the nets as sorted members with their counts, the sequence
/// index), so two kept states holding the same nets print alike whatever their net IDs.
#[derive(Clone, Default)]
pub struct Connectivity {
    node_net: HashMap<Node, NetId>,
    nets: HashMap<NetId, NetData>,
    next: u64,
    /// Wire number → nets carrying it → how many of their conductors carry it.
    by_number: BTreeMap<String, BTreeMap<NetId, u32>>,
    /// Number prefix ([`split_number`]) → sequence → how many conductors carry it.
    seqs: BTreeMap<String, BTreeMap<u64, u32>>,
    /// Nets a cut may have split, with the nodes on either side of each cut, to be searched by
    /// [`Connectivity::settle`].
    suspect: BTreeMap<NetId, Vec<Node>>,
}

impl std::fmt::Debug for Connectivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut nets: Vec<_> = self.nets.values().map(|n| (n.members(), n.numbers.clone(), n.fixed.clone(), n.potentials.clone())).collect();
        nets.sort();
        f.debug_struct("Connectivity").field("nets", &nets).field("seqs", &self.seqs).field("unsettled", &self.suspect.len()).finish()
    }
}

/// Whether `node` exists in `doc`.
fn present(doc: &Document, node: &Node) -> bool {
    match node {
        Node::Terminal(t) => doc.terminals.contains_key(t),
        Node::Conductor(c) => doc.conductors.contains_key(c),
        Node::Name(k) => doc.index.continuations.contains_key(k),
    }
}

/// The name group a terminal's component joins it to, if it is a named continuation.
fn name_of_terminal(doc: &Document, t: TerminalId) -> Option<String> {
    let c = doc.terminals.get(&t)?.component;
    continuation_key(&doc.components.get(&c)?.properties)
}

/// The terminals bridged to `t` inside its device.
fn bridged(doc: &Document, t: TerminalId) -> Vec<TerminalId> {
    let Some(term) = doc.terminals.get(&t) else { return Vec::new() };
    let Some(key) = term.key.as_deref() else { return Vec::new() };
    let Some(def) = doc.components.get(&term.component).and_then(|c| c.definition).and_then(|d| doc.definitions.get(&d)) else {
        return Vec::new();
    };
    if def.bridges.is_empty() {
        return Vec::new();
    }
    let keys: BTreeSet<&str> = def.bridged_with(key).collect();
    if keys.is_empty() {
        return Vec::new();
    }
    doc.terminals_of(term.component).filter(|(id, x)| *id != t && x.key.as_deref().is_some_and(|k| keys.contains(k))).map(|(id, _)| id).collect()
}

/// About how many edges `node` has, read from the indexes without listing them.
fn degree(doc: &Document, node: &Node) -> usize {
    match node {
        Node::Terminal(t) => doc.index.terminal_ends.get(t).map_or(0, BTreeMap::len).saturating_add(2),
        Node::Conductor(_) => 2,
        Node::Name(k) => doc.index.continuations.get(k).map_or(0, BTreeMap::len),
    }
}

/// The present nodes joined to `node` by one edge.
fn neighbors(doc: &Document, node: &Node) -> Vec<Node> {
    let mut out = Vec::new();
    match node {
        Node::Terminal(t) => {
            for (c, _) in doc.ends_at(*t) {
                if doc.conductors.contains_key(&c) {
                    out.push(Node::Conductor(c));
                }
            }
            if let Some(k) = name_of_terminal(doc, *t) {
                out.push(Node::Name(k));
            }
            out.extend(bridged(doc, *t).into_iter().map(Node::Terminal));
        }
        Node::Conductor(c) => {
            if let Some(cond) = doc.conductors.get(c) {
                for t in cond.ends.iter().flatten() {
                    if doc.terminals.contains_key(t) {
                        out.push(Node::Terminal(*t));
                    }
                }
            }
        }
        Node::Name(k) => {
            for comp in doc.continuations_named(k) {
                out.extend(doc.terminals_of(comp).map(|(t, _)| Node::Terminal(t)));
            }
        }
    }
    out
}

impl Connectivity {
    /// The nets of `doc`, computed from scratch: every node, walked once.
    pub fn build(doc: &Document) -> Self {
        let mut c = Connectivity::default();
        let all = doc
            .terminals
            .keys()
            .map(|t| Node::Terminal(*t))
            .chain(doc.conductors.keys().map(|k| Node::Conductor(*k)))
            .chain(doc.index.continuations.keys().map(|k| Node::Name(k.clone())));
        for node in all {
            if c.node_net.contains_key(&node) {
                continue;
            }
            let id = c.fresh();
            c.nets.insert(id, NetData::default());
            let mut stack = vec![node];
            while let Some(n) = stack.pop() {
                if c.node_net.contains_key(&n) {
                    continue;
                }
                stack.extend(neighbors(doc, &n).into_iter().filter(|m| !c.node_net.contains_key(m)));
                c.attach(doc, n, id);
            }
        }
        c
    }

    fn fresh(&mut self) -> NetId {
        self.next = self.next.saturating_add(1);
        NetId(self.next)
    }

    /// Put `node` (not in any net) into net `id` with its contributions.
    fn attach(&mut self, doc: &Document, node: Node, id: NetId) {
        let contribution = match &node {
            Node::Conductor(c) => doc.conductors.get(c).map(|x| Contribution::of(&x.properties)),
            _ => None,
        };
        if let Some(net) = self.nets.get_mut(&id) {
            match &node {
                Node::Terminal(t) => {
                    net.terminals.insert(*t);
                }
                Node::Conductor(c) => {
                    net.conductors.insert(*c);
                }
                Node::Name(k) => {
                    net.names.insert(k.clone());
                }
            }
        }
        if let Some(con) = contribution {
            self.contribute(id, &con, true);
            self.count_seq(&con, true);
        }
        self.node_net.insert(node, id);
    }

    /// Add (`up`) or remove one conductor's values to net `id`'s counts and the number index.
    fn contribute(&mut self, id: NetId, con: &Contribution, up: bool) {
        let Some(net) = self.nets.get_mut(&id) else { return };
        let step = if up { count_up } else { count_down };
        if let Some(n) = &con.number {
            step(&mut net.numbers, n);
            if con.fixed {
                step(&mut net.fixed, n);
            }
            let slot = self.by_number.entry(n.clone()).or_default();
            if up {
                let v = slot.entry(id).or_insert(0);
                *v = v.saturating_add(1);
            } else if let Some(v) = slot.get_mut(&id) {
                *v = v.saturating_sub(1);
                if *v == 0 {
                    slot.remove(&id);
                }
            }
            if slot.is_empty() {
                self.by_number.remove(n);
            }
        }
        if let Some(p) = &con.potential {
            step(&mut net.potentials, p);
        }
    }

    /// Add or remove a conductor's number from the project-wide sequence index.
    fn count_seq(&mut self, con: &Contribution, up: bool) {
        let Some((prefix, n)) = con.number.as_deref().and_then(split_number) else { return };
        let slot = self.seqs.entry(prefix.to_string()).or_default();
        if up {
            let v = slot.entry(n).or_insert(0);
            *v = v.saturating_add(1);
        } else if let Some(v) = slot.get_mut(&n) {
            *v = v.saturating_sub(1);
            if *v == 0 {
                slot.remove(&n);
            }
        }
        if slot.is_empty() {
            self.seqs.remove(prefix);
        }
    }

    /// Move `node` from its net to net `to` (both exist).
    fn relabel(&mut self, doc: &Document, node: &Node, to: NetId) {
        let Some(from) = self.node_net.get(node).copied() else { return };
        if from == to {
            return;
        }
        if let Some(net) = self.nets.get_mut(&from) {
            match node {
                Node::Terminal(t) => {
                    net.terminals.remove(t);
                }
                Node::Conductor(c) => {
                    net.conductors.remove(c);
                }
                Node::Name(k) => {
                    net.names.remove(k);
                }
            }
        }
        if let Node::Conductor(c) = node
            && let Some(x) = doc.conductors.get(c)
        {
            let con = Contribution::of(&x.properties);
            self.contribute(from, &con, false);
            self.contribute(to, &con, true);
        }
        if let Some(net) = self.nets.get_mut(&to) {
            match node {
                Node::Terminal(t) => {
                    net.terminals.insert(*t);
                }
                Node::Conductor(c) => {
                    net.conductors.insert(*c);
                }
                Node::Name(k) => {
                    net.names.insert(k.clone());
                }
            }
        }
        self.node_net.insert(node.clone(), to);
    }

    /// Join the nets of `a` and `b` (both present): the smaller is relabelled into the larger.
    fn union(&mut self, doc: &Document, a: &Node, b: &Node) {
        let (Some(x), Some(y)) = (self.node_net.get(a).copied(), self.node_net.get(b).copied()) else { return };
        if x == y {
            return;
        }
        let (lx, ly) = (self.nets.get(&x).map_or(0, NetData::len), self.nets.get(&y).map_or(0, NetData::len));
        let (small, large) = if lx < ly { (x, y) } else { (y, x) };
        let moving = self.nets.get(&small).map(NetData::nodes).unwrap_or_default();
        for n in &moving {
            self.relabel(doc, n, large);
        }
        self.nets.remove(&small);
        if let Some(frontier) = self.suspect.remove(&small) {
            self.suspect.entry(large).or_default().extend(frontier);
        }
    }

    /// Make sure a present `node` has a net, joined to every present neighbour.
    fn ensure(&mut self, doc: &Document, node: Node) {
        if !present(doc, &node) {
            return;
        }
        let mut work = vec![node];
        while let Some(n) = work.pop() {
            if !self.node_net.contains_key(&n) {
                let id = self.fresh();
                self.nets.insert(id, NetData::default());
                self.attach(doc, n.clone(), id);
            }
            for m in neighbors(doc, &n) {
                if self.node_net.contains_key(&m) {
                    self.union(doc, &n, &m);
                } else {
                    // A present neighbour without a net (an op order a transaction may take):
                    // it gets one and is joined in turn.
                    work.push(m);
                }
            }
        }
    }

    /// Mark the net of `node` as possibly split, `node` being on one side of a cut.
    fn cut(&mut self, node: &Node) {
        if let Some(id) = self.node_net.get(node) {
            self.suspect.entry(*id).or_default().push(node.clone());
        }
    }

    /// Take `node` out of its net (it no longer exists; `doc` still shows it); the rest of the net
    /// is suspect, with the node's neighbours as the frontier.
    fn remove(&mut self, doc: &Document, doc_props: Option<&Properties>, node: &Node) {
        let Some(id) = self.node_net.get(node).copied() else { return };
        let around: Vec<Node> = neighbors(doc, node).into_iter().filter(|m| m != node && self.node_net.get(m) == Some(&id)).collect();
        self.node_net.remove(node);
        if let Some(props) = doc_props {
            let con = Contribution::of(props);
            self.contribute(id, &con, false);
            self.count_seq(&con, false);
        }
        let empty = match self.nets.get_mut(&id) {
            Some(net) => {
                match node {
                    Node::Terminal(t) => {
                        net.terminals.remove(t);
                    }
                    Node::Conductor(c) => {
                        net.conductors.remove(c);
                    }
                    Node::Name(k) => {
                        net.names.remove(k);
                    }
                }
                net.len() == 0
            }
            None => false,
        };
        if empty {
            self.nets.remove(&id);
            self.suspect.remove(&id);
        } else {
            self.suspect.entry(id).or_default().extend(around);
        }
    }

    /// The terminals of `component` (present ones).
    fn terminals_of(doc: &Document, component: ComponentId) -> Vec<Node> {
        doc.terminals_of(component).map(|(t, _)| Node::Terminal(t)).collect()
    }

    /// Before `op` is applied to `doc`: what it is about to cut.
    ///
    /// # Update map (design 022 §11)
    /// | Op | Here |
    /// | --- | --- |
    /// | `CreateConductor` / `DeleteConductor` | node added and joined to its ends / node removed, its net suspect |
    /// | `AttachEnd` | `before` cut (net suspect), `after` joined |
    /// | `AddTerminal` / `RemoveTerminal` | node added and joined (ends, name group, bridges) / removed |
    /// | `CreateComponent` / `DeleteComponent` of a continuation | its terminals joined to / cut from its name group |
    /// | `SetProperty` component `kind` or `name` | cut from the old name group, joined to the new |
    /// | `SetProperty` conductor `wire_number`, `wire_number_fixed`, `potential` | the net's counts and the number index |
    /// | `EmbedDefinition` / `RemoveDefinition` with bridges | its components' bridged terminals joined / cut |
    /// | every other op | nothing |
    pub(crate) fn before_op(&mut self, doc: &Document, op: &Op) {
        match op {
            Op::DeleteConductor { id, .. } => {
                let props = doc.conductors.get(id).map(|c| c.properties.clone());
                self.remove(doc, props.as_ref(), &Node::Conductor(*id));
            }
            Op::AttachEnd { conductor, before: Some(t), .. } => {
                self.cut(&Node::Conductor(*conductor));
                self.cut(&Node::Terminal(*t));
            }
            Op::RemoveTerminal { id, .. } => self.remove(doc, None, &Node::Terminal(*id)),
            Op::DeleteComponent { id, .. } => {
                for t in Self::terminals_of(doc, *id) {
                    self.cut_around(doc, &t);
                }
            }
            Op::SetProperty { object: ObjectRef::Component(id), key, .. } if key == KIND || key == NAME => {
                for t in Self::terminals_of(doc, *id) {
                    self.cut_around(doc, &t);
                }
            }
            Op::SetProperty { object: ObjectRef::Conductor(id), key, .. } if key == WIRE_NUMBER || key == WIRE_NUMBER_FIXED || key == POTENTIAL => {
                if let (Some(net), Some(c)) = (self.node_net.get(&Node::Conductor(*id)).copied(), doc.conductors.get(id)) {
                    let con = Contribution::of(&c.properties);
                    self.contribute(net, &con, false);
                    self.count_seq(&con, false);
                }
            }
            Op::RemoveDefinition { id, definition } if !definition.bridges.is_empty() => {
                for comp in doc.components_using(*id) {
                    for t in Self::terminals_of(doc, comp) {
                        self.cut_around(doc, &t);
                    }
                }
            }
            _ => {}
        }
    }

    /// `node` and its name group and bridged points are on the sides of a cut.
    fn cut_around(&mut self, doc: &Document, node: &Node) {
        self.cut(node);
        for m in neighbors(doc, node) {
            if !matches!(m, Node::Conductor(_)) {
                self.cut(&m);
            }
        }
    }

    /// After `op` was applied to `doc`: what it joined. See [`Connectivity::before_op`].
    pub(crate) fn after_op(&mut self, doc: &Document, op: &Op) {
        match op {
            Op::CreateConductor { id, .. } => self.ensure(doc, Node::Conductor(*id)),
            Op::AttachEnd { conductor, after: Some(t), .. } => {
                self.ensure(doc, Node::Conductor(*conductor));
                self.ensure(doc, Node::Terminal(*t));
            }
            Op::AddTerminal { id, .. } => self.ensure(doc, Node::Terminal(*id)),
            Op::CreateComponent { id, component } | Op::DeleteComponent { id, component } => {
                self.name_groups(doc, component.properties.get(NAME).map(|n| name_key(n)));
                if matches!(op, Op::CreateComponent { .. }) {
                    for t in Self::terminals_of(doc, *id) {
                        self.ensure(doc, t);
                    }
                }
            }
            Op::SetProperty { object: ObjectRef::Component(id), key, before, after } if key == KIND || key == NAME => {
                // The old group may be gone, the new one may be new.
                let old = (key == NAME).then(|| before.as_deref().map(name_key)).flatten();
                let new = (key == NAME).then(|| after.as_deref().map(name_key)).flatten();
                self.name_groups(doc, old);
                let current = doc.components.get(id).and_then(|c| c.properties.get(NAME)).map(|n| name_key(n));
                self.name_groups(doc, current);
                self.name_groups(doc, new);
                for t in Self::terminals_of(doc, *id) {
                    self.ensure(doc, t);
                }
            }
            Op::SetProperty { object: ObjectRef::Conductor(id), key, .. } if key == WIRE_NUMBER || key == WIRE_NUMBER_FIXED || key == POTENTIAL => {
                if let (Some(net), Some(c)) = (self.node_net.get(&Node::Conductor(*id)).copied(), doc.conductors.get(id)) {
                    let con = Contribution::of(&c.properties);
                    self.contribute(net, &con, true);
                    self.count_seq(&con, true);
                }
            }
            Op::EmbedDefinition { id, definition } if !definition.bridges.is_empty() => {
                for comp in doc.components_using(*id) {
                    for t in Self::terminals_of(doc, comp) {
                        self.ensure(doc, t);
                    }
                }
            }
            _ => {}
        }
    }

    /// Bring the name group `key` in line with the name index: a group with no continuation left
    /// is removed (its net suspect), a new one is added and joined.
    fn name_groups(&mut self, doc: &Document, key: Option<String>) {
        let Some(k) = key else { return };
        let node = Node::Name(k);
        if present(doc, &node) {
            self.ensure(doc, node);
        } else {
            self.remove(doc, None, &node);
        }
    }

    /// Split every suspect net that fell apart (see the module docs). Cost: about twice the parts
    /// that fell off; a net that did not split costs at most its size.
    pub fn settle(&mut self, doc: &Document) {
        let suspects = std::mem::take(&mut self.suspect);
        for (id, frontier) in suspects {
            if !self.nets.contains_key(&id) {
                continue;
            }
            let mut seen = BTreeSet::new();
            let frontier: Vec<Node> = frontier.into_iter().filter(|n| self.node_net.get(n) == Some(&id) && seen.insert(n.clone())).collect();
            if frontier.len() < 2 {
                continue;
            }
            if !self.split_from(doc, id, frontier) {
                self.walk_all(doc, id);
            }
        }
    }

    /// The interleaved searches of [`Connectivity::settle`] in net `id` from `frontier` (two or
    /// more of its nodes). Returns `false` when they could not decide (never expected), so the
    /// caller walks the whole net.
    fn split_from(&mut self, doc: &Document, id: NetId, frontier: Vec<Node>) -> bool {
        let k = frontier.len();
        let mut parent: Vec<usize> = (0..k).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            let mut guard = 0usize;
            while let Some(&p) = parent.get(x) {
                if p == x || guard > parent.len() {
                    break;
                }
                x = p;
                guard += 1;
            }
            x
        }
        let mut owner: HashMap<Node, usize> = HashMap::new();
        let mut queues: Vec<VecDeque<Node>> = vec![VecDeque::new(); k];
        let mut members: Vec<Vec<Node>> = vec![Vec::new(); k];
        let mut done = vec![false; k];
        for (i, n) in frontier.into_iter().enumerate() {
            owner.insert(n.clone(), i);
            if let (Some(q), Some(m)) = (queues.get_mut(i), members.get_mut(i)) {
                q.push_back(n.clone());
                m.push(n);
            }
        }
        loop {
            // The running searches; the one with the least done and the cheapest next node takes the
            // next step, so a part of one wire runs out before a hub of many is opened.
            let running: Vec<usize> = (0..k).filter(|i| parent.get(*i) == Some(i) && !done.get(*i).copied().unwrap_or(true)).collect();
            if running.len() <= 1 {
                break;
            }
            let cost = |i: usize| {
                let next = queues.get(i).and_then(VecDeque::front).map_or(0, |n| degree(doc, n));
                members.get(i).map_or(0, Vec::len).saturating_add(next)
            };
            let Some(r) = running.iter().copied().min_by_key(|i| cost(*i)) else { break };
            let Some(n) = queues.get_mut(r).and_then(VecDeque::pop_front) else {
                if let Some(d) = done.get_mut(r) {
                    *d = true;
                }
                continue;
            };
            for m in neighbors(doc, &n) {
                if self.node_net.get(&m) != Some(&id) {
                    continue;
                }
                let me = find(&mut parent, r);
                match owner.get(&m).copied() {
                    None => {
                        owner.insert(m.clone(), me);
                        if let (Some(q), Some(v)) = (queues.get_mut(me), members.get_mut(me)) {
                            q.push_back(m.clone());
                            v.push(m);
                        }
                    }
                    Some(j) => {
                        let other = find(&mut parent, j);
                        if other == me {
                            continue;
                        }
                        // They met: one part. The smaller search moves into the larger.
                        let (small, large) =
                            if members.get(me).map_or(0, Vec::len) < members.get(other).map_or(0, Vec::len) { (me, other) } else { (other, me) };
                        let q = queues.get_mut(small).map(std::mem::take).unwrap_or_default();
                        let v = members.get_mut(small).map(std::mem::take).unwrap_or_default();
                        if let (Some(lq), Some(lv)) = (queues.get_mut(large), members.get_mut(large)) {
                            lq.extend(q);
                            lv.extend(v);
                        }
                        if let Some(p) = parent.get_mut(small) {
                            *p = large;
                        }
                        let d = done.get(small).copied().unwrap_or(false) && done.get(large).copied().unwrap_or(false);
                        if let Some(x) = done.get_mut(large) {
                            *x = d;
                        }
                    }
                }
            }
        }
        let roots: Vec<usize> = (0..k).filter(|i| parent.get(*i) == Some(i)).collect();
        let running: Vec<usize> = roots.iter().copied().filter(|i| !done.get(*i).copied().unwrap_or(true)).collect();
        // The part that keeps the net: the one still running (it holds every node not searched);
        // when every search finished, the largest.
        let keep = match running.as_slice() {
            [one] => *one,
            [] => {
                let searched: usize = roots.iter().map(|i| members.get(*i).map_or(0, Vec::len)).sum();
                if searched != self.nets.get(&id).map_or(0, NetData::len) {
                    return false;
                }
                match roots.iter().copied().max_by_key(|i| members.get(*i).map_or(0, Vec::len)) {
                    Some(r) => r,
                    None => return true,
                }
            }
            _ => return false,
        };
        for r in roots {
            if r == keep {
                continue;
            }
            let part = members.get_mut(r).map(std::mem::take).unwrap_or_default();
            let new = self.fresh();
            self.nets.insert(new, NetData::default());
            for n in &part {
                self.relabel(doc, n, new);
            }
        }
        true
    }

    /// Walk the whole of net `id` and split it into its parts (the fallback of
    /// [`Connectivity::settle`]).
    fn walk_all(&mut self, doc: &Document, id: NetId) {
        {
            let Some(net) = self.nets.get(&id) else { return };
            let members = net.nodes();
            let mut seen: BTreeSet<Node> = BTreeSet::new();
            let mut parts: Vec<Vec<Node>> = Vec::new();
            for start in members {
                if seen.contains(&start) {
                    continue;
                }
                let mut part = Vec::new();
                let mut stack = vec![start.clone()];
                seen.insert(start);
                while let Some(n) = stack.pop() {
                    for m in neighbors(doc, &n) {
                        // Only this net's members: a neighbour elsewhere would be a missed join.
                        if self.node_net.get(&m) == Some(&id) && seen.insert(m.clone()) {
                            stack.push(m);
                        }
                    }
                    part.push(n);
                }
                parts.push(part);
            }
            if parts.len() < 2 {
                return;
            }
            // The largest part keeps the identity (ties: the first found).
            let keep = parts.iter().enumerate().max_by(|a, b| a.1.len().cmp(&b.1.len()).then(b.0.cmp(&a.0)));
            let keep = keep.map_or(0, |(i, _)| i);
            for (i, part) in parts.into_iter().enumerate() {
                if i == keep {
                    continue;
                }
                let new = self.fresh();
                self.nets.insert(new, NetData::default());
                for n in &part {
                    self.relabel(doc, n, new);
                }
            }
        }
    }

    /// After `op` failed its preconditions (nothing in the document changed): undo what
    /// [`Connectivity::before_op`] did.
    pub(crate) fn repair(&mut self, doc: &Document, op: &Op) {
        match op {
            Op::DeleteConductor { id, .. } => self.ensure(doc, Node::Conductor(*id)),
            Op::RemoveTerminal { id, .. } => self.ensure(doc, Node::Terminal(*id)),
            _ => self.after_op(doc, op),
        }
    }

    /// Whether every cut has been settled.
    pub fn is_settled(&self) -> bool {
        self.suspect.is_empty()
    }

    /// The net of `node`.
    pub fn net_of(&self, node: &Node) -> Option<NetId> {
        self.node_net.get(node).copied()
    }

    /// The net of conductor `c`.
    pub fn net_of_conductor(&self, c: ConductorId) -> Option<NetId> {
        self.net_of(&Node::Conductor(c))
    }

    /// The net of terminal `t`.
    pub fn net_of_terminal(&self, t: TerminalId) -> Option<NetId> {
        self.net_of(&Node::Terminal(t))
    }

    /// Net `id`'s members and counts.
    pub fn net(&self, id: NetId) -> Option<&NetData> {
        self.nets.get(&id)
    }

    /// Every net, in no particular order.
    pub fn nets(&self) -> impl Iterator<Item = (NetId, &NetData)> {
        self.nets.iter().map(|(k, v)| (*k, v))
    }

    /// How many nets there are.
    pub fn len(&self) -> usize {
        self.nets.len()
    }

    /// Whether there are no nets.
    pub fn is_empty(&self) -> bool {
        self.nets.is_empty()
    }

    /// The nets whose conductors carry wire number `number`, with how many of them carry it.
    pub fn nets_numbered(&self, number: &str) -> impl Iterator<Item = (NetId, u32)> + '_ {
        self.by_number.get(number).into_iter().flat_map(|m| m.iter().map(|(k, v)| (*k, *v)))
    }

    /// Every wire number in use, in order, with the nets carrying it.
    pub fn numbers(&self) -> impl Iterator<Item = (&str, &BTreeMap<NetId, u32>)> {
        self.by_number.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// The next free sequence number after prefix `prefix`: the highest used plus one (gaps are
    /// never filled, design 022 §5.3).
    pub fn next_in(&self, prefix: &str) -> u64 {
        self.seqs.get(prefix).and_then(|m| m.keys().next_back()).map_or(1, |n| n.saturating_add(1))
    }

    /// The nets as comparable sets of members, sorted.
    fn partition(&self) -> Vec<(Vec<TerminalId>, Vec<ConductorId>, Vec<String>)> {
        let mut out: Vec<_> = self.nets.values().map(NetData::members).collect();
        out.sort();
        out
    }

    /// Where this differs from `other` (a fresh build), in words; `None` when the nets, their
    /// counts and the indexes are the same.
    pub fn differs_from(&self, other: &Connectivity) -> Option<String> {
        if !self.suspect.is_empty() {
            return Some(format!("{} nets are not settled", self.suspect.len()));
        }
        if self.partition() != other.partition() {
            return Some("the nets have different members".into());
        }
        let counts = |c: &Connectivity| {
            let mut v: Vec<_> = c.nets.values().map(|n| (n.members(), n.numbers.clone(), n.fixed.clone(), n.potentials.clone())).collect();
            v.sort();
            v
        };
        if counts(self) != counts(other) {
            return Some("the nets' counted numbers or potentials differ".into());
        }
        let numbers = |c: &Connectivity| {
            c.by_number
                .iter()
                .map(|(k, m)| {
                    let mut v: Vec<_> = m.iter().filter_map(|(n, x)| c.nets.get(n).map(|d| (d.members(), *x))).collect();
                    v.sort();
                    (k.clone(), v)
                })
                .collect::<Vec<_>>()
        };
        if numbers(self) != numbers(other) {
            return Some("the number index differs".into());
        }
        if self.seqs != other.seqs {
            return Some("the sequence index differs".into());
        }
        for (node, id) in &self.node_net {
            let listed = self.nets.get(id).is_some_and(|n| match node {
                Node::Terminal(t) => n.terminals.contains(t),
                Node::Conductor(c) => n.conductors.contains(c),
                Node::Name(k) => n.names.contains(k),
            });
            if !listed {
                return Some(format!("{node:?} is labelled with a net that does not list it"));
            }
        }
        if self.node_net.len() != other.node_net.len() {
            return Some("a different number of nodes".into());
        }
        None
    }
}
