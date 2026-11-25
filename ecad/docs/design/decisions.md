# Decisions (binding)

The owner's decisions, by topic. This is the one decision log: every change follows it, and a change that would
need to bend a decision is raised with the owner first. A new decision is added under its topic as the rule it
sets, and a decision that replaces another replaces its text here. The source tree is [`layout.md`](layout.md);
the crate table and layer rules are explained in [`../developer/architecture.md`](../developer/architecture.md).
The reasons behind each area are in the topic notes beside this file.

## 1. Names

Glossary first ([`../reference/glossary.md`](../reference/glossary.md)); code, ops, file keys, JSON keys and
command ids use the same words.

- **ConnectionPoint**: the place a conductor lands on a device, and the pins of a connector.
- **TerminalBlock**: one unit on the rail, which may have levels (IEC 60947-7-1).
- **TerminalStrip**: a tagged run of terminal blocks plus accessories (`-X1`).
- **Connector**: plug or socket; its pins are connection points.
- "Terminal" alone never appears in code.
- **Jumper bars**, not link lines. A link made of real wire is a wire.
- **References**, one word for cross-references. **Draw**, never "look". Area names are plural, from the glossary.
- **Chunk**, never "part", for a piece of a saved snapshot: "parts" reads as electrical components.
- Old words are renamed everywhere, not kept for compatibility: command JSON parameter and answer keys, fields,
  locals, file chunk keys, enum variants and command ids (terminal → connection_point or terminal_block by meaning,
  link → jumper_bar, xref/refs → references, look → draw, outputs → reports).
- Standards are defaults, never the law: IEC 81346 letter codes, IEC 60617 symbols and IEC 60757 colour codes are
  what a new project uses, and any other letters, symbols or colour words are accepted.

## 2. Crates

- No `ecad-inspect`: the event log lives in `ecad-engine` `history/`, the snapshot in `ecad-engine` `dev/`.
- `ecad-widgets` is the UI kit (generic controls and screen tokens; tokens are a module).
- Core is ordered by what a crate knows: `ecad-model` defines what things are (a project, a page, a title block
  and its fields, the text kinds), `ecad-electrical` what they mean electrically (location, function, revisions,
  what fills a field), and `ecad-drawing-style` how they look (built-in title blocks and frames, text styles, fonts
  and text fitting). The order is model → store → file → electrical → drawing-style.
- Electrical lays out its own content but owns no look: it asks through a `Look` trait that `ecad-model`
  defines and `ecad-drawing-style` implements: a text kind's style, whether a text is shown, a text's width, and a
  page frame's text styles, lines and title block (the built-in one when the frame defines none). The checks that
  read the look (a title block field cut, the cover legend) stay in electrical and are given it.
- The defaults registry's mechanism (keys, levels, resolution) is `ecad-model`'s; each crate contributes its own
  entries (electrical the electrical ones, drawing-style the look ones). Drawing-style assembles them into one
  `Registry`, and the engine passes it to commands and views.
- The canvas palette (`Color`, sheet colours, markers, wire colours and dashes, line weights, the sheet font) is
  `ecad_model::palette`, so the UI kit uses `ecad-model` and nothing else of ours.
- No Core crate depends on a `cadcraft-*` crate. Only `ecad-engine` touches the drawing engine; App and Programs
  reach it through `ecad_engine::cad`.
- No Core crate except `ecad-store` touches the OS: no files, environment, processes, network, clocks or
  randomness; the caller passes them in. Threads are allowed.
- Dependencies: `anyhow` only in Programs, `ecad-bench` and `xtask` (libraries return their own error type); no
  `tokio` anywhere, dev-dependencies included; `rayon` only in `ecad-model`, `ecad-file`, `ecad-electrical` and
  `ecad-engine`; Core and Engine use no UI toolkit.
- App tests that need `ecad-mcp` or `ecad-desktop` live in those programs' own suites.
- No layer depends on one above it, dev-dependencies included; `cargo xtask layers` checks all of the above.
- No `ecad-cli`: headless use goes through `ecad-mcp`.

## 3. Layer purposes

- **ecad-widgets**: generic controls and screen tokens, no ECAD knowledge.
- **ecad-app**: the ECAD user interface: screens, panels, lists, editors and UI state. It turns user actions into
  commands and shows their results. No rules, no disk, no OS, no parsing of domain values.
