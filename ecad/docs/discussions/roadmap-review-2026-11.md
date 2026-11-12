# Roadmap review, November 2026: from v0.1 to v1 and beyond

Status: **Proposal** (draft for the owner, 2026-11-12). Nothing here is approved. Sizes use the
scale of `roadmap-v0.1.md`: agent working days, not counting reviews. **S** is 1–2 days, **M** is
3–5 days, **L** is 1–2 weeks and **XL** is more than 2 weeks, which means the feature should be
split into briefs.

Written from `dev` at `717722b4` (029 merged) and design 030 revision 3 on
`feature/terminal-accessories`. It replaces nothing. If the owner accepts it, the agreed parts move
into `roadmap-v0.1.md` (or a new `roadmap.md`) and the open questions in §5 become decision rows.

The owner's direction, which every choice below is checked against:

- **No bloat.** "A clean modern experience that does what's needed for the drawing you need, not
  10 different ways just because."
- **IEC is the default, never the law.**
- **No dead-end refusals.** Every refusal names a way forward.
- **Live views with update maps.** No refresh buttons, no full rescans on an ordinary commit.

---

## 1. Where ECAD stands

Everything below is in `dev` unless the State column says otherwise. "Owner-tried" is not claimed
here; `quick-start.md` records which steps the owner has tried.

| Area | Briefs | What exists | State |
|---|---|---|---|
| Platform | 000–003, 007, 023 | Native Windows and Linux, CI, upstream CADCraft 0.19.0, a weekly upstream review | In `dev` |
| File and history | 002, 012–015, 021, 020a/b | One-file `.ecad` on redb (container v2), safe saves, writer locking, crash recovery. Edits cost about 1 ms on 500 pages. History, versions, Restore, branches and merge | In `dev` |
| Agent access | 011 | Headless `ecad-mcp`. Every command has an MCP contract test and records `Author::Agent` | In `dev`. The bridged (live-app) channel is deferred |
| Project manager | 016, 019 | Project panel with pages, folders, numbers, preview, reorder. A3 to A0 and custom sheets, a lettered and numbered frame, title block, project and page properties, revisions and Issue | In `dev` |
| Components and tags | 018 | Devices with IEC tags (any letters allowed), properties, **21 built-in starter symbols** (coil, NO/NC, 3-pole power contact, MCB 1/2-pole, fuse, PSU, push buttons, selector, lamp, 1- and 3-phase motor, transformer, terminal, PE terminal, PLC DI and DO channels, junction, continuation), a search-box Place, and Convert to Component for drawn outlines | In `dev`. No symbol editor, no library file |
| Cables | 004, 005, 009, 010 | Slash to make a cable, traditional and fan views, cores, cross-page references | In `dev` |
| Connectivity | 022 | Derived nets, junctions, continuations, potentials, stored wire numbers, labels in IEC 60757 colours, a Checks tab with fixes | In `dev` |
| Outputs | 024 | Locations `+` and location boxes, PDF book, generated cover, contents, revision list and BOM by location. BOM, wire report and cable report | In `dev` |
| Cross-references | 025 | General reference index, coil and contact references, contact mirror tables | In `dev` |
| Numbering | 026 | Renumber, fixed flag, swap, provisional tags, Finalise, "Make them one device" | In `dev` |
| Release | 027 | Pre-release check, named revision, stamped release folder with PDF and CSVs | In `dev` |
| Terminal strips | 028 | Strips as devices, a spare-position offer (`12+2PE`), wire bridges, terminal plan (report and generated page), cable schedule | In `dev` |
| Inspector | 029 | `ecadinspect serve`, the inspector protocol, fast Verify | In `dev` |
| Terminal accessories | 030 | Placed end stops, end plates, partitions, markers and strip labels. Jumpers as links owned by the strip (Jumper tool, lanes, bars on the BOM) | **Being built** (`feature/terminal-accessories`) |

**What this adds up to.** The core is strong: an authoritative transaction model, real
connectivity, live references, numbering, release, and fast, crash-safe files with branching. That
is further than most electrical CAD tools reach in their first years. What is still thin is
everything about **content and data entry**:

- only 21 symbols, and no way to make more except Convert to Component;
- PLC support is a single DI or DO channel symbol;
- one connection point per terminal (no levels);
- lists come out of ECAD but nothing goes in;
- there is no parts catalogue;
- tables can be read but mostly not edited.

That is where real drawings will stop next, and the roadmap below is ordered by it.

**Multi-level terminals: status review (asked for by the owner).** Today a terminal is a device
with one connection point (`terminal` and `terminalpe` starters, key `1`). Design 028 Q4 left
levels out of v0.1 and recorded the path: a multi-level starter is a definition with one connection
point per level (`1.1`, `1.2`, `1.3`), plus a Level column in the plan. No model change is needed.
Design 030 links join whole positions and name "links per level" as the likely next brief. What a
real build needs on top of 028's note:

- **Level-aware positions.** The plan sorts by position and then level. The strip offer
  (`12+2PE`) gains a level count per family (`12x2`).
- **Links per level.** A link's positions become `position.level` (`link.1 = 1.1-4.1`). This is
  still a strip property, with no format change. A position without a level means level 1, not
  "every level", so an old file reads the same (question 5). Renumber rewrites the values as 030
  FP4 does.
- **Internal level bridges.** Some multi-level terminals join levels inside the block (PE base,
  N-disconnect). This is `Definition::bridges`, which already exists (022 FP5).
- **Drawing.** One symbol per level, so a level can be drawn on the page where its circuit is. This
  is the same representation model that lets a coil and its contacts sit on different pages (I14,
  relaxed in 018).
- **Disconnect, fuse and sensor terminals** become starters (or library symbols) with internal
  bridges. Again there is no model change.

**Verdict:** size **M**, no format change, ready for a brief after 030 lands. It belongs in the
first milestone because control panels with sensor wiring use 2- and 3-level terminals routinely.

---

## 2. Gap analysis against existing electrical CAD

ECAD is clean-room. This section compares **capabilities**, using public product documentation
(sources in Appendix A). Nothing was copied, and nothing here proposes copying a workflow, layout or
term that is not generic electrical practice.

### 2.1 Per product

