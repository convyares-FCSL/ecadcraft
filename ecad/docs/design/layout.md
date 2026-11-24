# Source layout

Status: **approved by the owner, 2026-11-22 (checkpoint 1).** It records the owner's
decisions of 2026-11-21 (end-gate review and the layout discussion) and the generic command runner's
classification (`commands.md`). Source findings: end-gate review #25, #26, #35, #36, #42, #43, #65–#69.

## Rules

1. **Names come from `docs/glossary.md`** (updated 2026-11-21):
   - **ConnectionPoint**: a place a conductor lands on a device, and the pins of a connector. It replaces
     the model's `Terminal`.
   - **TerminalBlock**: one unit on the rail, may have levels (IEC 60947-7-1).
   - **TerminalStrip**: X1, a run of terminal blocks plus accessories.
   - **Connector**: plug or socket; its pins are connection points.
   - **"Terminal" alone never appears in code.**
   - **Jumper bar**, never "link" or "link line": a bar that joins terminal blocks. A link made of
     real wire is a wire.
   - **references**: the one word for cross-references (never xref, refs or xrefs).
   - **draw**, never "look".
   - Folders are plural area names from the glossary.
2. **One folder per feature in ecad-electrical.** It holds everything of that feature: its rules, its
   command declarations (params + rules, run by the engine's generic runner), its kept view if one
   survives, and its words. `plc/` shows the shape.
3. **The engine mirrors the same feature names, for the session only**: drawing (`draw`),
   interaction (`interact`) and the commands that need the session (drawing, cursor, selection,
   CADCraft entities). A feature is **a file until it has both draw or interact code and session
   commands, then a folder**. No rules in the engine (the 074 guard stays).
4. **Commands are generic by default** (owner decision 5): one runner in the engine does params, the
   rules call, the file I/O a command declares (the engine reads and writes; Core never touches disk),
   commit, the JSON answer, the MCP schema, undo and the agent gate. An engine command file exists only
   for a session command.
5. **Shared plumbing stays flat** and small.
6. **lib.rs lists the areas**, one line each.
7. **Tests follow the folders**: `tests/all/<feature>/…` in each crate; no `lanes/`, `tester_NNN`,
   `*_tester` or brief numbers. Rule tests (a `&Document` in, a result out) live in
   `ecad-electrical/tests/all/<feature>/`; the engine keeps session, drawing and click tests (#69).
8. **Narrow `pub`**: `pub(crate)` unless another crate uses it; `unreachable_pub` ratchet (#68). No
   `pub use` between crates.
9. **Size**: a file over ~1,500 lines is split by job inside its feature folder.

## Crates (owner decisions 2 and 3)

| Crate | Layer | Purpose |
| --- | --- | --- |
| ecad-model | Core | the typed document and how it changes; definitions (title blocks, text kinds, the registry mechanism) and the canvas palette |
| ecad-file | Core | the `.ecad` format: records, chunks, history, verify |
| ecad-store | Core | the only crate that touches disk for projects (redb) |
| ecad-electrical | Core | the electrical rules, one folder per feature |
| ecad-drawing-style | Core | the look of a page: built-in title blocks and frames, text styles, fonts, text fitting; last in Core |
| ecad-engine | Engine | the adapter over CADCraft: session, drawing, interaction, the generic command runner, export |
| **ecad-widgets** | UI kit | **new name: ecad-ui + ecad-tokens merged** (tokens become a module). Generic controls and screen tokens, no ECAD knowledge |
| ecad-app | App | the ECAD user interface: screens, panels, lists, editors, UI state; turns user actions into commands and shows results. **No rules, no disk, no OS, no parsing of domain values** |
| ecad-desktop | Program | process, window, OS integration, wiring the app to the engine |
| ecad-cli, ecad-mcp | Programs | unchanged |
| dev/ecad-bench, dev/ecad-inspector, dev/xtask | Dev | ecad-inspector reads and shows snapshots; it does not make them |
| ~~ecad-inspect~~ | | **deleted** (#24): the event log goes to ecad-engine `history/`, the snapshot to ecad-engine `dev/snapshot.rs` (the engine's developer module: it reads the live project, which only the engine holds; #122) |

The layers guard also checks dev-dependencies: no crate depends on a layer above it, not even in tests
(#53: the App tests that use ecad-mcp or ecad-desktop move into those programs' own suites).
`crates.md`, the layers guard and the style guard paths follow these names.

## ecad-model (#67)

```
ecad-model/src/
  lib.rs, error.rs, dev.rs (verify mode and test hooks)
  document/        doc/ (drawing, device, cable, settings, definition, document; the Document's maps are read
                   through accessors, built with `from_parts`), project, page, sheet, book, op, effect, diff, invariant, index,
                   view (DerivedView protocol), strict, ledger/ (types, stacks, journal, open, read, edit)
  merge/           three-way merge of branches (#8): state, conflicts, rules, apply
  objects/         devices/ (device.rs + device/; ConnectionPoint, TerminalBlock, TerminalStrip types),
                   plc, wire, route, aid, loose_part, title_block, user, role, styles/
  connectivity/    connectivity, jumper_bars (today links.rs), wire_order, wiring, derive
  naming/          id, tag, prefix, numbering, keys, words, text (Text<N>), builtins
  geometry/        geom, point (connection point names), units, colour
```

## ecad-file and ecad-store

ecad-file stays flat, with one folder: `project/` (project.rs 2,154, #18). V2 suffixes, LinkV1, the
`extra` maps and the op upcast are already gone. ecad-store stays flat.

## ecad-electrical: feature folders

```
ecad-electrical/src/
  lib.rs, error.rs
  commands.rs      the Generic trait, Cx, Outcome, CmdError, shared argument readers (today cmd/mod.rs, cmd/run)
  views.rs         the view pipeline: Views, Changes, Lazy (today derived/mod.rs + changes.rs)
  defaults/        built-in data, one file per feature (unchanged)

  devices/         devices (split), kinds, traits, place rules, device links, the device list and its view,
                   ratings/ — each device type's rating reader (today rating/reader/*, rating/parse, dataset)
  symbols/         symbols (split: starters, catalogue, bridges, draw rules), reps, proposals
  wires/           wires (split), nets, wire numbers, connect
  cables/          cables, cores, slash, layout (cable_look), cable words (words.rs)
  continuations/   continuation arrows (aids)
  references/      device references (xref.rs split: parse, classify, places, index, table), cable
                   references (references.rs), the refs view
  connectors/      connectors, halves, pins
  terminal_strips/ terminal strips (strips.rs split), strip pages, terminal blocks and levels
                   (terminal_types), accessories, jumper bars (today link_lines.rs)
  plc/             unchanged shape; look.rs becomes draw.rs
  circuits/        wire and cable sizing and circuit maths: circuit (sizing and volt drop), trace, protection,
                   load, kept (results and update map), rows (Checks rows), commands; the rating tables' dataset
                   stays in devices/ratings. Results are Checks rows. Future power budget,
                   short circuit and selectivity go here, nothing else
  locations/       locations, functions, location boxes, designations
  numbering/       renumber, fixed and provisional tags
  pages/           page types, folders, frames, generated pages, the cover, their views
  title_blocks/    what a block's fields show (bind), what an op changes of the frames, the checks
  releases/        revisions, issue, release, the pre-release check
  reports/         report models and the BOM and their view: mod (rows and tables' types), kept/ (mod, update,
                   checks, rows, tables), bom, view; commands/ (bom, boxes, lists)
  checks/          checks split by family: mod (kinds, rows, scope), nets, places, grid, devices, cables; kept,
                   words_check, the checks view
  styles/          the wire, page, cable and rating table catalogues, what spans every catalogue, project
                   defaults (setdefault); where texts go is devices/text_place.rs
  properties/      the Properties palette's ECAD section and setprop (props.rs, cmd/prop, cmd/props/* as one
                   file per object kind, not per lane)
  merge.rs         the merge dialog's rules
```
Every command declaration sits in its feature folder, as a `commands.rs` beside the rules it calls.

## ecad-drawing-style: the look

```
ecad-drawing-style/src/
  lib.rs           REGISTRY: every crate's data set
  defaults/        the look's registry entries (text.rs, styles.rs) and its data set (data.rs)
  text/            text styles (mod.rs), fonts and FontLook (fonts.rs), textstyle and showtext rules (commands.rs)
  catalogues/      the text, location box and frame catalogues
  title_blocks/    the block a frame draws (block_of), the built-in house block, the title block commands
```
The rules ask the look through `ecad_model::look::Look`, which `FontLook` implements; the cover
layout and text fitting (`fit`) are still in `ecad-electrical/src/pages/`.

## ecad-engine: session only

```
ecad-engine/src/
  lib.rs
  cad.rs           the CADCraft boundary
  convert.rs       ECAD items <-> CADCraft entities
  access.rs        ProjectHandle and the session lock
  background.rs    background jobs (R11)
  generic.rs       the generic command runner: lock, declared file I/O, commit, echo, answer, schema, gate
  commands.rs      the command table (generic declarations + session commands; today cmd/mod.rs, lanes.rs)
  pointer/         what is under the cursor, the running tool's intent, picking and capture, previews, page marks, hover
  project/         open, persistence, owner, changes, handle map, page tree, parts (the session state groups)
  history/         history (split), merging, the event log (from ecad-inspect)
  translate/       translate, oops
  agent/           unchanged
  export/          formats: PDF, Print Book (printbook.rs), the release folder's rendering; later DXF
  dev/             debugger (debug.rs, split), verify, test_support (feature-gated)

  devices/         draw, interact, session commands: place, convertcomponent, addconnectionpoint
  symbols.rs       session command: makesymbol; draw (proposals)
  wires/           draw, interact (wiring forms), session commands: wire, joinwires
  cables/          draw (look.rs), views (traditional or fan), session commands: cableslash, cableadd,
                   cablecore, cablefan, cableview
  continuations.rs draw and interact
  references/      draw, interact (refs + xrefs merged), session commands: cablegoto, refgoto
  connectors.rs    interact, session commands: connector, pinlabel
  terminal_strips/ draw (strip_page.rs), interact (terminal blocks, accessories), session command: bridge
  plc/             draw, session commands: plcchannels, plcpage
  locations.rs     draw, interact (location form)
  pages/           draw (frame, cover, generated pages), preview, session commands: newpage, openpage,
                   ecadinspect, deletepage, openrevision, closerevision, undoissue
  title_blocks.rs  draw (layout, dxf)
  releases.rs      session commands: prerelease, release (renders through export/)
  reports/         draws report pages (device list, tables); session commands: printbook, reports
  styles.rs        draw (text styles), interact (text tools), session commands: showtext, hiddentext
  properties/      the Properties selection half (props.rs), forms (forms.rs split per form)
```
No `cmd/` folder: every generic command lives in electrical; each session command lives in its feature.

## ecad-app: by screen area (from the app check)

Only `lib.rs` at the root. Lines are today's (`wc -l`, sum 44,628).

| Area | What lives there | Lines |
| --- | --- | --- |
| `shell/` | window frame, bars, keys, wheel, layout, status, placing, dialogs, the project file dialog and close flow, the `App` struct | 7,086 |
| `commands/` | the action table, what the app runs, the Tool Sets tab | 2,593 |
| `inspector/` | the right panel: device, page, fields, details, layout tab, checks tab | 3,307 |
| `canvas/` | sheet, selection, markers, leaders | 575 |
| `pages/` | the Project panel's page list, preview | 1,875 |
| `workspace/` | the list workspace frame and shared detail | 1,503 |
| `devices/` | Devices tab and list, Renumber dialog | 1,329 |
| `cables/` | cable list, editor, manager, cable view | 1,556 |
| `wires/` | wire list, rating section | 862 |
| `terminal_strips/` | terminal strip plan | 372 |
| `plc/` | PLC list, strip, card editor | 1,730 |
| `references/` | reference style and rows (today scattered in sheets_ui and the right panel) | new |
| `checks/` | Checks tab and list | 561 |
| `reports/` | report lists and windows | 1,384 |
| `publish/` | the Publish workspace | 642 |
| `title_blocks/` | the title-block editor | 2,973 |
| `library/` | the symbol library and search | 4,784 |
| `history/` | History tab, merge dialog, release dialog | 1,933 |
| `settings/` | project settings windows and popouts | 5,233 |
| `agent/` | agent banner, last write, undo session | 2,085 |
| `developer/` | Developer section and the debugger | 1,333 |
| out | `folder.rs` → ecad-desktop; disk and OS parts of `file_ops.rs` → engine or desktop (app check A.1) | 34+ |

## Walkthroughs: the files you open today and after

| Feature | Today | After |
| --- | --- | --- |
| **Cables** | 13 files in 3 crates: electrical `cables.rs`, `slash.rs`, `cable_look.rs`, `words.rs`, `cmd/cable.rs`, `cmd/cable_draw.rs`; engine `cmd/cable.rs`, `cmd/cable_draw.rs`, `look.rs`, `views.rs`; app `lists/cable_*.rs` (4) | electrical `cables/` + engine `cables/` + app `cables/` |
| **PLC** | 27 files: electrical `plc/` (14), `cmd/plc.rs`, `defaults/plc.rs`; engine `cmd/plc/` (8), `draw/plc/` (3); app `lists/plc.rs`, `lists/card_editor.rs` | electrical `plc/` + engine `plc/` + app `plc/` (defaults stay in `defaults/plc.rs`) |
| **Terminal strips** | 17 files: electrical `strips.rs`, `strips/pages.rs`, `accessories.rs`, `terminal_types.rs`, `link_lines.rs`, `cmd/terminal.rs`, `cmd/accessory.rs`, `cmd/level.rs`; engine `strip_page.rs`, `cmd/terminal.rs`, `cmd/accessory.rs`, `cmd/level.rs`, `interact/terminals.rs`, `interact/accessories.rs`; app `lists/terminals.rs`; model `device/terminal_strip.rs` | electrical `terminal_strips/` + engine `terminal_strips/` + app `terminal_strips/`; the TerminalStrip and TerminalBlock types stay in model `objects/devices/` |
| **References** | 12 files, 4 names: electrical `xref.rs`, `references.rs`, `cmd/xref.rs`, `cmd/reference.rs`, `derived/refs.rs`; engine `draw/xref.rs`, `draw/references.rs`, `interact/refs.rs`, `interact/xrefs.rs`, `cmd/xref.rs`, `cmd/reference.rs` | electrical `references/` + engine `references/` + app `references/`; one word |
| **Title blocks** | 12 files: electrical `title_block/` (4), `frames.rs`, `cmd/title_block.rs`, `styles/frame.rs`; engine `draw/title_block/` (3), `frame.rs`, `cmd/title_block.rs`; app `library/title_blocks/` (6); model `title_block.rs`, `style/frame.rs` | electrical `title_blocks/` + engine `title_blocks.rs` + app `title_blocks/`; the frame (border, columns) belongs to `pages/` in both crates, its style to `styles/` |
| **Circuits / devices** (ratings split) | electrical `rating/` (mod, circuit, dataset, load, parse, protection, trace, reader.rs + reader/ 6 files), `cmd/rating.rs`; engine `cmd/rating.rs`; app `lists/rating.rs` | the readers in electrical `devices/ratings/`; the sizing and circuit maths in electrical `circuits/`; no engine file (the rating commands are generic) |

## Order of the moves (after approval)

One feature per slice, two workers at most, with `git mv`. Each slice does, for its feature: the folder
(#25, #65), its big-file splits (#26, #43), repointing away from the engine facade (#35, #49, #50), the
names (#42, #66), narrow `pub` (#68), its tests by area (#11, #20, #46, #54, #60) and its rule tests into
`ecad-electrical/tests/all` (#69). Order: references, cables, terminal_strips, plc, title_blocks,
circuits/devices, then the rest by size; ecad-model's grouping (#67); then the app; then the crate merges
and deletes (ecad-widgets, ecad-inspect). Small lanes (#63, #48, #57, #39, #32 + approved device gaps) run
between slices.
