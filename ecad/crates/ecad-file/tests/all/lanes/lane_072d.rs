//! Lane 072 D (design 072 F31): routes are kept in their page's chunk and come back the same when
//! the project is opened, from the snapshot and from the log.

use crate::common::*;
use crate::container_v2::TempDir;
use ecad_file::{ProjectV2, Store};
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
        ProjectV2::create(Store::create(&p).unwrap(), PROJECT, &f.ledger, Some(1), WRITER, every).unwrap();
        let opened = ProjectV2::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
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
    let on_line = CableMember { position: CorePosition::Route { route: r, landed: None }, core: Some("1".into()) };
    let spare = CableMember { position: CorePosition::Unplaced, core: Some("2".into()) };
    f.ledger
        .commit(
            Author::User,
            "slash",
            vec![
                Op::AddRoute { id: r, route: loose },
                Op::CreateCable { id: k, cable: Cable::default() },
                Op::AddCableMember { cable: k, index: 0, member: on_line },
                Op::AddCableMember { cable: k, index: 1, member: spare },
            ],
        )
        .unwrap();
    let (lo, hi) = ecad_file::schema::SNAPSHOT_EVERY_RANGE;
    for (name, every) in [("snap.ecad", lo), ("log.ecad", hi)] {
        let p = dir.path(name);
        ProjectV2::create(Store::create(&p).unwrap(), PROJECT, &f.ledger, Some(1), WRITER, every).unwrap();
        let opened = ProjectV2::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(opened.ledger.document().cables, f.ledger.document().cables, "{name}");
    }
}