| Product | Genuinely useful, and ECAD lacks it | Bloat to avoid |
|---|---|---|
| **EPLAN Electric P8** (+ Pro Panel) | Navigators: editable tables per object type (devices, terminals, cables, PLC, potentials) that edit the same data as the pages. **Unplaced devices**: a device exists in the list before it is drawn, and you drag it onto a page. PLC overview with automatic addressing and assignment-list import/export to PLC tools. Multi-level terminals with per-level bridges. A graphical terminal-strip editor. Macros (reusable circuit snippets) with value sets. In Pro Panel: a 3D mounting layout from part dimensions, wire lengths, and drilling data | Hundreds of report forms and a form editor nobody masters. Many overlapping ways to do one thing (navigator, dialog, page, property mask). A huge configuration surface (schemes, filters and settings per action). Module-licensed features. Placeholder objects and value-set machinery for simple reuse |
| **AutoCAD Electrical** | A PLC module builder from a catalogue database, plus a spreadsheet-to-PLC I/O workflow (draw the I/O drawings from a sheet). A terminal strip editor and terminal tables. Many schematic reports (PLC I/O address and description, component connections, terminal plan). Panel footprints linked to schematic devices | Everything is a block with attributes on top of DWG. Two databases to keep in sync, and "Update/Retag" passes. Separate reports where a live view would do. Five drawing styles for the same PLC module |
| **SEE Electrical** (IGE+XAO / ETAP) | Graphical lists (component, terminal, PLC I/O, cable) that **navigate back to the diagram**. Integrated relay-contact, connector and cable managers. Automatic list and label generation. Panel heat-dissipation and temperature-rise calculation (Expert) | Tiered editions that split one workflow across licences. Separate "levels" of the product with different data models |
| **Zuken E3.series** | One object-oriented database behind every view (schematic, cable, panel, formboard), so a change shows everywhere at once. ECAD already has this by design (P1, P6). Cable and harness structures (twisted pairs, shields) shown automatically in the schematic. Automatic panel wiring routes from connectivity and duct fill | Harness formboard and wire-harness manufacturing are a different trade. Its breadth (fluid, harness, cable, panel) is aimed at enterprise teams |
| **WSCAD ELECTRIX** | A terminal manager with export to terminal-maker tools. Import of terminal charts, material lists and connection lists from other ECAD tools. A cabinet layout with collision checks and wire lengths. Production data for drilling and milling. A large free parts portal. AI layout from the schematic (2026) | Six engineering disciplines in one product (fluid, building automation, installation). Portal and shop sign-in woven into the tool. A mobile AR app |
| **QElectroTech** (open source, GPL: compared from its user manual and changelog only, never its code) | A simple element (symbol) editor in its own window. A user collection of symbols. A nomenclature (BOM) table placed on a page. A terminal-strip editor (0.100) with undo. It shows how small a useful tool can be | Symbols as loose XML files with no identity or update path. Little connectivity semantics, so its lists are thin. Not a model to copy, and its code must never be read (GPL, house rules) |
| **SOLIDWORKS Electrical** (successor to SEE's schematic line, through Dassault) | A complete terminal-strip system from insertion to generated drawings and cabling. Live 2D–3D link to the MCAD model | Being tied to one MCAD product. A central SQL server for single users |
| **KiCad** (open source, GPL code; its file formats are publicly documented) | A clean symbol editor and library manager. Text-based libraries with a public format | It is a PCB tool. Its schematic semantics (nets by label, no devices across pages) are not panel semantics |

### 2.2 Consolidated

**Must-have before v1** (real control-panel drawings stop without it):

1. **Making symbols**: a symbol builder, and saving symbols into a library.
2. **Libraries and a parts catalogue**: reuse across projects, part data (maker, number, dimensions,
   accessories) kept out of every device's free properties.
3. **PLC cards**: a multi-channel PLC device, addresses, an I/O list, automatic addressing.
4. **Multi-level terminals** and per-level links (§1).
5. **Editable tables**: the terminal, cable, device and PLC managers, editing the same model as the
   pages, both ways, live.
6. **Lists in**: device-list and I/O-list import, creating unplaced devices to draw from.
7. **Wire types**: already deferred in `roadmap-v0.1.md`, needed for real wire reports.
8. **A report generator**: column choice, filter, sort and group over fixed data sources, out to a
   page, CSV and XLSX.
9. **A 2D panel layout**: rails, ducts and devices from part dimensions; rail fill; a mounting
   drawing; the link to schematic devices.
10. **Format stability for v1**: migration and upcasting (E4), so a v1 file opens in every later
    build. Today newer additions are refused by older builds, which is acceptable pre-alpha but not
    at v1.
11. **Graphical terminal-strip pages, continuation cross-references, connectors and a complete
    BOM.** The drawing-pack test (§6) showed each of these in a real pack. The competitors all have
    the first two.

**Nice-to-have** (adds real value; can follow v1 or slot in if cheap):

- **Revision compare**: a visual difference between two revisions (clouds, a changed-objects
  list). ECAD's history makes this cheaper than in any competitor, and it is a differentiator.
- Reusable **circuit snippets** (a motor starter, a safety relay circuit) stored in `.ecadlib`.
- **Label export**: wire markers, terminal markers and device labels as files for label printers.
- **Panel heat-dissipation estimate** from part data (losses per device, enclosure size).
- **Terminal and assignment-list exchange** with terminal makers' and PLC tools (CSV and
  AutomationML-style) once the internal model is settled.
- 3D panel (SolidCraft) and KiCad exchange: post-v1, §3.4.

**Avoid** (deliberately not building):

- A form or template **designer** for reports. Offer a column, filter and group chooser and a fixed
  set of page styles instead.
- **More than one editor for the same data.** A manager *is* the table view of the model; there is
  no separate dialog, property mask or report doing the same edit.
- **Configuration schemes** (named sets of settings per action). Project settings plus sensible
  defaults are enough.