- **ecad-desktop**: the process, the window, OS integration, and wiring the app to the engine.

## 4. ecad-electrical feature folders

As in [`layout.md`](layout.md), and:

- `terminal_strips/` (not `strips/`) holds jumper bars too.
- `reports/` (not `outputs/`) holds report models and the BOM.
- A device type reads its own rating: rating readers and the rating tables' dataset live in `devices/ratings/`.
  Wire and cable sizing lives in `circuits/` (`circuit` for sizing and volt drop, `trace`, `protection`, `load`);
  results are Checks rows. Future circuit maths (power budget, short circuit, selectivity) goes in `circuits/` and
  nowhere else.

## 5. ecad-engine

- Commands are generic by default: one generic runner (parameters struct, rules call, the file I/O the command
  declares, commit, JSON answer, MCP schema, undo, agent gate). An engine command file exists only where the command
  needs the session (drawing, cursor, selection, CADCraft entities).
- Feature names mirror ecad-electrical's: a file until it has draw or interact code, then a folder.
- `reports/` draws report pages; `export/` holds formats (PDF, Print Book, later DXF); `dev/` holds the debugger,
  verify and test support.
- A command's JSON form never prompts and never opens a dialog; only the UI starts its interactive form.

## 6. The dev tree

- Developer and test tools live under `ecad/dev/` (`ecad-bench`, `ecad-inspector`, `xtask`) and are never shipped:
  no product crate depends on them (`cargo xtask layers`).
- Product code is Rust only. A developer tool under `ecad/dev/` may use HTML, CSS and JavaScript (the inspector's
  page); it ships with no product, runs no server by default, never changes a project and escapes every value it
  shows.
- The debugger is read-only: it copies what it shows and releases the project before drawing, and runs replay and
  file checks only on request, on copies.

## 7. Upstream CADCraft

- CADCraft is used as it is, by path. ECAD code lives in the `ecad-*` crates; nothing ECAD-specific enters an
  upstream crate.
- Every change to a file outside `ecad/` is a listed upstream patch (`ecad/UPSTREAM-PATCHES.md`, checked by
  `cargo xtask patches`), marked `// ECAD-SEAM(owner):` in code, and kept generic.
- On ECAD-owned pages the drawing engine's block editing, external reference editing and LISP evaluation are
  refused before the drawing changes (the `refuse_start` hook). Plain drawings behave as upstream.
- ECAD IDs are allocated by ECAD, never taken from CADCraft handles; the engine keeps a map between them, because
  handles change on import, paste and explode.
- Upstream releases are merged as real merges of the release tag; the incorporated commit is recorded in
  `ecad/UPSTREAM.md`.

## 8. Platforms and versions

- ECAD is desktop-only: no web or wasm product, and no ECAD crate has to build for wasm.
- ECAD builds, tests and runs natively on Windows and Linux. Project files belong in a local folder; network,
  synced and removable folders are unverified until tested.
- Versions: the product version carries `-dev` between releases, release tags are `ecad-v…`, and file format
  versions are independent of the product version.

## 9. The document model and ops

- Typed objects change through whole-object ops (`ModifyDevice`, `ModifyWire`, `ModifyCable`, `ModifyPage`,
  `ModifyProject` and the like). `SetProperty` is only for display choices and per-object defaults; a reserved key
  in a property bag is a read error.
- A new op is added only when existing ops cannot express a change cleanly. A change to the format of existing
  records goes to the owner.
- A device's type is stored, copied from the symbol when the device is made, and never inferred again; changing
  it is one `ModifyDevice`.
- One device has any number of representations, of any embedded definitions, each with its own role; a
  representation with connection points gives each point's position in its own definition.
- A linked device cannot be deleted until its links are removed.
- Stored angles are radians; degrees appear only at the command and UI edges.
- Typed text is `Text<N>` / `LongText<N>` and refuses hidden and bidirectional control characters.
- Folders are typed (`AddFolder`, `RemoveFolder`, `ModifyFolder`): a parent exists, there is no cycle, and depth
  is at most 8.

## 10. File format and storage

