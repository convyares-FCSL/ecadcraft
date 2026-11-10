//! Contract tests for brief 013: atomic ops, in-place transactions with rollback, and the scoped
//! invariant check against the full one.

use std::collections::BTreeSet;

use ecad_model::{
    Author, Cable, CableMember, Color, Component, Conductor, Definition, Document, End, Form, IdAllocator, Item, ItemKind, Layer, LayerId, Ledger,
    LengthUnit, ModelError, ObjectRef, Op, Page, PageId, Placement, Primitive, ProjectSettings, Pt, Representation, Style, Subject, SymbolInstance,
    Terminal, TerminalDef, Touched,
};

const ALL_OPS: [&str; 27] = [
    "AddPage",
    "RemovePage",
    "AddLayer",
    "RemoveLayer",
    "AddItem",
    "ModifyItem",
    "RemoveItem",
    "CreateComponent",
    "DeleteComponent",
    "AddTerminal",
    "RemoveTerminal",
    "LinkRepresentation",
    "ModifyRepresentation",
    "UnlinkRepresentation",
    "CreateConductor",
    "DeleteConductor",
    "AttachEnd",
    "CreateCable",
    "DeleteCable",
    "AddCableMember",
    "RemoveCableMember",
    "SetProperty",
    "EmbedDefinition",
    "RemoveDefinition",
    "SetProjectSettings",
    "NameVersion",
    "UnnameVersion",
];

fn style(layer: LayerId) -> Style {
    Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

fn line(page: PageId, layer: LayerId, x: f64) -> Item {
    Item { page, style: style(layer), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 0.0), b: Pt::new(x, 10.0) }) }
}

/// A valid document holding one of everything, built through the ledger, and the objects in it.
struct Base {
    ledger: Ledger,
    ids: IdAllocator,
    page: PageId,
    page_v: Page,
    layer: LayerId,
    layer_v: Layer,
    def: ecad_model::DefinitionId,
    def_v: Definition,
    comp: ecad_model::ComponentId,
    comp_v: Component,
    term: ecad_model::TerminalId,
    term_v: Terminal,
    sym: ecad_model::ItemId,
    item: ecad_model::ItemId,
    item_v: Item,
    rep_sym: ecad_model::RepresentationId,
    rep_sym_v: Representation,
    rep_wire: ecad_model::RepresentationId,
    rep_wire_v: Representation,
    wire: ecad_model::ConductorId,
    wire_v: Conductor,
    wire2: ecad_model::ConductorId,
    cable: ecad_model::CableId,
    cable_v: Cable,
}

fn base() -> Base {
    let mut ids = IdAllocator::seeded(13);
    let mut ledger = Ledger::new(5);
    let page = ids.fresh();
    let page_v = Page::new("1");
    let layer = ids.fresh();
    let layer_v = Layer { name: "0".into() };
    let def = ids.fresh();
    let def_v = Definition {
        bridges: Vec::new(),
        name: "K".into(),
        origin: None,
        graphics: vec![Primitive::Circle { center: Pt::new(0.0, 0.0), radius: 2.0 }],
        terminals: vec![TerminalDef { key: "1".into(), label: None, at: Pt::new(0.0, 2.0) }],
    };
    let comp = ids.fresh();
    let mut comp_v = Component { definition: Some(def), ..Component::default() };
    comp_v.properties.insert("tag".into(), "K1".into());
    let term = ids.fresh();
    let term_v = Terminal { component: comp, label: Some("1".into()), key: Some("1".into()) };
    let sym = ids.fresh();
    let sym_v = Item { page, style: style(layer), kind: ItemKind::Symbol(SymbolInstance { definition: def, placement: Placement::default() }) };
    let item = ids.fresh();
    let item_v = line(page, layer, 5.0);
    let rep_sym = ids.fresh();
    let rep_sym_v = Representation {
        subject: Subject::Component(comp),
        page,
        form: Form::Symbol,
        role: "main".into(),
        items: vec![sym],
        connection_points: Default::default(),
    };
    let wire = ids.fresh();
    let wire_v = Conductor { ends: [Some(term), None], ..Conductor::default() };
    let wire2 = ids.fresh();
    let rep_wire = ids.fresh();
    let rep_wire_v = Representation {
        subject: Subject::Conductor(wire),
        page,
        form: Form::Drawn,
        role: "route".into(),
        items: vec![item],
        connection_points: Default::default(),
    };
    let cable = ids.fresh();
    let cable_v = Cable { members: vec![CableMember { conductor: wire, core: None }], ..Cable::default() };
    ledger
        .commit(
            Author::User,
            "base",
            vec![
                Op::AddPage { id: page, page: page_v.clone(), index: 0 },
                Op::AddLayer { id: layer, layer: layer_v.clone() },
                Op::EmbedDefinition { id: def, definition: def_v.clone() },
                Op::CreateComponent { id: comp, component: comp_v.clone() },
                Op::AddTerminal { id: term, terminal: term_v.clone() },
                Op::AddItem { id: sym, item: sym_v },
                Op::AddItem { id: item, item: item_v.clone() },
                Op::LinkRepresentation { id: rep_sym, rep: rep_sym_v.clone() },
                Op::CreateConductor { id: wire, conductor: wire_v.clone() },
                Op::CreateConductor { id: wire2, conductor: Conductor::default() },
                Op::LinkRepresentation { id: rep_wire, rep: rep_wire_v.clone() },
                Op::CreateCable { id: cable, cable: cable_v.clone() },
            ],
        )
        .unwrap();
    ledger.commit(Author::User, "bookmark", vec![Op::NameVersion { name: "v".into(), at: 1, description: None }]).unwrap();
    Base {
        ledger,
        ids,
        page,
        page_v,
        layer,
        layer_v,
        def,
        def_v,
        comp,
        comp_v,
        term,
        term_v,
        sym,
        item,
        item_v,
        rep_sym,
        rep_sym_v,
        rep_wire,
        rep_wire_v,
        wire,
        wire_v,
        wire2,
        cable,
        cable_v,
    }
}

fn bytes(doc: &Document) -> Vec<u8> {
    serde_json::to_vec(doc).unwrap()
}

