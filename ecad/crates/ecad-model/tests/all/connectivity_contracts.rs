//! Contract tests for brief 022: nets kept incrementally (design 022 §2, §4) and the numbering
//! pass (§5). Table-driven where the design gives a table.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::connectivity::{CONTINUATION, KIND, NAME};
use ecad_model::wiring::{self, Scheme};
use ecad_model::{
    Author, Connectivity, Definition, DefinitionId, Device, DeviceId, Document, IdAllocator, Ledger, ObjectRef, Op, Page, PageId, Terminal,
    TerminalDef, TerminalId, Wire, WireId,
};

/// A small project driven through the ledger, with the numbering pass as ECAD commits run it.
struct P {
    ledger: Ledger,
    ids: IdAllocator,
}

impl P {
    fn new() -> Self {
        let mut p = P { ledger: Ledger::new(7), ids: IdAllocator::seeded(11) };
        let page: PageId = p.ids.fresh();
        p.commit(vec![Op::AddPage { id: page, page: Page::titled("1").unwrap(), index: 0 }]);
        p
    }

    fn doc(&self) -> &Document {
        self.ledger.document()
    }

    /// Commit with the numbering pass, as `apply_edit` does.
    fn commit(&mut self, ops: Vec<Op>) {
        self.ledger.commit_extended(Author::User, "t", ops, None, wiring::assign).unwrap();
    }

    fn try_commit(&mut self, ops: Vec<Op>) -> ecad_model::Result<()> {
        self.ledger.commit_extended(Author::User, "t", ops, None, wiring::assign).map(|_| ())
    }

    /// A device with `n` terminals keyed `1`..`n`.
    fn device(&mut self, n: usize) -> (DeviceId, Vec<TerminalId>) {
        self.device_with(n, Device::default())
    }

    fn device_with(&mut self, n: usize, component: Device) -> (DeviceId, Vec<TerminalId>) {
        let c: DeviceId = self.ids.fresh();
        let mut ops = vec![Op::CreateDevice { id: c, device: component }];
        let mut ts = Vec::new();
        for k in 1..=n {
            let t: TerminalId = self.ids.fresh();
            ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: c, label: Some(k.to_string()), key: Some(k.to_string()) } });
            ts.push(t);
        }
        self.commit(ops);
        (c, ts)
    }

    fn continuation(&mut self, name: &str) -> (DeviceId, TerminalId) {
        let mut comp = Device::default();
        comp.properties.insert(KIND.into(), CONTINUATION.into());
        comp.properties.insert(NAME.into(), name.into());
        let (c, ts) = self.device_with(1, comp);
        (c, ts[0])
    }

    /// A connection point of its own (a one-terminal device), joined to nothing: where a wire
    /// that goes nowhere else ends.
    fn point(&mut self) -> TerminalId {
        self.device(1).1[0]
    }

    /// A wire whose bare ends are given connection points: the wire is one commit.
    fn bare(&mut self, a: TerminalId, b: TerminalId) -> Wire {
        Wire::new(self.ids.fresh(), self.ids.fresh(), a, b)
    }

    fn wire(&mut self, a: TerminalId, b: TerminalId) -> WireId {
        self.wire_props(a, b, &[])
    }

    fn wire_props(&mut self, a: TerminalId, b: TerminalId, props: &[(&str, &str)]) -> WireId {
        let w: WireId = self.ids.fresh();
        let mut wire = self.bare(a, b);
        for (k, v) in props {
            put(&mut wire, k, Some(v));
        }
        self.commit(vec![Op::CreateWire { id: w, wire }]);
        w
    }

    /// A wire from `a` to a point of its own.
    fn loose(&mut self, a: TerminalId) -> WireId {
        let b = self.point();
        self.wire(a, b)
    }

    /// A wire between two points of its own.
    fn free_props(&mut self, props: &[(&str, &str)]) -> WireId {
        let (a, b) = (self.point(), self.point());
        self.wire_props(a, b, props)
    }

    fn delete(&mut self, w: WireId) {
        let wire = self.doc().wires.get(&w).unwrap().clone();
        self.commit(vec![Op::DeleteWire { id: w, wire }]);
    }

    fn number(&self, w: WireId) -> Option<String> {
        self.doc().wires.get(&w)?.number_text().map(str::to_string)
    }

    fn fixed(&self, w: WireId) -> bool {
        self.doc().wires.get(&w).is_some_and(Wire::number_fixed)
    }

    fn same_net(&self, a: WireId, b: WireId) -> bool {
        let c = self.doc().connectivity();
        c.net_of_conductor(a).is_some() && c.net_of_conductor(a) == c.net_of_conductor(b)
    }

    fn set(&mut self, w: WireId, key: &str, value: Option<&str>) {
        let before = self.doc().wires.get(&w).unwrap().clone();
        let mut after = before.clone();
        put(&mut after, key, value);
        self.commit(vec![Op::ModifyWire { id: w, before, after }]);
    }

    /// Fix `value` on the whole net of `w`, as `wirenumber` does.
    fn fix(&mut self, w: WireId, value: &str) {
        let doc = self.doc();
        let id = doc.connectivity().net_of_conductor(w).unwrap();
        let ops = wiring::write_ops(doc, doc.connectivity().net(id).unwrap(), value, true);
        self.commit(ops);
    }

    fn check(&self) {
        let fresh = Connectivity::build(self.doc());
        assert_eq!(self.doc().connectivity().differs_from(&fresh), None);
    }
}