- Fluid, hydraulic, building-installation and harness formboard disciplines.
- Module licensing, sign-in portals, and online-only parts.
- Block-and-attribute thinking (AutoCAD Electrical's model). ECAD's typed model is the point.
- A "retag/update project" button. Views are live (owner rule).

---

## 3. Proposed roadmap

### 3.1 Principles for the order

1. **What unblocks real drawings comes first.** The drawing-pack test (§6) ranked the gaps by how
   much of a real 43-page pack each one blocks. In order, the first walls are:
   - **graphical terminal-strip pages** (about a third of that pack);
   - **multi-level terminals**;
   - **PLC racks and cards**;
   - **continuation cross-references**;
   - **connectors**;
   - **more symbols**.

   M1 below follows that order within each lane.
2. **Data entry before data output.** ECAD already produces good outputs. It needs to take data
   in (symbols, parts, lists) and to edit it in tables.
3. **The UI shell goes early.** The look-and-feel pass touches every panel, so its foundation
   (tokens, layout, the table component, the radial wheel) lands before the managers are built,
   not after. Otherwise every manager is restyled twice.
4. **Format changes are batched** behind one owner decision per milestone, and **v1 freezes the
   format with migration**.

### 3.2 Milestones to v1

**M0: close v0.1 (now).** 030 has landed, the drawing-pack findings are triaged, the bugs the
stage 2 redraw found are fixed, and the v0.1 QA gate runs (with the three stale slow tests from the
2026-11-11 QA pass fixed first).

| # | Feature | Purpose (one line) | Depends on | Size | Format change |
|---|---|---|---|---|---|
| 0.1 | 030 accessories and jumpers | Strips as built, links that join nets | 028 | Done (merged to `dev`) | No (properties) |
| 0.2 | Drawing-pack quick fixes | Small gaps from §6. **Revision table columns** the project chooses (for example change request, validated, verified), stored as `rev.<n>.<field>` properties as today. **Tag display style**, as a project choice: no forced leading `-`, and the case kept as typed. Input already accepts either (`ecad-model/src/tag.rs`), so only the display changes. If the owner accepts question 23, the **pending-revision marker** in the title block goes here too. Every bigger gap is an M1 item | 030 | S each | Additive: optional project settings for the tag style and the pending marker |
| 0.3 | Stale slow tests | Make `cargo xtask ci` pass for the QA gate | none | S | No |
| 0.4 | **Bug sweep** (stage 2 of the drawing-pack test) | Fix the seven defects in §6.2, each with a test that fails before the fix. The first, wires drawn end to end that do not join, is a correctness bug and goes first. Items 2 and 5 overlap 1.13: the sweep removes the refusal and the worst overlaps, 1.13 sets the rules | none | S–M | No |
| 0.5 | v0.1 QA gate | `dev` → `main`, owner trial | 0.1–0.4 | S | No |

**M1: real drawings (v0.2).** Draw a real panel's schematics without workarounds.

| # | Feature | Purpose | Depends on | Size | Format change |
|---|---|---|---|---|---|
| 1.0 | **Live MCP bridge** (first, see below) | An agent drives the **running** app and the owner watches the drawing appear, as SolidCraft does; the server is registered for Claude so it is always available | 011 | M | No |
| 1.1 | **Symbol builder** | Draw a symbol with connection points and text slots, save it into the project as an embedded definition, and "promote" a drawn component to a symbol (proposal §4.1) | 018 | L | Additive: definition text slots (proposal §4.2, deferred so far). Optional and absent by default |
| 1.2 | **Multi-level terminals** | Levels per position, links per level, internal bridges, disconnect, fuse and sensor terminals. A **fuse or disconnect terminal breaks the net**: its two sides are different nets, as for any fuse. **Relabelling a strip terminal** (labels such as `1.L`, `1.N`, `1.PE`), shown on the page, the plan and the strip pages | 030 | M | No (definitions and properties) |
| 1.3 | **PLC racks and cards** | A rack or CPU with slots, and cards in them (DI, DQ, AI, AO, RTD, IO-Link, power) as devices. Each card's channels are drawn on any page, with channel numbers, addresses (`%I0.0`, `I0.0`, free text), labels that may be rotated, and spare channels listed. Automatic addressing by slot and channel. A generated **I/O list** page and CSV | 018, 025 | XL. Split it: (a) rack, slot and card model; (b) channel symbols and addressing; (c) I/O list | No if slots and channels are connection points and properties; FP to confirm in design |
| 1.4 | **Wire types** | Named conductor types per potential, overridable per wire, on wire reports and the BOM. Conductors already store their own `cross_section` and `colour` (022 FP3). The gap the pack shows is setting them quickly and showing them, which this item covers | 022 | M | Additive: project setting `wire_types` |
| 1.5 | **UI shell** (look-and-feel, part 1) | Tokens, type, panel layout, the History polish from 020b, a calm modern frame. Never a copy of another product | none | L | No |
| 1.6 | **Radial wheel** | Right-click quick-use wheel: the eight most likely actions for what is under the cursor | 1.5 | M | No |
| 1.7 | **Table component** | One editable, virtualised, live table (sort, filter, multi-select, inline edit, go-to on the page) that every manager uses | 1.5 | M | No |
| 1.8 | **Graphical terminal-strip pages** | A generated page per strip, drawn as the strip is built. Each terminal shows its inside target above and outside target below, with wire numbers, page references, cable boxes with core colours, levels, links and accessories (end plates). It is the same derived data as 028's tabular plan, in a second page style; it is not a second model | 1.2, 030 | L | No (derived) |
| 1.9 | **Continuation cross-references** | Continuation arrows on 025's general reference index, each showing where its other ends are (`page-column-row`), grouped under captions. Live, never typed by hand. **A cable that crosses pages through continuations** can be drawn on the second page as well: today neither a second cable slash nor `cablecontinue` does it, so the cable stops at the page edge | 022, 025 | M | No: derived; a caption is a continuation property. The cable case reuses the existing cable references |
| 1.10 | **Symbol pack** | More built-in starters, available before the builder: change-over contacts (11/12/14), 4-pole contactor, RCBO, residual-current device, SSR, proximity and float sensors, valve, thermocouple, heater, interlock, fuse terminal with a rating, and a **fuse bank**: one device holding several fuses, with connection points per fuse. All are our own definitions | 018 | M | No (built-in definitions, embedded on use as today) |
| 1.11 | **Connector devices** | Plug and socket halves as one device that mate, plus D-sub, fieldbus and RJ45/USB bulkhead connectors. One connection point per pin, and the halves can be on different pages | 1.1 | M | Probably no (properties); FP to confirm in design |
| 1.12 | **BOM completeness** | An internal article-number column. Untagged project rows (trunking and DIN rail in metres, crimps, fuse inserts). Assembly rows for a device's add-on parts (aux block, memory card), kept as `part.<n>.*` properties until the parts catalogue (2.3) takes them over. The jumper bars and end plates already come from 030. **Continuation sheets numbered in sequence** (400, 401, …) rather than all printing the first sheet's number; the rule applies to every generated report, not only the BOM | 024, 030 | M | No (properties) |
| 1.13 | **Drawing presentation and text styles** (the owner's top gap from the pack test) | Pages that read like a professional drawing. **A sans print font with bold** in place of the thin single-stroke drafting font (from `storytold/craft-fonts`, as for every Craft font; the stroke font stays as the fallback when it is absent). **Per-project text styles**: a size for each kind of text (tag, label, terminal, references, title block), with defaults for dense schematics (about 2.5 mm tags, 1.6 mm labels, 1.5 mm terminals); today most sizes are fixed in code at 3.5 mm. **Tag placement** that follows the symbol's rotation, avoids collisions and keeps a position the user moved across regeneration. **Wire labels** once per net per page, or only on segments longer than a set length. **No repeated tags**: a strip's tag once per group of adjacent terminals, and PLC channels without the card's tag. **Continuations** may show only the target reference. **Text** centred and right-aligned. Overlaps bug-sweep items 2 and 5 (§6.2): the sweep fixes the refusal and the worst overlaps, this item makes the rules | 1.0 | M–L | Additive: project text-style settings; a moved tag's offset as a property |

**Order within M1, by lane.** Each lane's order follows the ranking in §6.

- **Lane S:** 1.10 symbol pack, then 1.1 symbol builder, then 1.11 connectors.
- **Lane D:** 1.2 multi-level terminals, then 1.3 PLC racks and cards, then 1.9 continuation
  cross-references, then 1.4 wire types.
- **Lane U:** 1.0 live bridge, then 1.13 drawing presentation, then 1.8 graphical strip pages, then
  1.12 BOM completeness, then 1.5 UI shell, then 1.6 wheel, then 1.7 table.

1.13 comes straight after the bridge because every page the pack test drew is judged on it, and
1.8's strip pages should be drawn with its text styles rather than restyled later. Its settings and
font live in lane U (`look.rs`, the text-style module); tag and wire-label placement read those
settings from lane S's and lane D's files through one small, append-only hook each, agreed in the
brief.

Lane U is the longest. 1.6 and 1.7 may slip to the start of M2, but the managers wait for them
(principle 3). 1.8 starts only after 1.2 lands, because it reads lane D's strip data; it writes
only `generated.rs` and the page styles.

**1.0 Live MCP bridge, in detail.** Why first: every later brief is tested, demonstrated and
increasingly drafted by agents, and the drawing-pack test is an agent building a real pack. Today
that work is invisible until a file is opened. Watching it happen catches wrong-looking output in
seconds, and it is mostly composition.

- **Today.** `ecad-mcp` is headless only. Brief 011 excluded "the bridged mode (an agent driving
  the running trial; a later brief)" (`ecad/apps/ecad-mcp/src/lib.rs:20`). `deferred-work.md`
  ("Bridged agent access") holds the owner's condition for picking it up: authorship **per call**.
  The trial's crate doc lists "the control channel" as excluded on purpose
  (`ecad/apps/ecad-trial/src/lib.rs:37`).
- **Already upstream (CADCraft, used as is):**
  - the loopback control server and its JSON-lines protocol (`docs/control-protocol.md`);
  - the per-session control token in an owner-only file (`crates/mcp/src/control_token.rs`, CWE-306);
  - `cadcraft-mcp`'s `Remote` backend (`crates/mcp/src/backend.rs`), with reply-size caps and
    reconnect;
  - `Remote::lazy`, which serves tools before the app is open and connects at the first call.
- **New in ECAD:**
  1. The trial opens the control port behind an opt-in (`--control PORT`, off by default) and serves
     **ECAD's own tool set** over it: `ecad-mcp`'s `execute`, reads, reports and screenshot. It does
     not offer CADCraft's raw `cmdline.input` or `script`, for the same reasons brief 011 left them
     out of the headless server.
  2. ECAD's write gate, `Author::Agent(<client>)` and the per-call caps (`MAX_TRANSACTIONS_PER_CALL`)
     applied on the app side. Authorship is set **per call** and restored to `Author::User`
     afterwards, also when the call fails or panics (the deferred-work condition). A person's edit
     is never recorded as the agent's, and the agent's never as the person's.
  3. A visible banner while an agent is connected, showing when it last wrote. Every agent
     transaction shows its author in History.
  4. Calls run on the UI thread between frames, like a typed command. An agent call never
     interleaves with a person's half-finished interactive command; it waits, or it is refused with
     the reason ("the designer is in the middle of Place; try again").
  5. `ecad-mcp --connect PORT` selects the remote backend through `Remote::lazy`. **Launching is new
     work on Windows and Linux**: upstream's lazy launch opens the app on macOS only. Without a
     launch, a call that finds no app says how to start it, which is not a dead end.
  6. **Registering the server for Claude**, documented in `quick-start.md` with the exact
     `claude mcp add` line for Windows and Linux, so ECAD is always available in Claude Code. One
     server: headless when no app is open, bridged when one is.
  7. MCP contract tests over the bridge, matching brief 011's: the transaction, its author, refusal
     with writes off, and a person's edit between two agent calls keeping `Author::User`.
- **Format:** none. Authorship is already recorded per transaction (FILE-07), and the bridge adds
  no op and no field.
- **Size: M**, with about two thirds of it in the trial's side (the port, the author scope, the
  banner, queueing) and the rest in `ecad-mcp` (the backend switch, launch, docs, tests). It runs in
  lane U before the UI shell, because both edit the trial's frame.

