//! The shell's own icons: search, checks, agent, history, pages, devices, collapse, close,
//! chevrons.

use super::Pen;

/// Search: a lens.
pub fn search(p: &mut Pen) {
    p.circle(6.75, 6.75, 4.75);
    p.line((10.25, 10.25), (14.0, 14.0));
}

/// Checks: a tick in a ring.
pub fn checks(p: &mut Pen) {
    p.circle(8.0, 8.0, 6.5);
    p.poly(&[(5.0, 8.25), (7.0, 10.25), (11.0, 6.0)], false);
}

/// Agent: a ring with two eyes.
pub fn agent(p: &mut Pen) {
    p.circle(8.0, 8.0, 6.5);
    p.dot(6.0, 7.0, 1.0);
    p.dot(10.0, 7.0, 1.0);
}

/// History: a clock arc with hands, open at the top left, where time turns back.
pub fn history(p: &mut Pen) {
    p.arc(8.0, 8.0, 6.25, 220.0, 520.0);
    p.poly(&[(1.75, 2.75), (3.2, 4.0), (4.6, 2.6)], false);
    p.poly(&[(8.0, 4.5), (8.0, 8.0), (10.5, 9.5)], false);
}

/// Pages: stacked sheets.
pub fn pages(p: &mut Pen) {
    p.poly(&[(5.0, 4.0), (5.0, 1.5), (14.5, 1.5), (14.5, 11.0), (11.0, 11.0)], false);
    p.rect(1.5, 4.0, 11.0, 14.5);
}

/// Devices: a component box with two leads.
pub fn devices(p: &mut Pen) {
    p.line((8.0, 1.5), (8.0, 4.0));
    p.rect(4.5, 4.0, 11.5, 12.0);
    p.line((8.0, 12.0), (8.0, 14.5));
}

/// Collapse: a bar and an arrow into it.
pub fn collapse(p: &mut Pen) {
    p.line((3.0, 2.0), (3.0, 14.0));
    p.line((13.5, 8.0), (6.0, 8.0));
    p.poly(&[(9.0, 5.0), (6.0, 8.0), (9.0, 11.0)], false);
}

/// Close: a cross.
pub fn close(p: &mut Pen) {
    p.line((4.0, 4.0), (12.0, 12.0));
    p.line((12.0, 4.0), (4.0, 12.0));
}

/// A chevron through three grid points.
pub fn chevron(p: &mut Pen, a: (f32, f32), b: (f32, f32), c: (f32, f32)) {
    p.poly(&[a, b, c], false);
}