/// Design 022 §2 and §15 B: what joins and what does not.
#[test]
fn nets_join_through_attachments_continuations_and_bridges_only() {
    let mut p = P::new();
    // Two ends on one connection point: one net.
    let (_, k) = p.device(2);
    let a = p.loose(k[0]);
    let b = p.loose(k[0]);
    assert!(p.same_net(a, b));
    // A T without a junction (the end merely lies on the other wire): two nets.
    let c = p.free_props(&[]);
    assert!(!p.same_net(a, c));
    // With a junction: one net.
    let mut j = Device::default();
    j.properties.insert(KIND.into(), "junction".into());
    let (_, jt) = p.device_with(1, j);
    let d = p.wire(jt[0], k[1]);
    let e = p.loose(jt[0]);
    assert!(p.same_net(d, e));
    // A crossing never connects: two wires with no shared point.
    let (_, x) = p.device(4);
    let h = p.wire(x[0], x[1]);
    let v = p.wire(x[2], x[3]);
    assert!(!p.same_net(h, v));
    // Continuations with the same name (any case, trimmed) are one net, on any page.
    let (_, c1) = p.continuation("24V");
    let (_, c2) = p.continuation(" 24v ");
    let w1 = p.loose(c1);
    let w2 = p.loose(c2);
    assert!(p.same_net(w1, w2));
    let (_, c3) = p.continuation("0V");
    let w3 = p.loose(c3);
    assert!(!p.same_net(w1, w3));
    // A definition's bridge joins a device's terminals.
    let def: DefinitionId = p.ids.fresh();
    let definition = Definition {
        name: "bridged".into(),
        origin: None,
        graphics: vec![],
        terminals: ["1", "2", "3"].iter().map(|k| TerminalDef { key: (*k).into(), label: None, at: ecad_model::Pt::new(0.0, 0.0) }).collect(),
        bridges: vec![vec!["1".into(), "2".into()]],
        ..Default::default()
    };
    p.commit(vec![Op::EmbedDefinition { id: def, definition }]);
    let (_, bt) = p.device_with(3, Device { definition: Some(def), ..Device::default() });
    let b1 = p.loose(bt[0]);
    let b2 = p.loose(bt[1]);
    let b3 = p.loose(bt[2]);
    assert!(p.same_net(b1, b2));
    assert!(!p.same_net(b1, b3));
    p.check();
}

/// A cable core is an ordinary conductor: a cable continued on another page keeps one net per
/// core, and the wire on the same terminal is in it (design 022 §2).
#[test]
fn a_cable_core_runs_its_net_through() {
    let mut p = P::new();
    let (_, x) = p.device(2);
    let (_, m) = p.device(2);
    let core1 = p.wire(x[0], m[0]);
    let core2 = p.wire(x[1], m[1]);
    let cable: ecad_model::CableId = p.ids.fresh();
    p.commit(vec![Op::CreateCable {
        id: cable,
        cable: ecad_model::Cable {
            properties: BTreeMap::new(),
            members: vec![ecad_model::CableMember::on_wire(core1, Some("1".into())), ecad_model::CableMember::on_wire(core2, Some("2".into()))],
        },
    }]);
    let field = p.loose(x[0]);
    assert!(p.same_net(core1, field));
    assert!(!p.same_net(core1, core2));
    assert_eq!(p.number(core1), p.number(field));
}