**M2: lists and libraries (v0.3).** Reuse across projects, design from lists, and edit in tables.

| # | Feature | Purpose | Depends on | Size | Format change |
|---|---|---|---|---|---|
| 2.1 | **`.ecadlib` stage 3** | The library file: definitions, parts and snippets, with identity and history. Embedding by copy (LIB-01 to LIB-05), and "update available" | brief 006 contract (approve first), 1.1 | XL (split into 3 briefs as 006 proposes) | **Yes**: new container; `.ecad` unchanged apart from origins, which already exist |
| 2.2 | **Symbol library viewer** | Browse and search libraries and the project's embedded symbols, preview, place, update to a newer version | 2.1, 1.7 | M | No |
| 2.3 | **Parts catalogue** | Parts (maker, number, description, dimensions, connection-point map, accessories, losses) in a library. A device points at a part, and the BOM reads the part | 2.1 | L | Additive: device `part` reference (origin + content hash, as for definitions) |
| 2.4 | **Device manager** (device list) | Every device, placed or **unplaced**, in one live table. Create devices here and place them later (P6, DOC-06 already allow it) | 1.7 | M | No |
| 2.5 | **List-driven design** | Import a device list or I/O list (CSV/XLSX) as a proposal, review it, accept it; unplaced devices then appear in the device manager to draw from. Re-import shows differences, never overwrites silently | 2.4, 1.3 | L | No |
| 2.6 | **Terminal manager** | The terminal plan made editable: positions, levels, accessories, links, targets, reorder by drag, renumber. It **replaces** Reports ▸ Terminals; it is not built beside it | 1.2, 1.7 | L | No |
| 2.7 | **Cable manager** | Cables, cores, types, lengths, from and to, spare cores; assign cores in the table. A cable-type summary (used length and count per type). Cable structure: shields and twisted pairs, and a cable referenced from more than two places (today a cable has at most two references, `references.rs`) | 1.4, 1.7 | L (split: structure, then manager) | Additive: cable types typed like wire types; pair and shield membership as properties, FP to confirm |
| 2.8 | **PLC manager** | Channels, addresses, symbols, descriptions and the connected wire in one table; export and import of the assignment list | 1.3, 1.7 | M | No |
| 2.9 | **Report generator** | Report definitions: data source (devices, wires, cables, terminals, PLC, parts) × columns × filter × sort × group, out to a generated page, CSV or XLSX, live. It ships a **fuse list** (each fuse and the circuit it supplies) as one of its built-in definitions | 1.7 | L | Additive: project `reports` definitions |

