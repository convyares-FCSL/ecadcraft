//! Lane 072 B step 5 (design 072 §2.6, §2.8): device links and user fields are stored and read
//! back, through the op records and through the project chunk of a snapshot.

use std::path::PathBuf;

use crate::common::{PROJECT, SEED, WRITER};
use ecad_file::{ProjectV2, Store};
use ecad_model::text::Text;
use ecad_model::{
    Author, Device, DeviceId, DeviceLink, IdAllocator, Ledger, LinkId, LinkKind, Op, Project, UserFieldDef, UserFieldId, UserFieldKind, UserValue,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-072b-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ledger() -> Ledger {
    let mut l = Ledger::new(SEED);
    let mut ids = IdAllocator::seeded(72);
    let (a, b, link, f): (DeviceId, DeviceId, LinkId, UserFieldId) = (ids.fresh(), ids.fresh(), ids.fresh(), ids.fresh());
    let def = UserFieldDef { id: f, name: Text::new("Panel row").unwrap(), kind: UserFieldKind::Text };
    let project = Project { user_fields: vec![def], ..Project::default() };
    let mut with_value = Device::default();
    with_value.user.insert(f, UserValue::Text(Text::new("top").unwrap()));
    let ops = vec![
        Op::ModifyProject { before: Project::default(), after: project },
        Op::CreateDevice { id: a, device: with_value },
        Op::CreateDevice { id: b, device: Device::default() },
        Op::AddDeviceLink { id: link, link: DeviceLink { kind: LinkKind::AuxiliaryBlock, a, b } },
    ];
    l.commit(Author::User, "step 5", ops).unwrap();
    l
}

#[test]
fn links_and_user_values_round_trip_through_a_project_file_and_its_snapshots() {
    let dir = TempDir::new("roundtrip");
    let ledger = ledger();
    assert_eq!(ledger.document().device_links.len(), 1);
    for (name, every) in [("snap.ecad", 16), ("log.ecad", 1_000_000)] {
        let p = dir.path(name);
        let store = Store::create(&p).unwrap();
        let proj = ProjectV2::create(store, PROJECT, &ledger, Some(1), WRITER, every).unwrap().0;
        drop(proj);
        let o = ProjectV2::open(Store::open(&p, true).unwrap(), SEED ^ 1, WRITER).unwrap();
        assert_eq!(o.ledger.document(), ledger.document(), "{name}");
        assert_eq!(o.ledger.full_log().unwrap(), ledger.full_log().unwrap(), "{name}: the log reads back the same ops");
        let v = ecad_file::verify::verify(o.project.store());
        assert!(v.ok, "{name}: {:#?}", v.lines);
    }
}

/// The op record names the link's kind and devices; an unknown kind is a read error.
#[test]
fn the_link_record_is_strict() {
    let l = ledger();
    let tx = l.full_log().unwrap().into_iter().next().unwrap();
    let v = ecad_file::record::tx_value(&tx, &[0u8; 32]).unwrap();
    assert_eq!(v["ops"][3]["link"]["kind"], "auxiliary_block", "{v}");
    assert_eq!(ecad_file::record::decode_tx(&v).unwrap().ops, tx.ops);
    let mut bad = v.clone();
    bad["ops"][3]["link"]["kind"] = "glued".into();
    assert!(ecad_file::record::decode_tx(&bad).is_err());
    let mut bad = v;
    bad["ops"][1]["device"]["user"] = serde_json::json!({"x": {"text": "top"}});
    assert!(ecad_file::record::decode_tx(&bad).is_err(), "a user value under a key that is not a field id");
}
