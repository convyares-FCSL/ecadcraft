# Source layout

This layout is **binding**: a change follows it, and a change that would bend it goes to the owner first. The
names and crate decisions behind it are in [`decisions.md`](decisions.md); how the generic command runner works
is in [`../developer/commands.md`](../developer/commands.md).

## Rules

1. **Names come from the [glossary](../reference/glossary.md)**:
   - **ConnectionPoint**: a place a conductor lands on a device, and the pins of a connector.
   - **TerminalBlock**: one unit on the rail, may have levels (IEC 60947-7-1).
   - **TerminalStrip**: `-X1`, a run of terminal blocks plus accessories.
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
   commands, then a folder**. No rules in the engine (`cargo xtask layers` checks it).
4. **Commands are generic by default**: one runner in the engine does params, the rules call, the file
   I/O a command declares (the engine reads and writes; Core never touches disk), commit, the JSON answer,
   the MCP schema, undo and the agent gate. An engine command file exists only for a session command.
5. **Shared plumbing stays flat** and small.
6. **lib.rs lists the areas**, one line each.
7. **Tests follow the folders**: `tests/all/<feature>/…` in each crate; no `lanes/` folders, no files
   named after a brief or an agent role, no brief numbers. Rule tests (a `&Document` in, a result out) live in
   `ecad-electrical/tests/all/<feature>/`; the engine keeps session, drawing and click tests.
8. **Narrow `pub`**: `pub(crate)` unless another crate uses it, modules included: a `pub mod` is one
   another crate (its integration tests and doc examples count) names. The compiler enforces it:
   `unreachable_pub`, `private_interfaces` and `private_bounds` are denied for the workspace. No
   `pub use` between crates.
9. **Size**: a file over ~1,500 lines is split by job inside its feature folder.

## Crates

| Crate | Layer | Purpose |
| --- | --- | --- |
| ecad-model | Core | the typed document and how it changes; definitions (title blocks, text kinds, the registry mechanism), the `Look` trait and the canvas palette |
| ecad-store | Core | the only crate that touches disk for projects (redb) |
| ecad-file | Core | the `.ecad` format: records, chunks, history, verify |
| ecad-electrical | Core | the electrical rules, one folder per feature |
| ecad-drawing-style | Core | the look of a page: built-in title blocks and frames, text styles, fonts, text fitting; last in Core |
| ecad-engine | Engine | the adapter over CADCraft: session, drawing, interaction, the generic command runner, export |
| ecad-widgets | UI kit | generic controls and screen tokens (tokens are a module), no ECAD knowledge |
| ecad-app | App | the ECAD user interface: screens, panels, lists, editors, UI state; turns user actions into commands and shows results. **No rules, no disk, no OS, no parsing of domain values** |
| ecad-desktop | Program | process, window, OS integration, wiring the app to the engine |
| ecad-mcp | Program | the MCP server; headless use goes through it |
| dev/ecad-bench, dev/ecad-inspector, dev/xtask | Dev | never shipped; ecad-inspector reads and shows snapshots, it does not make them |

The event log lives in ecad-engine `history/`, the snapshot in ecad-engine `dev/snapshot.rs` (the engine's
developer module: it reads the live project, which only the engine holds).