**M3: v1.0.** A panel shop can run a real job from quote to build in ECAD, and files are safe for
years.

| # | Feature | Purpose | Depends on | Size | Format change |
|---|---|---|---|---|---|
| 3.1 | **Panel layout (2D)** | A mounting-plate page kind: enclosure, rails, ducts, devices drawn from part dimensions (a footprint representation of the same device, I14), rail fill and duct-fill warnings, an unplaced-in-panel list | 2.3 | XL (split: page kind and footprints; rails and ducts; checks) | Additive: page kind `panel`; footprint representations already allowed by I14 |
| 3.2 | **Revision compare** | Pick two revisions, see clouds and a list of changes; optional clouds in the PDF of an issue | 020, 027 | M | No (derived) |
| 3.3 | **Label export** | Wire, terminal and device markers as CSV for label printers, from the managers | 2.6, 2.7 | S | No |
| 3.4 | **XLSX export** | Every report and manager can be exported as XLSX | 2.9 | S | No |
| 3.5 | **Format freeze and migration** | E4 migration rewriting and op upcasting; a v1 compatibility promise; `ecad upgrade` for older pre-v1 files | everything with a format change | L | **Yes**: the v1 baseline |
| 3.6 | **Checkpoints (stage 4)** | Faster opening of long histories | 3.5 | M | Yes (cache; disposable, P9) |
| 3.7 | **Look-and-feel, part 2** | Polish over the managers, onboarding, the quick start rewritten for v1 | M2 | M | No |
| 3.8 | **v1 QA gate** | Full suite, bench, owner trial on a real pack | all | M | No |

### 3.3 Lanes: three in parallel without touching the same files

Each lane owns its crates and modules. Shared files are edited **append-only**: command
registration in `commands.rs`, starter or library registration, and the glossary. Design documents
name the lane's files in their §0, so the overseer can see an overlap before a worker starts.

| Lane | Owns | M1 | M2 | M3 |
|---|---|---|---|---|
| **S: symbols and libraries** | `symbols.rs`, `convert.rs`, a new `ecad-lib` crate, `ecad-file`'s library reader and writer, the symbol editor window | 1.10 symbol pack → 1.1 symbol builder → 1.11 connectors | 2.1 `.ecadlib` → 2.2 viewer → 2.3 parts | 3.1 panel layout (needs parts), 3.5 format freeze |
| **D: devices and data** | `devices.rs`, `strips.rs`, `terminal_cmds.rs`, a new `plc.rs`, `wires.rs` (wire types), `references.rs` and `xref.rs`, `ecad-model` property rules | 1.2 multi-level → 1.3 PLC racks and cards → 1.9 continuation references → 1.4 wire types | 2.4 device manager → 2.5 list-driven → 2.8 PLC manager | 3.3 labels |
| **U: experience and outputs** | `ecad-trial` shell and panels, `look.rs`, the design tokens, `outputs.rs`, `generated.rs`, `printbook.rs`, `ecad-mcp` | 1.0 live MCP bridge → 1.8 graphical strip pages → 1.12 BOM completeness → 1.5 UI shell → 1.6 wheel → 1.7 table | 2.6 terminal manager, 2.7 cable manager, 2.9 report generator | 3.2 revision compare, 3.4 XLSX, 3.7 polish |

The known collisions, and how to avoid them:

- **Managers sit in lane U** but read lane D's data. Lane D exposes a read model and commands; lane
  U only builds the table over them, so the files stay apart. The terminal manager (2.6) waits for
  multi-level terminals (1.2).
- **`ecad-model`** is touched by D (property rules) and S (library origins, the part reference).
  Each such brief proposes its format additions in its design §FP. The overseer serialises format
  PRs, never the whole lane.
- **The UI shell (1.5) touches every panel.** It runs while S builds the symbol editor in its own
  window and D works in model and commands, so nobody else is editing panels at the same time.

### 3.4 After v1