/// One op of every type that applies to the base document (invariants aside).
fn applicable(b: &mut Base) -> Vec<Op> {
    let moved = line(b.page, b.layer, 7.0);
    let mut rep_after = b.rep_wire_v.clone();
    rep_after.role = "moved".into();
    vec![
        Op::AddPage { id: b.ids.fresh(), page: Page::new("2"), index: 1 },
        Op::RemovePage { id: b.page, page: b.page_v.clone(), index: 0 },
        Op::AddLayer { id: b.ids.fresh(), layer: Layer { name: "x".into() } },
        Op::RemoveLayer { id: b.layer, layer: b.layer_v.clone() },
        Op::AddItem { id: b.ids.fresh(), item: line(b.page, b.layer, 1.0) },
        Op::ModifyItem { id: b.item, before: b.item_v.clone(), after: moved },
        Op::RemoveItem { id: b.item, item: b.item_v.clone() },
        Op::CreateComponent { id: b.ids.fresh(), component: Component::default() },
        Op::DeleteComponent { id: b.comp, component: b.comp_v.clone() },
        Op::AddTerminal { id: b.ids.fresh(), terminal: Terminal { component: b.comp, label: None, key: None } },
        Op::RemoveTerminal { id: b.term, terminal: b.term_v.clone() },
        Op::LinkRepresentation { id: b.ids.fresh(), rep: b.rep_wire_v.clone() },
        Op::ModifyRepresentation { id: b.rep_wire, before: b.rep_wire_v.clone(), after: rep_after },
        Op::UnlinkRepresentation { id: b.rep_sym, rep: b.rep_sym_v.clone() },
        Op::CreateConductor { id: b.ids.fresh(), conductor: Conductor { ends: [Some(b.term), Some(b.term)], ..Conductor::default() } },
        Op::DeleteConductor { id: b.wire, conductor: b.wire_v.clone() },
        Op::AttachEnd { conductor: b.wire2, end: End::B, before: None, after: Some(b.term) },
        Op::CreateCable { id: b.ids.fresh(), cable: b.cable_v.clone() },
        Op::DeleteCable { id: b.cable, cable: b.cable_v.clone() },
        Op::AddCableMember { cable: b.cable, index: 1, member: CableMember { conductor: b.wire2, core: Some("2".into()) } },
        Op::RemoveCableMember { cable: b.cable, index: 0, member: CableMember { conductor: b.wire, core: None } },
        Op::SetProperty { object: ObjectRef::Component(b.comp), key: "tag".into(), before: Some("K1".into()), after: None },
        Op::EmbedDefinition { id: b.ids.fresh(), definition: b.def_v.clone() },
        Op::RemoveDefinition { id: b.def, definition: b.def_v.clone() },
        Op::SetProjectSettings {
            before: ProjectSettings::default(),
            after: ProjectSettings { length_unit: LengthUnit::Inch, ..ProjectSettings::default() },
        },
        Op::NameVersion { name: "w".into(), at: 2, description: None },
        Op::UnnameVersion { name: "v".into(), at: 1, description: None },
    ]
}

/// Ops of every type whose preconditions fail on the base document, several of them only after
/// an earlier precondition passed.
fn failing(b: &mut Base) -> Vec<Op> {
    let missing_item = b.ids.fresh();
    let missing_conductor = b.ids.fresh();
    let missing_comp = b.ids.fresh();
    vec![
        // Not atomic before brief 013: the page went in before the index was rejected.
        Op::AddPage { id: b.ids.fresh(), page: Page::new("2"), index: 9 },
        Op::AddPage { id: b.page, page: b.page_v.clone(), index: 0 },
        Op::RemovePage { id: b.page, page: Page::new("other"), index: 0 },
        Op::RemovePage { id: b.page, page: b.page_v.clone(), index: 1 },
        Op::AddLayer { id: b.layer, layer: b.layer_v.clone() },
        Op::RemoveLayer { id: b.layer, layer: Layer { name: "other".into() } },
        Op::AddItem { id: b.item, item: b.item_v.clone() },
        Op::ModifyItem { id: b.item, before: line(b.page, b.layer, 99.0), after: b.item_v.clone() },
        Op::RemoveItem { id: missing_item, item: b.item_v.clone() },
        Op::CreateComponent { id: b.comp, component: Component::default() },
        Op::DeleteComponent { id: b.comp, component: Component::default() },
        Op::AddTerminal { id: b.term, terminal: b.term_v.clone() },
        Op::RemoveTerminal { id: b.ids.fresh(), terminal: b.term_v.clone() },
        Op::LinkRepresentation { id: b.rep_sym, rep: b.rep_sym_v.clone() },
        Op::ModifyRepresentation { id: b.rep_wire, before: b.rep_sym_v.clone(), after: b.rep_wire_v.clone() },
        Op::UnlinkRepresentation { id: b.ids.fresh(), rep: b.rep_sym_v.clone() },
        Op::CreateConductor { id: b.wire, conductor: Conductor::default() },
        Op::DeleteConductor { id: b.wire, conductor: Conductor::default() },
        Op::AttachEnd { conductor: b.wire, end: End::A, before: None, after: Some(b.term) },
        Op::AttachEnd { conductor: missing_conductor, end: End::A, before: None, after: Some(b.term) },
        Op::CreateCable { id: b.cable, cable: Cable::default() },
        Op::DeleteCable { id: b.ids.fresh(), cable: b.cable_v.clone() },
        Op::AddCableMember { cable: b.cable, index: 5, member: CableMember { conductor: b.wire2, core: None } },
        Op::RemoveCableMember { cable: b.cable, index: 0, member: CableMember { conductor: b.wire2, core: None } },
        Op::SetProperty { object: ObjectRef::Component(b.comp), key: "tag".into(), before: None, after: None },
        Op::SetProperty { object: ObjectRef::Component(missing_comp), key: "tag".into(), before: None, after: None },
        Op::EmbedDefinition { id: b.def, definition: b.def_v.clone() },
        Op::RemoveDefinition { id: b.def, definition: Definition { name: "other".into(), ..b.def_v.clone() } },
        Op::SetProjectSettings {
            before: ProjectSettings { length_unit: LengthUnit::Inch, ..ProjectSettings::default() },
            after: ProjectSettings::default(),
        },
        Op::NameVersion { name: "v".into(), at: 1, description: None },
        Op::UnnameVersion { name: "v".into(), at: 2, description: None },
    ]
}

fn names(ops: &[Op]) -> BTreeSet<&'static str> {
    ops.iter().map(Op::name).collect()
}

#[test]
fn the_tables_cover_every_op_type() {
    let mut b = base();
    let all: BTreeSet<_> = ALL_OPS.into_iter().collect();
    assert_eq!(names(&applicable(&mut b)), all);
    assert_eq!(names(&failing(&mut b)), all);
}

#[test]
fn every_op_is_atomic_on_failure() {
    let mut b = base();
    let before = b.ledger.document().clone();
    for op in failing(&mut b) {
        let mut doc = before.clone();
        let r = op.apply(&mut doc);
        assert!(matches!(r, Err(ModelError::Precondition(_))), "{}: {r:?}", op.name());
        assert_eq!(bytes(&doc), bytes(&before), "{} changed the document", op.name());
        assert_eq!(doc, before, "{}", op.name());
        doc.check_index().unwrap_or_else(|e| panic!("{} left the index stale: {e}", op.name()));
    }
}

