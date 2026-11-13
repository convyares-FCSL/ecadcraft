# 048 Symbol builder: how to try it

Brief 048 (`docs/briefs/048-symbol-builder.md`, design `docs/design/048.md`). A symbol drawn once
in the project, with connection points, text slots and its own defaults, places like a built-in;
editing it later lists what changes on every placed copy before anything is saved.

Status: **part 048a** (model and commands) built by the worker; **not yet tried by the owner**.
The builder in the Library workspace (048b) and Make symbol from the page with "update available"
(048c) follow on the same branch. Until then a symbol is made through the commands below, typed at
the command line as JSON or sent by an agent over MCP. What is listed under "Checked by the
worker" ran as automated tests only.

## A symbol by command (048a)

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
| `makesymbol {handles? \| component, name, key, points?, slots?, defaults?, replace?}` | A symbol from plain lines, arcs and texts (points and slots in page coordinates), replacing them with a device; or a drawn component promoted, keeping its component, connection point and wire IDs |
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

## Not yet

- The builder (Library ▸ New symbol, Edit symbol on the wheel, the review drawer): part 048b.
- Make symbol from the selection wheel, "update available" in Properties and Devices: part 048c.
