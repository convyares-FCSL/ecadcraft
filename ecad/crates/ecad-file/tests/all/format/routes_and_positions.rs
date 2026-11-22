//! Lane 072 D (design 072 F31): routes are kept in their page's chunk and come back the same when
//! the project is opened, from the snapshot and from the log.

use crate::common::*;
use crate::container::files::TempDir;
use ecad_file::{Project, Store};
use ecad_model::*;

#[test]
fn routes_come_back_from_the_snapshot_and_the_log() {
    let dir = TempDir::new("072d-routes");
    let mut f = fixture();
    let (r1, r2): (RouteId, RouteId) = (f.ids.fresh(), f.ids.fresh());
    let a = Route::new(f.page, vec![Pt::new(0.0, 0.0), Pt::new(40.0, 0.0)], [RouteEnd::Free, RouteEnd::Free]);
    let tee = Route::new(f.page, vec![Pt::new(20.0, 30.0), Pt::new(20.0, 0.0)], [RouteEnd::Free, RouteEnd::On { route: r1, at: Pt::new(20.0, 0.0) }]);
    f.ledger.commit(Author::User, "draw", vec![Op::AddRoute { id: r1, route: a.clone() }, Op::AddRoute { id: r2, route: tee }]).unwrap();
    // Snapshot every transaction for one file, never for the other: both open the same routes.
    let (lo, hi) = ecad_file::schema::SNAPSHOT_EVERY_RANGE;
    for (name, every) in [("snap.ecad", lo), ("log.ecad", hi)] {
        let p = dir.path(name);
        Project::create(Store::create(&p).unwrap(), PROJECT, &f.ledger, Some(1), WRITER, every).unwrap();
        let opened = Project::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(opened.ledger.document().routes, f.ledger.document().routes, "{name}");
    }
}

#[test]
fn core_positions_come_back_from_the_snapshot_and_the_log() {
    // Owner ruling, 2026-11-18: a core's position is stored as written, never worked out on read.
    let dir = TempDir::new("072d-cores");
    let mut f = fixture();
    let (r, k): (RouteId, CableId) = (f.ids.fresh(), f.ids.fresh());
    let loose = Route::new(f.page, vec![Pt::new(0.0, 0.0), Pt::new(40.0, 0.0)], [RouteEnd::Free, RouteEnd::Free]);
    // Design 072 §4.5 (c1): a core's label, colours and purpose come back as written too.
    let on_line = Core { position: CorePosition::Route { route: r, landed: None }, ..Core::unplaced(CoreLabel::new("1").ok()) };
    let spare = Core {
        label: CoreLabel::new("2").ok(),
        colours: ecad_model::objects::wire::Colours::parse("BN"),
        purpose: Purpose::Spare,
        position: CorePosition::Unplaced,
        through: Vec::new(),
    };
    f.ledger
        .commit(
            Author::User,
            "slash",
            vec![Op::AddRoute { id: r, route: loose }, Op::CreateCable { id: k, cable: Cable { cores: vec![on_line, spare], ..Cable::default() } }],
        )
        .unwrap();
    let (lo, hi) = ecad_file::schema::SNAPSHOT_EVERY_RANGE;
    for (name, every) in [("snap.ecad", lo), ("log.ecad", hi)] {
        let p = dir.path(name);
        Project::create(Store::create(&p).unwrap(), PROJECT, &f.ledger, Some(1), WRITER, every).unwrap();
        let opened = Project::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(opened.ledger.document().cables, f.ledger.document().cables, "{name}");
    }
}