#[test]
fn every_op_inverse_after_apply_restores_the_exact_document() {
    let mut b = base();
    let before = b.ledger.document().clone();
    for op in applicable(&mut b) {
        let mut doc = before.clone();
        op.apply(&mut doc).unwrap_or_else(|e| panic!("{}: {e}", op.name()));
        doc.check_index().unwrap_or_else(|e| panic!("{} left the index stale: {e}", op.name()));
        op.inverse().apply(&mut doc).unwrap_or_else(|e| panic!("inverse of {}: {e}", op.name()));
        assert_eq!(bytes(&doc), bytes(&before), "inverse of {}", op.name());
        assert_eq!(doc, before, "inverse of {}", op.name());
        doc.check_index().unwrap_or_else(|e| panic!("inverse of {} left the index stale: {e}", op.name()));
    }
}

/// Brief 021: the index built on threads (items and representations apart, as on open) equals
/// one built on a single thread, on a document with every kind of object and after every op.
#[test]
fn the_parallel_index_build_equals_a_serial_build() {
    let mut b = base();
    let before = b.ledger.document().clone();
    assert!(before.index_build_is_deterministic());
    for op in applicable(&mut b) {
        let mut doc = before.clone();
        op.apply(&mut doc).unwrap_or_else(|e| panic!("{}: {e}", op.name()));
        assert!(doc.index_build_is_deterministic(), "after {}", op.name());
    }
}

#[test]
fn an_intermediate_state_that_breaks_an_invariant_still_commits() {
    let mut b = base();
    // Rename the page by removing and re-adding it: in between, every item and representation on
    // it points at a missing page.
    let renamed = Page::new("renamed");
    let ops = vec![Op::RemovePage { id: b.page, page: b.page_v.clone(), index: 0 }, Op::AddPage { id: b.page, page: renamed.clone(), index: 0 }];
    b.ledger.commit(Author::User, "rename", ops).unwrap();
    assert_eq!(b.ledger.document().pages.get(&b.page), Some(&renamed));
    // Delete a terminal, then detach the conductor that was attached to it: in between, the
    // conductor is attached to a missing terminal.
    let ops = vec![
        Op::RemoveTerminal { id: b.term, terminal: b.term_v.clone() },
        Op::AttachEnd { conductor: b.wire, end: End::A, before: Some(b.term), after: None },
    ];
    b.ledger.commit(Author::User, "detach and delete", ops).unwrap();
    assert!(b.ledger.document().terminals.is_empty());
    b.ledger.document().check().unwrap();
}

#[test]
fn a_later_op_sees_what_an_earlier_op_created() {
    let mut b = base();
    let comp = b.ids.fresh();
    let term = b.ids.fresh();
    let wire = b.ids.fresh();
    let ops = vec![
        Op::CreateComponent { id: comp, component: Component::default() },
        Op::AddTerminal { id: term, terminal: Terminal { component: comp, label: None, key: None } },
        Op::CreateConductor { id: wire, conductor: Conductor::default() },
        Op::AttachEnd { conductor: wire, end: End::A, before: None, after: Some(term) },
        Op::AddCableMember { cable: b.cable, index: 1, member: CableMember { conductor: wire, core: None } },
    ];
    b.ledger.commit(Author::User, "chain", ops).unwrap();
    let doc = b.ledger.document();
    assert_eq!(doc.ends_at(term), vec![(wire, End::A)]);
    assert_eq!(doc.cables_of(wire), vec![b.cable]);
    assert_eq!(doc.terminals_of(comp).map(|(t, _)| t).collect::<Vec<_>>(), vec![term]);
}

#[test]
fn a_transaction_failing_at_op_three_leaves_the_document_byte_identical() {
    let mut b = base();
    let before = bytes(b.ledger.document());
    let head = b.ledger.head();
    let missing = b.ids.fresh();
    let ops = vec![
        Op::AddPage { id: b.ids.fresh(), page: Page::new("2"), index: 1 },
        Op::AttachEnd { conductor: b.wire2, end: End::A, before: None, after: Some(b.term) },
        Op::RemoveItem { id: missing, item: b.item_v.clone() },
        Op::AddLayer { id: b.ids.fresh(), layer: Layer { name: "never".into() } },
    ];
    let r = b.ledger.commit(Author::User, "fails at 3", ops);
    assert!(matches!(r, Err(ModelError::Precondition(_))), "{r:?}");
    assert_eq!(bytes(b.ledger.document()), before);
    assert_eq!(b.ledger.head(), head);
    b.ledger.document().check_index().unwrap();
    assert_eq!(b.ledger.document().ends_at(b.term), vec![(b.wire, End::A)]);
}

#[test]
fn a_transaction_failing_the_end_check_is_rolled_back() {
    let mut b = base();
    let before = bytes(b.ledger.document());
    // Every op applies, but the end state leaves the conductor attached to a missing terminal.
    let ops =
        vec![Op::AddItem { id: b.ids.fresh(), item: line(b.page, b.layer, 3.0) }, Op::RemoveTerminal { id: b.term, terminal: b.term_v.clone() }];
    let r = b.ledger.commit(Author::User, "dangling", ops);
    let Err(ModelError::Invariant(m)) = r else { panic!("{r:?}") };
    assert!(m.contains("attached to missing terminal"), "{m}");
    assert_eq!(bytes(b.ledger.document()), before);
    b.ledger.document().check_index().unwrap();
}

#[test]
fn queries_answer_from_the_index_as_a_scan_would() {
    let b = base();
    let doc = b.ledger.document();
    assert_eq!(doc.ends_at(b.term), vec![(b.wire, End::A)]);
    assert_eq!(doc.cables_of(b.wire), vec![b.cable]);
    assert!(doc.cables_of(b.wire2).is_empty());
    assert_eq!(doc.representation_of_item(b.sym).map(|(id, _)| id), Some(b.rep_sym));
    assert_eq!(doc.representation_of_item(b.item).map(|(id, _)| id), Some(b.rep_wire));
    assert_eq!(doc.representations_of(Subject::Component(b.comp)).map(|(id, _)| id).collect::<Vec<_>>(), vec![b.rep_sym]);
    // A deserialised document rebuilds its index.
    let back: Document = serde_json::from_slice(&bytes(doc)).unwrap();
    back.check_index().unwrap();
    assert_eq!(back.cables_of(b.wire), vec![b.cable]);
    assert!(doc.definitions.contains_key(&b.def));
}