| Feature | Purpose | Depends on | Size | Format change |
|---|---|---|---|---|
| **SolidCraft link (3D)** | Send the panel layout (enclosure, rails, ducts, devices as boxes or part models) to SolidCraft over MCP or a file; bring back clashes. ECAD stays 2D; SolidCraft owns 3D | 3.1, parts with 3D references | L | Additive: part 3D-model reference |
| **KiCad integration** | Two directions only: (a) a PCB inside the panel appears as a device whose connection points come from a KiCad schematic's connectors (netlist or `.kicad_sch` read through the public format documentation); (b) import of KiCad symbol libraries as a source for the symbol builder, licence permitting (KiCad's libraries are CC-BY-SA-4.0, which is on the allowlist; record each in `ATTRIBUTION.md`). Never read KiCad's GPL code | 2.1 | M each | No |
| **PDF adoption with AI** | Rebuild an old project from its PDF through the MCP commands, as a proposal with confidence (owner idea, 2026-11-08) | 2.1, 2.3 | XL | No |
| **DWG / `.wdp` adoption** | After PDF adoption | ECAD-side DWG boundary (proposal §16) | XL | No |
| **Exchange formats** | AutomationML-style PLC exchange; terminal-maker exports | 2.6, 2.8 | M each | No |
| **Panel thermal estimate** | Heat loss from part data against the enclosure | 2.3, 3.1 | M | No |
| **Circuit snippets** | Reusable circuits in `.ecadlib`, placed with renumbering | 2.1 | M | Additive in `.ecadlib` |
| **Translation of texts** | Project texts in more than one language | — | L | Additive |

---

## 4. "One way to do it", per area

For each area: the single workflow I would pick, and why. Where other ways exist today, the
proposal is to keep them only if they are the same command reached from another place (menu,
wheel, command line, MCP), never a second editor.

