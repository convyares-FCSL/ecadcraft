# Devices: the device comes first

A device (`-K01`) is a real part with a tag, its data, and any number of symbols that draw it on any
pages. This note says why the device, not any one symbol, holds everything, and how a device's
location and function are kept. How to place and edit devices is in
[`../user/devices.md`](../user/devices.md) and [`../user/locations.md`](../user/locations.md); the
object model behind it is in [`object-model.md`](object-model.md).

## The device is the master and is never drawn

All of a device's data is stored on the device: its tag, description, part data, location, function,
rating and type. Its symbols (representations) only draw it, and **every symbol is equal**: there is no
"main" symbol. A person can place a contact before the coil, delete the first symbol placed, or draw
the device on twenty pages, and nothing about the device changes.

Why: keeping the data on whichever symbol came first would make the result depend on the order of
work. A designer often draws a contact on page 7 before the coil on page 3; the device must be the same
device either way.

**What a device is** is its `DeviceType`, stored on the device and never worked out from its symbol,
tag or a property word. The symbol declares the type, and placing that symbol as a new device copies
it. Adding another symbol to the device never changes the type; changing it is one `ModifyDevice`.

## A symbol's part in the device comes from its definition

Each symbol has a **part role**, set from its definition when it is placed, never from placement order
or ID:

- **head**: the device's exclusive symbol (coil, actuator, breaker body, PLC card body);
- **contact**: normally open, normally closed, change-over, power poles;
- **pole**: a further part of a multi-part device (a fuse bank output);
- **terminal block**: a strip position or level;
- **channel**: a PLC channel cell;
- **half** and **pin**: a connector half and its pins;
- **other**: anything else (a symbol with no starter kind).

The relations between a device's symbols are derived within the device, never stored: a contact's
**parent** is the head; a head's **children** are its contacts, poles and channels; **siblings** are
the other symbols of the same role (the poles of a breaker, the levels of a strip, a pin and its mate).
That is enough for references and contact mirror tables (see [`references.md`](references.md)); a
relation that cannot be derived from role and device would need a stored link, and none does today.

Each symbol also has a **view**; today every symbol is a schematic symbol. The view exists so a panel
layout view adds a variant and a source, not a new model.

**The head.** The head is the head as soon as it exists, whichever symbol was placed first. Ties go by
drawing order (book position, then the sheet's zones, then top-left, then ID), which is stable under
random IDs. Placing a second head on a device is refused; only a merge can make one, and the Checks
list shows it (`two-heads`). A device is numbered at its head, else at its first symbol in drawing
order, so symbols can be placed in any order before numbering.

## Location and function are stored on the device

A device's **location** (`+CP1`, the IEC 81346 `+` aspect) and **function** (`=F1`, the `=` aspect)
are what is stored on it. Unset means none, shown `?`. Nothing is inherited from boxes, pages or the
project: those only **propose**.

Why stored and not derived from where the symbols sit: a device's symbols sit on many pages and in
many boxes. Deriving its place from them would give different answers depending on which symbol you
looked at, and moving one symbol would silently move the device. The designer's statement of where a part is must
not change behind their back.

Each value is stored with how it came to be, as one value (`Place`): **unset**, **manual** (typed, or
set by hand) or **proposed** (taken from a box or a page). The state carries its entry, so a state
without an entry cannot be written. A value set by hand that disagrees with a box holding one of the
device's symbols is an error row on the Checks list; a value used by none of the device's symbols is a
warning. Neither is ever refused.

### Proposals

When a symbol is placed or moved, a box is drawn or given another entry, or a page's or the project's
value changes, the **context** there proposes a function and a location for the devices concerned.
A symbol's context for an aspect is the innermost box carrying that aspect at its anchor, else its
page's value, else the project's.

- A device with no value takes the proposal, in the same transaction (after a move, in one follow-up
  transaction, so a first undo removes the location and a second the move).
- A device with the same value: nothing.
- A device with another value: a **conflict**. It is listed and asked about, never changed silently.
  The default answer is always **Leave**; the person may **Move** one, all, or choose per device and
  per aspect.
- JSON calls never prompt: their `location` and `function` parameters decide (`propose`, the default,
  sets unset devices and leaves conflicts; `move` also moves conflicting devices; `keep` proposes
  nothing; a code sets that value).
- Erasing a box changes nothing: no box does not mean "no location".

The follow-up after a move is a separate transaction so the movement code stays free of location
rules.

Where a symbol's context differs from its device's value, its tag shows the device's value as a
prefix (`=F1+CP1-K01`, function then location then product, as IEC 81346 orders them). That reads
only the symbol's own page, so it never makes another page stale.

### Part of a device somewhere else

A device may store an override of its location for one kind of symbol, `location.<role>`, meaning
"this part of the device is somewhere else": a connector's socket in the cabinet and its plug on the
field cable (`location.socket`, `location.plug`), or a field motor's terminal blocks drawn in the
cabinet. The head has no override (the head is the device). A strip's positions and levels always take
the strip's location. Functions have no overrides: a part of a device serves the same function.

## A connector is one device with two halves

A connector (`-X10`) is one device. Each pin of each half is one connection point: its label is the
pin name, shared by both halves, and its key says the side (`.s` socket, `.p` plug). Each half's
definition bridges `.s` with `.p`, so pin 3 of the socket and pin 3 of the plug are one net and one
wire number, on any pages, and nothing else is joined.

Why one device and not two: a tag is an identity, so two devices `-X10.S` and `-X10.P` would be two
tags related only by their spelling, which renumbering, retagging and merging would have to keep in
step. One device keeps one tag, one number and one reference group, and still gives a BOM row and a
location per half (through the `location.socket` and `location.plug` overrides). The same dotted-key
rule serves terminal block levels (see [`terminal-strips.md`](terminal-strips.md)), so connectivity
needs no catalogue to read it.

## The project's list of functions and locations

The project keeps one list per aspect. Each **entry** has a stable key, a code (`+CP1`) unique in its
aspect, and an optional description. Devices, boxes, pages and the project store **the key**;
everything a person sees shows the code. So renaming an entry is one change and every tag, box label,
title block field, BOM row and report follows it: references use IDs, never names.

Typing an unknown code anywhere adds an entry in the same transaction, and says so. Entries are
managed with `designations`, `designationadd`, `designationedit` (rename), `designationmerge` and
`designationremove` (refused while anything uses the entry). `setlocation` and `setfunction` set a
device's value directly, with no prompt. Only a merge can leave a key with no entry or two entries
with one code; both are Checks rows with a way out.

## Location boxes

A location box is a drawing aid carrying a location, a function or both, drawn as one closed outline
on a page (`locationbox`, `functionbox`). Its label is derived from its entries. Containment is point
in polygon, with a small tolerance on the edge; the innermost box is the smallest by area. Boxes only
propose; they never own a device.
