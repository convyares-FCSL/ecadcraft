//! Overseer item O1 (design `ui-architecture.md` §8.4): the window re-projects the active page
//! after a commit only when the commit touched it. Generated pages are named in the touched set by
//! their own update maps, so a shown terminal plan still redraws when its strip changes.
//! Commands run through the session directly, so the harness's own checks (which work every page
//! out again without counting) are not counted; `H::check` then proves the skipped page is still
//! what a fresh projection draws.

use ecad_model::{ComponentId, PageId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

fn place(h: &mut H, symbol: &str, at: [f64; 2]) -> ComponentId {
    id(&h.run("place", json!({"symbol": symbol, "at": at})).unwrap(), "component")
}

/// Run `cmd` on the session; the window projections it caused and the pages it names.
fn counted(h: &mut H, cmd: &str, params: Value) -> (u64, Option<std::collections::BTreeSet<PageId>>) {
    let (before, n) = h.p(|p| (p.projections, p.change_count()));
    h.s.execute(cmd, &params).unwrap();
    h.p(|p| (p.projections - before, p.changes_since(n)))
}

/// An edit on another page does not re-project the page on screen; an edit on it does, once.
#[test]
fn an_unrelated_commit_does_not_redraw_the_active_page() {
    let mut h = H::new(0x01_0001);
    let p1 = h.first_page();
    let here = place(&mut h, "coil", [100.0, 200.0]);
    let p2: PageId = id(&h.run("newpage", json!({"title": "P2"})).unwrap(), "page");
    let there = place(&mut h, "coil", [100.0, 200.0]);
    h.activate(p1);

    let (n, named) = counted(&mut h, "setprop", json!({"component": there.hex(), "key": "maker", "value": "Acme"}));
    let named = named.expect("pages known");
    assert!(named.contains(&p2) && !named.contains(&p1), "{named:?}");
    assert_eq!(n, 0, "an edit on page 2 re-projected page 1 {n} time(s)");
    h.check();

    let (n, named) = counted(&mut h, "setprop", json!({"component": here.hex(), "key": "maker", "value": "Acme"}));
    assert!(named.expect("pages known").contains(&p1));
    assert_eq!(n, 1, "an edit on page 1 re-projects it once");
    h.check();
}

/// A shown terminal plan page is named and re-projected when its strip changes, and left alone
/// by an edit no strip shows.
#[test]
fn a_shown_generated_page_redraws_when_its_content_changes() {
    let mut h = H::new(0x01_0002);
    for x in [320.0, 330.0] {
        h.run("wire", json!({"points": pts(&[(x, 270.0), (x, 160.0)])})).unwrap();
    }
    let v = h.run("terminals", json!({"from": [310, 200], "to": [340, 200]})).unwrap();
    let strip: ComponentId = id(&v, "strip");
    let far = place(&mut h, "coil", [100.0, 260.0]);
    let plan: PageId = id(&h.run("newpage", json!({"generated": "terminals"})).unwrap(), "page");
    h.activate(plan);

    let (n, named) = counted(&mut h, "setprop", json!({"component": far.hex(), "key": "part_number", "value": "K-99"}));
    assert!(!named.expect("pages known").contains(&plan));
    assert_eq!(n, 0, "an edit no strip shows re-projected the plan page {n} time(s)");
    h.check();

    let (n, named) = counted(&mut h, "setprop", json!({"component": strip.hex(), "key": "terminals", "value": "10"}));
    let named = named.expect("pages known");
    assert!(named.contains(&plan), "the plan page is named when its strip changes: {named:?}");
    assert_eq!(n, 1, "the plan page re-projects once for its strip");
    h.check();
}