#[test]
fn undo_and_redo_use_the_kept_stacks_and_restore_state() {
    let mut b = base();
    let s2 = b.ledger.document().clone();
    let ops = vec![Op::AddCableMember { cable: b.cable, index: 1, member: CableMember { conductor: b.wire2, core: None } }];
    b.ledger.commit(Author::User, "join", ops).unwrap();
    assert_eq!(b.ledger.undo_label(), Some("join"));
    b.ledger.undo(Author::User).unwrap();
    assert_eq!(b.ledger.document(), &s2);
    assert_eq!(b.ledger.redo_label(), Some("join"));
    assert_eq!(b.ledger.undo_label(), Some("bookmark"));
    b.ledger.redo(Author::User).unwrap();
    assert_eq!(b.ledger.document().cables_of(b.wire2), vec![b.cable]);
    let replayed = Ledger::replay(b.ledger.full_log().unwrap(), 0).unwrap();
    assert_eq!(replayed.undo_label(), b.ledger.undo_label());
    assert_eq!(replayed.redo_label(), b.ledger.redo_label());
    assert_eq!(replayed.document(), b.ledger.document());
    let verified = Ledger::replay_verified(b.ledger.full_log().unwrap(), 0).unwrap();
    assert_eq!(verified.document(), b.ledger.document());
}

#[test]
fn a_damaged_history_opens_at_its_last_good_transaction() {
    let b = base();
    let mut log = b.ledger.full_log().unwrap();
    let mut bad = log[1].clone();
    bad.seq = 3;
    bad.ops = vec![Op::AddLayer { id: LayerId(1), layer: Layer { name: "a".into() } }, Op::RemoveTerminal { id: b.term, terminal: b.term_v.clone() }];
    log.push(bad);
    let (ledger, failure) = Ledger::replay_prefix(log, 0);
    assert_eq!(ledger.head(), 2);
    assert_eq!(ledger.document(), b.ledger.document());
    ledger.document().check_index().unwrap();
    assert!(matches!(failure, Some((2, ModelError::Invariant(_)))), "{failure:?}");
}

// ---------------------------------------------------------------------------------------------
// Property test: random op sequences, valid and invalid, through both checks.

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn pick<T: Copy>(&mut self, v: &[T]) -> Option<T> {
        if v.is_empty() { None } else { v.get(self.below(v.len())).copied() }
    }
    fn pick_key<K: Copy + Ord, V>(&mut self, m: &std::collections::BTreeMap<K, V>) -> Option<K> {
        let n = m.len();
        if n == 0 { None } else { m.keys().nth(self.below(n)).copied() }
    }
}

/// Every ID ever allocated, per kind, so ops may name existing, removed or never-created objects.
#[derive(Default)]
struct Pools {
    pages: Vec<PageId>,
    layers: Vec<LayerId>,
    items: Vec<ecad_model::ItemId>,
    comps: Vec<ecad_model::ComponentId>,
    terms: Vec<ecad_model::TerminalId>,
    reps: Vec<ecad_model::RepresentationId>,
    conds: Vec<ecad_model::ConductorId>,
    cables: Vec<ecad_model::CableId>,
    defs: Vec<ecad_model::DefinitionId>,
}

struct Gen {
    rng: Rng,
    ids: IdAllocator,
    pools: Pools,
}

macro_rules! fresh_in {
    ($g:expr, $pool:ident) => {{
        let id = $g.ids.fresh();
        $g.pools.$pool.push(id);
        id
    }};
}

impl Gen {
    fn page(&mut self) -> PageId {
        if self.pools.pages.is_empty() || self.rng.chance(3) { fresh_in!(self, pages) } else { self.rng.pick(&self.pools.pages).unwrap_or(PageId(0)) }
    }
    fn layer(&mut self) -> LayerId {
        if self.pools.layers.is_empty() || self.rng.chance(3) {
            fresh_in!(self, layers)
        } else {
            self.rng.pick(&self.pools.layers).unwrap_or(LayerId(0))
        }
    }
    fn def(&mut self) -> ecad_model::DefinitionId {
        self.rng.pick(&self.pools.defs).unwrap_or_else(|| fresh_in!(self, defs))
    }
    fn comp(&mut self) -> ecad_model::ComponentId {
        self.rng.pick(&self.pools.comps).unwrap_or_else(|| fresh_in!(self, comps))
    }
    fn term(&mut self) -> Option<ecad_model::TerminalId> {
        if self.rng.chance(30) { None } else { self.rng.pick(&self.pools.terms) }
    }
    fn cond(&mut self) -> ecad_model::ConductorId {
        self.rng.pick(&self.pools.conds).unwrap_or_else(|| fresh_in!(self, conds))
    }

