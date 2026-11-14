//! The seven tool groups of the Draw workspace (spec §2). Tool icons echo the schematic.

use super::Pen;

/// Select: an arrow pointer.
pub fn select(p: &mut Pen) {
    p.poly(&[(4.0, 2.0), (4.0, 13.0), (7.0, 10.0), (9.0, 14.0), (10.5, 13.3), (8.6, 9.4), (12.5, 9.4)], true);
}

/// Wire: a line between two end rings.
pub fn wire(p: &mut Pen) {
    p.circle(2.75, 8.0, 1.75);
    p.line((4.5, 8.0), (11.5, 8.0));
    p.circle(13.25, 8.0, 1.75);
}

/// Place: a symbol box with a connection point ring on its lead.
pub fn place(p: &mut Pen) {
    p.rect(4.0, 1.5, 12.0, 9.5);
    p.line((8.0, 9.5), (8.0, 11.75));
    p.circle(8.0, 13.25, 1.5);
}

/// Cable: a slash across three lines.
pub fn cable(p: &mut Pen) {
    for y in [5.0, 8.0, 11.0] {
        p.line((1.5, y), (14.5, y));
    }
    p.line((5.0, 14.0), (11.0, 2.0));
}

/// Draw: an arc over a line (plain geometry).
pub fn draw(p: &mut Pen) {
    p.arc(8.0, 10.0, 5.5, 180.0, 360.0);
    p.line((1.5, 13.0), (14.5, 13.0));
}

/// Box: a dashed box (a location or function box).
pub fn boxed(p: &mut Pen) {
    let (x0, y0, x1, y1) = (2.0, 3.0, 14.0, 13.0);
    p.dashed((x0, y0), (x1, y0), 2.0, 1.5);
    p.dashed((x1, y0), (x1, y1), 2.0, 1.5);
    p.dashed((x1, y1), (x0, y1), 2.0, 1.5);
    p.dashed((x0, y1), (x0, y0), 2.0, 1.5);
}

/// Text: a letter T.
pub fn text(p: &mut Pen) {
    p.line((3.0, 3.0), (13.0, 3.0));
    p.line((8.0, 3.0), (8.0, 13.0));
    p.line((6.0, 13.0), (10.0, 13.0));
}

// 041 (brief 041, U4b): the actions' icons, drawn here, none traced from any product.

/// Delete: a bin with a lid.
pub fn delete(p: &mut Pen) {
    p.line((2.5, 4.0), (13.5, 4.0));
    p.poly(&[(6.0, 4.0), (6.5, 2.0), (9.5, 2.0), (10.0, 4.0)], false);
    p.poly(&[(4.0, 4.0), (5.0, 14.0), (11.0, 14.0), (12.0, 4.0)], false);
    p.line((8.0, 6.5), (8.0, 11.5));
}

/// Go to: an arrow leaving a box.
pub fn goto(p: &mut Pen) {
    p.poly(&[(9.0, 3.0), (3.0, 3.0), (3.0, 13.0), (13.0, 13.0), (13.0, 7.0)], false);
    p.line((7.0, 9.0), (14.0, 2.0));
    p.poly(&[(9.5, 2.0), (14.0, 2.0), (14.0, 6.5)], false);
}

/// Show its row: three rows, the middle one marked.
pub fn show_row(p: &mut Pen) {
    p.line((5.0, 3.5), (14.0, 3.5));
    p.rect(4.0, 6.0, 14.5, 10.0);
    p.dot(2.0, 8.0, 1.0);
    p.line((5.0, 12.5), (14.0, 12.5));
}

/// Edit: a pencil.
pub fn edit(p: &mut Pen) {
    p.poly(&[(3.0, 13.0), (3.5, 10.0), (11.0, 2.5), (13.5, 5.0), (6.0, 12.5)], true);
    p.line((9.5, 4.0), (12.0, 6.5));
}

