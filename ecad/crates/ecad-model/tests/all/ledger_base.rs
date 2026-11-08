//! A ledger opened from a snapshot (brief 021): it behaves exactly like one replayed from the root,
//! and reads transactions before its base only when undo, redo or `state_at` reach them.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ecad_model::{Author, Base, Document, History, Layer, LayerId, Ledger, ModelError, Op, Page, PageId, Transaction};

/// The full log, counting reads.
#[derive(Debug)]
struct Store {
    log: Vec<Transaction>,
    reads: AtomicUsize,
    fail: Mutex<bool>,
}

impl History for Store {
    fn transaction(&self, seq: u64) -> ecad_model::Result<Transaction> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if *self.fail.lock().map_err(|_| ModelError::CorruptLog("poisoned".into()))? {
            return Err(ModelError::CorruptLog("store unreadable".into()));
        }
        self.log.get(seq as usize - 1).cloned().ok_or(ModelError::CorruptLog(format!("no {seq}")))
    }

    fn state_at(&self, seq: u64) -> ecad_model::Result<Document> {
        Ok(Ledger::replay(self.log[..seq as usize].to_vec(), 0)?.document().clone())
    }
}

fn page(n: u128) -> Op {
    Op::AddPage { id: PageId(n), page: Page::new(n.to_string()), index: 0 }
}

/// Ten pages added one by one, two undone, one redone, then one more edit and one undo.
fn full() -> Ledger {
    let mut l = Ledger::new(1);
    l.commit(Author::User, "layer", vec![Op::AddLayer { id: LayerId(99), layer: Layer { name: "0".into() } }]).unwrap();
    for n in 1..=10 {
        l.commit(Author::User, &format!("page {n}"), vec![page(n)]).unwrap();
    }
    l.undo(Author::User).unwrap();
    l.undo(Author::User).unwrap();
    l.redo(Author::User).unwrap();
    l
}

fn based(l: &Ledger, at: u64) -> (Ledger, Arc<Store>) {
    let store = Arc::new(Store { log: l.log().to_vec(), reads: AtomicUsize::new(0), fail: Mutex::new(false) });
    let prefix = Ledger::replay(l.log()[..at as usize].to_vec(), 0).unwrap();
    let (undo, redo) = prefix.stacks();
    let base = Base { seq: at, doc: prefix.document().clone(), undo, redo, history: store.clone() };
    let (b, failure) = Ledger::from_base(base, l.log()[at as usize..].to_vec(), 1).unwrap();
    assert!(failure.is_none(), "{failure:?}");
    (b, store)
}

#[test]
fn a_ledger_from_any_snapshot_matches_the_replayed_one_step_for_step() {
    let reference = full();
    for at in 0..=reference.head() {
        let (mut b, store) = based(&reference, at);
        let mut r = reference.clone();
        assert_eq!(b.head(), r.head());
        assert_eq!(b.document(), r.document(), "at {at}");
        assert_eq!(b.stacks(), r.stacks(), "at {at}");
        assert!(!b.is_unsaved());
        // Only stack tops are read: two, plus one per tail step that exposes another.
        assert!(store.reads.load(Ordering::SeqCst) as u64 <= 2 + (r.head() - at));
        // Undo all the way back, then redo all the way forward: the same documents.
        loop {
            let (x, y) = (b.undo(Author::User), r.undo(Author::User));
            assert_eq!(x.is_ok(), y.is_ok(), "at {at}");
            assert_eq!(b.document(), r.document(), "at {at}");
            assert_eq!(b.undo_label(), r.undo_label());
            if x.is_err() {
                break;
            }
        }
        while b.redo(Author::User).is_ok() {
            r.redo(Author::User).unwrap();
            assert_eq!(b.document(), r.document());
        }
        assert!(r.redo(Author::User).is_err());
        for seq in 0..=b.head() {
            assert_eq!(b.state_at(seq).unwrap(), r.state_at(seq).unwrap(), "state_at {seq} from base {at}");
        }
        assert!(b.replay_matches().unwrap());
    }
}

#[test]
fn an_unreadable_history_refuses_undo_past_the_base_and_changes_nothing() {
    let reference = full();
    let (mut b, store) = based(&reference, reference.head());
    // Undo inside what is resident works; reaching an unread step needs the store.
    *store.fail.lock().unwrap() = true;
    let mut ok = 0;
    loop {
        let before = (b.head(), b.document().clone());
        match b.undo(Author::User) {
            Ok(_) => ok += 1,
            Err(e) => {
                assert!(matches!(e, ModelError::CorruptLog(_)), "{e}");
                assert_eq!((b.head(), b.document().clone()), before, "a refused undo changes nothing");
                break;
            }
        }
    }
    assert!(ok >= 1, "the resident top undoes without the store");
    *store.fail.lock().unwrap() = false;
    b.undo(Author::User).unwrap();
}

#[test]
fn inconsistent_snapshot_stacks_are_refused() {
    let reference = full();
    let store = Arc::new(Store { log: reference.log().to_vec(), reads: AtomicUsize::new(0), fail: Mutex::new(false) });
    let (undo, _) = reference.stacks();
    let bad = Base { seq: 3, doc: Document::default(), undo, redo: vec![], history: store };
    assert!(matches!(Ledger::from_base(bad, vec![], 1), Err(ModelError::CorruptLog(_))));
}
