# Structure decisions (binding)

The owner's decisions on names, crates, layers and the source layout. Every change follows them; a change that
would need to bend one is raised with the owner first. The target tree is [`layout.md`](layout.md); the crate
table and layer rules are in [`crates.md`](crates.md). Recorded 2026-11-21 and 2026-11-22.

## 1. Names

Glossary first ([`../glossary.md`](../glossary.md)); code, ops, file keys, JSON keys and command ids use the same
words.

- **ConnectionPoint**: the place a conductor lands on a device, and the pins of a connector.
- **TerminalBlock**: one unit on the rail, which may have levels (IEC 60947-7-1).
- **TerminalStrip**: a tagged run of terminal blocks plus accessories (`-X1`).
- **Connector**: plug or socket; its pins are connection points.
- "Terminal" alone never appears in code.
- **Jumper bars**, not link lines. A link made of real wire is a wire.
- **References**, one word for cross-references. **Draw**, never "look". Area names are plural, from the glossary.
- Old words are renamed everywhere, not kept for compatibility: command JSON parameter and answer keys, fields,
  locals, file chunk keys, enum variants and command ids (terminal → connection_point or terminal_block by meaning,
  link → jumper_bar, xref/refs → references, look → draw, outputs → reports).

## 2. Crates

- No `ecad-inspect`: the event log lives in `ecad-engine` `history/`, the snapshot in `ecad-engine` `dev/`.
- `ecad-widgets` is the UI kit (generic controls and screen tokens; tokens are a module).
- Core is ordered by what a crate knows (owner, 2026-11-23): `ecad-model` defines what things are (a project, a
  page, a title block and its fields, the text kinds), `ecad-electrical` what they mean electrically (location,
  function, revisions, what fills a field), and `ecad-drawing-style` how they look (built-in title blocks and frames,
  text styles, fonts and text fitting). The order is model → store → file → electrical → drawing-style.
- Electrical lays out its own content but owns no look: it asks through a `Look` trait that `ecad-model`
  defines and `ecad-drawing-style` implements: a text kind's style, whether a text is shown, a text's width, and a
  page frame's text styles, lines and title block (the built-in one when the frame defines none) (owner,
  2026-11-23). The checks that read the look (a title block field cut, the cover legend) stay in electrical and
  are given it.
- The defaults registry's mechanism (keys, levels, resolution) is `ecad-model`'s; each crate contributes its own
  entries (electrical the electrical ones, drawing-style the look ones). Drawing-style assembles them into one
  `Registry`, and the engine passes it to commands and views (owner, 2026-11-23).
- The canvas palette (`Color`, sheet colours, markers, wire colours and dashes, line weights, the sheet font) is
  `ecad_model::palette`, so the UI kit uses `ecad-model` and nothing else of ours.
- App tests that need `ecad-mcp` or `ecad-desktop` live in those programs' own suites.
- No layer depends on one above it, dev-dependencies included; `cargo xtask layers` checks both.
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
- A device type reads its own rating: rating readers live in `devices/`. Wire and cable sizing lives in
  `circuits/` (`trace`, `sizing`, `volt_drop`, `protection`, `load`, `tables`); results are Checks rows. Future
  circuit maths (power budget, short circuit, selectivity) goes in `circuits/` and nowhere else.

## 5. ecad-engine

- Commands are generic by default: one generic runner (parameters struct, rules call, the file I/O the command
  declares, commit, JSON answer, MCP schema, undo, agent gate). An engine command file exists only where the command
  needs the session (drawing, cursor, selection, CADCraft entities).
- Feature names mirror ecad-electrical's: a file until it has draw or interact code, then a folder.
- `reports/` draws report pages; `export/` holds formats (PDF, Print Book, later DXF); `dev/` holds the debugger,
  verify and test support.

## 6. Motor rated voltage

`Motor.rated_voltage` (`Option<Volts>`) with a mismatch check against its circuit. The UI shows the field and
suggests the circuit's or the part's value, never filling it in silently, and shows a mismatch inline.

## 7. Update maps

A view, list or index keeps an update map only where a full rebuild costs more than about 5 ms per edit on the
large bench project; otherwise it rebuilds after each commit. No update maps for terminal strip pages, PLC,
connectors or the book outline.

## 8. Docs and comments

- **Docs are part of done.** A change updates, in the same commit, every README, crate and module doc and comment
  it touches so they describe the code as it now is. No "docs later".
- **No history in source.** Comments in `src/` and in a crate's built-in data (`defaults/*.toml`) say what the
  code or value is and why, in domain words. No brief, design, lane, review-finding, question or requirement
  numbers, no section marks, no owner/overseer/reviewer/tester rulings, no dates. History lives in git and in
  `ecad/docs`; only READMEs may link to documents in `ecad/docs`. `cargo xtask layers` enforces this.
- Test module docs say what they test, not which brief or role they came from.

## 9. Pre-v1 file format

No compatibility before v1: an older file opens read-only with "older format version". Any format change bumps
`SCHEMA_VERSION`; the golden file pins it.