/// Design 022 §5.2, every row of the rules table.
#[test]
fn the_numbering_rules() {
    let mut p = P::new();
    let (_, k) = p.device(6);
    // New net: next free number of its scope; with no potential the prefix is empty.
    let a = p.wire(k[0], k[1]);
    assert_eq!(p.number(a).as_deref(), Some("01"));
    // Another new net: the next one; adding a wire never renumbers the first.
    let b = p.wire(k[2], k[3]);
    assert_eq!(p.number(b).as_deref(), Some("02"));
    assert_eq!(p.number(a).as_deref(), Some("01"));
    // A conductor added to a numbered net gets its number.
    let a2 = p.loose(k[1]);
    assert_eq!(p.number(a2).as_deref(), Some("01"));
    // Merge, both automatic: the number on more conductors wins (01 has two, 02 one).
    let bridge = p.wire(k[1], k[2]);
    for w in [a, a2, b, bridge] {
        assert_eq!(p.number(w).as_deref(), Some("01"), "{w}");
    }
    // Split, automatic: the part with more conductors keeps it, the other gets the next free.
    p.delete(bridge);
    assert_eq!(p.number(a).as_deref(), Some("01"));
    assert_eq!(p.number(a2).as_deref(), Some("01"));
    // Next free is the highest used plus one: 02 is no longer carried, so it returns (Q6).
    assert_eq!(p.number(b).as_deref(), Some("02"));
    // Merge, one fixed: the fixed number wins and is written fixed on all.
    p.fix(b, "24V-99");
    let join = p.wire(k[1], k[3]);
    for w in [a, a2, b, join] {
        assert_eq!(p.number(w).as_deref(), Some("24V-99"));
        assert!(p.fixed(w));
    }
    // Split, fixed: both keep it (a Checks error).
    p.delete(join);
    assert_eq!(p.number(a).as_deref(), Some("24V-99"));
    assert_eq!(p.number(b).as_deref(), Some("24V-99"));
    assert_eq!(wiring::sharing(p.doc(), "24V-99").len(), 2);
    // Merge, two different fixed: nothing rewritten.
    p.fix(b, "24V-98");
    let join = p.wire(k[1], k[3]);
    assert_eq!(p.number(a).as_deref(), Some("24V-99"));
    assert_eq!(p.number(b).as_deref(), Some("24V-98"));
    let net = p.doc().connectivity().net_of_conductor(join).unwrap();
    assert_eq!(p.doc().connectivity().net(net).unwrap().fixed.len(), 2);
    p.check();
}

#[test]
fn a_potential_sets_the_prefix_and_a_potential_change_renumbers_an_automatic_number() {
    let mut p = P::new();
    let (_, k) = p.device(4);
    let a = p.wire_props(k[0], k[1], &[("potential", "24V")]);
    assert_eq!(p.number(a).as_deref(), Some("24V-01"));
    let b = {
        let f = p.point();
        p.wire_props(k[2], f, &[("potential", "24V")])
    };
    assert_eq!(p.number(b).as_deref(), Some("24V-02"));
    // PE is unnumbered: every net shows the prefix alone, and sharing it is no error.
    let pe1 = p.free_props(&[("potential", "PE")]);
    let pe2 = p.free_props(&[("potential", "PE")]);
    assert_eq!(p.number(pe1).as_deref(), Some("PE"));
    assert_eq!(p.number(pe2).as_deref(), Some("PE"));
    assert!(wiring::sharing(p.doc(), "PE").is_empty());
    // Changing the potential renumbers to the next free of the new prefix (Q7)...
    p.set(b, "potential", Some("0V"));
    assert_eq!(p.number(b).as_deref(), Some("0V-01"));
    // ...but a fixed number stays.
    p.fix(a, "X-1");
    p.set(a, "potential", Some("0V"));
    assert_eq!(p.number(a).as_deref(), Some("X-1"));
    // A continuation named for a potential gives a net with none its prefix.
    let (_, c) = p.continuation("N");
    let n = p.loose(c);
    assert_eq!(p.number(n).as_deref(), Some("N-01"));
    p.check();
}

#[test]
fn settings_changes_never_renumber_until_renumber_wires() {
    let mut p = P::new();
    let a = p.free_props(&[("potential", "24V")]);
    assert_eq!(p.number(a).as_deref(), Some("24V-01"));
    let before = p.doc().settings.clone();
    let mut after = before.clone();
    after.wiring.scheme = Scheme::Page;
    after.wiring.digits = Some(3);
    p.commit(vec![Op::SetProjectSettings { before, after }]);
    assert_eq!(p.number(a).as_deref(), Some("24V-01"));
    let b = p.free_props(&[("potential", "24V")]);
    // No route: page 1 column 1.
    assert_eq!(p.number(b).as_deref(), Some("24V-1.001"));
    let ops = wiring::renumber(p.doc(), true);
    p.commit(ops);
    let mut got: Vec<String> = [a, b].iter().filter_map(|w| p.number(*w)).collect();
    got.sort();
    assert_eq!(got, vec!["24V-1.001", "24V-1.002"]);
}

