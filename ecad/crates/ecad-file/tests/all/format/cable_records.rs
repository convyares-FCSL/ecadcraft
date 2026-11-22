//! Lane 072 D, part c2 (design 072 §4.5): a cable's shield and a `ModifyCable` come back the same
//! from the snapshot and from the log, and a hostile record is a read error.

use crate::common::*;
use crate::container::files::TempDir;
use ecad_file::{Project, Store};
use ecad_model::objects::styles::cable::Shield;
use ecad_model::*;

#[test]
fn a_cables_shield_and_its_modify_op_come_back_from_the_snapshot_and_the_log() {
    let dir = TempDir::new("072d-cable");
    let mut f = fixture();
    let k: CableId = f.ids.fresh();
    let made = Cable { cores: vec![Core::unplaced(CoreLabel::new("1").ok())], ..Cable::default() };
    f.ledger.commit(Author::User, "new", vec![Op::CreateCable { id: k, cable: made.clone() }]).unwrap();
    let mut after = made.clone();
    after.shield = Some(Shield::Pairs);
    after.cores.push(Core { purpose: Purpose::Spare, ..Core::unplaced(CoreLabel::new("GNYE").ok()) });
    f.ledger.commit(Author::User, "edit", Op::modify_cable(k, &made, after.clone()).into_iter().collect()).unwrap();
    let (lo, hi) = ecad_file::schema::SNAPSHOT_EVERY_RANGE;
    for (name, every) in [("snap.ecad", lo), ("log.ecad", hi)] {
        let p = dir.path(name);
        Project::create(Store::create(&p).unwrap(), PROJECT, &f.ledger, Some(1), WRITER, every).unwrap();
        let mut opened = Project::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(opened.ledger.document().cables().get(&k), Some(&after), "{name}");
        // The log holds the op whole: undoing it gives the cable as made.
        opened.ledger.undo(Author::User).unwrap();
        assert_eq!(opened.ledger.document().cables().get(&k), Some(&made), "{name} undo");
    }
}

#[test]
fn a_hostile_modify_cable_record_is_a_read_error() {
    use ecad_file::records::Op as Rec;
    let k = CableId(0x72d);
    let before = Cable::default();
    let after = Cable { shield: Some(Shield::Overall), ..Cable::default() };
    let op = Op::ModifyCable { id: k, before: before.clone(), after: after.clone() };
    let v = serde_json::to_value(Rec::from(&op)).unwrap();
    assert_eq!(v["ModifyCable"]["after"]["shield"], "overall", "{v}");
    let back: Op = Rec::try_into(serde_json::from_value::<Rec>(v.clone()).unwrap()).unwrap();
    assert_eq!(back, op);
    let mutate = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut x = v.clone();
        f(&mut x);
        x
    };
    let hostile = [
        mutate(&|x| x["ModifyCable"]["after"]["shield"] = serde_json::json!("tartan")),
        mutate(&|x| x["ModifyCable"]["after"]["shield"] = serde_json::json!(7)),
        mutate(&|x| x["ModifyCable"]["after"]["colour"] = serde_json::json!("red")),
        mutate(&|x| x["ModifyCable"]["index"] = serde_json::json!(0)),
        mutate(&|x| {
            x["ModifyCable"].as_object_mut().map(|o| o.remove("before"));
        }),
        mutate(&|x| x["ModifyCable"]["after"]["cores"] = serde_json::json!([{"label": "1"}])),
    ];
    for h in hostile {
        assert!(serde_json::from_value::<Rec>(h.clone()).is_err(), "must not read: {h}");
    }
    // The member ops are gone from the format.
    let old = serde_json::json!({"AddCableMember": {"cable": v["ModifyCable"]["id"], "index": 0, "member": {}}});
    assert!(serde_json::from_value::<Rec>(old).is_err());
}