    fn item(&mut self, doc: &Document) -> Item {
        // Mostly on an existing page and layer, so items are often valid.
        let page = if self.rng.chance(90) { self.rng.pick_key(&doc.pages).unwrap_or_else(|| self.page()) } else { self.page() };
        let layer = if self.rng.chance(90) { self.rng.pick_key(&doc.layers).unwrap_or_else(|| self.layer()) } else { self.layer() };
        let kind = if self.rng.chance(30) {
            let scale = if self.rng.chance(5) { 0.0 } else { 1.0 };
            let definition = if self.rng.chance(90) { self.rng.pick_key(&doc.definitions).unwrap_or_else(|| self.def()) } else { self.def() };
            ItemKind::Symbol(SymbolInstance { definition, placement: Placement { scale, ..Placement::default() } })
        } else {
            let x = if self.rng.chance(3) { f64::NAN } else { self.rng.below(100) as f64 };
            ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 0.0), b: Pt::new(1.0, 1.0) })
        };
        Item { page, style: style(layer), kind }
    }

    fn rep(&mut self, doc: &Document) -> Representation {
        // Half the time, aim for a valid symbol representation of a component with a definition.
        if self.rng.chance(50)
            && let Some((cid, c)) = doc.components.iter().nth(self.rng.below(doc.components.len()))
            && let Some(def) = c.definition
            && let Some((iid, it)) = doc
                .items
                .iter()
                .find(|(i, it)| matches!(&it.kind, ItemKind::Symbol(s) if s.definition == def) && doc.representation_of_item(**i).is_none())
        {
            return Representation {
                subject: Subject::Component(*cid),
                page: it.page,
                form: Form::Symbol,
                role: "main".into(),
                items: vec![*iid],
                connection_points: Default::default(),
            };
        }
        // Sometimes a symbol of any definition with connection points of the component's own
        // terminals (I14 as relaxed for brief 018: valid with any embedded definition).
        if self.rng.chance(30)
            && let Some((cid, _)) = doc.components.iter().nth(self.rng.below(doc.components.len()))
            && let Some(t) = doc.terminals_of(*cid).map(|(t, _)| t).next()
            && let Some((iid, it)) =
                doc.items.iter().find(|(i, it)| matches!(&it.kind, ItemKind::Symbol(_)) && doc.representation_of_item(**i).is_none())
        {
            let mut connection_points = std::collections::BTreeMap::new();
            connection_points.insert(t, Pt::new(0.0, 10.0));
            return Representation {
                subject: Subject::Component(*cid),
                page: it.page,
                form: Form::Symbol,
                role: "no".into(),
                items: vec![*iid],
                connection_points,
            };
        }
        let subject = match self.rng.below(3) {
            0 => Subject::Component(self.comp()),
            1 => Subject::Conductor(self.cond()),
            _ => Subject::Cable(self.rng.pick(&self.pools.cables).unwrap_or_else(|| fresh_in!(self, cables))),
        };
        let page = self.rng.pick_key(&doc.pages).unwrap_or_else(|| self.page());
        let mut items = Vec::new();
        for _ in 0..self.rng.below(3) {
            // Prefer unowned items on the same page.
            let candidate = if self.rng.chance(70) {
                doc.items
                    .iter()
                    .filter(|(i, it)| it.page == page && doc.representation_of_item(**i).is_none())
                    .map(|(i, _)| *i)
                    .nth(self.rng.below(3))
            } else {
                self.rng.pick(&self.pools.items)
            };
            if let Some(i) = candidate {
                items.push(i);
            }
        }
        let mut connection_points = std::collections::BTreeMap::new();
        if let (Subject::Component(c), true) = (subject, self.rng.chance(50)) {
            let t = if self.rng.chance(80) { doc.terminals_of(c).map(|(t, _)| t).next() } else { self.term() };
            if let Some(t) = t {
                let y = if self.rng.chance(5) { f64::INFINITY } else { 1.0 };
                connection_points.insert(t, Pt::new(0.0, y));
            }
        }
        let form = if self.rng.chance(15) { Form::Symbol } else { Form::Drawn };
        Representation { subject, page, form, role: "r".into(), items, connection_points }
    }

    fn definition(&mut self) -> Definition {
        let mut terminals = vec![TerminalDef { key: "1".into(), label: None, at: Pt::new(0.0, 1.0) }];
        if self.rng.chance(20) {
            let key = if self.rng.chance(50) { "1" } else { "2" };
            terminals.push(TerminalDef { key: key.into(), label: None, at: Pt::new(0.0, -1.0) });
        }
        let r = if self.rng.chance(5) { f64::NAN } else { 1.0 };
        Definition {
            bridges: Vec::new(),
            name: "d".into(),
            origin: None,
            graphics: vec![Primitive::Circle { center: Pt::new(0.0, 0.0), radius: r }],
            terminals,
        }
    }

    /// One op against `doc`. With small probability its recorded `before` state is wrong.
    fn op(&mut self, doc: &Document, head: u64) -> Op {
        let spoil = self.rng.chance(4);
        // Weighted so that additions outnumber removals and the document grows.
        let kind = self.rng.below(40);
        match kind {
            0..=1 => {
                let id = if spoil { self.rng.pick(&self.pools.pages).unwrap_or_else(|| fresh_in!(self, pages)) } else { fresh_in!(self, pages) };
                let index = self.rng.below(doc.page_order.len() + 1 + usize::from(spoil));
                Op::AddPage { id, page: Page::new("p"), index }
            }
            2 => match self.rng.pick(&doc.page_order) {
                Some(id) => {
                    let index = doc.page_order.iter().position(|p| *p == id).unwrap_or(0) + usize::from(spoil);
                    Op::RemovePage { id, page: doc.pages.get(&id).cloned().unwrap_or(Page::new("?")), index }
                }
                None => Op::AddPage { id: fresh_in!(self, pages), page: Page::new("p"), index: 0 },
            },
            3 => Op::AddLayer { id: fresh_in!(self, layers), layer: Layer { name: "l".into() } },
            4 => match self.rng.pick_key(&doc.layers) {
                Some(id) => {
                    let name = if spoil { "spoilt".into() } else { doc.layers.get(&id).map(|l| l.name.clone()).unwrap_or_default() };
                    Op::RemoveLayer { id, layer: Layer { name } }
                }
                None => Op::AddLayer { id: fresh_in!(self, layers), layer: Layer { name: "l".into() } },
            },
            5..=9 => {
                let id = if spoil { self.rng.pick(&self.pools.items).unwrap_or_else(|| fresh_in!(self, items)) } else { fresh_in!(self, items) };
                Op::AddItem { id, item: self.item(doc) }
            }
            10..=11 => match self.rng.pick_key(&doc.items) {
                Some(id) => {
                    let before = if spoil { self.item(doc) } else { doc.items.get(&id).cloned().unwrap_or_else(|| self.item(doc)) };
                    let mut after = self.item(doc);
                    if self.rng.chance(50) {
                        // Keep page and kind, change geometry only: the common edit.
                        after = before.clone();
                        if let ItemKind::Primitive(Primitive::Line { a, .. }) = &mut after.kind {
                            a.x += 1.0;
                        }
                    }
                    Op::ModifyItem { id, before, after }
                }
                None => Op::AddItem { id: fresh_in!(self, items), item: self.item(doc) },
            },
            12 => match self.rng.pick_key(&doc.items) {
                Some(id) => Op::RemoveItem { id, item: doc.items.get(&id).cloned().unwrap_or_else(|| self.item(doc)) },
                None => Op::AddItem { id: fresh_in!(self, items), item: self.item(doc) },
            },
            13..=14 => {
                let definition = if self.rng.chance(50) { None } else { Some(self.rng.pick_key(&doc.definitions).unwrap_or_else(|| self.def())) };
                Op::CreateComponent { id: fresh_in!(self, comps), component: Component { definition, ..Component::default() } }
            }
            15 => match self.rng.pick_key(&doc.components) {
                Some(id) => Op::DeleteComponent { id, component: doc.components.get(&id).cloned().unwrap_or_default() },
                None => Op::CreateComponent { id: fresh_in!(self, comps), component: Component::default() },
            },
            16..=17 => {
                let component = if self.rng.chance(85) { self.rng.pick_key(&doc.components).unwrap_or_else(|| self.comp()) } else { self.comp() };
                Op::AddTerminal { id: fresh_in!(self, terms), terminal: Terminal { component, label: None, key: None } }
            }
            18 => match self.rng.pick_key(&doc.terminals) {
                Some(id) => Op::RemoveTerminal {
                    id,
                    terminal: doc.terminals.get(&id).cloned().unwrap_or(Terminal { component: self.comp(), label: None, key: None }),
                },
                None => Op::AddTerminal { id: fresh_in!(self, terms), terminal: Terminal { component: self.comp(), label: None, key: None } },
            },
            19..=21 => Op::LinkRepresentation { id: fresh_in!(self, reps), rep: self.rep(doc) },
            22 => match self.rng.pick_key(&doc.representations) {
                Some(id) => {
                    let before = doc.representations.get(&id).cloned().unwrap_or_else(|| self.rep(doc));
                    let after = if self.rng.chance(50) {
                        let mut a = before.clone();
                        a.items.pop();
                        a
                    } else {
                        self.rep(doc)
                    };
                    Op::ModifyRepresentation { id, before, after }
                }
                None => Op::LinkRepresentation { id: fresh_in!(self, reps), rep: self.rep(doc) },
            },
            23 => match self.rng.pick_key(&doc.representations) {
                Some(id) => Op::UnlinkRepresentation { id, rep: doc.representations.get(&id).cloned().unwrap_or_else(|| self.rep(doc)) },
                None => Op::LinkRepresentation { id: fresh_in!(self, reps), rep: self.rep(doc) },
            },
            24..=26 => {
                let ends = [self.term(), self.term()];
                Op::CreateConductor { id: fresh_in!(self, conds), conductor: Conductor { ends, ..Conductor::default() } }
            }
            27 => match self.rng.pick_key(&doc.conductors) {
                Some(id) => Op::DeleteConductor { id, conductor: doc.conductors.get(&id).cloned().unwrap_or_default() },
                None => Op::CreateConductor { id: fresh_in!(self, conds), conductor: Conductor::default() },
            },
            28..=29 => match self.rng.pick_key(&doc.conductors) {
                Some(conductor) => {
                    let end = if self.rng.chance(50) { End::A } else { End::B };
                    let current = doc.conductors.get(&conductor).and_then(|c| c.end(end));
                    let before = if spoil { self.term() } else { current };
                    Op::AttachEnd { conductor, end, before, after: self.term() }
                }
                None => Op::CreateConductor { id: fresh_in!(self, conds), conductor: Conductor::default() },
            },
            30 => {
                let mut members = Vec::new();
                for _ in 0..self.rng.below(3) {
                    members.push(CableMember { conductor: self.cond(), core: None });
                }
                Op::CreateCable { id: fresh_in!(self, cables), cable: Cable { members, ..Cable::default() } }
            }
            31 => match self.rng.pick_key(&doc.cables) {
                Some(id) => Op::DeleteCable { id, cable: doc.cables.get(&id).cloned().unwrap_or_default() },
                None => Op::CreateCable { id: fresh_in!(self, cables), cable: Cable::default() },
            },
            32..=33 => match self.rng.pick_key(&doc.cables) {
                Some(cable) => {
                    let len = doc.cables.get(&cable).map_or(0, |c| c.members.len());
                    let index = self.rng.below(len + 1 + usize::from(spoil));
                    Op::AddCableMember { cable, index, member: CableMember { conductor: self.cond(), core: None } }
                }
                None => Op::CreateCable { id: fresh_in!(self, cables), cable: Cable::default() },
            },
            34 => {
                let found = doc.cables.iter().filter(|(_, c)| !c.members.is_empty()).map(|(id, c)| (*id, c.members.clone())).nth(self.rng.below(2));
                match found {
                    Some((cable, members)) => {
                        let index = self.rng.below(members.len());
                        let member = members.get(index).cloned().unwrap_or(CableMember { conductor: self.cond(), core: None });
                        let index = index + usize::from(spoil);
                        Op::RemoveCableMember { cable, index, member }
                    }
                    None => Op::CreateCable { id: fresh_in!(self, cables), cable: Cable::default() },
                }
            }
            35 => match self.rng.pick_key(&doc.components) {
                Some(id) => {
                    let current = doc.components.get(&id).and_then(|c| c.properties.get("tag").cloned());
                    let before = if spoil { Some("spoilt".into()) } else { current };
                    // Written two ways (`K3`, `-K03`) so the tag index sees the same tag under two values.
                    let after = match self.rng.below(3) {
                        0 => None,
                        1 => Some(format!("K{}", self.rng.below(9))),
                        _ => Some(format!("-K0{}", self.rng.below(9))),
                    };
                    Op::SetProperty { object: ObjectRef::Component(id), key: "tag".into(), before, after }
                }
                None => Op::CreateComponent { id: fresh_in!(self, comps), component: Component::default() },
            },
            36..=37 => Op::EmbedDefinition { id: fresh_in!(self, defs), definition: self.definition() },
            38 => match self.rng.pick_key(&doc.definitions) {
                Some(id) => Op::RemoveDefinition { id, definition: doc.definitions.get(&id).cloned().unwrap_or_else(|| self.definition()) },
                None => Op::EmbedDefinition { id: fresh_in!(self, defs), definition: self.definition() },
            },
            _ => match self.rng.below(3) {
                0 => {
                    let before =
                        if spoil { ProjectSettings { length_unit: LengthUnit::Inch, ..ProjectSettings::default() } } else { doc.settings.clone() };
                    let after = ProjectSettings {
                        length_unit: if self.rng.chance(50) { LengthUnit::Inch } else { LengthUnit::Millimetre },
                        // Brief 018's tag settings change with it sometimes.
                        tag_padding: if self.rng.chance(30) { 3 } else { ecad_model::DEFAULT_TAG_PADDING },
                        ..ProjectSettings::default()
                    };
                    Op::SetProjectSettings { before, after }
                }
                1 => {
                    // Sometimes names a transaction that does not precede this one.
                    let at = self.rng.below(head as usize + 1 + usize::from(spoil) * 2) as u64;
                    let name = if spoil && !doc.versions.is_empty() {
                        doc.versions.keys().next().cloned().unwrap_or_default()
                    } else {
                        format!("v{}", self.rng.next() % 1000)
                    };
                    Op::NameVersion { name, at, description: None }
                }
                _ => match doc.versions.iter().next() {
                    Some((name, at)) => Op::UnnameVersion { name: name.clone(), at: *at + u64::from(spoil), description: None },
                    None => Op::NameVersion { name: "v".into(), at: 0, description: None },
                },
            },
        }
    }
}