/// Properties: a list beside a pointer box.
pub fn properties(p: &mut Pen) {
    p.rect(2.0, 2.0, 14.0, 14.0);
    for y in [5.5, 8.0, 10.5] {
        p.dot(4.75, y, 0.9);
        p.line((6.5, y), (12.0, y));
    }
}

/// Rotate: an open circle with an arrow head.
pub fn rotate(p: &mut Pen) {
    p.arc(8.0, 8.5, 5.0, 200.0, 470.0);
    p.poly(&[(1.8, 6.0), (3.3, 9.0), (6.0, 7.2)], false);
}

/// Mirror: two triangles about a dashed axis.
pub fn mirror(p: &mut Pen) {
    p.dashed((8.0, 1.5), (8.0, 14.5), 1.5, 1.5);
    p.poly(&[(6.0, 4.0), (6.0, 12.0), (2.0, 12.0)], true);
    p.poly(&[(10.0, 4.0), (10.0, 12.0), (14.0, 12.0)], true);
}

/// Duplicate: two overlapping boxes.
pub fn duplicate(p: &mut Pen) {
    p.rect(2.0, 5.0, 11.0, 14.0);
    p.poly(&[(5.0, 5.0), (5.0, 2.0), (14.0, 2.0), (14.0, 11.0), (11.0, 11.0)], false);
}

/// Connect: a lead into a ring.
pub fn connect(p: &mut Pen) {
    p.line((1.5, 8.0), (9.5, 8.0));
    p.poly(&[(7.0, 5.5), (9.5, 8.0), (7.0, 10.5)], false);
    p.circle(12.5, 8.0, 2.0);
}

/// Disconnect: a lead apart from a ring, with a gap mark.
pub fn disconnect(p: &mut Pen) {
    p.line((1.5, 8.0), (6.0, 8.0));
    p.line((8.0, 5.0), (8.0, 11.0));
    p.circle(12.5, 8.0, 2.0);
}

/// Number: a hash.
pub fn number(p: &mut Pen) {
    p.line((6.0, 2.0), (5.0, 14.0));
    p.line((11.0, 2.0), (10.0, 14.0));
    p.line((2.5, 5.5), (14.0, 5.5));
    p.line((2.0, 10.5), (13.5, 10.5));
}

/// Move: a cross with four arrow heads.
pub fn moving(p: &mut Pen) {
    p.line((8.0, 1.5), (8.0, 14.5));
    p.line((1.5, 8.0), (14.5, 8.0));
    p.poly(&[(6.0, 3.5), (8.0, 1.5), (10.0, 3.5)], false);
    p.poly(&[(6.0, 12.5), (8.0, 14.5), (10.0, 12.5)], false);
    p.poly(&[(3.5, 6.0), (1.5, 8.0), (3.5, 10.0)], false);
    p.poly(&[(12.5, 6.0), (14.5, 8.0), (12.5, 10.0)], false);
}

/// Paste: a sheet on a board with a clip.
pub fn paste(p: &mut Pen) {
    p.rect(2.5, 3.0, 11.0, 14.0);
    p.rect(5.0, 1.5, 8.5, 4.0);
    p.rect(7.0, 7.0, 14.0, 14.5);
}

/// New page: a sheet with a plus.
pub fn new_page(p: &mut Pen) {
    p.poly(&[(9.0, 1.5), (3.0, 1.5), (3.0, 14.5), (13.0, 14.5), (13.0, 5.5), (9.0, 1.5)], false);
    p.line((8.0, 7.5), (8.0, 12.5));
    p.line((5.5, 10.0), (10.5, 10.0));
}

/// Line: a diagonal with end points.
pub fn line(p: &mut Pen) {
    p.line((3.0, 13.0), (13.0, 3.0));
    p.dot(3.0, 13.0, 1.25);
    p.dot(13.0, 3.0, 1.25);
}

/// Polyline: a bent line.
pub fn polyline(p: &mut Pen) {
    p.poly(&[(2.0, 13.0), (5.5, 4.0), (10.0, 11.0), (14.0, 3.0)], false);
}