| Area | The one way | Why |
|---|---|---|
| **Placing devices** | Place from the search box (or drag from the library viewer or the device manager's unplaced list): one Place command whatever the source | All three sources end in the same `place` transaction; the user learns one gesture |
| **Making symbols** | The symbol builder. Convert to Component stays only as the quick way to give an existing drawn outline meaning, and "Promote to symbol" goes through the builder | Two ways to make a symbol definition would drift apart |
| **Wiring** | Draw wires as wired; T needs a junction; continuations between pages | Already decided (022); matches how panels are built and checked |
| **Cables** | The slash makes a cable; the cable manager edits it | The slash is the fastest gesture on the page; the table is for data. Neither duplicates the other |
| **Terminals** | Place terminals on wires (028); the terminal manager edits strips, levels, accessories and links. The Jumper tool stays as the on-page shortcut for links (owner decision, 030) | The page shows connectivity; the strip as built is a table problem |
| **PLC** | Place channels on the circuit pages; address them in the PLC manager (automatic first, then by hand) | Engineers draw the circuit where it lives and address in bulk |
| **Lists in** | Import as a proposal (review, then accept) into unplaced devices | The same proposal pattern as AI adoption and merges; nothing is overwritten silently |
| **Numbering** | Provisional while drawing, Finalise before issue (026) | Already decided; one pass, in drawing order |
| **Reports** | One report generator; the built-in reports (BOM, wires, cables, terminals, I/O list) become shipped report definitions in it | One engine, one look, one export path. No form designer |
| **Outputs** | The release (027) is the only way to produce a deliverable set; PDF print stays for quick prints | A release is stamped and checked; ad-hoc prints are not deliverables |
| **Libraries** | A project embeds what it uses; updates are explicit, per symbol or part (LIB-03) | P8: a project opens anywhere; nothing changes behind the user's back |
| **Commands** | Every action is a command reachable from the command line and MCP; the wheel, menus and toolbar call the same command | The house rule "everything is a command"; agents and people do the same thing |
| **Navigation** | Every table row and Checks row goes to the object on the page; every object's hover lens offers its table row | One link in each direction instead of separate "find" tools |
| **Panel layout** | The footprint is another representation of the same device, placed from the "not in panel yet" list | I14 already allows it; no second device, no sync step |
| **Standards** | IEC 81346 tags, IEC 60617-style symbols and IEC 60757 colours as defaults; project settings change them; nothing refuses a non-IEC choice | The owner's rule: IEC is the default, never the law |

---

## 5. Questions for the owner

Each has a recommendation. Answer "as recommended" to accept them all.

1. **Order of M1.** Is M1 as ranked by the drawing-pack test (§3.2, "Order within M1") the right
   next milestone? **Recommend yes.**
2. **Symbols before libraries.** Build the symbol builder first, saving into the project, with
   `.ecadlib` after it? **Recommend yes.** It unblocks drawings weeks before the library container
   is designed, approved and built. The definitions it makes are the same ones a library will hold.
3. **Approve brief 006 (stage 3 contract) now, as a design-only task** in lane S beside the symbol
   builder? **Recommend yes.** The contract has been waiting since 2026-10-31, and 2.1 cannot start
   without it.
4. **Parts and symbols in one `.ecadlib` container?** **Recommend yes**, as separate tables in the
   same redb container. One file to share, one identity and history scheme. A part references the
   symbols it uses.
5. **Multi-level terminals: a link without a level means level 1.** **Recommend yes.** Old files
   read the same, and a link across levels is written explicitly (`1.1-1.2`).
6. **PLC addresses.** A free-text address per channel, with an automatic-addressing command that
   fills them by card and channel in a chosen style (`I0.0`, `%IX0.0`, `0:I/00`)? **Recommend
   yes.** No PLC vendor is built in.
7. **The radial wheel: fixed or configurable?** **Recommend fixed and context-aware for M1** (eight
   slots per object kind, chosen by the designer), with configuration only if the owner misses it
   after a month of use. Configuration surfaces are where bloat starts.
8. **Managers replace the read-only Reports tabs** (Terminals, Cables) rather than sitting beside
   them? **Recommend yes**, with a read-only toggle for checking without editing.
9. **List import: CSV and XLSX in, as a proposal.** **Recommend yes**, with re-import showing a
   difference and never overwriting a value the designer changed in ECAD without asking.
10. **Report generator scope: no free-form page designer.** Fixed page styles (table, grouped
    table, graphical strip), with column, filter, sort and group choices? **Recommend yes.**
11. **Panel layout in 2D for v1, 3D through SolidCraft after v1?** **Recommend yes.** 2D answers
    rail fill, duct sizing and the mounting drawing, which is what a panel shop needs at v1.
12. **v1 means a format promise.** v1 files open in every later build (migration and upcasting,
    3.5), and pre-v1 files are converted once with `ecad upgrade`. **Recommend yes.**
13. **Revision compare in v1** (3.2)? **Recommend yes.** It is cheap on ECAD's history, customers
    ask for clouded changes, and few tools do it well.
14. **KiCad: only the two directions in §3.4** (a PCB as a device; symbol import), and only after
    v1? **Recommend yes.**
15. **Lane U owns the managers' UI; lane D owns their data and commands.** **Recommend yes.** This
    keeps three lanes from colliding in `ecad-trial`.
16. **Wire types in M1, not after v0.1 as previously listed.** **Recommend yes.** Real wire reports
    and the cable manager need them, and the change is small (a project setting).
17. **Live MCP bridge (1.0) first in M1**, with the deferred-work condition (per-call authorship)
    as an acceptance criterion and ECAD's own tool set only over the port (no raw command line)?
    **Recommend yes.** It makes every later agent-built brief visible as it happens. Picking it up
    removes the "Bridged agent access" entry from `deferred-work.md` when its brief is approved.
18. **Control port off by default**, switched on per run with `--control PORT` or by a preference
    the owner sets once? **Recommend the preference, off by default.** "Always available" then needs
    one setting, and a machine that never opts in never listens.
19. **Graphical strip pages as a second page style of the one terminal plan** (1.8), not a separate
    report? **Recommend yes.** One derived plan, two styles (table and graphical), and the project
    chooses which goes in the book.
20. **A symbol pack (1.10) before the symbol builder?** It is about a week of lane S that unblocks
    most of the pack's missing symbols while the builder is designed. **Recommend yes.**
21. **Continuation captions** (1.9): a caption is a property of the continuation, and the arrows
    that share it are grouped under it? **Recommend yes.** No new object is needed.
22. **Cables with more than two references and with pairs and shields go in M2** (2.7), not M1?
    They ranked last of the ten blocking gaps. **Recommend M2**, unless the owner's next real job
    needs them sooner.
23. **A pending revision in the title block.** After a revision is issued and the design is edited
    again, the title block still shows the issued revision, and nothing marks that changes are
    pending. **Recommend showing it as pending** (for example `D*` or `E (draft)`), derived from
    history (edits after the last issue), with the style a project setting. It is S and fits 0.2.

---

## 6. Findings from the drawing-pack test

### 6.1 Stage 1: desk review

A separate session reviewed one of the owner's real control-panel packs (43 pages, confidential)
against ECAD. This is stage 1, a desk review: it compares the pack with what ECAD can draw, and
nothing has been redrawn yet. Only general gaps are recorded here, with no names, tags, pages or
company details. The table is ranked by how much of the pack each gap blocks. The "ECAD today"
column was checked against `dev` where the code could settle it.

| # | Gap (generic) | ECAD today | Roadmap item | Effect on the order |
|---|---|---|---|---|
| 1 | **Graphical terminal-strip pages**, about a third of the pack. Each terminal shows its inside target above and outside target below, wire numbers, page references, cable boxes with core colours, and end plates | Tabular terminal plan only (028), with accessories and links coming in 030 | **1.8** (new) | New M1 item, second in lane U after the bridge |
| 2 | **Multi-level terminals**: L/N/PE, 3-level sensor terminals, stacked and double-deck | One connection point per terminal (§1 status review) | **1.2** | Confirmed as the first item in lane D |
| 3 | **PLC racks and cards**: CPU with slots; DI, DQ, AI, RTD and IO-Link cards with channel numbers, addresses, rotated labels and spare channels | Single-channel DI and DO symbols only | **1.3**, widened from "PLC cards" to racks, slots and card types; resized to XL and split | Second in lane D |
| 4 | **Off-page continuation arrows with derived cross-references**, grouped under captions, 10–20 per page | Continuations join nets (022) but are not on the reference index (025), so their references would be typed by hand | **1.9** (new) | New M1 item in lane D |
| 5 | **Connector devices**: male and female halves, D-sub, fieldbus, RJ45/USB bulkheads | None | **1.11** (new) | New M1 item in lane S, after the builder |
| 6 | **A reusable user symbol library**: float switches, interlocks, proximity sensors, valves, thermocouples, heaters, SSRs, RCBO with RCD, 4-pole contactors | 21 starters; no builder, no library | **1.10** symbol pack (new), then **1.1** builder, then **2.1** `.ecadlib` | Symbol pack added first in lane S as quick relief |
| 7 | **Change-over contacts** (C/NO/NC, 11/12/14) on relays and switches | NO and NC only | **1.10** | In the symbol pack |
| 8 | **BOM completeness**: accessory and assembly rows (contactor + aux block, CPU + memory card); untagged rows (trunking and DIN rail in metres, end plates, jumper bars, fuse inserts, crimps); an internal article-number column | Devices and strip terminals (024, 028); end plates and jumper bars arrive with 030 | **1.12** (new); assemblies move to **2.3** parts later | New M1 item in lane U |
| 9 | **Cross-section and colour per conductor**, not only per potential | Partly there: conductors already store `cross_section` and `colour` (022 FP3); what is missing is setting them quickly and showing them | **1.4** wire types | No change; 1.4's purpose now names it |
| 10 | **Shielded and twisted-pair cables**, and cables referenced from more than two places | A cable has at most two references (`references.rs`); no pairs or shields | **2.7** cable manager, widened | Stays M2 (question 22) |

Smaller gaps:

| Gap | Where it goes |
|---|---|
| Panel layout | **3.1**, unchanged (M3) |
| Cable-type summary (used length and count per type) | **2.7** |
| Fuse list (each fuse and the circuit it supplies) | **2.9**, as a built-in report definition |
| Revision table columns (change request, validated, verified) | **0.2** quick fix |
| Fuse terminals with ratings | **1.10** symbol pack and **1.2** multi-level terminals |
| Tags without the leading `-` | **0.2** quick fix (display only; input already accepts both) |

**What changed in the roadmap because of these findings:**

- **Five new M1 items:** 1.8 graphical strip pages, 1.9 continuation cross-references, 1.10 symbol
  pack, 1.11 connectors and 1.12 BOM completeness.
- **PLC (1.3) widened** to racks, slots and card types, and resized to XL.
- **The cable manager (2.7) widened** to pairs, shields and more than two references.
- **0.2 made concrete:** two small quick fixes.
- **Lane order re-ranked** (§3.2, "Order within M1").
- **The UI shell moves later in lane U.** It now comes after the gaps that block drawings, and still
  before the managers.

### 6.2 Stage 2: redrawing the pack through MCP

An agent then redrew the whole pack in ECAD through the headless MCP server: about 1,900 calls, of
which 18 were refused. Again only general results are recorded here.

- **No page was faithful.** 28% of pages were recognisable, 40% partly reproduced and 33% not
  reproduced.
- **Of 47 kinds of element** in the pack, 34% work fully and 77% at least partly.
- **030** (strip accessories and links) and the existing continuation references behaved as
  designed.

The redraw confirms the stage 1 ranking. It adds these gaps:

| Gap | Where it goes |
|---|---|
| Relabelling a strip terminal (labels such as `1.L`) | **1.2**, with multi-level terminals |
| A cable that crosses pages through continuations cannot be drawn on the second page (neither a second slash nor `cablecontinue` works) | **1.9**, with continuation cross-references (M1) |
| A fuse bank: one device holding several fuses | **1.10** symbol pack |
| Fuse terminals that break the net | **1.2** |
| BOM continuation sheets numbered in sequence, not all with the first sheet's number | **1.12**, as a rule for every generated report |
| Tag display style: no forced leading `-`, case kept | **0.2** (already there) |

It also found seven defects, which become **0.4 bug sweep** before the QA gate:

1. **High: wires drawn end to end do not join.** The second wire's start floats (no `from`), it gets
   a new net, and nothing on screen shows it. Starting on the middle of a wire adds a junction, and
   `junction` repairs it afterwards. A silent disconnection is a correctness bug, so it goes first:
   an end that lands on another wire's end joins it, or the page shows that it is open.
2. `text` with `justify` is refused, although the command's documentation lists it. (1.13 then
   makes centred and right-aligned text part of the text styles.)
