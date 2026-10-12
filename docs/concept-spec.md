# ECAD Concept Spec

Oct 12, 2026 · Carl

## Vision and principles

A fast, modern electrical CAD that does for schematics what Onshape did for 3D: the designer just draws, and the software infers wires, cables, devices and cross-references in the background. It is not an AutoCAD clone and not an attempt to kill EPLAN; it is a better tool for the people current ECAD serves badly.

- **Infer, don't ask.** No database tables, project setup or library admin before drafting. Relationships come from what is drawn.
- **Every inference is visible and overridable.** A designer who can't see what was inferred won't trust it.
- **Late binding.** Draw generic first ("24V PSU", "7-core cable"); pick the real part when ready.
- **The document is the truth.** One file holds the project as a log of the user's actions; everything else is derived.
- **Prove it for the user.** Every feature is judged by whether it makes the lone designer's day easier. Legacy conventions are kept only when they earn their place.
- **Traditional 2D look.** Matte black canvas, thin red/green/cyan vectors, utilitarian UI: fast and familiar to anyone who has used CAD.

## Target user

The lone electrical designer, or a small team of two or three. In most companies electrical design is one person, not a department, and that person has no CAD administrator, no database specialist and nobody to ask.

- **Bar to clear:** open the app, start drawing, get correct outputs, with no setup step at all.
- **Collaboration:** single user first. Concurrent multi-user editing is out of scope.
- **History:** a current snapshot plus named versions (Rev A, pre-FAT) is enough; full branching history is a later nice-to-have.
- **Deliverables:** a PDF set of the drawings with BOM, wire list, cable schedule and terminal plan; DWG copies; CSV/Excel exports of BOM, wire list and labels. No ERP integration.

## Problems and solutions

Four pain points in today's ECAD, each solved by inference rather than administration.

| Pain today | Our solution | How it feels to the user |
| --- | --- | --- |
| Cross-references tied to page/column coordinates break on renumbering; coil/contact pairing needs a database record first | **Identity by tag.** `K1` anywhere is the same device; cross-ref text is a live field; hover lens previews the target; orphans get a red marker | Renumber 200 pages and nothing breaks |
| Wires and cables live in separate worlds; cores assigned one by one in a cable manager | **Cable slash gesture.** A slash across wires infers a cable, cores in order; same tag on another page = same cable; fan-in/fan-out sidebar builds itself | Draw the slash, the cable schedule exists |
| Project databases, server sync, locked projects, many sidecar files (.wdp etc.) | **One-file project as an operation log.** No server, no tables, nothing to sync | Email one file; it opens anywhere |
| Parts, symbols and macros must exist before drafting can start | **Draw first, promote later.** A drawn box with pins is a provisional type; pick the real part later, pins map by label; BOM shows `?` until resolved | Electrically complete before commercially complete |

The biggest technical risk across all four is ambiguous geometry: crossing vs joining lines, near-miss endpoints, text that may or may not be a tag. Strict junction-dot rules, snap tolerance, and a connectivity overlay make inference safe.

## Architecture

A Rust fork of CADCraft supplies the drawing engine; our own crates add the operation log, the inference engine and the outputs.

```
            DWG import (adopt) ──┐            ┌── Libraries (.ecadlib)
                                 ▼            ▼
   Draw (CADCraft engine) ──► Operation log (.ecad) ──► Inference engine
                                                              │
                                                              ▼
                                          Derived state (cached in checkpoints)
                                                              │
                                                              ▼
                                   Views and outputs: overlays, report pages,
                                   PDF / DWG / CSV / XLSX, MCP server
```

The operation log is the only source of truth. Imported DWGs and library symbols enter as operations too, so every path into a project is recorded and replayable.