#[derive(Default, Debug)]
struct Tally {
    committed: usize,
    precondition_failures: usize,
    invariant_failures: usize,
    undos: usize,
    redos: usize,
}

/// Run one random sequence; returns what happened.
fn sequence(seed: u64, tally: &mut Tally) {
    let mut g = Gen { rng: Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1), ids: IdAllocator::seeded(seed), pools: Pools::default() };
    let mut ledger = Ledger::new(seed);
    let steps = 6 + g.rng.below(10);
    for step in 0..steps {
        if step > 0 && g.rng.chance(10) {
            let before = ledger.document().clone();
            let (r, redo) = if g.rng.chance(50) { (ledger.undo(Author::User), false) } else { (ledger.redo(Author::User), true) };
            match r {
                Ok(_) => {
                    if redo {
                        tally.redos += 1
                    } else {
                        tally.undos += 1
                    }
                    ledger.document().check().unwrap();
                }
                Err(ModelError::NothingTo(_)) => assert_eq!(ledger.document(), &before),
                Err(e) => panic!("seed {seed}: undo/redo of a valid history failed: {e}"),
            }
            continue;
        }
        let before = ledger.document().clone();
        let head = ledger.head();
        // Generate on a scratch copy so op N can build on what ops before it did.
        let mut scratch = before.clone();
        let mut ops = Vec::new();
        let mut applies = true;
        for _ in 0..1 + g.rng.below(4) {
            let op = g.op(&scratch, head);
            if op.apply(&mut scratch).is_err() {
                applies = false;
                ops.push(op);
                break;
            }
            ops.push(op);
        }
        let bookmarks_ok = ops.iter().all(|op| !matches!(op, Op::NameVersion { at, .. } if *at > head));
        let expected = if applies {
            // Both checks on the end state, from a valid start.
            let full = scratch.check();
            let scoped = scratch.check_touched(&Touched::of(&ops));
            assert_eq!(full.is_ok(), scoped.is_ok(), "seed {seed} step {step}: full {full:?}, scoped {scoped:?}, ops {ops:#?}");
            scratch.check_index().unwrap_or_else(|e| panic!("seed {seed} step {step}: {e}"));
            full.is_ok() && bookmarks_ok
        } else {
            false
        };
        let r = ledger.commit(Author::User, "random", ops.clone());
        assert_eq!(r.is_ok(), expected, "seed {seed} step {step}: commit {r:?}, ops {ops:#?}");
        match r {
            Ok(_) => {
                tally.committed += 1;
                assert_eq!(ledger.document(), &scratch);
            }
            Err(e) => {
                match e {
                    ModelError::Precondition(_) => tally.precondition_failures += 1,
                    _ => tally.invariant_failures += 1,
                }
                assert_eq!(ledger.document(), &before, "seed {seed} step {step}: rollback");
                ledger.document().check_index().unwrap_or_else(|e| panic!("seed {seed} step {step}: {e}"));
            }
        }
    }
    let replayed = Ledger::replay_verified(ledger.full_log().unwrap(), 0).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
    assert_eq!(replayed.document(), ledger.document(), "seed {seed}: replay");
    assert_eq!(replayed.undo_label(), ledger.undo_label());
    assert_eq!(replayed.redo_label(), ledger.redo_label());
}