- A project is one self-contained `.ecad` file. Git is an optional integration, never the model; there are no
  folder projects.
- The container is redb, pinned to one exact version. Rows are canonical JSON records (codec `json/1`) compressed
  with Zstandard (`zstd`, level 1). The hash chain over transactions is kept.
- Snapshots are split into chunks: `page:<id>`, `model:project` and `model:<kind>:<bucket>` (about 1,024 objects
  each, bucketed by a fixed mix of the ID), so an edit rewrites one chunk. The tag high-water mark travels in a
  `model:meta` chunk.
- An automatic snapshot is written every 2,000 transactions (`cache.every`, 16 to 1,000,000), as a `versions` row
  of kind `auto`, never a name prefix. Every named point gets a snapshot in the save that writes it. A snapshot
  records at most 1,024 entries of each undo and redo stack.
- **No compatibility before v1.** Any format change bumps `SCHEMA_VERSION`; the golden file pins it. An older or
  newer file opens read-only with "older format version" or "newer …". An unknown `meta` name, field or word opens
  the project read-only, names it, and leaves the file as it is. Transactions, ops and records refuse unknown
  fields.
- A writer holds an operating-system lock on `<file>.lock`; another opener is read-only. A crash never locks a
  project out, and a lock left by an ended session is reported when reclaimed.
- Save refuses when the file was replaced, moved or removed by something else, and offers Save As.

## 11. Tags and numbering

- A tag is an identity: the same tag anywhere means the same device. Case, a leading `-` and leading zeros do not
  matter (`-K1` and `-K01` are one tag). A new project pads numbers to 2 digits.
- A tag's state is stored with its value: `TagSlot` is `None`, `Automatic(tag)`, `Fixed(tag)` or
  `Provisional(tag)`. A provisional tag is stored bare and shown with `?` (`-K?3`); the `?` is read once at entry
  and made once for display. A provisional and a real `-K3` are different tags.
- Devices and cables share one tag namespace. The next automatic number is above the high-water mark, so a deleted
  tag is not given again; provisional tags do not raise the mark. Finalise gives a provisional tag a real number.
- One planner serves Renumber, Finalise, the next free number and merge renumbers; a plan never produces a
  duplicate, and a renumber applied after the project changed since its preview is refused.
- A duplicate is never a dead end: a tag someone else holds offers swap, make them one device, or the next free
  number, done only on the user's choice; without a choice the command refuses and names all three.

## 12. Wires and nets

- Nets are derived, never stored. Crossings never join; a route end landing on another route does.
- Wire numbers are stored on the wires, as user data: the numbering pass writes them inside the user's own
  transaction, on the nets the commit touched only, and never on undo, redo, replay or open.
- A typed wire number already used by another net offers swap, another number, or merging the nets when that is
  electrically sensible.
- A join that would short two potentials, or bring two different fixed wire numbers together, is refused with a
  way out.
- A wire type names the potential it is the default for; two wire types naming one potential is refused.
- Wire colours are IEC 60757 codes by default, with a three-letter style and free text allowed.

## 13. Pages, sheets and references

- A page's number is stored when the page is made; moving a page or putting it in a folder never renumbers it.
- The model origin is the page's bottom-left corner, the paper edge. The grid's zones span the whole frame, title
  block included. A new project's sheet is A3 landscape with columns 1–8 and rows A–F.
- The sheet frame is drawn on the locked layer `ECAD-FRAME` and is never edited as drawing.
- Reference texts are derived, never stored; editing one is refused. A reference reads from the frame of each
  end's own page, in the project's reference style (a new project uses `105/B3`). An orphan reads `?`.
- References are by ID, never by tag or page name.

## 14. History, branches and releases

- Versions and revisions are named points keyed by kind and name, so a version `B` and a revision `B` coexist. A
  revision follows the project's revision scheme (letters or numbers), and the scheme is locked once anything is
  issued. Revision records are typed.
- Revisions are issued only on `main`, and `main` cannot be archived. Issuing records Finalise and the issue in
  one transaction. Undoing an issue is asked about, never a plain undo.
- A branch merges into, and updates from, its parent only. A merge is one step of ordinary ops on the target;
  nothing is preselected and every conflict offers at least two choices; undoing a merge reopens the source branch.
  Version names are not merged, and revision fields never merge into `main`.