- **Engine (CADCraft fork):** geometry, snapping, commands, rendering, DXF/DWG I/O. MIT/Apache licensed; its UI is separate from the engine.
- **`ecad-ledger`:** the op-log file format, checkpoints, named versions, compaction.
- **`ecad-topology`:** connectivity graph from geometry and pins, with strict junction rules.
- **`ecad-infer`:** tag identity, cables, off-page matching, orphans and open ends.
- **Outputs:** report pages, release check, PDF/DWG/CSV/XLSX writers.

## File formats

The project is one file, and that file is a log of what the user did, not a database. Libraries use the same format.

**`.ecad` project file**

- Header, then an append-only sequence of operations encoded compactly (CBOR or MessagePack): `AddWire`, `SetTag`, `SlashCable`, `PromotePart`, `MovePage`.
- Current state is produced by replaying the log through the inference engine. Nets, cross-refs, BOM and diagnostics are never stored as fact.
- **Checkpoints** of materialised state are written periodically so opening is fast. They are a cache: delete them and the project still opens, more slowly.
- **Named versions** (Rev A, pre-FAT) are bookmarks in the log.
- **Crash safety:** saving only appends; an incomplete trailing operation is ignored.
- **Compaction** rewrites the file as latest checkpoint plus named versions to keep size down.
- **Schema evolution:** every operation type is versioned; old operations are upcast on replay. No migrations.
- **Pages are first-class objects** inside the file: reorder, insert, renumber, and cross-references follow.

**`.ecadlib` library file**

- Same operation-log format, holding symbols instead of pages, with its own named versions.
- Any number of libraries: personal, company standard, per-customer.

**Determinism rule:** user decisions ("not a connection", "K1 is part X", "cable is 7G1.5") are always recorded as explicit operations. Inference only fills what the user never stated, so a newer engine cannot silently change an old project.

## Symbols, parts, search and transfer

Symbols are copied into projects but remember where they came from, so a project always opens on its own and never changes underneath a released drawing.

- **Symbol = graphics + named pins + properties.** A part (manufacturer, part number) is separate, so one contactor symbol serves many parts.
- **Embedded copy with origin.** Using a symbol puts a copy in the project tagged with its library and version. Library improvements show a quiet "update available"; nothing updates automatically.
- **Harvest.** Drag any ad-hoc symbol from a project into a library to make it reusable.
- **Copy and paste between projects** carries the symbol with it.
- **One search box** across the current project, all open libraries and recent symbols: by name, tag prefix (`-K`, `-S`), function ("timer relay"), pin labels ("A1 A2 15 16 18") or property ("24V coil").
- **Search by sketch.** Draw a rough box with four pins and get matching symbols; pin count and layout narrow results quickly.
- **Same-name conflicts** are never merged. Both show in search with their origin; inside a project each copy keeps its own.

Open: whether parts live in the same `.ecadlib` as symbols or in a separate catalogue file.

## DWG interoperability

DWG is an exchange format, not the working format. Full round-trip editing is not required; our exports must open in AutoCAD without errors, and AutoCAD drawings should convert in automatically.

**Export**

- One multi-layout DWG, or one DWG per page.
- Plain standard entities plus block attributes only, so tags and wire numbers stay visible to the recipient. No custom object classes (they appear as proxy objects in AutoCAD).
- Automated test on every build: exports open cleanly in a reference DWG viewer.

**Import ("adopt")**

- Accepts a single DWG, a folder of DWGs, or an AutoCAD Electrical project (the `.wdp` is a text list of drawings, giving page order and titles).
- Inference reads block names and attributes, wire layers and off-page text patterns such as `/11.4`.
- Ends with a confidence review ("142 nets found, 6 uncertain"); the user confirms, and the result is one clean `.ecad` project.

**Engine note:** CADCraft's own DWG support is early (its documentation is inconsistent on write versions, and unmodelled data is dropped on save). Because DWG is exchange-only here, that matters far less: export only needs clean, widely readable output.

## Outputs and release

Reports are generated pages inside the project, so they can never be out of date, and one Release step produces a matched set.