#[test]
#[ignore = "slow: 10,000 random sequences, about 4 s"]
fn the_scoped_check_agrees_with_the_full_check_on_random_sequences() {
    let mut tally = Tally::default();
    for seed in 1..=10_000u64 {
        sequence(seed, &mut tally);
    }
    // Both outcomes, and both kinds of failure, must have been exercised in volume.
    assert!(tally.committed > 15_000, "{tally:?}");
    assert!(tally.invariant_failures > 30_000, "{tally:?}");
    assert!(tally.precondition_failures > 400, "{tally:?}");
    assert!(tally.undos > 1_500 && tally.redos > 150, "{tally:?}");
}

#[test]
fn a_refused_transaction_holding_nan_rolls_back() {
    // Regression: NaN never equals itself, so undoing such an op by comparing recorded values
    // would fail and leave the document half-changed.
    let mut b = base();
    let before = bytes(b.ledger.document());
    let mut nan = line(b.page, b.layer, f64::NAN);
    nan.style.lineweight = 13;
    let mut moved = b.item_v.clone();
    moved.kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(f64::NAN, 0.0), b: Pt::new(1.0, 1.0) });
    let ops = vec![
        Op::AddItem { id: b.ids.fresh(), item: nan },
        Op::ModifyItem { id: b.item, before: b.item_v.clone(), after: moved },
        Op::SetProperty { object: ObjectRef::Component(b.comp), key: "tag".into(), before: Some("K1".into()), after: None },
    ];
    let r = b.ledger.commit(Author::User, "nan", ops);
    assert!(matches!(r, Err(ModelError::Invariant(_))), "{r:?}");
    assert_eq!(bytes(b.ledger.document()), before);
    b.ledger.document().check_index().unwrap();
}

/// What a preview must leave exactly as it was: the document's bytes and `==`, its indexes, the
/// log, the undo and redo stacks and the ledger's health.
struct Snapshot {
    bytes: Vec<u8>,
    doc: Document,
    log: usize,
    head: u64,
    undo: Option<String>,
    redo: Option<String>,
    ends: Vec<(ecad_model::ConductorId, End)>,
    cables: Vec<ecad_model::CableId>,
}

fn snapshot(b: &Base) -> Snapshot {
    let l = &b.ledger;
    Snapshot {
        bytes: bytes(l.document()),
        doc: l.document().clone(),
        log: l.log().len(),
        head: l.head(),
        undo: l.undo_label().map(str::to_owned),
        redo: l.redo_label().map(str::to_owned),
        ends: l.document().ends_at(b.term),
        cables: l.document().cables_of(b.wire),
    }
}

fn assert_as_before(b: &Base, s: &Snapshot, case: &str) {
    let l = &b.ledger;
    assert_eq!(bytes(l.document()), s.bytes, "{case}: document bytes");
    assert_eq!(*l.document(), s.doc, "{case}: document");
    l.document().check_index().unwrap_or_else(|e| panic!("{case}: indexes stale: {e}"));
    assert_eq!(l.document().ends_at(b.term), s.ends, "{case}: terminal index");
    assert_eq!(l.document().cables_of(b.wire), s.cables, "{case}: cable index");
    assert_eq!(l.log().len(), s.log, "{case}: log length");
    assert_eq!(l.head(), s.head, "{case}: head");
    assert_eq!(l.undo_label(), s.undo.as_deref(), "{case}: undo stack");
    assert_eq!(l.redo_label(), s.redo.as_deref(), "{case}: redo stack");
    assert!(l.diagnostics().is_empty(), "{case}: {:?}", l.diagnostics());
    assert!(l.replay_matches().unwrap(), "{case}: the log no longer replays to the document");
    assert_eq!(l.state_at(s.head).unwrap(), s.doc, "{case}: state at head");
}

