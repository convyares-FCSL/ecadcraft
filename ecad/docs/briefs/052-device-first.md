# Brief 052: The device-first model

Status: Draft
Branch: `feature/device-first`, from `origin/dev` `8b6c2ccd` (v0.2.0 plus #102, #103).
Designer: **yes** (`docs/design/052.md`) · Size: L, in three parts (052a, 052b, 052c)
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
| `ecad/crates/ecad-cadcraft/src/location_cmds.rs` (**new**) | `setlocation`, `adoptlocations` |
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
5. **Opening today's projects** (design §7): nothing is written on open. **Adopt Locations** (one
   command, one transaction) stores what today's derivation would give.
6. **Checks** (design §6): "-K01 is +CP2, but none of its symbols is in +CP2"; "no location";
   two heads.

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
6. MCP contract tests for `setlocation`, `adoptlocations` and the `location` parameter of `place`
   and `locationbox`: the recorded transaction, `Author::Agent`, and refusal with writes off.
7. Bench (medium; large once at the end): `symin ms` and `boxmove ms` no worse than `dev`;
   `locset ms` on 50 devices.

## Tester

**Yes.** This is a model change, touches movement (the move follow-up) and the inspector, and
narrows a redraw map that once had a bug.

## Parts

- **052a, types and no `main`** (design §3, §4): `reps.rs`, the readers and writers, xref,
  numbering hook. No visible change except heads chosen by type, never by ID or order.
- **052b, stored location** (design §5.1–§5.2, §6, §7, §8): resolve, overrides, tag pages, outputs,
  checks, inspector, `adoptlocations`. **Starts after 053 merges** (`outputs.rs`).
- **052c, propose UX** (design §5.3–§5.6): place, join, move, box, page and project prompts;
  `setlocation`.

052a can start now. 052b and 052c can be one worker run after 053.

## Open questions

Design §12 (owner questions Q1–Q9) and §11 (FP1–FP6 for the overseer).
