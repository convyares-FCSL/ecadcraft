# Terminal strips: why the model is shaped this way

A terminal strip (`-X01`) is a tagged run of terminal blocks on a rail, with its accessories and jumper
bars. This note says what is stored, what is derived, and why. How to place and edit strips is in
[`../user/terminal-strips.md`](../user/terminal-strips.md); the words are in
[`../reference/glossary.md`](../reference/glossary.md).

## A strip is one device, its blocks are connection points

There is no strip object. A strip is a **device** whose definition is a terminal block starter, and
each drawn level of a terminal block is one **connection point** of that device, drawn by its own
symbol. The strip's tag is the device's tag.

Why: a strip is bought, listed and numbered like any device, and its blocks are drawn wherever their
circuits are, on many pages. The device model already gives one tag, many symbols on many pages, and
references between them. A separate object would have needed its own copies of all of that.

The strip's typed data (`TerminalStrip` in `ecad-model`) holds only what needs a type: the rating of
each fuse position. Everything else about a strip is an ordinary property or is derived.

## Positions and levels

- A **position** is one terminal block on the rail: the strip's connection points that share one
  position label (`3`).
- A **level** is one deck of a position. It is read from the connection point's key: `.1`, `.2`, `.3`
  for one-point levels, `.1i` / `.1o` for the in and out points of a two-point level. A key that does
  not start with `.` is level 1 of a one-level position.

Why the position label on every level, and not `1.L` as the label: renumbering, accessories, jumper
bars, spare positions and the BOM all work by position. Keeping the position label shared makes them
work per position unchanged; the label a person sees is derived. Why the dot in the key: it lets
`ecad-model` tell a level from a plain point without knowing the catalogue.

The **shown label** of a level is the strip property `block_label.<position>.<level>` when set, else
the position for a one-level type, else position + separator + level name (`1.L`). The separator and
level names are registry defaults, resolved strip → project → built-in, so a house style can rename
levels without touching any jumper bar (levels are stored as numbers).

## Terminal block types are catalogue code, not defaults

A position's type comes from the starter of the symbols that draw it; spare positions get theirs from
the strip's offer. The catalogue (`terminal_strips/terminal_blocks.rs`) holds what a part **is**: its
starter, offer code, levels, internal bridges, whether it breaks the net, and where the PE mark goes.
These are facts about a physical part, so they are code, not registry entries; only names, BOM
descriptions and the separator are defaults.

The types are: feed-through, PE, two-level, two-level bridged, three-level, L/N/PE, sensor,
disconnect and fuse. When the levels of one position are drawn with symbols of different types, the
first drawn level decides and the Checks list says so. Adding a type is a catalogue row and its
starters, with no change to the file.

**Disconnect and fuse levels break the net**: their in and out points are two connection points, two
nets and two wire numbers, and nothing joins them. A jumper bar or internal bridge never crosses from
in to out. This is the physical truth of the part: opening the disconnect separates the circuits.

## Three ways blocks are joined, kept apart

1. **Internal bridges** belong to the type (the bridged two-level block joins its decks inside). They
   are stored as the bridges of the level symbol's definition and join only the levels of the same
   position. Two positions of a bridged type stay two nets.
2. **Jumper bars** are parts on the strip (a plug-in or screw bridge). See below.
3. **Wire bridges** are real wires whose two ends are on two blocks of one strip (`bridge`, Draw
   Bridge). A bridge made of wire is a wire: it stays in the wire report, can be erased and undone,
   and nothing in the file says "bridge"; it is recognised from its two ends.

Keeping them apart matters for the BOM and for the person reading the plan: a jumper bar is bought,
a wire bridge is cut from wire, an internal bridge comes with the block.

## The jumper bar model: owned by the strip, never a conductor

A jumper bar joins positions of one strip electrically: one net and one wire number, with **no
wire**. It is stored as a strip property, one key per jumper bar:

- `jumper_bar.<n>` lists the positions: ranges `1-4` (every position from 1 to 4 in plan order) and
  single labels `7, 9, 11` (a skip bar), stored in canonical form.
- An endpoint may name a level (`4.2`); a range whose two ends share a position joins levels of that
  position (`1.1-1.3`, a cross-level bar); any other range joins one level across positions
  (`1.2-8.2`).
- `jumper_bar.<n>.part_number` and its maker and description give one bar its own part (a red bar
  for 24 V, a blue one for 0 V); otherwise the strip's, then the project's part is used.
- `<n>` is given when the bar is made (highest + 1 on the strip) and is never reused while the key
  exists, so it names the bar in selection, MCP and merges.

`ecad-model`'s connectivity reads these keys (catalogue-free) and joins the connection points they
name; unreadable values join nothing and are never an error there. A jumper bar edit touches the nets
of every block in the bar before and after, so the wire numbering pass merges or splits wire numbers
in the same transaction, and one undo undoes the bar and its numbers.

Why owned by the strip and not drawn as a wire: the designer's reference drawings show jumper bars as
part of the strip, the BOM needs them as parts counted in bars, and a jumper bar joins blocks that are
often drawn on different pages. A wire would need a route on one page. Spare positions may be in a
jumper bar (the bar is bought for them); only drawn blocks join nets.

**Drawn, never stored.** On every page that shows blocks of a jumper bar, a dashed line is drawn
through them in plan order, derived on each draw and never editable as geometry. Overlapping and skip
bars run in **lanes** (greedy by first position, then number), offset so they never cross; the plan's
jumper bar column uses the same lanes, so the page and the plan agree. Where a bar continues past the
blocks shown on a page, a short stub runs on, as the reference convention does. Erasing a line deletes
its jumper bar, saying so.

**Made where you draw.** The Jumper tool (`jumper`) feels like drawing a wire: each pick adds the
positions from the previous pick (or only the pick, with Skip), and a first pick on an existing bar
extends it. The same `jumper` command takes typed positions too, so scripts and agents make the same
property. Making bars only from a table was considered and rejected: the designer could not make or
see a bar where they draw.

**Bought in bars.** Per jumper bar, bars = ceil(span / bar length), where span counts positions in
plan order (spare included, accessories not); the bar length comes from the strip, then the project,
then the built-in default.

## Accessories are placed, never inferred

End stops, end plates, partitions, strip labels and markers are strip properties naming where they
sit (`acc.start`, `acc.end`, `acc.after.<label>`, `acc.on.<label>`). Nothing is placed by a rule:
the designer decides, and the plan, the BOM and the strip pages read what they placed. Missing part
data and odd placements are Checks rows; nothing is refused because of a check.

## The terminal plan is derived

The terminal plan (`stripplan`) and the strip pages are built from the drawing every time, never
stored. Each row is one level, grouped by position in plan order; a position whose type has more
levels than are drawn shows **free** levels; positions the strip offers (`12`, `12+2PE`, `12+2PE+4x3`)
with no drawn block are **spare**. Counts and BOM quantities are in positions.

Which side a wire is on is derived per row, in this order: a cable core is **outside**; else the far
device's location against the strip's; else the route's direction at the block (up or left is
**inside**). The order puts the designer's facts (cables, locations) before geometry, which is only a
guess.

The row is laid out **inside | strip | outside**, mirrored, with the fields a person may edit (the
position, a level's own label, a fuse rating, jumper bars, accessories) kept visibly apart from the
derived ones. Strip pages draw the same rows, either as a table or as the strip graphic
(`terminal_block.plan.style`, resolved page → project → built-in).
