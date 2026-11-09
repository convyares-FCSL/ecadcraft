//! Tester probe for brief 014 re-test round 3. It times the whole-document work that now runs on
//! every drawing commit, on a kept benchmark project. Diagnostic only; not an acceptance test.
//!
//! The probe times four things:
//! - brief 010's `references::linked` and `orphaned_line`, which both run `cable_references` over
//!   the whole document;
//! - brief 009's `views::drawings_on_page`;
//! - the page projection itself.
//!
//! It runs only when `ECAD_R3_PROJECT` names a kept `.ecad` file.

use std::collections::BTreeSet;
use std::time::Instant;

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn time(runs: usize, mut f: impl FnMut()) -> f64 {
    median(
        (0..runs)
            .map(|_| {
                let t = Instant::now();
                f();
                t.elapsed().as_secs_f64() * 1000.0
            })
            .collect(),
    )
}

#[test]
fn tester_014_r3_whole_document_work_per_commit() {
    let Some(path) = std::env::var_os("ECAD_R3_PROJECT") else {
        eprintln!("ECAD_R3_PROJECT not set: probe skipped");
        return;
    };
    ecad_cadcraft::install();
    ecad_cadcraft::views::isolate_for_tests();
    let dir = std::env::temp_dir().join(format!("ecad-r3-cost-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let copy = dir.join("cost.ecad");
    std::fs::copy(&path, &copy).unwrap();
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file_in(&mut s, copy.to_str().unwrap(), 14, ecad_cadcraft::views::Settings::none()).unwrap();
    let doc = project.read(&mut s, |p, _| p.ledger.document().clone()).unwrap();
    let page = *doc.page_order.first().unwrap();
    let runs = 9;
    let refs = ecad_model::derive::cable_references(&doc);
    let reps_of_page1 = doc.representations.values().filter(|r| r.page == page).count();
    let linked = ecad_cadcraft::references::linked(&doc);
    let t_refs = time(runs, || drop(ecad_model::derive::cable_references(&doc)));
    let t_linked = time(runs, || drop(ecad_cadcraft::references::linked(&doc)));
    let t_orphan = time(runs, || drop(ecad_cadcraft::references::orphaned_line(&doc, &linked)));
    let t_any = time(runs, || {
        std::hint::black_box(doc.representations.values().any(ecad_cadcraft::references::is_reference));
    });
    let t_draw = time(runs, || drop(ecad_cadcraft::views::drawings_on_page(&doc, page, &BTreeSet::new(), 1e-6)));
    eprintln!(
        "r3 cost: items {}, representations {} ({} on page 1), cables {}, references {}, linked {}",
        doc.items.len(),
        doc.representations.len(),
        reps_of_page1,
        doc.cables.len(),
        refs.len(),
        linked.len()
    );
    eprintln!(
        "r3 cost ms (median of {runs}): cable_references {t_refs:.2}, linked {t_linked:.2}, orphaned_line {t_orphan:.2}, any(is_reference) {t_any:.3}, drawings_on_page {t_draw:.2}"
    );
    drop(project);
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}