| Output | Format | Typical recipient |
| --- | --- | --- |
| Drawings + report pages | PDF; DWG per page or multi-layout | Customer, workshop, archive |
| BOM | CSV / XLSX | Purchasing |
| Wire list (from, to, size, colour, length) | CSV / XLSX | Panel builder, wire-processing machines |
| Cable schedule | CSV / XLSX | Installers |
| Terminal plan | In PDF; CSV / XLSX | Panel builder |
| Labels (wire, terminal, device) | CSV | Label printer software |

PDF/DWG plus CSV/Excel is the full scope for the foreseeable future. Column layouts work by default and can be adjusted as templates stored in the user's library.

**Release**

1. Pre-release check: open ends, orphans and unresolved parts are listed; `?` parts block release until resolved or deliberately accepted.
2. A named version (e.g. Rev C) is added to the log.
3. PDF and all exports are written to one folder or zip, every file stamped with that revision, so they always come from the same moment.

## AI and MCP server

The operation log makes the product AI-ready by design: an agent edits a project the same way a person does, by adding operations, so every AI change is visible, undoable and attributable.

**Why the architecture fits**

- **Ops are the API.** Each operation type maps to an MCP tool. No separate automation layer to keep in sync.
- **Derived state is queryable.** Nets, devices, cables, BOM and diagnostics are already computed views; an agent reads them instead of parsing geometry.
- **Every op records its author** (user or named agent), so the log shows exactly what the AI did.
- **Deterministic replay** means an AI change can be reviewed, reproduced and reverted precisely.

**Two levels of tools**