/// Brief 014, acceptance 4, from the outside: a panic inside the preview closure of
/// `Ledger::with_applied` and `Document::with_applied`, caught with `catch_unwind`, leaves the
/// document, its indexes and the log (with its undo and redo stacks) exactly as they were, for a
/// transaction whose end state is valid (so the closure runs and panics) and for every single op
/// type (the closure panics when it runs; refusals must leave things alone too).
#[test]
fn a_panic_inside_the_preview_closure_leaves_document_indexes_and_log_unchanged() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let mut b = base();
    // Leave something on the redo stack as well.
    b.ledger.commit(Author::User, "extra", vec![Op::AddLayer { id: b.ids.fresh(), layer: Layer { name: "e".into() } }]).unwrap();
    b.ledger.undo(Author::User).unwrap();
    assert!(b.ledger.redo_label().is_some());
    let before = snapshot(&b);

    let valid = vec![
        Op::AttachEnd { conductor: b.wire2, end: End::B, before: None, after: Some(b.term) },
        Op::AddItem { id: b.ids.fresh(), item: line(b.page, b.layer, 1.0) },
        Op::ModifyItem { id: b.item, before: b.item_v.clone(), after: line(b.page, b.layer, 7.0) },
        Op::AddCableMember { cable: b.cable, index: 1, member: CableMember { conductor: b.wire2, core: None } },
        Op::SetProperty { object: ObjectRef::Component(b.comp), key: "tag".into(), before: Some("K1".into()), after: Some("K2".into()) },
        Op::AddPage { id: b.ids.fresh(), page: Page::new("2"), index: 1 },
    ];
    let mut cases: Vec<(String, Vec<Op>)> = vec![("valid transaction".into(), valid.clone())];
    for op in applicable(&mut b) {
        cases.push((format!("single {}", op.name()), vec![op]));
    }
    let mut ran = 0;
    for (case, ops) in &cases {
        let mut closure_ran = false;
        let r = catch_unwind(AssertUnwindSafe(|| {
            b.ledger.with_applied(ops, |d| {
                closure_ran = true;
                // The closure sees the change before it panics.
                let _ = d.check_index();
                std::panic::resume_unwind(Box::new(format!("panic in the preview of {case}")));
            })
        }));
        if closure_ran {
            ran += 1;
            assert!(r.is_err(), "{case}: the panic must propagate");
        } else {
            assert!(matches!(r, Ok(Err(_))), "{case}: the closure did not run, so the ops were refused");
        }
        assert_as_before(&b, &before, &format!("Ledger::with_applied, {case}"));

        // The plain document form.
        let mut d = b.ledger.document().clone();
        let r = catch_unwind(AssertUnwindSafe(|| d.with_applied(ops, |_| std::panic::resume_unwind(Box::new("boom")))));
        assert_eq!(r.is_err(), closure_ran, "{case}: the document form behaves as the ledger form");
        assert_eq!(bytes(&d), before.bytes, "Document::with_applied, {case}");
        assert_eq!(d, before.doc, "Document::with_applied, {case}");
        d.check_index().unwrap_or_else(|e| panic!("Document::with_applied, {case}: {e}"));
    }
    assert!(ran >= 2, "the closure ran (and panicked) in only {ran} cases");

    // The ledger carries on: a preview without a panic, a redo, and a commit of the same ops.
    let pages = b.ledger.with_applied(&valid, |d| d.pages.len()).unwrap();
    assert_eq!(pages, 2);
    assert_as_before(&b, &before, "after a normal preview");
    b.ledger.redo(Author::User).unwrap();
    b.ledger.commit(Author::User, "valid", valid).unwrap();
    assert!(b.ledger.replay_matches().unwrap());
    b.ledger.document().check_index().unwrap();
}

/// Brief 020a, Restore: the ops [`ecad_model::diff::ops_between`] gives turn the live document
/// into the state at any earlier point exactly as undoing every step after it does (versions
/// aside, which Restore never touches), and they commit as an ordinary transaction.
#[test]
fn a_state_diff_restores_exactly_what_undoing_the_steps_after_the_point_restores() {
    let mut points = 0;
    for seed in 1..=150u64 {
        let mut g = Gen { rng: Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1), ids: IdAllocator::seeded(seed), pools: Pools::default() };
        let mut ledger = Ledger::new(seed);
        for step in 0..14 {
            if step > 0 && g.rng.chance(15) {
                let _ = if g.rng.chance(50) { ledger.undo(Author::User) } else { ledger.redo(Author::User) };
                continue;
            }
            let mut scratch = ledger.document().clone();
            let mut ops = Vec::new();
            for _ in 0..1 + g.rng.below(4) {
                let op = g.op(&scratch, ledger.head());
                if op.apply(&mut scratch).is_err() {
                    break;
                }
                ops.push(op);
            }
            if !ops.is_empty() {
                let _ = ledger.commit(Author::User, "random", ops);
            }
        }
        let live = ledger.document().clone();
        let head = ledger.head();
        let log = ledger.full_log().unwrap();
        for k in 0..head {
            let target = ledger.state_at(k).unwrap();
            let walk: Vec<Op> = log
                .iter()
                .rev()
                .take_while(|t| t.seq > k)
                .flat_map(|t| t.ops.iter().rev().filter(|o| !matches!(o, Op::NameVersion { .. } | Op::UnnameVersion { .. })).map(Op::inverse))
                .collect();
            let diff = ecad_model::diff::ops_between(&live, &target, &|_| true);
            let by_walk = ledger.preview(&walk).unwrap_or_else(|e| panic!("seed {seed} k {k}: walk {e}"));
            let by_diff = ledger.preview(&diff).unwrap_or_else(|e| panic!("seed {seed} k {k}: diff {e}: {diff:#?}"));
            assert_eq!(by_diff, by_walk, "seed {seed} k {k}");
            let mut want = target.clone();
            want.versions = live.versions.clone();
            assert_eq!(by_diff, want, "seed {seed} k {k}: the target state");
            // It scales with what differs: nothing differs, no ops (however many steps lie between).
            let mut same = live.clone();
            same.versions = want.versions.clone();
            assert_eq!(diff.is_empty(), same == want, "seed {seed} k {k}: {} ops", diff.len());
            if !diff.is_empty() {
                let mut l = ledger.clone();
                l.commit(Author::User, "Restore", diff).unwrap_or_else(|e| panic!("seed {seed} k {k}: commit {e}"));
                assert_eq!(l.document(), &want);
            }
            points += 1;
        }
    }
    assert!(points > 500, "{points} points restored");
}
