# ECADCraft roadmap

The current plan, release by release. It replaces nothing: [`roadmap-v0.1.md`](roadmap-v0.1.md)
and [`discussions/roadmap-review-2026-11.md`](discussions/roadmap-review-2026-11.md) keep the
history and the reasoning. Near releases are firm; later ones are headlines and will change as we
learn.

**How we work.** Each feature is a brief with a design, built on its own branch and merged into
`dev` after one review and one fix round. A release is a QA gate from `dev` to `main`: the full
suite, the slow tests, the full bench and an owner trial. Every visible change is checked side by
side against the UI design boards.

## Released

### v0.2.0, 14 November 2026

M1 and UI rev 3:

- **Engine:** a live MCP bridge, so an agent can draw in the running app; multi-level terminals;
  connector devices with mating halves; PLC racks, cards, channels and an I/O list; graphical
  terminal-strip pages; a complete BOM; the symbol builder; continuation cross-references; the
  style model with wire categories.
- **UI rev 3:** a calm shell with one bar at each edge, panels that pin or float, inspector tabs,
  a page well, and the Draw, Project, Library and Publish workspaces.
- **Measured:** recreating a real 43-page drawing pack went from 12 to 34 recognisable pages.

## Next: v0.3.0, UI rev 4 (target: the week of 16 November)

Rev 4 keeps the rev 3 shell and adds the engineering a small or medium team needs. Datasheet
values (ratings, dissipation, sizes) are typed on a device's part set until the parts catalogue
arrives in 0.4.0.

### 0.2.1: foundations

- **Device-first model.** The device is the master: location, function, part number and every
  other device-level value are stored on it. Every symbol is an equal representation, typed by
  its definition (head, contact, pole, terminal, connector half), with derived parent, child and
  sibling relations. Location and function boxes *propose* a value and ask when it conflicts.
  The project keeps one list of functions and locations, with rename and merge.
- **Performance.** The BOM column change, strip build and PLC rack drop under their targets.
- **CI.** The full suite runs in parallel (nextest and sharded slow tests).
- **A faster dev harness** in the style of SolidCraft's testing, started now because it speeds up
  everything after it.

### 0.2.2: pages, title blocks, PLC signals, ratings

- **Page types and Settings ▸ Pages and sheets.** Each type brings its sheet, frame, title block
  and zones. Pages number from 1; a folder can carry a start number, with a Renumber pages button.
- **Generated front pages:** a cover with notes, wire-colour and symbol legends, revision
  history and contents, all kept up to date.
- **Title block editor** (Library ▸ Title blocks), and a built-in house-style border.
- **PLC signal list** with sync to the PLC project by CSV: compare, export and import tags.
- **Wire and cable rating checks**, and volt drop. Rating tables are supplied by the user.

### 0.2.3: layout, power, tests, network

- **Panel layout page:** plate and door, rails, trunking with fill %, cut-outs, dimensions, and
  devices placed from the schematic at real size. No routing, no 3D.
- **Power:** the load tree traced from the drawing, budgets per source with peak and diversity,
  and enclosure heat.
- **Tests and FAT:** tests generated from the PLC list, retest when something changes after a
  pass, and a printable FAT sheet.
- **Network:** ports, cables, VLANs and zones, with a generated topology drawing.

Network and Tests may move to 0.3.1 if the week runs short.

### 0.3.0: release

- The rev 4 boards checked side by side, the QA gate, and a re-run of the drawing-pack test.
- **A new README** with screenshots of the app and diagrams, in place of today's wall of text.

## Later (headlines)

### v0.4.0: libraries, parts and standards as data

- **The `.ecadlib` library format:** readable text (a folder of small TOML files, CSV for large
  tables), easy for anyone to write and review, validated by a checker, versioned by content.
  Projects keep embedding what they use.
- **Layered libraries:** `ecadstd` (ECADCraft's own neutral base), then `IEC` and `ANSI`, each
  carrying its letter codes, colours, grid pitch and calculation method. A company library can
  sit on top. A project picks its standard.
- **Symbols reviewed group by group** against real drawings, keeping what engineers use. All
  symbols are independently authored; no standards database is reproduced.
- **Symbols on an abstract grid**, so inch and mm standards both get correctly proportioned
  symbols and text.
- **The parts catalogue**, which gives ratings, power, heat and panel layout real data.
- **Calculations from first principles** (volt drop, current rating) where possible, validated
  against licensed tables rather than shipping them.
- **Image logos** in title blocks.
- **A standards review:** what may be built in, what stays user-supplied, how to describe it.
- **House style and example projects.** A generic border in the house style, and public example
  projects in an `examples/` folder, from a small motor circuit to a full machine:
  - power distribution with drives;
  - a 24 V supply with UPS and electronic circuit protection;
  - a small PLC with fieldbuses;
  - a safety circuit;
  - a motor circuit with its terminal strip.

### v0.5.0: manufacturer data

- PLC exchange formats beyond CSV (for example TIA Portal and TwinCAT tag tables).
- Manufacturer part data and device templates.

### v0.6.0: the project file pipeline

- A simpler, leaner `.ecad` pipeline. The old v1 container is removed.
- Groundwork for the v1 format freeze.

Out of scope on purpose: hazardous areas, functional-safety calculations, 3D and automatic
wire routing.