- Branch actions are written to the file at once and are not undoable steps. Restore is one ordinary step holding
  the difference between the live state and the point.
- The file keeps times in UTC; the History tab shows local time.

## 15. Devices, locations and functions

- A device's location and function are stored on the device as `Place` (`Unset`, `Manual(id)`, `Proposed(id)`),
  never inherited. Location boxes, pages and the project only propose. A conflict is never changed without asking;
  the default answer is Leave; JSON calls never prompt.
- A connector is one device; its two halves are told apart by their keys, written `-X10.S` and `-X10.P`.
- A jumper bar is a terminal strip property, never a conductor.
- `Motor.rated_voltage` (`Option<Volts>`) has a mismatch check against its circuit. The UI shows the field and
  suggests the circuit's or the part's value, never filling it in silently, and shows a mismatch inline.

## 16. Circuits and rating tables

- **ECAD ships no IEC or BS 7671 table values.** Rating tables are entered by the user or imported from a source
  they are licensed to use; the only rows in the repository are synthetic test fixtures.
- Every rated wire and cable is checked against **Ib ≤ In ≤ Iz**, and a cable's volt drop against its limit:
  5 % by default, settable from 0.5 to 20 %.
- Calculations are checks, not tools: every result is a row in the Checks list. A project that does not rate wires
  makes no rows.

## 17. Agent access

- One gate serves every agent call, headless and live. An agent never runs the agent access setting or the file
  and session commands, and `save` writes only to the project's own file.
- One agent writes at a time. Every agent edit is recorded with the agent as author; over the live bridge an
  agent undoes only its own steps. Undo session is a run of ordinary undos recorded as the person's.
- Agents connect over MCP only: the app has no built-in chat.

## 18. User interface

- A feature does not add a panel, dialog or toolbar. It adds a column set, page type, wheel table, Standards
  catalogue or tool; a missing slot is fixed in the shell.
- Every manager is a column set on the one `ecad-widgets` table.
- Panels use only `ecad-widgets` components for values; anything the person can change looks editable, and a read
  value never looks like an input.
- No refresh buttons: views stay live.
- The window's own state (shell layout, each page's zoom, a cable's traditional or fan view) is kept outside the
  `.ecad` file and is never a transaction.
- Sheet text sizes are cap heights. Print is mono by default, with the colour code in the wire label.

## 19. Views and update maps

- Every derived view has one way in, the project's `Views`; no full builds outside it and the view's own module.
- A view, list or index keeps an update map only where a full rebuild costs more than about 5 ms per edit on the
  large bench project; otherwise it rebuilds after each commit. No update maps for terminal strip pages, PLC,
  connectors or the book outline. Where a view keeps one, the map says which effects change what it shows, what it
  does for each, and that every other effect does nothing.

## 20. Defaults

- Built-in defaults are data: embedded TOML, one file per area, holding values only; each key's kind, range and
  levels stay in code, so a data file cannot widen what is allowed.
- "Not set" is stored as not set, so a project follows a changed built-in.
- A company standard reaches a project only as the project's own copy.
- Drawn text reads its text kind's style; the drawing crates hold no literal heights or style names.

## 21. Fonts

- The interface font (IBM Plex Sans) and the sheet font (Barlow Semi Condensed) are in ECAD's repository, under
  `ecad/assets/fonts`, unmodified, each with its licence (OFL-1.1) and a row in `ATTRIBUTION.md`; a test checks
  their checksums. `ecad-widgets` embeds the interface font and `ecad_model::palette` the sheet font, so nothing is
  read from disk at run time.

## 22. Docs and comments

- **Docs are part of done.** A change updates, in the same commit, every README, crate and module doc and comment
  it touches so they describe the code as it now is. No "docs later".
- **No history in source.** Comments in `src/` and in a crate's built-in data (`defaults/*.toml`) say what the
  code or value is and why, in domain words. No brief, design, lane, review-finding, question or requirement
  numbers, no section marks, no owner/overseer/reviewer/tester rulings, no dates. History lives in git and in
  `ecad/docs/archive`; only READMEs may link to documents in `ecad/docs`. `cargo xtask layers` enforces this.
- Test module docs say what they test, not which brief or role they came from.