#[test]
fn number_by_is_stored_as_a_word_and_comes_back_from_the_snapshot_and_the_log() {
    // Design 072 F40: `wiring.number_by` is `net` (left out of the file) or `conductor`.
    use ecad_file::records::WiringSettings as Rec;
    use ecad_model::connectivity::wiring::NumberBy;
    let mut w = ecad_model::connectivity::wiring::WiringSettings::default();
    let net = serde_json::to_value(Rec::from(&w)).unwrap();
    assert!(net.get("number_by").is_none(), "the default is left out: {net}");
    w.number_by = NumberBy::Conductor;
    let mut v = serde_json::to_value(Rec::from(&w)).unwrap();
    assert_eq!(v["number_by"], "conductor");
    let back: ecad_model::connectivity::wiring::WiringSettings = serde_json::from_value::<Rec>(v.clone()).unwrap().into();
    assert_eq!(back.number_by, NumberBy::Conductor);
    v["number_by"] = "wire".into();
    let err = serde_json::from_value::<Rec>(v).unwrap_err().to_string();
    assert!(err.contains("`wire`") && err.contains("conductor"), "an unknown word is a read error naming the words: {err}");

    let dir = TempDir::new("072d-number-by");
    let mut f = fixture();
    let before = f.ledger.document().settings.clone();
    let mut after = before.clone();
    after.wiring.number_by = NumberBy::Conductor;
    f.ledger.commit(Author::User, "number by", vec![Op::SetProjectSettings { before, after }]).unwrap();
    let (lo, hi) = ecad_file::schema::SNAPSHOT_EVERY_RANGE;
    for (name, every) in [("snap.ecad", lo), ("log.ecad", hi)] {
        let p = dir.path(name);
        Project::create(Store::create(&p).unwrap(), PROJECT, &f.ledger, Some(1), WRITER, every).unwrap();
        let opened = Project::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(opened.ledger.document().settings.wiring.number_by, NumberBy::Conductor, "{name}");
    }
}

#[test]
fn accepts_comes_back_and_a_hostile_count_is_a_read_error() {
    // Design 072 F38: a point's and a definition terminal's `accepts`, 1 to 64, left out at two.
    use ecad_file::records::{ConnectionPoint as Rec, ConnectionPointDef as DefRec};
    let dir = TempDir::new("072d-accepts");
    let mut f = fixture();
    let (d, t): (DeviceId, ConnectionPointId) = (f.ids.fresh(), f.ids.fresh());
    let bar = ConnectionPoint { owner: d.into(), label: Some("PE".into()), key: Some("PE".parse().unwrap()), accepts: Accepts::new(12).unwrap() };
    f.ledger
        .commit(
            Author::User,
            "bar",
            vec![Op::CreateDevice { id: d, device: Device::default() }, Op::AddConnectionPoint { id: t, connection_point: bar.clone() }],
        )
        .unwrap();
    let (lo, hi) = ecad_file::schema::SNAPSHOT_EVERY_RANGE;
    for (name, every) in [("snap.ecad", lo), ("log.ecad", hi)] {
        let p = dir.path(name);
        Project::create(Store::create(&p).unwrap(), PROJECT, &f.ledger, Some(1), WRITER, every).unwrap();
        let opened = Project::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(opened.ledger.document().connection_points.get(&t), Some(&bar), "{name}");
    }
    let mut v = serde_json::to_value(Rec::from(&bar)).unwrap();
    assert_eq!(v["accepts"], 12);
    let two = serde_json::to_value(Rec::from(&ConnectionPoint { accepts: Accepts::TWO, ..bar.clone() })).unwrap();
    assert!(two.get("accepts").is_none(), "two is left out: {two}");
    for hostile in [serde_json::json!(0), serde_json::json!(65), serde_json::json!(100_000), serde_json::json!(-3)] {
        v["accepts"] = hostile.clone();
        assert!(serde_json::from_value::<Rec>(v.clone()).is_err(), "{hostile} must not read");
    }
    let def = serde_json::json!({"key": "PE", "label": null, "at": [0.0, 0.0], "accepts": 8});
    let read = serde_json::from_value::<DefRec>(def.clone()).unwrap_or_else(|e| panic!("{e}: {def}"));
    assert_eq!(read.accepts.get(), 8);
    let mut bad = def;
    bad["accepts"] = serde_json::json!(0);
    assert!(serde_json::from_value::<DefRec>(bad).is_err());
}
