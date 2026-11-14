# Brief 052: The device-first model

Status: Draft
Branch: `feature/device-first`, from `origin/dev` `8b6c2ccd` (v0.2.0 plus #102, #103).
Designer: **yes** (`docs/design/052.md`) · Size: XL, in four parts (052a M, 052b L, 052c S, 052d L)
Roadmap: v0.2.1. It runs alongside 053 (performance) and the CI harness. Lanes: D (`locations.rs`,
`devices.rs`, `connectors.rs`, `xref.rs`, new `reps.rs`), U (inspector, Checks), and one
`ecad-model` change (`numbering.rs`).

## Goal

The device is the master. Location, part data, ratings, function and every other device-level datum
are stored on the device, and every symbol is an equal representation of it. Each symbol has a
**type** taken from its definition (head, contact, pole, terminal, channel, half, pin). Relations
between symbols (parent and child, sibling, cross-view) come from those types. No symbol is
privileged by a `main` role. Location boxes and page locations **propose** a location when you
place or move a symbol: the proposal is an explicit, undoable "set location". Nothing derives a
device's location from geometry live.

**Function (`=`) works the same way, now** (owner, 2026-11-14). It is stored on the device,
function boxes and pages propose it, and it uses the same conflict popup and the same `?` rule.
The project keeps a **list of functions and locations** (Settings ▸ Project info, rev 3 board
5.01): each entry has a code, a description and the number of devices that use it. Devices store
which entry they use, so **renaming an entry renames it on every tag**.

## Why

- **The `main` role is an accident of placement order.** The first symbol placed becomes `main`
  (`devices.rs`, `connectors.rs`, `commands.rs`). Three readers then treat it as the device:
  - `locations::main_rep`: role `main`, else the only symbol, else the earliest page, top-left;
  - `numbering`'s `prefer_main` anchor;
  - `numbering::rep_anchor` for Checks.

  Strips, multi-level terminals and pin-only connector halves have no natural master, so the
  fallback decides.
- **Live derivation crosses pages.** A device's location comes from the context of its main symbol
  (box, else page, else project). So moving one symbol, or editing a box or a page location, can
  change the located tags on other pages. That was #75's redraw bug, fixed in #102 by
  `project::located_tag_pages`. Every new view would need the same widening.
- **The owner's design discussion (v0.2.1)** settled it. The device is the master and is never
  drawn. Its symbols are equal. The cross-reference head comes from the definition kind. Boxes
  propose, and they never change a device silently.

## What exists today (`origin/dev` `8b6c2ccd`)

- `Representation { subject, page, form, role: String, items, connection_points }`. `role` is
  `main` on a device's first symbol, the starter key on joined ones, `box` on location boxes,
  `slash` and `reference` on cables.
- 018 refuses a second exclusive symbol ("already has a coil"; the fallback noun is "main symbol").
- 024 §3.2–3.3: a device's location is its own `location`, else the context of its main symbol.
  Its symbols show `+LOC-TAG` where their own context differs. Checks warns "own location differs
  from its box".
- 025: the head is the lowest-ID `Kind::Fixed { exclusive: true }` symbol. That is already
  type-based, but "lowest ID" is arbitrary with random IDs.
- 044: a connector half's "main symbol" is its whole-half symbol, else its first pin in book order.
  It decides the half's location (`half.<s>.location`, else the connector's own, else its context),
  its BOM go-to and its reference class.
- 028, 030, 036: a strip's terminals and levels are symbols of one device with no master.

## §0 Files touched

Design §0 has the full table. In short:

| File | Change |
| --- | --- |
| `ecad/crates/ecad-cadcraft/src/reps.rs` (**new**) | `RepType`, `rep_type`, relations (`parent`, `children`, `siblings`, `views`), go-to anchor |
| `ecad/crates/ecad-cadcraft/src/locations.rs` | Stored location only: `resolve`, `rep_location` (overrides); `main_rep` and `own_differs` go; `propose` (context, unchanged) and `plan_proposal` |
| `ecad/crates/ecad-cadcraft/src/devices.rs`, `connectors.rs`, `commands.rs`, `xref.rs` | Stop writing `main`; read types; join and place propose a location |
| `ecad/crates/ecad-cadcraft/src/project.rs` | `located_tag_pages` narrows; the move follow-up (§5.4) |
| `ecad/crates/ecad-cadcraft/src/outputs.rs` | **Overlaps 053.** Device rows lose `main` and the box/page scope rows; go-to by `reps::goto` |
| `ecad/crates/ecad-cadcraft/src/output_cmds.rs`, `interact/location_form.rs` | Box tool: propose and conflict prompt |
| `ecad/crates/ecad-cadcraft/src/location_cmds.rs` (**new**) | `setlocation` / `setfunction`, `adoptdesignations` |
| `ecad/crates/ecad-cadcraft/src/designations.rs`, `designation_cmds.rs` (**new**) | The project's list of functions and locations: entries, use index, `designations`, `designationadd`, `designationedit` (rename), `designationremove` |
| `ecad/crates/ecad-cadcraft/src/sheet_cmds.rs`, `props.rs`, `frame.rs` | Page and project values stored as entry keys and shown as codes |
| `ecad/apps/ecad-trial/src/settings/project.rs` | The **Functions and locations** card (replaces its placeholder) |
| `ecad/apps/ecad-trial/src/tools.rs` | **Function Box** tool |
| `ecad/crates/ecad-cadcraft/src/checks.rs`, `ecad-trial` inspector and Checks | New check rows; Location row without "(from …)" |
| `ecad/crates/ecad-model/src/numbering.rs` | `prefer_main` becomes a rank hook from the adapter |
| `ecad/crates/ecad-bench/src/generate.rs` | Role `main` → starter key |
| Tests `tests/all/lanes/lane_052*.rs` in ecad-cadcraft, ecad-mcp, ecad-trial, ecad-model | Design §10 |
| `ecad/docs/quick-start/052-device-first.md` (**new**), `ecad/docs/glossary.md` (`location` row) | Docs |