The layers guard also checks dev-dependencies: no crate depends on a layer above it, not even in tests (the
App tests that use ecad-mcp or ecad-desktop live in those programs' own suites).

## ecad-model

```
ecad-model/src/
  lib.rs, error.rs, dev.rs (verify mode and test hooks)
  look.rs          the Look trait: text kinds, a kind's style, shown or hidden, frame text and title block
  document/        doc/ (drawing, device, cable, settings, definition, document; the Document's maps are read
                   through accessors, built with `from_parts`), project, page, sheet, book, rev, op, effect,
                   diff, invariant, index, view (DerivedView protocol), strict, ledger/ (types, stacks,
                   journal, open, read, edit)
  merge/           three-way merge of branches: state, conflicts, rules, apply
  objects/         devices/ (device.rs + devices/: device types and their fields, TerminalStrip),
                   plc, wire, route, aid, loose_part, title_block, user, role, styles/
  connectivity/    connectivity, jumper_bars, wire_order, wiring, supply, derive
  naming/          id, tag, prefix, numbering, keys, words, text (Text<N>), builtins
  geometry/        geom, point (connection point names), units, colour
  palette/         the canvas palette: line weights, markers, sheet colours, wire colours, the sheet font
  registry/        the defaults registry: entries, resolution, built-in data sets, style field resolution
```

## ecad-file and ecad-store

ecad-file stays flat, with one folder: `project/` (branches, lineage, open, save, report). ecad-store stays
flat (backend, db, disk, power).

## ecad-electrical: feature folders

```
ecad-electrical/src/
  lib.rs, error.rs
  commands/        the Generic trait, Cx, Outcome, CmdError, shared argument readers (run.rs, input.rs)
  views/           the view pipeline: Views, Changes, Lazy
  fields.rs        typed input: the grammars of what a person types into a field, with named refusals
  defaults/        built-in data, one file per feature

  devices/         devices, kinds, traits, place rules, device links, the device list and its view,
                   ratings/ (each device type's rating reader, the rating tables' dataset)
  symbols/         symbols (starters, catalogue, bridges, draw rules), the symbol builder, reps, proposals
  wires/           wires, nets, wire numbers, connect, joining, labels
  cables/          cables, cores, slash, layout, cable words
  continuations/   continuation arrows (aids)
  references/      device references (parse, classify, places, index, table), cable references, the view
  connectors/      connectors, halves, pins
  terminal_strips/ terminal strips, strip pages, terminal blocks and levels, accessories, jumper bars
  plc/             racks, cards, channels, signals, the I/O list, draw
  circuits/        wire and cable sizing and circuit maths: circuit (sizing and volt drop), trace, protection,
                   load, kept (results and update map), rows (Checks rows), commands. Results are Checks
                   rows. Future power budget, short circuit and selectivity go here, nothing else
  locations/       locations, functions, location boxes, designations
  numbering/       renumber, fixed and provisional tags
  pages/           page types, folders, frames, generated pages, the cover, their views
  title_blocks/    what a block's fields show (bind), the checks
  releases/        revisions, issue, release, the pre-release check
  reports/         report models and the BOM and their view: mod (rows and tables' types), kept/, bom, view,
                   commands/
  checks/          checks split by family: mod (kinds, rows, scope), nets, places, grid, devices, cables; kept,
                   words_check, the checks view
  styles/          the wire, page, cable and rating table catalogues, what spans every catalogue, project
                   defaults (setdefault); where texts go is devices/text_place.rs
  properties/      the Properties palette's ECAD section and setprop, one file per object kind
  merge.rs         the merge dialog's rules
```
Every command declaration sits in its feature folder, as a `commands.rs` or `commands/` beside the rules it
calls.

## ecad-drawing-style: the look

```
ecad-drawing-style/src/
  lib.rs           REGISTRY: every crate's data set
  defaults/        the look's registry entries (text.rs, styles.rs) and its data set (data.rs)
  text/            text styles (mod.rs), fonts and FontLook (fonts.rs), textstyle and showtext rules (commands.rs)
  catalogues/      the text, location box and frame catalogues
  title_blocks/    the block a frame draws (block_of), the built-in house block, the title block commands
```
The rules ask the look through `ecad_model::look::Look`, which `FontLook` implements; the cover layout and text
fitting (`fit`) are still in `ecad-electrical/src/pages/`.

## ecad-engine: session only

```
ecad-engine/src/
  lib.rs
  cad.rs           the CADCraft boundary
  convert.rs       ECAD items <-> CADCraft entities
  access.rs        ProjectHandle and the session lock
  background.rs    background jobs
  warm.rs          the kept views' first builds off the UI thread (report models, the Checks list)
  file.rs          a project's file on disk: the home folder, path normalising
  checks.rs        the engine side of the Checks list: text measured in CADCraft's fonts, go-to handles
  generic.rs       the generic command runner: lock, declared file I/O, commit, echo, answer, schema, gate
  commands/        the command table (generic declarations + session commands), typed input, replies
  pointer/         what is under the cursor, the running tool's intent, picking and capture, previews, page marks, hover
  project/         open, persistence, owner, changes, handle map, page tree, parts (the session state groups)
  history/         history, branches, versions, points, merging, the event log
  translate/       translate, oops
  agent/           agent access, the bridge, the write gate, policy, tools
  export/          formats: PDF, Print Book (printbook.rs); later DXF
  dev/             debugger, verify, snapshot, objects, checks, test_support (feature-gated)

  devices/         draw, interact, session commands: place, convertcomponent, addconnectionpoint
  symbols/         draw (proposals), session command: makesymbol
  wires/           draw, interact (wiring forms), session commands: wire, joinwires
  cables/          draw, views (traditional or fan), session commands: cableslash, cableadd, cablecore,
                   cablefan, cableview
  continuations/   draw and interact
  references/      draw, interact, session commands (cables, devices): cablegoto, refgoto
  connectors/      interact, session commands: connector, pinlabel
  terminal_strips/ draw (strip pages), interact (terminal blocks, accessories), session command: bridge
  plc/             draw, session commands: plcchannels, plcpage
  locations/       draw, interact (location form)
  pages/           draw (frame, cover, generated pages), preview, session commands: newpage, openpage,
                   ecadinspect, deletepage, openrevision, closerevision, undoissue
  title_blocks/    draw (layout, dxf), session command: titleblockimport
  releases/        session commands: prerelease, release (renders through export/)
  reports/         draws report pages (device list, tables); session commands: printbook, reports
  styles/          draw (text styles), interact (text tools), session commands: showtext, hiddentext
  properties/      the Properties selection half (props.rs), forms (one file per form)
```
No `cmd/` folder: every generic command lives in electrical; each session command lives in its feature.

## ecad-widgets

```
ecad-widgets/src/
  lib.rs
  tokens/          screen tokens: type, size, space, radius, line, shadow, icon, shell, wheel
  prim/            primitives: button, field, label, row, segmented, kit
  control.rs       value controls (text entry, select, toggle, value box, read text, link)
  panel.rs, dialog.rs, splitter.rs, style.rs, wheel.rs
  table/           the one table component: source, view model, cells, edit, selection, strip, tint, export
  icons/           icons drawn in code
  fonts.rs         the embedded interface and sheet fonts, registered for egui
  showcase.rs      the component gallery
```

## ecad-app: by screen area

Only `lib.rs` at the root. Lines are `wc -l` of each folder.

| Area | What lives there | Lines |
| --- | --- | --- |
| `shell/` | window frame, bars, keys, wheel, layout, status, placing, dialogs, the project file dialog and close flow, the `App` struct | 8,076 |
| `commands/` | the action table, what the app runs, the tools | 2,652 |
| `inspector/` | the right panel: device, page, fields, details, layout tab, checks tab | 3,266 |
| `canvas/` | sheet, selection, markers, leaders | 574 |
| `pages/` | the Project panel's page list, preview | 1,935 |
| `workspace/` | the list workspace frame and shared detail | 1,151 |
| `devices/` | Devices tab and list, Renumber dialog | 1,421 |
| `cables/` | cable list, editor, manager, cable view | 1,655 |
| `wires/` | wire list, rating section | 945 |
| `terminal_strips/` | terminal strip plan | 453 |
| `plc/` | PLC list, strip, card editor | 1,714 |
| `references/` | reference style and rows | 93 |
| `checks/` | Checks tab and list | 575 |
| `reports/` | report lists and windows | 1,524 |
| `publish/` | the Publish workspace | 644 |
| `title_blocks/` | the title-block editor | 2,987 |
| `library/` | the symbol library and search | 4,705 |
| `history/` | History tab, merge dialog, release dialog | 1,951 |
| `settings/` | project settings windows and popouts | 5,228 |
| `agent/` | agent banner, last write, undo session | 2,049 |
| `developer/` | Developer section and the debugger | 1,331 |

## Programs

`apps/ecad-desktop/src`: `main.rs`, `lib.rs`, `args.rs`, `startup.rs`, `window.rs`, `host.rs`, `folder.rs` (the
default projects folder). `apps/ecad-mcp/src`: `main.rs`, `lib.rs`, `args.rs`, `bridge.rs`, `launch.rs`, `demo.rs`,
`bin/`.
