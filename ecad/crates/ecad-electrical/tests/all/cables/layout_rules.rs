//! Geometry rules of a cable's drawing at its ends (design 005).

use ecad_electrical::cables::layout::{clip_outside, leaving};
use ecad_model::Pt;

#[test]
fn clipping_removes_the_middle_of_a_segment() {
    let pieces = clip_outside(&[Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)], Pt::new(4.0, -1.0), Pt::new(6.0, 1.0), 1e-9);
    assert_eq!(pieces, vec![vec![Pt::new(0.0, 0.0), Pt::new(4.0, 0.0)], vec![Pt::new(6.0, 0.0), Pt::new(10.0, 0.0)]]);
}

#[test]
fn leaving_snaps_to_the_nearest_axis() {
    let p = [Pt::new(0.0, 0.0), Pt::new(3.0, 3.0)];
    assert_eq!(leaving(p.iter().copied(), 1e-9), Some(Pt::new(1.0, 0.0)));
    let p = [Pt::new(0.0, 0.0), Pt::new(1.0, -3.0)];
    assert_eq!(leaving(p.iter().copied(), 1e-9), Some(Pt::new(0.0, -1.0)));
}