Not touched: `ecad-store`, `ecad-file`, upstream, CI. **No `.ecad` format change and no new op**
(design §2).

## Scope

1. **Representation types and relations** (design §3): `reps::rep_type` from the definition's
   starter kind, plus relations derived from types within one device. 025's reference rule, 044's
   halves, numbering order and go-to all read them.
2. **Drop `main`** (design §4): no code writes it, and no code gives it meaning. Every reader in the
   §4 table gets its replacement.
3. **Location stored on the device** (design §5.1–§5.2): a device's location is its `location`
   property, else none. A representation's location is the device's override for its type
   (`location.<type>`), else the device's. Strip positions and levels inherit the strip's. Tag
   prefixes stay (each symbol's own context against its device's stored location).
4. **Propose location** (design §5.3–§5.6): placing, joining, moving, drawing or reshaping a box,
   relabelling a box and changing a page or project location propose a location. Unset devices
   take it in the same transaction. A device with a different location is listed and asked about
   (Move or Leave); it is never changed silently.
5. **Function, the same way** (design §5.8): stored on the device, with no overrides. Boxes carry
   a function, a location or both, and a function-only box is drawn dash-dot. Tags show
   `=F1+CP1-K01` where a symbol's context differs. One popup covers both aspects.
6. **The project's list** (design §5.9, §9.4): entries with key, code and description. Devices,
   boxes, pages and the project store keys. A typed unknown code adds an entry. Rename, describe,
   remove-when-unused, and device counts are in Settings ▸ Project info.
7. **Opening today's projects** (design §7): nothing is written on open. **Adopt Functions and
   Locations** (one command, one transaction) turns stored codes into list entries (Q10) and gives
   unset devices what today's derivation would give.
8. **Checks** (design §6): unused and missing location or function; an entry that is no longer
   in the list; duplicate codes after a merge; two heads.

## Acceptance criteria

Design §10 lists every criterion. The main ones:

1. No representation written by any command has role `main`. A project whose symbols carry
   `main` behaves exactly as if they did not.
2. Moving any symbol, or editing a box or a page location, changes no device's location without a
   transaction that says so. Undo removes it.
3. A device whose location changes redraws its tags on every page with one of its symbols, and
   nothing else does (update map §8).
4. A coil placed after its contacts is still the head: the contacts point at it, and numbering
   orders the device by the coil.
5. A strip terminal placed in another box keeps the strip's location. A conflict prompt is shown,
   and its default answer is Leave.
6. Renaming `+CP1` to `+CP10` in the list is one transaction. Every tag, box label, title block
   field and BOM row follows it, and undo restores it.
7. MCP contract tests for `setlocation`, `setfunction`, `adoptdesignations`, the four
   `designation*` commands, and the `location` and `function` parameters of `place`, `locationbox`
   and `functionbox`. Each checks the recorded transaction, `Author::Agent`, and refusal with
   writes off.
8. Bench (medium; large once at the end): `symin ms` and `boxmove ms` no worse than `dev`;
   `locset ms` on 50 devices; `renamebench ms` (rename an entry used by 500 devices).

## Tester

**Yes.** This is a model change, touches movement (the move follow-up) and the inspector, and
narrows a redraw map that once had a bug. The stored values change from codes to list keys (FP8). One tester run, after 052d, covers all four parts.

## Parts

- **052a, types and no `main`** (design §3, §4): `reps.rs`, the readers and writers, xref,
  numbering hook. No visible change except heads chosen by type, never by ID or order.
- **052b, stored function and location and the list** (design §5.1–§5.2, §5.7–§5.9, §6, §7, §8,
  §9.1): resolve, overrides, prefixes, list and rename, outputs, checks, inspector, adopt. **Starts
  after 053 merges** (`outputs.rs`). Size L.
- **052c, the Settings list** (design §9.4): lane U only, after 052b. Size S.
- **052d, proposals and function boxes** (design §5.3–§5.6, §5.8, §9.2, §9.5): place, join, move,
  box, page and project popups for both aspects; the Function Box tool. After 052b, in parallel
  with 052c. Size L.

052a can start now and is unchanged by the function addition. The addition grows the old
052b/052c from M+M to L+L+S.

## Open questions

- Q1–Q9 were answered by the owner on 2026-11-14 (design §12.1).
- FP1–FP6 were approved by the overseer on 2026-11-14 (design §11).
- FP7–FP9 were approved by the overseer on 2026-11-14 (design §11).
- Open: the new owner questions Q10–Q11 (design §12.2).