- **Domain level (ours, primary):** agents work in devices, pins, nets, cables and pages, not lines. Examples: `find_device`, `get_net`, `list_diagnostics`, `query_bom`, `search_symbols`, `add_device`, `connect`, `set_tag`, `slash_cable`, `promote_part`, `add_page`.
- **Drawing level (CADCraft's existing MCP server and JSON command channel):** for layout tweaks, annotations and title blocks.
- **Resources:** each page rendered as SVG/PNG so a vision model can check layout; report pages as CSV.

**Guardrails**

- AI changes land as a **proposal**: a batch of ops shown as a diff on the canvas (added in cyan, removed in red), accepted or rejected as a whole or per item.
- A named version is taken automatically before any AI batch is applied.
- **Release is human-only.** No tool can run the release step.
- The MCP server runs locally against the open project; nothing leaves the machine unless the user's chosen AI client sends it.

**Example uses**

- "Check this project against IEC 81346 tagging and list fixes."
- "Add four more motor starters like page 4, on new pages."
- "Total the 24 V loads and tell me if the PSU is big enough."
- "Fill wire numbers using my usual scheme."
- "Explain what this circuit does" (for review or handover).

## Lessons from Onshape

Onshape's data model is almost exactly what we are building, so we copy it closely. Its cloud-only delivery is what we avoid. It was founded in 2012 by former SolidWorks leaders who chose to start from scratch rather than port, arguing that cloud and collaboration could not be bolted onto file-based CAD ([Onshape blog](https://www.onshape.com/en/blog/why-we-started-from-scratch-in-the-cad-business), [Wikipedia](https://en.wikipedia.org/wiki/Onshape)).

**Adopt**

| What Onshape did | Why it worked | What we do |
| --- | --- | --- |
| Three layers: UI state (not stored), definition (stored), regeneration results (cached, always rebuildable) | Only the definition needs protecting; everything else can be thrown away | Same split: op log stored; nets, BOM, diagnostics cached in checkpoints; view state not stored |
| Every edit is an immutable **microversion** holding only the change relative to its parent | Full history at low storage cost; any state can be viewed or restored | Our op log is the same idea in a local file |
| Changes target features by **internal ID, not name** | Edits still apply correctly when things are renamed or reordered | Ops reference stable IDs; a tag like K1 is a property, never the key |
| **Undo appends the inverse change** instead of rolling back | History is never rewritten | Undo = a new inverse op |
| Branch and merge on the microversion tree, like git | Variants without copying the whole design | Later option: branch = copy the log at a point |
| **Built-in features are written in FeatureScript**, its open (MIT) user language; the standard library is public | One API for the vendor and users; extensions get first-class power | Our own tools (cable slash, inference rules) use the same public op API that the MCP server and plugins use |

Sources: [how collaboration works](https://www.onshape.com/en/blog/under-the-hood-how-collaboration-works), [FeatureScript intro](https://cad.onshape.com/FsDoc/intro.html).

**Avoid**

| What Onshape did | Downside | What we do instead |
| --- | --- | --- |
| Internet-only, server-side processing | Outages stop work; a dropped connection forces a refresh ([engineering.com](https://mobile.engineering.com/amp/17048.html)) | Local-first desktop app; fully offline; cloud optional later |
| No files at all; data lives in the vendor's database | Users don't hold their own design; leaving is hard | One `.ecad` file the user owns, plus open exports (PDF, DWG, CSV) |
| Merge conflicts resolved by "source wins" silently, so merges never block | Fine for geometry, risky for wiring where a silent overwrite is a safety issue | If branching is added, conflicts on connections and tags are always shown for a decision |

The core lesson: Onshape proved that a design stored as a history of changes, with everything else derived, is better than files plus a database. We take that model and deliver it as a local file instead of a cloud service.

## Roadmap

Build the core that everything rests on before any drafting features, and check the riskiest assumption (the CADCraft fork) first.

| Phase | Scope | Gate to next phase |
| --- | --- | --- |
| 0 Assess | Build CADCraft, map its crates, choose fork route | Fork plan approved |
| 1 Core | Op-log file, topology graph, tag inference, MCP spike; first milestone (below) | Replay equals checkpoint; golden tests pass; milestone demo works |
| 2 Draft | Wires, cables, cross-refs, symbols and libraries | A real project drawn in full |
| 3 Output | Report pages, Release step, PDF / CSV / XLSX / DWG export | DWG export opens cleanly in AutoCAD |
| 4 Adopt | DWG import, `.wdp` projects, review step | A legacy project converts cleanly |

No dates are set yet. Phase 0 is what the kickoff prompt below covers; it ends with a fork recommendation for review.

## First milestone: wires to cable, traditional ⇄ fan

The first visible win, inside Phase 1, because it shows the whole idea in one gesture.

1. Draw a handful of parallel wires between two devices.
2. Draw a slash across them: a cable is inferred, cores numbered in order, spares tracked.
3. Toggle the view between **traditional** (every core drawn as its own wire, cable shown by the slash) and **fan** (cores merge into one cable line that fans out at each end), with one key or button.

The toggle is a **view setting, not an edit**: both views are drawn from the same cable data, so switching adds nothing to the operation log, is instant, and can never lose a connection. Done when: draw, slash, toggle both ways, and the cable schedule matches in both views.

## Open questions

- [ ] Parts in the same `.ecadlib` as symbols, or a separate catalogue file?
- [ ] Which drawing standard to default to (IEC 81346 tagging, IEC 60617 symbols)?
- [ ] Does CADCraft's architecture allow a domain layer beside its drawing database without deep forking, or is a hard fork needed?
- [x] Licence decided: personal, non-commercial project outside FCSL, dual MIT / Apache-2.0.
- [ ] Product name.

## Kickoff prompt for a Claude cloud instance

Paste this into a new Claude Code session (cloud) with the forked repo attached. It covers Phase 0 and the first steps of Phase 1, and stops for review before anything large.

```markdown
Read docs/concept-spec.md first.

# Role
You are the lead Rust engineer on a new electrical CAD (ECAD) product. Work in small, reviewed steps and commit often.

# Product in one paragraph
A fast, modern 2D ECAD for the lone electrical designer. The user just draws; the software infers wires, cables, devices and cross-references. No relational database, no project setup, no library admin before drafting. Think "Onshape for schematics", not an AutoCAD clone.

# Fixed decisions (do not revisit without asking)
- Language: Rust. Base: fork of CADCraft (https://github.com/storytold/cadcraft, MIT/Apache-2.0). Use its geometry, snapping, commands and rendering; our domain model sits beside its drawing database.
- Native project file `.ecad` = append-only OPERATION LOG (CBOR or MessagePack), NOT SQLite or any database. Current state = replay of the log. Periodic checkpoints are a cache only. Named versions = bookmarks in the log. Every op type is versioned and upcast on replay.
- Libraries `.ecadlib` use the same op-log format.
- User decisions are always explicit ops; inference only fills what the user never stated. Replay must be deterministic.
- DWG is import/export only. Exports must be plain entities + block attributes, no custom objects.
- Outputs in scope: PDF, DWG, CSV/XLSX. No ERP integration.
- Single user. No concurrent editing. Local-first and fully offline.
- Ops reference stable internal IDs, never tags or names. Undo appends the inverse op; history is never rewritten. Every op records its author (user or named AI agent).
- AI-ready: every op type must be callable through one public API that our own tools, plugins and an MCP server all use (Onshape/FeatureScript model). AI edits arrive as a proposal batch the user accepts; release is human-only.
- Dependencies must be MIT/Apache/BSD compatible. No GPL.
- Project licence: dual MIT / Apache-2.0. Keep CADCraft's licence and copyright notices.

# Phase 0 - assess (stop and report when done)
1. Build the CADCraft fork, run its tests. Note toolchain version and any build problems.
2. Write `docs/cadcraft-assessment.md`: crate map, where the drawing database lives, how commands and undo work, how entities are identified, whether XDATA/attributes are kept, DWG read/write versions actually supported, and the cleanest place to attach a domain layer (fork vs wrapper).
3. Recommend fork strategy (hard fork vs upstream-friendly) with reasons.
Stop here and summarise findings before Phase 1.

# Phase 1 - first spikes (after approval)
A. Crate `ecad-ledger`:
   - Op enum with versioned variants: AddWire, AddSymbolInstance, SetTag, SlashCable, MarkNotConnected, PromotePart, AddPage, MovePage, NameVersion.
   - File: header + framed ops (length + checksum). Append-only save, fsync.
   - Recovery: truncated or corrupt trailing frame is ignored; test it.
   - Checkpoint write/read; prove state(log) == state(checkpoint + tail) with property tests.
   - Compaction keeping named versions.
B. Crate `ecad-topology`:
   - From line/polyline geometry + symbol pins, build a connectivity graph.
   - Rules: endpoint-on-endpoint or endpoint-on-pin within snap tolerance connects; T-junction connects only with a junction dot; plain crossing never connects.
   - Output nets with stable IDs; flag open ends and near misses (gap under 2x tolerance).
   - Golden tests with small hand-built schematics.
C. Crate `ecad-infer` (minimal): tag identity (same tag = same device across pages), off-page connector matching by label, orphan detection.
D. Crate `ecad-mcp` (thin): an MCP server over the same op API. Read tools first (find_device, get_net, list_diagnostics); write tools (add_device, connect, set_tag) produce a proposal batch, never a direct commit.
E. FIRST MILESTONE (priority once A-C work): wires to cable with a traditional/fan view toggle. Draw parallel wires, slash across them to infer a cable (cores in order, spares tracked), then switch the view between traditional (each core drawn, slash marks the cable) and fan (cores merge into one cable line, fanning out at each end). The toggle is a view setting, not an op. Done when draw, slash and toggle both ways work and the cable schedule matches in both views.

# Working rules
- Write tests first for format and topology rules.
- Keep a `docs/decisions.md` log: one line per decision with reason.
- Ask before adding a dependency over 1 MB or any with unclear licence.
- If a fixed decision looks wrong, raise it with evidence; don't silently change it.
- Open one pull request per spike so each can be reviewed on its own.
```
