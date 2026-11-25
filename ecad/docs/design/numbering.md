# Numbering

How tags, wire numbers and terminal block labels are numbered, and why there is one mechanism for all of them.
How to use Renumber and Finalise is in [../user/devices.md](../user/devices.md) and
[../user/wires.md](../user/wires.md); page numbers are in [../user/pages.md](../user/pages.md).

## The idea

The designer draws first, then numbers everything in drawing order in one undoable step, with a preview. A
clash is never a dead end. Numbers a person chose by hand are never changed by a renumber.

## One planner

One planner in the model serves every numbering job: Renumber (`renumber`), Finalise (`finalise`), the next
free number offered when a tag is taken, and a merge's renumber choices. It reads the project and a request
(what kind, which items, start, digits) and returns a plan: one row per item in scope (old, new, or why it is
kept) and the ops that write it. It writes nothing itself; the command commits the ops as one step.

Why one planner: devices, cables, wires and terminal blocks must agree on order and on what is taken. Four
mechanisms would drift apart, and a renumber in one would collide with another.

The plan is built on request only (a preview, an Apply), reads each item once, sorts, and writes one op per
changed item. It never runs on commit.

## Drawing order

Page in book order, then the column and row of that page's frame grid, then top to bottom and left to right
inside a cell, then ID. Each item has one **anchor**:

- a device: its main representation, else its first in drawing order;
- a cable: its first representation;
- a net: its first wire's route start;
- a terminal block: its drawn symbol (terminal blocks are numbered within their strip).

An item with no anchor sorts last. All kinds share this order, so a renumbered drawing reads the same way for
tags, cables and wire numbers.

## Numbers

- Numbers are handed out per tag prefix (devices and cables) or per wire-number scope, `start`, `start + 1`, …
  in drawing order, skipping every number taken by an item not being renumbered: fixed items, items out of
  scope, the kind not chosen.
- **A plan never produces a duplicate.** The duplicate rule holds on the step's end state, so exchanges such as
  `-K01 ↔ -K02` are plain ops.
- **Devices and cables share one tag namespace.** A device `-W01` and a cable `-W01` clash.
- Terminal blocks are numbered per strip, feed-through and PE in one sequence. A relabelled block keeps its ID,
  so its wires stay attached.
- **High-water mark.** After a renumber, each prefix whose tags changed has its high-water mark reset to the
  highest number now in use, so new devices continue after the renumbered set, not after the old top. A
  deleted tag is otherwise not reused.

Which items: all, a selection, one prefix (or one potential for wires), one folder, a set of pages, or the
provisional items only. Items that are fixed, have no tag, or have a tag not in the usual form (letters then a
number) are left alone and listed as skipped with the reason.

## Preview, then apply

The Renumber dialog shows the plan before anything is written. Apply passes back the project's change count
the preview was made at; if the project changed meanwhile (an agent edit, another window), the command refuses
("The project changed since the preview; check the new list and Apply again.") and the dialog recomputes.

Why: a preview that may be out of date is worse than none. Refusing a stale apply keeps what is written equal
to what the person saw.

## Fixed

A tag's state is stored with the tag, never read off its spelling: a tag is automatic, **fixed** or
provisional (a device's or cable's `TagSlot`). A wire number may likewise be fixed.

- **Typing a tag fixes it**, as typing a wire number does. Place and a new cable give automatic tags.
- Renumber and Finalise never change a fixed item.
- Fixed is shown in lists and the Properties palette, never drawn on the sheet, so prints stay clean.

Why: a number someone chose (to match a site label or an old drawing) must survive every renumber.

## Provisional tags

A project can choose **provisional tags until Finalise** (`numbering`), off by default. Then a new device gets
a placeholder (`-K?3`), a new cable `-W?1`, and a new net no wire number (its label shows `24V-?`).

- A provisional tag is stored bare with the state *provisional*; the `?` is added only when it is shown, and
  read once when typed. A provisional `-K3` and a real `-K3` are different tags.
- The high-water mark skips provisional tags.
- **Finalise numbering** gives the provisional devices and cables real numbers in drawing order, after each
  prefix's highest number in use, and numbers the unnumbered nets: one step.
- The Checks tab lists provisional tags and unnumbered nets, each with **Finalise**.
- Issuing a revision with provisional items present asks first: finalise them (Finalise and the issue are one
  step) or issue as they are. A release lists them as a release gap.

Why: early in a design the numbers mean nothing yet. Placeholders that are obviously not final, replaced in
one step before issue, avoid renumbering an issued drawing.

## A tag that is taken

Wherever a tag is typed (Properties, the Devices list, `setprop`), a tag another device or cable holds is never
a dead end. The person chooses:

1. **Swap**: the two exchange tags;
2. **Make it the same device** (or cable): the typed one joins the holder, when they can be one;
3. **Use the next free number**.

Without a choice, the JSON form refuses naming all three. A choice that is impossible (the holder is a cable,
a device already has a coil) is shown with its reason while the others remain. Wire numbers have their own
three choices (see [nets.md](nets.md)); a merge's tag conflicts use the same planner (see
[history.md](history.md)).

## Why it scales

Numbering is an action, not a view: it keeps no update map. The plan reads each item once and sorts, so a
renumber of the large bench project is one pass, and nothing is computed on ordinary commits.

Background: [archive/designs/](../archive/designs/) holds the original numbering design.
