//! The shell's own icons: search, checks, agent, history, pages, devices, collapse, close and chevrons; the
//! frame's Book panel and PLC group, the Snap, Grid and Ortho toggles, the light/dark switch, help, the bottom
//! bar's page well and view group, the Devices panel and Share.

use crate::icons::Pen;

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

/// Book: an open book, two pages meeting at the spine.
pub fn book(p: &mut Pen) {
    p.poly(&[(8.0, 4.0), (5.0, 2.75), (1.5, 2.75), (1.5, 12.5), (5.0, 12.5), (8.0, 13.75)], false);
    p.poly(&[(8.0, 4.0), (11.0, 2.75), (14.5, 2.75), (14.5, 12.5), (11.0, 12.5), (8.0, 13.75)], false);
    p.line((8.0, 4.0), (8.0, 13.75));
}

/// PLC: a card with a grid of six channels.
pub fn plc(p: &mut Pen) {
    p.rect(2.5, 1.5, 13.5, 14.5);
    for (x, y) in [(5.5, 5.0), (10.5, 5.0), (5.5, 8.0), (10.5, 8.0), (5.5, 11.0), (10.5, 11.0)] {
        p.line((x - 1.0, y), (x + 1.0, y));
    }
    p.line((8.0, 3.5), (8.0, 12.5));
}

/// Snap: a cross hair on a dot.
pub fn snap(p: &mut Pen) {
    p.dot(8.0, 8.0, 1.5);
    p.line((8.0, 1.5), (8.0, 5.0));
    p.line((8.0, 11.0), (8.0, 14.5));
    p.line((1.5, 8.0), (5.0, 8.0));
    p.line((11.0, 8.0), (14.5, 8.0));
}

/// Grid: a hash of four lines.
pub fn grid(p: &mut Pen) {
    p.line((5.5, 1.5), (5.5, 14.5));
    p.line((10.5, 1.5), (10.5, 14.5));
    p.line((1.5, 5.5), (14.5, 5.5));
    p.line((1.5, 10.5), (14.5, 10.5));
}

/// Ortho: a right angle with a square corner mark.
pub fn ortho(p: &mut Pen) {
    p.poly(&[(3.0, 1.5), (3.0, 13.0), (14.5, 13.0)], false);
    p.poly(&[(3.0, 9.0), (7.0, 9.0), (7.0, 13.0)], false);
}

/// Dark theme: a crescent.
pub fn moon(p: &mut Pen) {
    p.arc(8.0, 8.0, 6.0, 60.0, 330.0);
    p.arc(11.0, 5.5, 4.25, 105.0, 255.0);
}

/// Light theme: a ring with eight rays.
pub fn sun(p: &mut Pen) {
    p.circle(8.0, 8.0, 3.0);
    for i in 0..8 {
        let a = (i as f32 * 45.0).to_radians();
        let (c, s) = (a.cos(), a.sin());
        p.line((8.0 + 4.75 * c, 8.0 + 4.75 * s), (8.0 + 6.5 * c, 8.0 + 6.5 * s));
    }
}

/// Help: a question mark in a ring.
pub fn help(p: &mut Pen) {
    p.circle(8.0, 8.0, 6.5);
    p.arc(8.0, 6.25, 2.25, 190.0, 400.0);
    p.line((8.75, 8.25), (8.0, 9.25));
    p.dot(8.0, 11.5, 0.9);
}

/// Page group: a folder with a tab.
pub fn folder(p: &mut Pen) {
    p.poly(&[(1.5, 13.0), (1.5, 3.0), (6.0, 3.0), (7.5, 4.75), (14.5, 4.75), (14.5, 13.0)], true);
    p.line((1.5, 6.75), (14.5, 6.75));
}

/// Previous group: two chevrons pointing left.
pub fn chevrons_left(p: &mut Pen) {
    p.poly(&[(8.0, 3.0), (3.0, 8.0), (8.0, 13.0)], false);
    p.poly(&[(13.0, 3.0), (8.0, 8.0), (13.0, 13.0)], false);
}

/// Next group: two chevrons pointing right.
pub fn chevrons_right(p: &mut Pen) {
    p.poly(&[(3.0, 3.0), (8.0, 8.0), (3.0, 13.0)], false);
    p.poly(&[(8.0, 3.0), (13.0, 8.0), (8.0, 13.0)], false);
}

/// Fit: four corner marks.
pub fn fit(p: &mut Pen) {
    p.poly(&[(2.0, 6.0), (2.0, 2.0), (6.0, 2.0)], false);
    p.poly(&[(10.0, 2.0), (14.0, 2.0), (14.0, 6.0)], false);
    p.poly(&[(14.0, 10.0), (14.0, 14.0), (10.0, 14.0)], false);
    p.poly(&[(6.0, 14.0), (2.0, 14.0), (2.0, 10.0)], false);
}

/// Zoom out: a dash.
pub fn minus(p: &mut Pen) {
    p.line((3.0, 8.0), (13.0, 8.0));
}

/// Zoom in: a plus.
pub fn plus(p: &mut Pen) {
    p.line((3.0, 8.0), (13.0, 8.0));
    p.line((8.0, 3.0), (8.0, 13.0));
}

/// Devices panel: three squares and a plus.
pub fn device_grid(p: &mut Pen) {
    p.rect(2.0, 2.0, 7.0, 7.0);
    p.rect(9.0, 2.0, 14.0, 7.0);
    p.rect(2.0, 9.0, 7.0, 14.0);
    p.line((11.5, 9.0), (11.5, 14.0));
    p.line((9.0, 11.5), (14.0, 11.5));
}

/// Share: an arrow rising out of a tray.
pub fn share(p: &mut Pen) {
    p.poly(&[(5.0, 7.0), (2.5, 7.0), (2.5, 14.0), (13.5, 14.0), (13.5, 7.0), (11.0, 7.0)], false);
    p.line((8.0, 10.0), (8.0, 1.5));
    p.poly(&[(5.0, 4.5), (8.0, 1.5), (11.0, 4.5)], false);
}
