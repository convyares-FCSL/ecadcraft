# 048 Symbol builder: how to try it

Brief 048 (`docs/briefs/048-symbol-builder.md`, design `docs/design/048.md`). A symbol drawn once
in the project, with connection points, text slots and its own defaults, places like a built-in;
editing it later lists what changes on every placed copy before anything is saved.

Status: **048a, 048b and 048c built by the worker**; **not yet tried by the owner**. What is listed
under "Checked by the worker" ran as automated tests only.

## Draw a new symbol (048b, click only)

1. **Library** (Ctrl+3), then **New symbol** at the top right of the browser. The builder opens on
   an empty 2.5 mm grid. The right panel's **Before it can be placed** list shows what is missing.
2. **Draw** (D) ▸ Rectangle: click two corners to draw a 40 × 20 mm box. Draw ▸ Line: draw three
   short leads down from its lower edge (Esc ends a chain). **Text** (T): click above the box and
   type a group heading such as `POWER`, then Enter.
3. **Connection point** (C): click the end of each lead. A point goes on the 5 mm grid, and a
   click off it is refused at the cursor. Type a label (`L+`, `M`, `PE`) or press Enter for none.
4. **Text slot** (S): the Tag chip is chosen; click beside the box. R turns the selected slot 90°,
   and H and V step its alignment.
5. In Details, type a name, a key (`psu1`) and a tag prefix (`G`). When every row shows Done,
   **Save to project** turns blue. Click it. The browser shows the tile under **This project** as
   `Project · v1`. Click **Place**, then click the page three times.

## Edit it and read the review (048b)

1. Draw a wire from one copy's point. Right-click any copy ▸ **Edit symbol** (the wheel's top-left
   slot). Select the wired point, press Delete, and drag another point 5 mm.
2. **Save…** opens **Before saving** in the drawer: one row per copy, and the wired copy's row in
   red, naming the wire, unticked. **Update selected** is pre-selected.
3. **Save** writes the new version and the two updates in one step. Select the third copy:
   Properties reads `Symbol: v1 · update available (v2)` with **Update** and **Compare**.

## Make a symbol from the page (048c)

1. In Draw, select a few plain lines and a text such as `-LS1`, right-click ▸ **Make symbol** (the
   wheel's right slot). The builder shows them on the grid. Free line ends are offered as
   connection points and the tag-like text as the tag slot. Offers are drawn dashed, and nothing
   is applied until you accept them in Details.
2. **Accept all and save to project** replaces the drawing on the page with a device of the new
   symbol (untick **Replace the drawing…** to keep the lines).
3. A drawing made with Make component gets **Make symbol** on its device wheel. Saving it keeps
   the device, its connection points and its wires.

The builder keeps the draft while you work, also when you leave the Library or the app stops.
Drafts show in the browser under **Drafts**, with Resume and Discard. Save is the only write.

## A symbol by command (048a)

Agents and scripts use the JSON forms; the builder's Save runs the same commands.

1. Open any project. At the command line type (one line):

   ```
   symbolnew {"name": "Float switch", "key": "float1",
     "graphics": [{"kind": "polyline", "vertices": [[-2.5,-7.5],[2.5,-7.5],[2.5,7.5],[-2.5,7.5]], "closed": true}],
     "points": [{"key": "1", "label": "BN", "at": [0, 5]}, {"key": "2", "label": "BU", "at": [0, -5]}],
     "slots": [{"text": "tag", "at": [-4, 0], "h": "right", "v": "middle"}],
     "defaults": {"prefix": "B", "description": "Float switch"}}
   ```

2. `place {"symbol": "float1", "at": [100, 100]}` three times at different spots: `-B01`, `-B02`,
   `-B03`, each with points BN and BU and its tag left of the box, at the tag slot.
3. Draw a wire from point BU of `-B01`.
4. `symbolimpact` with the same content, `"definition": "<the id symbolnew returned>"` and only
   point 1: `-B01` is listed as a clash (`Point 2 is removed; wire … is attached to it.`), the
   other two as `Point 2 removed`. Nothing is recorded.
5. `symboledit` with `"update": "all"` is refused and names `-B01`; with
   `"update": "selected", "copies": [<the other two representations>]` it saves version 2 and
   updates those two. `-B01` keeps version 1.
6. `u` undoes the edit as one step.
7. `defaults {"key": "symbol.float1.prefix"}` reads `B` from `definition`;
   `setdefault {"key": "symbol.float1.prefix", "value": "S"}` makes the next placement `-S01`.

Text slots place a device's tag, description, rating (`rating.<label>` for one point's), part
number, maker, article number, function and location. A symbol whose definition has slots places
its texts there instead of in the automatic tag block.

## Commands

| Command | Does |
| --- | --- |
| `symbolnew {name, key, graphics, points, slots?, defaults?, bridges?, from?}` | Embeds version 1 of a project symbol; one transaction |
| `symboledit {definition, …content…, update: all \| selected \| none \| new, copies?, builtins?}` | A new version and the chosen placed copies updated; editing a built-in makes a project copy that remembers its source |
| `symbolimpact {definition, …content…}` | Read only: every placed copy, what changes there and whether it clashes |
| `symbolupdate {copies \| lineage, to?}` | Brings placed copies to the newest (or `to`) version; refused while one clashes |
| `makesymbol {handles? \| component, name, key, points?, slots?, defaults?, replace?}` | A symbol from plain lines, arcs and texts (points and slots in page coordinates; or `graphics` as edited plus `at`, all in the symbol's frame), replacing them with a device; or a drawn component promoted, keeping its component, connection point and wire IDs |
| `place {symbol: <project key>, at, device?}` | The newest version of that project symbol; `device` for a symbol that is part of a device |

An update keeps every component, connection point and wire ID; the end of a wire on a moved point
follows it. A connection point label that was the old default takes the new one; a label set by
hand stays. A version no copy uses any more after an update is removed in the same transaction
(history keeps it).

## Checked by the worker (automated, not owner-tried)

- `cargo test -p ecad-model lane_048`, `-p ecad-file lane_048`, `-p ecad-cadcraft lane_048`,
  `-p ecad-mcp lane_048`: the op `SetComponentDefinition` (apply, undo, diff, merge), the
  definition's new fields and their content hash, design 048 §12 rows 1–9 and 11–14, and the MCP
  contract.
- `cargo test -p ecad-trial lane_048`: the builder, the review with a clash, Make symbol from
  lines and from a drawn device, the wheel slots, Compare and Update; screenshots with
  `ECAD_TESTER_SHOTS`.