/// Rectangle.
pub fn rectangle(p: &mut Pen) {
    p.rect(2.0, 4.0, 14.0, 12.0);
}

/// Circle.
pub fn circle(p: &mut Pen) {
    p.circle(8.0, 8.0, 5.75);
}

/// Arc: a half circle with its centre.
pub fn arc(p: &mut Pen) {
    p.arc(8.0, 11.0, 6.0, 180.0, 360.0);
    p.dot(8.0, 11.0, 1.0);
}

/// Terminal: a block with a screw ring between two leads.
pub fn terminal(p: &mut Pen) {
    p.rect(5.0, 2.0, 11.0, 14.0);
    p.circle(8.0, 8.0, 1.75);
    p.line((1.5, 8.0), (5.0, 8.0));
    p.line((11.0, 8.0), (14.5, 8.0));
}

/// Connector (brief 044): a socket's cup, open downwards, over a plug's pin, each on its lead.
pub fn connector(p: &mut Pen) {
    p.line((8.0, 1.5), (8.0, 3.0));
    p.arc(8.0, 6.0, 3.0, 180.0, 360.0);
    p.rect(7.0, 7.5, 9.0, 12.0);
    p.line((8.0, 12.0), (8.0, 14.5));
}

/// Junction: a dot where a line meets a line.
pub fn junction(p: &mut Pen) {
    p.line((1.5, 6.0), (14.5, 6.0));
    p.line((8.0, 6.0), (8.0, 14.5));
    p.dot(8.0, 6.0, 1.75);
}

/// Revision cloud: scallops round a box.
pub fn cloud(p: &mut Pen) {
    for x in [5.0, 8.0, 11.0] {
        p.arc(x, 4.0, 1.5, 180.0, 360.0);
        p.arc(x, 12.0, 1.5, 0.0, 180.0);
    }
    for y in [6.0, 10.0] {
        p.arc(3.5, y, 2.0, 90.0, 270.0);
        p.arc(12.5, y, 2.0, -90.0, 90.0);
    }
}

/// Show or hide: an eye.
pub fn eye(p: &mut Pen) {
    p.arc(8.0, 12.0, 7.21, 213.7, 326.3);
    p.arc(8.0, 4.0, 7.21, 33.7, 146.3);
    p.circle(8.0, 8.0, 2.0);
}

/// New symbol (brief 048): a symbol box on two leads, a plus at its top right.
pub fn new_symbol(p: &mut Pen) {
    p.rect(3.0, 5.0, 10.0, 12.0);
    p.line((6.5, 1.5), (6.5, 5.0));
    p.line((6.5, 12.0), (6.5, 14.5));
    p.line((12.75, 1.5), (12.75, 6.0));
    p.line((10.5, 3.75), (15.0, 3.75));
}

/// Connection point (brief 048, the builder's C tool): a hollow ring at a lead's end.
pub fn connection_point(p: &mut Pen) {
    p.line((8.0, 14.5), (8.0, 8.5));
    p.circle(8.0, 5.5, 3.0);
}

/// Text slot (brief 048, the builder's S tool): a dashed box with an anchor dot at its left.
pub fn text_slot(p: &mut Pen) {
    p.dashed((2.0, 5.0), (14.0, 5.0), 2.0, 1.5);
    p.dashed((2.0, 11.0), (14.0, 11.0), 2.0, 1.5);
    p.dashed((2.0, 5.0), (2.0, 11.0), 2.0, 1.5);
    p.dashed((14.0, 5.0), (14.0, 11.0), 2.0, 1.5);
    p.dot(2.0, 11.0, 1.5);
    p.line((5.0, 9.0), (11.0, 9.0));
}

/// Origin (brief 048, the builder's O tool): a cross with a ring at its centre.
pub fn origin(p: &mut Pen) {
    p.line((8.0, 1.5), (8.0, 14.5));
    p.line((1.5, 8.0), (14.5, 8.0));
    p.circle(8.0, 8.0, 3.0);
}
