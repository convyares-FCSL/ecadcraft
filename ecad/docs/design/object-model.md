# Object model: what is stored, and how things relate

This note says how ECAD's document holds its objects and why: what each kind of object is, where
its kind-specific data lives, and how objects refer to each other. The crate map and the document's
place in it are in [../developer/architecture.md](../developer/architecture.md); the words are the
glossary's ([../reference/glossary.md](../reference/glossary.md)).

## The principles

1. **Data, not inference.** What something is, and how it relates to other things, is stored in
   its type. No rule walks to a symbol, matches a name or keeps a guess to find out what a device
   is or which device a symbol belongs to.
2. **One struct per object type**, with its own fields. Kind-specific data lives in enum variants,
   so a wrong combination cannot be built.
3. **Shared behaviour is traits** implemented by the types. Each type owns its calculations as
   methods; checks, reports and power summaries call them and never recalculate.
4. **Relationships are stored explicitly**, as IDs, at the level they belong.
5. **Strict words.** An unknown word, field or invalid text in a file is a read error: the project
   opens read-only with a message naming it, and the file is never changed
   (`ecad-model/src/document/strict.rs`, the words check in ecad-electrical).
6. **No free text as a raw `String`.** Every text a person types is a validated `Text<N>` (one
   line) or `LongText<N>`: trimmed, 1 to N characters, with no control, hidden or bidi character
   (`ecad-model/src/naming/text.rs`). A file value not already in that form is a read error.
7. **No compatibility code before v1.** Fixtures and test files are regenerated; there are no
   shims for older shapes.
8. **No property bags** except display choices and per-object default values. A key that became a
   struct field is reserved: a bag holding it is a read error (`RESERVED` in
   `ecad-model/src/naming/keys.rs`).

Why: when the compiler knows every kind, field and unit, adding one is a compile error at every
place that must handle it. A rule that inferred meaning from a symbol name or a property word
would silently do the wrong thing for the new kind.

## Where the types live

Stored structs and enums are in **ecad-model**. Calculations that need the electrical rules or
tables are traits defined and implemented in **ecad-electrical** on the model's types
(`ecad-electrical/src/devices/traits.rs`: `Supply`, `Load`, `Protects`, `Senses`, `Signals`,
`Contacts`; `circuits/mod.rs`: `Circuit`; `plc/address.rs`: `Addressed`). So a power supply's
available power reads as its own method while Core stays layered. `Bom` (what goes on the bill of
materials) is answered from stored data, so it is in ecad-model.

## The objects

| Object | Stored as | Relates to |
| --- | --- | --- |
| Device | `Device`, with a `DeviceType` | its symbols (representations), its connection points, device links |
| PLC station, module | `DeviceType::PlcRack`, `PlcModule` | a module names its station; channels live inside the module |
| Wire (conductor) | `Wire` | its two connection points, its place in its net's order, its wire type |
| Route | `Route` | its page and what each end lands on: a connection point, another route, or nothing |
| Net | derived, never stored | from routes, jumper bars, device bridges and continuations |
| Cable | `Cable` | its cores, each naming the wire it carries; its cable type |
| Page | `Page`, with a `PageKind` | its folder, page type, sheet; its location and function |
| Folder | `Folder` | its parent; the number its pages start from |
| Project | `Project` | the location and function lists, revisions, user fields, the revision scheme |
| Drawing aid | `Aid`: a continuation or a location box | drawn by a representation; a continuation has a connection point |
| Loose part | `LoosePart` | its location and function; on the BOM, not drawn |
| Connection point | `ConnectionPoint` | its owner (a device or an aid), its key in the symbol |
| Representation | a subject, a page, items, a `Role` | what draws a device, wire, cable or aid on one page |
| Symbol | `Definition` | declares the device type and the part a placed symbol plays |
| Style records | one struct per catalogue | text style, wire type, cable type, page type, frame, location box, rating table |

## Device and its type

A device holds its tag (a `TagSlot`: none, automatic, fixed or provisional, the state holding the
tag), its location and function (each a `Place`: unset, manual or proposed, the state holding the
entry), its description and part data, its rating note, its values of the project's user fields,
and its `DeviceType`.

`DeviceType` has twelve groups, starting from IEC 81346-2's top-level classes: power supply,
protection, switching, sensor, motor, load, indicator, terminal strip, connector, PLC station, PLC
module, and general. Each group exists because it has fields and methods no other group has; a
group is added only when a symbol needs fields or a calculation no group has. Catalogue products
(one sensor model against another) are part data, never new variants.

**Where a type comes from.** The symbol declares it. Placing a symbol as a new device copies that
type, and only values are filled in afterwards. Adding a symbol to an existing device never changes
the type; changing it is one `ModifyDevice` edit. It is never worked out again from the symbol, the
tag or a property word.

**Ratings are typed fields.** Each group with a rating holds it as typed fields (a breaker's rated
current and curve, a fuse bank's rating per output, a motor's rated power). Text typed as a rating
is read once, at the edge, by one reader per group in `ecad-electrical/src/devices/ratings/`, used
alike by the app, MCP and the symbol defaults. Ambiguous input is refused naming what is missing;
text that is no rating at all goes to the device's rating note with the fields left empty. Checks
read only the typed fields.

## One device, many representations

A device is one thing however many symbols draw it: a contactor's coil on one page and its contacts
on two others are three representations of one device. Each representation's role records the part
it plays (`PartRole`: the head, a contact, and so on), set when the symbol is placed and stored. A
symbol representation without connection points shows its device's own definition; one with
connection points may show any embedded definition, and its connection points place the device's
connection points in that definition's frame, so they follow the symbol when it moves or rotates
(invariant I14 in `ecad-model/src/document/invariant.rs`). A device may have any number of
representations, of any definitions. Schematic and panel views of one device are two
representations, not two devices.

## Wires, routes and nets

What a person draws is a **route**: lines on a page that join connection points and each other.
Routes decide which connection points are on one net, nothing more. The physical **wires** run
between two connection points of the net, never to a T or a dot, in a stored order that ECAD
proposes and the person may reorder. Junction dots are drawn from the routes, never stored. How the
order is proposed and kept is in [nets.md](nets.md).

## Relationships at the right level

- Parent to child: a device to its symbols; a terminal strip to its terminal blocks; a PLC station
  to its modules (the module names its station); a module to its channels.
- Typed links between siblings sit at project level: `Document.device_links`, each with a kind
  (mechanical interlock, auxiliary block) and two devices. A linked device cannot be deleted until
  its links are removed, so every delete removes them first.
- The project holds the location and function lists; devices, pages, boxes and cable ends refer to
  an entry by ID, so renaming a code is one edit. No box ID is stored on a device: a box proposes
  its location to the devices inside it.

## Units and angles

Quantities have types (`Mm`, `Metres`, `Amps`, `Volts`, `Watts`, `Celsius`, `Percent` and more, and
a rated voltage that is AC or DC), so a value cannot be read in the wrong unit. Stored angles are
radians; commands take degrees and convert at the boundary.

## Ops change whole objects

Typed objects change through whole-object ops (`ModifyDevice`, `ModifyWire`, `ModifyCable`,
`ModifyPage`, `ModifyProject`, `ModifyAid`, `ModifyLoosePart`), each recording the object before
and after. `SetProperty` remains only for display choices and per-object defaults. A new op is
added only when existing ops cannot express a change cleanly.