#[test]
fn renumber_unnumbered_numbers_only_nets_without_a_number() {
    let mut p = P::new();
    // A conductor created without the pass (an older project).
    let (a, b) = (p.point(), p.point());
    let w: WireId = p.ids.fresh();
    let wire = p.bare(a, b);
    p.ledger.commit(Author::User, "old", vec![Op::CreateWire { id: w, wire }]).unwrap();
    assert_eq!(p.number(w), None);
    assert_eq!(wiring::unnumbered_nets(p.doc()), 1);
    let numbered = p.free_props(&[]);
    assert_eq!(p.number(numbered).as_deref(), Some("01"));
    let ops = wiring::renumber(p.doc(), false);
    p.commit(ops);
    assert_eq!(p.number(w).as_deref(), Some("02"));
    assert_eq!(p.number(numbered).as_deref(), Some("01"));
    assert_eq!(wiring::unnumbered_nets(p.doc()), 0);
}

#[test]
fn undo_restores_numbers_exactly_and_the_pass_is_one_transaction_with_the_edit() {
    let mut p = P::new();
    let (_, k) = p.device(2);
    let a = p.loose(k[0]);
    let f = p.point();
    let head = p.ledger.head();
    let b = p.wire(k[1], f);
    assert_eq!(p.ledger.head(), head + 1, "the wire and its number are one transaction");
    assert_eq!(p.number(b).as_deref(), Some("02"));
    p.ledger.undo(Author::User).unwrap();
    assert!(!p.doc().wires.contains_key(&b));
    p.ledger.redo(Author::User).unwrap();
    assert_eq!(p.number(b).as_deref(), Some("02"));
    assert_eq!(p.number(a).as_deref(), Some("01"));
    p.check();
}

#[test]
fn a_refused_transaction_leaves_nets_as_they_were() {
    let mut p = P::new();
    let (_, k) = p.device(2);
    let a = p.wire(k[0], k[1]);
    let before = Connectivity::build(p.doc());
    // The second op fails its precondition: the first (a delete) is rolled back.
    let wire = p.doc().wires[&a].clone();
    let bad = vec![Op::DeleteWire { id: a, wire: wire.clone() }, Op::DeleteWire { id: a, wire }];
    assert!(p.try_commit(bad).is_err());
    assert_eq!(p.doc().connectivity().differs_from(&before), None);
}

/// Design 022 §4: a long random op sequence; after every step the kept nets equal a fresh build.
#[test]
fn random_ops_keep_nets_equal_to_a_fresh_build() {
    random_ops(0x2545_f491_4f6c_dd1d, 600);
}

/// The same over more seeds and longer sequences.
#[test]
#[ignore = "slow: 3 seeds of 1,500 random steps, verified after each, about 30 s"]
fn random_ops_over_many_seeds_keep_nets_equal_to_a_fresh_build() {
    for seed in [0x9E37_79B9_7F4A_7C15_u64, 0xD1B5_4A32_D192_ED03, 0xFEED_FACE_CAFE_BEEF] {
        random_ops(seed, 1_500);
    }
}

