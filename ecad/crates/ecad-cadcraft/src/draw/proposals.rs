//! The engine side of placement proposals (design 052 §5.3; design 065 §3, cut 6): planning one
//! for a placement in a live project, and keeping its conflicts waiting there afterwards. The
//! rules are [`crate::proposals`].

use ecad_model::{DeviceId, PageId, Pt};
use serde_json::Value;

use crate::designations::{Aspect, Pending};
use crate::proposals::*;

/// Plan a placement's proposal (design 052 §5.3) for the symbol of device `c` at `at` on `page`,
/// with the `function` and `location` modes the place gave (function first). `half`: the side a
/// connector half or pin draws (its location conflict can then be answered **Half**; JSON
/// `location: "half"` answers it).
///
/// # Errors
/// A mode or a typed code that is not valid.
pub(crate) fn plan_place(
    p: &mut crate::project::Project, mut modes: [Mode; 2], c: DeviceId, page: PageId, at: Pt, half: Option<crate::symbols::Side>,
) -> Result<Planned, String> {
    let half_mode = modes[1] == Mode::Half;
    if half_mode {
        modes[1] = Mode::Propose;
    }
    let half = half.filter(|s| *s != crate::symbols::Side::Both);
    let spot = Spot { device: c, page, at: Some(at) };
    let mut pending = Pending::default();
    let mut planned = plan(p.ledger.document(), &mut p.ids, &[spot], &modes, &[], &mut pending)?;
    for x in planned.conflicts.iter_mut().filter(|x| x.aspect == Aspect::Location) {
        x.half = half;
    }
    if half_mode && half.is_some() {
        let (halves, rest): (Vec<Conflict>, Vec<Conflict>) = planned.conflicts.drain(..).partition(|x| x.half.is_some());
        planned.ops.extend(half_ops(p.ledger.document(), &halves));
        planned.set.extend(halves.iter().map(|x| (x.component, x.aspect, x.to.clone())));
        planned.conflicts = rest;
    }
    Ok(planned)
}

/// After a placement committed: keep its conflicts waiting (Leave by default), echo what was set
/// and what stayed, and add the proposal to the result `v`.
pub(crate) fn finish_place(
    p: &mut crate::project::Project, pg: &mut crate::access::Pages<'_>, c: DeviceId, joined: bool, planned: &Planned, v: &mut Value,
) {
    p.conflicts = if planned.moved == 0 { planned.conflicts.clone() } else { Vec::new() };
    let doc = p.ledger.document();
    if let (Some(o), Some(x)) = (v.as_object_mut(), json(doc, planned).as_object()) {
        o.extend(x.clone());
    }
    for line in place_words(doc, c, joined, planned) {
        pg.echo(line);
    }
}