3. `table` is refused, with a message whose grammar is broken.
4. Automatic contact tables under a coil low on the page overlap the title block.
5. Repeated tags overlap: strip terminals repeat the strip's tag, PLC channels repeat the card's
   tag, and continuation names overlap one another. (1.13 then removes the repetition by
   rule: once per group of terminals, no card tag on channels.)
6. Missing data prints as a yellow `?`, which cannot be seen on white paper.
7. The PDF is about ten times too large (32 MB for 30 sheets).

**What stage 2 changed in the roadmap:** M0 gains 0.4 bug sweep (the QA gate becomes 0.5); 1.2
gains relabelling and net-breaking fuse terminals; 1.9 gains cables across continuations; 1.10
gains the fuse bank; 1.12 gains sequential sheet numbers; and question 23 asks about a pending
revision in the title block.

**Presentation then became the owner's top gap** from the pack test: the pages are hard to read
because of the thin font, sizes fixed at 3.5 mm, repeated and colliding tags, and labels on every
wire segment. It is the new M1 item **1.13**, second in lane U after the bridge, before the strip
pages and the UI shell.

---

## Appendix A: sources

Public product pages and documentation, read on 2026-11-12. Vendor claims are marked as such where
they could not be checked. No product code or assets were read.

- EPLAN Electric P8, product page: <https://www.eplan-software.com/solutions/eplan-electric-p8>
- EPLAN help, PLC data and addressing: <https://www.eplan.help/en-us/infoportal/content/Plattform/2.9/Content/htm/plcgui_k_prinzip.htm>,
  PLC data exchange TechTip: <https://www.eplan.help/techtipps/en-us/SPS/TechTip-PLC-data-exchange.pdf>
- EPLAN 2.5 (macro navigator, terminal editor): <https://digitalengineering247.com/article/eplan-2-5-launches>
- EPLAN Pro Panel: <https://eplan.com/us-en/products/eplan-pro-panel.html>
- AutoCAD Electrical help, schematic reports: <https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-984D9F7F-80D5-44AD-BCFD-6EAE759E4220.htm>;
  PLC modules: <https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-1B613225-890C-428C-894C-BA75C399C2ED.htm>;
  command list: <https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-Electrical/files/GUID-33E21585-A333-42FF-AC86-9EE541B7FB77.htm>
- SEE Electrical (IGE+XAO / ETAP): <https://ige-xao.com/en/us/see-electrical>; SEE Electrical Expert
  documentation: <https://pdf.directindustry.com/pdf/etap-ige-ige-xao/see-electrical-expert-documentation/8807-921760.html>
- SOLIDWORKS Electrical help, terminal strips: <https://help.solidworks.com/2022/English/swelec/DrawTerminalStrip.htm>
- Zuken E3.series: <https://www.zuken.com/en/product/e3series/industrial-machinery-electrical-design/>;
  panel data sheet: <https://digital.zuken.com/rs/707-ZQM-176/images/DS-E3-panel-EN.pdf>
- WSCAD ELECTRIX: <https://www.wscad.com/us/electrix>; overview PDF:
  <https://blog.wscad.com/wp-content/uploads/2021/07/WSCAD_ELECTRIX_Overview_2021-07_EN.pdf>;
  cabinet engineering: <https://www.wscad.com/us/engineering-disciplines/cabinet-engineering/>
- QElectroTech user manual (menus, element editor): <https://download.qelectrotech.org/qet/manual_0.7/build/interface/menu_bar.html>;
  project roadmap: <https://qelectrotech.org/wiki_new/en/roadmap>
- Phoenix Contact, AI-generated cabinet with WSCAD (SPS 2025): <https://www.phoenixcontact.com/en-pc/events-and-news/news/the-worlds-first-ai-generated-control-cabinet-produced-by-phoenix-contact>

## Appendix B: ECAD documents this review relies on

`ecad/CLAUDE.md`; `ecad/docs/roadmap-v0.1.md`; `ecad/docs/features.md`;
`ecad/docs/deferred-work.md`; `docs/decisions.md`; `docs/document-architecture-proposal.md`
Part A (P1–P10, LIB-01–05, §4 representations); the brief list `ecad/docs/briefs/` (000–029);
design 028 (§2 levels, Q4) and design 030 revision 3 (on `feature/terminal-accessories`);
`ecad/docs/glossary.md`; the QA pass of 2026-11-11.