fn random_ops(seed: u64, steps: usize) {
    let mut p = P::new();
    let mut rng = seed;
    let mut next = move |n: usize| {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % n.max(1) as u64) as usize
    };
    let names = ["24V", "0V", "L1"];
    for step in 0..steps {
        let doc = p.doc();
        let terminals: Vec<TerminalId> = doc.terminals.keys().copied().collect();
        let conductors: Vec<WireId> = doc.wires.keys().copied().collect();
        let comps: Vec<DeviceId> = doc.devices.keys().copied().collect();
        let pick_t = |i: usize| terminals.get(i % terminals.len().max(1)).copied();
        match next(10) {
            0 | 1 => {
                let n = 1 + next(3);
                p.device(n);
            }
            2 => {
                let name = names[next(names.len())];
                p.continuation(name);
            }
            3 | 4 => {
                // An end that goes nowhere else lands on a point of its own.
                let a = if next(4) == 0 { None } else { pick_t(next(97)) };
                let b = if next(4) == 0 { None } else { pick_t(next(89)) };
                let a = a.unwrap_or_else(|| p.point());
                let b = b.unwrap_or_else(|| p.point());
                if a != b {
                    p.wire(a, b);
                }
            }
            5 if !conductors.is_empty() => {
                let w = conductors[next(conductors.len())];
                p.delete(w);
            }
            6 if !conductors.is_empty() => {
                // Move one end: the wire between other points is another wire (Delete + Create).
                let w = conductors[next(conductors.len())];
                let old = p.doc().wires.get(&w).unwrap().clone();
                let end = next(2);
                let to = if next(3) == 0 { p.point() } else { pick_t(next(101)).unwrap_or_else(|| p.point()) };
                let mut ends = old.ends;
                ends[end] = to;
                if ends[0] != ends[1] && ends != old.ends {
                    let id: WireId = p.ids.fresh();
                    let wire = old.values_onto(&p.bare(ends[0], ends[1]));
                    p.commit(vec![Op::DeleteWire { id: w, wire: old }, Op::CreateWire { id, wire }]);
                }
            }
            7 if !comps.is_empty() => {
                // Rename a continuation (or a plain device, which changes nothing).
                let c = comps[next(comps.len())];
                let before = p.doc().devices.get(&c).unwrap().properties.get(NAME).cloned();
                let after = Some(names[next(names.len())].to_string());
                if before != after {
                    p.commit(vec![Op::SetProperty { object: ObjectRef::Device(c), key: NAME.into(), before, after }]);
                }
            }
            8 if !comps.is_empty() => {
                // Remove a device whose terminals are free, with its terminals.
                let c = comps[next(comps.len())];
                let doc = p.doc();
                let ts: Vec<(TerminalId, Terminal)> = doc.terminals_of(c).map(|(t, x)| (t, x.clone())).collect();
                if ts.iter().all(|(t, _)| doc.ends_at(*t).is_empty()) {
                    let mut ops: Vec<Op> = ts.into_iter().map(|(id, terminal)| Op::RemoveTerminal { id, terminal }).collect();
                    ops.push(Op::DeleteDevice { id: c, device: doc.devices.get(&c).unwrap().clone() });
                    p.commit(ops);
                }
            }
            9 => {
                if next(2) == 0 {
                    let _ = p.ledger.undo(Author::User);
                } else {
                    let _ = p.ledger.redo(Author::User);
                }
            }
            _ => {}
        }
        let fresh = Connectivity::build(p.doc());
        assert_eq!(p.doc().connectivity().differs_from(&fresh), None, "step {step}");
    }
    // Invariant W after edits (undo may restore older states, which hold it too).
    for (_, net) in p.doc().connectivity().nets() {
        let fixed: BTreeSet<&String> = net.fixed.keys().collect();
        if fixed.len() <= 1 {
            assert!(net.numbers.len() <= 1, "one net, one number: {:?}", net.numbers);
        }
    }
}

/// An unrelated edit leaves every net untouched (design 022 §11's "nothing" rows): the kept
/// nets are the same object values, and the pass writes nothing.
#[test]
fn unrelated_ops_change_no_net_and_number_nothing() {
    let mut p = P::new();
    let (c, k) = p.device(2);
    p.wire(k[0], k[1]);
    let before = p.doc().connectivity().clone();
    let d = p.doc().devices[&c].clone();
    let tag = Op::ModifyDevice { id: c, before: d.clone(), after: d.with_tag(Some("-K1")) };
    let page: PageId = p.ids.fresh();
    let ops = vec![tag, Op::AddPage { id: page, page: Page::titled("2").unwrap(), index: 1 }];
    let head = p.ledger.head();
    p.commit(ops.clone());
    let tx = &p.ledger.log()[head as usize];
    assert_eq!(tx.ops.len(), ops.len(), "the pass added nothing");
    assert_eq!(p.doc().connectivity().differs_from(&before), None);
}

