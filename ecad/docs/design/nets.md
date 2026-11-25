# Nets, wires and wire numbers

What makes connection points one net, how wires are joined, how wire numbers are kept, and why. How to draw
and number wires is in [../user/wires.md](../user/wires.md); the Checks rows are in
[../user/checks.md](../user/checks.md).

## Routes, wires and nets

Three things are kept apart:

- A **route** is drawing: a line a person draws on one page. Each of its two ends lands on a connection point,
  on another route (a T), or on nothing (an open end).
- A **wire** (in code `Wire`) is a physical conductor between two connection points. Wires come from the
  routes: after each edit, the nets whose set of connection points changed get their wires rewritten in the
  same step. Nets whose points did not change keep their wires untouched, even when their routes are redrawn.
- A **net** is everything electrically joined. It is derived, never stored.

Why: what a person draws (a line with a T in it) and what an electrician installs (wires from point to point,
each landing on a connection point) are different things. Keeping routes as drawing lets the drawing be tidy
while the wire list, wire numbers and terminal plan stay true to the installation.

## What joins a net

Only stored references join; geometry is never consulted after the drawing step.

1. a route end that lands on a connection point;
2. a route that lands on another route (a T): the route is part of the net;
3. **continuations with the same name**, on any page (names are trimmed and compared without case);
4. a symbol's **bridges**: connection points joined inside the device by its definition;
5. **jumper bars** on a terminal strip, which join the positions (and levels) they name.

Nothing else joins:

- **Crossings never connect.** Two routes that cross are two nets, whatever the drawing shows.
- A loose end lying on another route is **not** joined. The page marks it and the Checks tab lists it as
  *Looks connected*, with a fix that joins it.
- A connection point with nothing on it is a net of its own and has no wire number.

## Joining while drawing

When a line is drawn, each of its ends is resolved in this order:

1. **A connection point there:** the end lands on it.
2. Otherwise, what meets at that point:
   - nothing: a deliberate open end;
   - exactly one free end of another route: the new line **extends that route** (one route, one wire);
   - the middle of another route, or several free ends: the end **lands on that route** (a T).
3. **Reverse T:** a free end of another route lying on the new line's middle lands on the new line.

Joining can be switched off for one drawing (the Wire tool's **Join** keyword, or `join: false` in the `wire`
JSON form). Then only step 1 runs and the ends stay open where they meet other lines.

A join is **refused, with nothing written**, when it would contradict what the wires already carry:

- the two nets carry different potentials ("joining them would short them");
- both nets have different fixed wire numbers.

Every refusal names the way out: draw with joining off, or end the line on a connection point. Ends where a
cable reference sits are never joined.

`joinwires` repairs what already meets: two open ends at a point become one route; an open end on a route's
middle, or three or more ends, become a T.

Why these rules: wires that look joined should be joined, but a join must never silently short two potentials
or lose a number someone fixed by hand. A refusal with a way out is better than a guess.

## Keeping nets current

Nets are kept up to date with every op, never rebuilt per edit:

- A **join** (an end attached, a continuation named into a group, a bridge added) unites two nets at once: the
  smaller net's members are relabelled into the larger.
- A **cut** (an end detached, something removed, a continuation renamed out of a group) only marks the net
  suspect. Once per step, a search runs from each side of each cut, smallest first; searches that meet join.
  The parts that fell off become new nets. A cut costs about twice the smaller part when the net splits, and
  the net's size when it does not; never the project.
- Every op that does not touch connectivity changes nothing.

A full build happens only on open, reload and branch switch. A test compares the kept nets with a fresh build
after each step of a long random sequence.

## Wire numbers

A **wire number** is what a net carries and every one of its wires shows (`24V-07`). Wire numbers are
**stored** on the wires, not derived. Why: people mark them on real wires, so once issued they are user data;
adding a wire must never renumber others.

The **numbering pass** runs inside every ECAD commit, after the command's own ops, on the nets that step
touched only, and adds its ops to the same step, so one undo takes back the wire and its number. It never runs
on undo, redo, replay or open.

| Situation | Result |
| --- | --- |
| a new net | the next free number of its scope |
| a wire added to a numbered net | it gets the net's number |
| two automatic nets join | the number on more wires wins (a tie goes to the lower sequence number, then text order) |
| a fixed and an automatic net join | the fixed number wins |
| two different fixed numbers meet | nothing is rewritten; Checks reports it |
| an automatic net splits | the larger part keeps the number, the other gets the next free |
| a fixed net splits | both keep it; Checks reports it |
| the net's potential changes | an automatic number with another prefix gets the next free of the new potential |
| settings change | nothing until Renumber Wires |

The project chooses what one number counts (`wiring.number_by`): per net, or per wire, where each wire of a
net is its own group. Switching it renumbers in the same step. The numbering scheme is by potential, by page,
or by page and column.

**A typed number already used by another net is never a dead end:** the person can swap numbers, take the
next free number, or join the nets with a pair of continuations named for the number (offered only when that
makes sense electrically, otherwise with the reason).

## Potentials and continuations

A net's potential is the potential its wires carry most, or else the name of a continuation in it that is in
the potentials table. Only names in the table give a potential: a signal continuation (`MOTOR-RUN`) does not
become a number prefix. A continuation whose name is neither a potential nor its net's own number is placed,
never refused, and shows on the Checks tab as a warning.

## Wire labels

Labels are derived when a page is drawn, never stored, and only the page on screen is drawn. The project's
label rule says whether a net is labelled once per page (`net`, the built-in) or every wire is (`each`, with a
shortest labelled length). In both, every net drawn on a page is labelled there at least once, and a wire with
its own size or colour gets its own label. Labels never touch another label or a wire.

## Why it scales

Each kept structure says which ops change it and does nothing for the rest: nets from connectivity ops, labels
from route, wire and wiring-setting changes on the pages concerned, looks-connected marks from route and cable
reference changes on the page drawn. Nothing scans the project per edit.

Background: [archive/designs/](../archive/designs/) holds the original connectivity and wire-joining designs.