/// Brief 030 (design 030 §5.2, FP7): a strip's `link.<n>` joins the nets of the terminals it
/// names, in the same transaction as the numbering pass; changing or clearing it splits them;
/// relabelling a terminal keeps the link on its label; the kept nets match a rebuild.
#[test]
fn links_join_terminals_by_label_and_number_in_the_same_transaction() {
    let mut p = P::new();
    let (strip, t) = p.device(6);
    let w: Vec<WireId> = t.iter().map(|x| p.loose(*x)).collect();
    assert!(!p.same_net(w[0], w[3]));
    let set = |p: &mut P, key: &str, value: Option<&str>| {
        let before = p.doc().devices.get(&strip).unwrap().properties.get(key).cloned();
        p.commit(vec![Op::SetProperty { object: ObjectRef::Device(strip), key: key.into(), before, after: value.map(Into::into) }]);
    };
    set(&mut p, "link.1", Some("1-4"));
    assert!(p.same_net(w[0], w[3]) && p.same_net(w[1], w[2]));
    assert!(!p.same_net(w[0], w[4]));
    assert_eq!(p.number(w[0]), p.number(w[3]), "one wire number for the linked net");
    let ops = &p.ledger.log().last().unwrap().ops;
    assert!(ops.len() > 1, "the numbering pass's ops are in the link's transaction");
    p.check();
    // A skip link beside it, then shortening link 1 splits 3-4 off.
    set(&mut p, "link.2", Some("5, 6"));
    assert!(p.same_net(w[4], w[5]) && !p.same_net(w[3], w[4]));
    set(&mut p, "link.1", Some("1-2"));
    assert!(p.same_net(w[0], w[1]) && !p.same_net(w[0], w[2]) && !p.same_net(w[2], w[3]));
    assert_ne!(p.number(w[0]), p.number(w[2]));
    p.check();
    // Relabelling 1 as 9 (Remove + Add of the same ID, as the planner does) takes it out of 1-2.
    let old = p.doc().terminals.get(&t[0]).unwrap().clone();
    let mut new = old.clone();
    new.label = Some("9".into());
    p.commit(vec![Op::RemoveTerminal { id: t[0], terminal: old }, Op::AddTerminal { id: t[0], terminal: new }]);
    assert!(!p.same_net(w[0], w[1]));
    p.check();
    // Undo of the link edit restores the joined net; clearing the key splits everything.
    set(&mut p, "link.2", None);
    assert!(!p.same_net(w[4], w[5]));
    p.check();
    // Unreadable or non-link keys join nothing.
    set(&mut p, "link.0", Some("1-6"));
    set(&mut p, "link.3", Some("1-x-6"));
    set(&mut p, "link.4.part_number", Some("2-6"));
    assert!(!p.same_net(w[4], w[5]));
    p.check();
}

/// Review of PR #57, 2: a 999-position strip with a long skip link and a full range builds its
/// nets in normal time, and the kept nets match a rebuild after an edit.
#[test]
#[ignore = "slow: 999 terminals, two incremental link commits (the build itself is timed below 5 s)"]
fn a_999_terminal_strip_with_long_links_builds_quickly() {
    let mut p = P::new();
    let (strip, t) = p.device(999);
    let skip: Vec<String> = (1..=999).step_by(2).map(|n| n.to_string()).collect();
    let set = |p: &mut P, key: &str, value: &str| {
        let before = p.doc().devices.get(&strip).unwrap().properties.get(key).cloned();
        p.commit(vec![Op::SetProperty { object: ObjectRef::Device(strip), key: key.into(), before, after: Some(value.into()) }]);
    };
    set(&mut p, "link.1", &skip.join(", "));
    set(&mut p, "link.2", "2-998");
    let start = std::time::Instant::now();
    let built = Connectivity::build(p.doc());
    let took = start.elapsed();
    assert!(took.as_secs_f64() < 5.0, "build took {took:?}");
    let c = p.doc().connectivity();
    assert_eq!(c.net_of(&ecad_model::Node::Terminal(t[0])), c.net_of(&ecad_model::Node::Terminal(t[998])));
    assert_eq!(c.net_of(&ecad_model::Node::Terminal(t[0])), c.net_of(&ecad_model::Node::Terminal(t[1])), "joined through 3");
    eprintln!("999-terminal build: {took:?}");
    assert_eq!(p.doc().connectivity().differs_from(&built), None);
}

/// Set a wire's value by its old property name (the tests' own words): `wire_number`,
/// `wire_number_fixed`, `potential`.
fn put(w: &mut Wire, key: &str, value: Option<&str>) {
    use ecad_model::text::Text;
    use ecad_model::wire::WireNumber;
    match key {
        "wire_number" => {
            let fixed = w.number_fixed();
            w.number = value.and_then(|v| Text::new(v).ok()).map(|text| WireNumber { text, fixed });
        }
        "wire_number_fixed" => {
            if let Some(n) = &mut w.number {
                n.fixed = value.is_some();
            }
        }
        "potential" => w.potential = value.and_then(|v| Text::new(v).ok()),
        other => panic!("no wire value {other}"),
    }
}
