# Archive

The history of ECAD's documentation, kept as written. Nothing here describes the product as it is now: the
current docs are listed in [docs/README.md](../README.md). Each archived file starts with one line naming the
current doc that took over its still-true facts.

Names in these files are the names of their day (for example `ecad-cadcraft` for `ecad-engine`, wire
categories for wire types, terminals for connection points and terminal blocks). Their links into other docs
point at the old tree and are not kept up to date.

The "Facts now in" column says where each file's still-true facts went, or "history only" when none remain.
Tables are oldest first.

## Briefs

The work orders, one per piece of work, as approved. The concept spec and the foundations note that came
before them are here too.

| No. | Date | Title | What it was | Facts now in |
| --- | --- | --- | --- | --- |
|  | 2026-10-12 | [ECAD concept spec](briefs/concept-spec.md) | The original product vision, target user, architecture and kickoff prompt | history only |
|  | 2026-10-12 | [ECAD architecture foundations](briefs/architecture-foundations.md) | The foundations: geometry without meaning, identity independent of names, meaning against drawing, incomplete designs | facts now in [developer/architecture.md](../developer/architecture.md) |
| 000 | 2026-10-31 | [Baseline review and plan](briefs/000-baseline-review.md) | Windows build check and review that produced review-2026-10-31.md and briefs 001-006 | history only |
| 001 | 2026-10-31 | [Windows CI job](briefs/001-windows-ci.md) | Add a native Windows CI job; Windows gaps shown as ignored tests with reasons | facts now in [developer/testing.md](../developer/testing.md) |
| 002 | 2026-10-31 | [Windows persistence](briefs/002-windows-persistence.md) | File identity (same-file), durable replace and lock behaviour on Windows | facts now in [developer/architecture.md](../developer/architecture.md) |
| 003 | 2026-10-31 | [Trial on Windows, doc corrections](briefs/003-windows-trial-and-docs.md) | Home-folder and path fixes for Windows; doc corrections D1-D14 | facts now in [user/projects.md](../user/projects.md) |
| 004 | 2026-10-31 | [Cable by slash gesture](briefs/004-cable-slash.md) | Slash across wires creates one cable of exactly the crossed conductors | facts now in [user/cables.md](../user/cables.md) |
| 005 | 2026-10-31 | [Traditional / fan cable view](briefs/005-cable-views.md) | One switch between traditional and fan view, presentation only | facts now in [user/cables.md](../user/cables.md) |
| 007 | 2026-10-31 | [Upstream sync to CADCraft 0.15](briefs/007-upstream-sync-0.15.md) | Sync from CADCraft 0.6.2 to 0.15.1, seams X1-X8 kept | facts now in [developer/architecture.md](../developer/architecture.md) |
| 008 | 2026-11-01 | [ECAD tools by mouse](briefs/008-mouse-tools.md) | Interactive prompt forms for the JSON-only ECAD commands | facts now in [design/interaction.md](../design/interaction.md), [developer/commands.md](../developer/commands.md) |
| 009 | 2026-11-07 | [Cable look and core designation](briefs/009-cable-look-and-cores.md) | Cable label box, thick spine, curved bends, core numbers/colours | facts now in [user/cables.md](../user/cables.md), [design/cables.md](../design/cables.md) |
| 010 | 2026-11-07 | [Cable references across pages](briefs/010-cable-references.md) | Live cable continuation references between pages | facts now in [user/references.md](../user/references.md), [design/references.md](../design/references.md) |
| 011 | 2026-11-07 | [Agent access through MCP](briefs/011-agent-access-mcp.md) | ECAD commands reachable by MCP; author recorded; writes switch | facts now in [agents/mcp.md](../agents/mcp.md), [developer/features.md](../developer/features.md) |
| 012 | 2026-11-07 | [Project load benchmark](briefs/012-load-benchmark.md) | cargo xtask bench, synthetic projects, open/save scaling | facts now in [developer/testing.md](../developer/testing.md) |
| 013 | 2026-11-07 | [In-place replay, incremental validation](briefs/013-incremental-validation.md) | Apply cost proportional to what a transaction touches; rollback journal | facts now in [reference/file-format.md](../reference/file-format.md) |
| 014 | 2026-11-07 | [Drawing edits scale with change](briefs/014-drawing-edit-speed.md) | Translate step (CADCraft commit → ECAD ops) made incremental | facts now in [developer/architecture.md](../developer/architecture.md) |
| 015 | 2026-11-08 | [Start-up timing](briefs/015-startup-timing.md) | ECAD_STARTUP_TIMING=1 marks to find where 2.9 s start-up goes | facts now in [developer/build.md](../developer/build.md) |
| 016 | 2026-11-08 | [Project tab](briefs/016-project-tab.md) | See and switch pages by one click | facts now in [user/pages.md](../user/pages.md), [user/workspace.md](../user/workspace.md) |
| 017 | 2026-11-08 | [Tests for rapid development](briefs/017-fast-tests.md) | One test executable per crate, fast default vs slow set | facts now in [developer/testing.md](../developer/testing.md) |
| 017 | 2026-11-08 | [017 appendix: test names before/after](briefs/017-fast-tests-test-names.md) | Generated list proving every test still ran after 017 | history only |
| 018 | 2026-11-08 | [Components, tags, starter symbols](briefs/018-components-and-tags.md) | Devices from a symbol search with IEC tags, one identity across pages | facts now in [user/devices.md](../user/devices.md), [user/properties.md](../user/properties.md) |
| 019 | 2026-11-08 | [Sheets, title block, properties, revisions](briefs/019-sheets-and-properties.md) | Frames at true scale, zone grid, title block from project properties | facts now in [user/pages.md](../user/pages.md) |
| 020 | 2026-11-10 | [History and branches (020a/020b)](briefs/020-history-and-branches.md) | Visible history graph; version/revision/branch/merge; split into 020a history, 020b merge | facts now in [user/history.md](../user/history.md), [design/history.md](../design/history.md), [reference/glossary.md](../reference/glossary.md) |
| 021 | 2026-11-08 | [Container v2 on redb](briefs/021-container-v2-redb.md) | .ecad as one redb file with agreed tables | facts now in [reference/file-format.md](../reference/file-format.md) |
| 022 | 2026-11-08 | [Connectivity and wire numbers](briefs/022-connectivity-wire-numbers.md) | Nets derived from the drawing; net carries the wire number; open ends listed | facts now in [user/wires.md](../user/wires.md), [design/nets.md](../design/nets.md) |
| 023 | 2026-11-09 | [Upstream sync to CADCraft 0.19](briefs/023-upstream-sync-0.19.md) | Sync 0.15.1 → 0.19.0, seams X1-X10 | history only |
| 024 | 2026-11-11 | [Outputs: locations, PDF book, reports](briefs/024-outputs.md) | + locations, PDF book, generated pages, BOM/wire/cable reports + CSV | facts now in [user/locations.md](../user/locations.md), [user/publish.md](../user/publish.md), [user/reports.md](../user/reports.md) |
| 025 | 2026-11-11 | [Device cross-references](briefs/025-device-cross-references.md) | Coil/contact references and contact mirror on a general reference index | facts now in [user/references.md](../user/references.md), [design/references.md](../design/references.md) |
| 026 | 2026-11-11 | [Numbering](briefs/026-numbering.md) | Renumber, fixed, swap, provisional tags; one planner for devices, cables, wires | facts now in [user/devices.md](../user/devices.md), [design/numbering.md](../design/numbering.md) |
| 027 | 2026-11-12 | [Release (basic)](briefs/027-release.md) | Pre-release check, issue named revision, stamped output folder | facts now in [user/releases.md](../user/releases.md) |
| 028 | 2026-11-12 | [Terminal strips and cable schedule](briefs/028-terminal-strips.md) | Strip devices, bridges, cores used/spare, derived terminal plan and schedule | facts now in [user/terminal-strips.md](../user/terminal-strips.md), [user/checks.md](../user/checks.md) |
| 029 | 2026-11-12 | [Inspector protocol, ecadinspect serve](briefs/029-inspector.md) | Dev-only file inspector over versioned JSON/WebSocket (dev/ecad-inspector) | facts now in [developer/debugger.md](../developer/debugger.md) |
| 030 | 2026-11-12 | [Terminal strip accessories](briefs/030-terminal-accessories.md) | User-placed end stops, plates, markers, jumpers on plan and BOM | facts now in [user/terminal-strips.md](../user/terminal-strips.md) |
| 031 | 2026-11-12 | [Text styles, show/hide, placement](briefs/031-text.md) | Sans font, bold tags, per-kind sizes, hide/move texts | facts now in [user/styles.md](../user/styles.md) |
| 032 | 2026-11-12 | [Wires and nets](briefs/032-wires-nets.md) | Wire join, cable across continuations, label pile-up | facts now in [user/wires.md](../user/wires.md), [design/nets.md](../design/nets.md) |
| 033 | 2026-11-12 | [Print and project](briefs/033-print-project.md) | Small readable PDF; pack numbering, tags, revisions; table | facts now in [user/publish.md](../user/publish.md) |
| 034 | 2026-11-12 | [Component library build-up](briefs/034-component-library.md) | Built-in symbols drawn in code with points, prefix, anchors, BOM row | facts now in [user/library.md](../user/library.md) |
| 035 | 2026-11-12 | [Live MCP bridge](briefs/035-mcp-bridge.md) | One MCP server, headless or bridged to the running app; author per call | facts now in [agents/mcp.md](../agents/mcp.md), [agents/control-protocol.md](../agents/control-protocol.md), [user/agent-access.md](../user/agent-access.md) |
| 036 | 2026-11-12 | [Multi-level terminals](briefs/036-multi-level-terminals.md) | Levels per position (1.L, 2.1S), fuse/disconnect terminals break the net | facts now in [user/terminal-strips.md](../user/terminal-strips.md), [design/terminal-strips.md](../design/terminal-strips.md) |
| 037 | 2026-11-13 | [UI tokens, primitives, fonts (U1)](briefs/037-ui-tokens.md) | One token set and nine primitives; nothing styled locally | facts now in [design/ui.md](../design/ui.md), [design/decisions.md](../design/decisions.md) |
| 038 | 2026-11-13 | [The shell (U2)](briefs/038-ui-shell.md) | ECADCraft shell slots; CADCraft chrome behind a developer switch | facts now in [design/ui.md](../design/ui.md) |
| 039 | 2026-11-13 | [The one table (U3)](briefs/039-ui-table.md) | One virtualised editable table; a manager = column set + source | facts now in [developer/ui-components.md](../developer/ui-components.md) |
| 040 | 2026-11-13 | [The canvas (U4a)](briefs/040-ui-canvas.md) | Paper sheet, print ink vs overlay colours, sheet text in Barlow | facts now in [user/drawing.md](../user/drawing.md), [user/publish.md](../user/publish.md) |
| 041 | 2026-11-13 | [Wheel, intent line, tool groups (U4b)](briefs/041-wheel-tools.md) | Tools only start new things; edits on the object's wheel; one registry | facts now in [user/drawing.md](../user/drawing.md), [developer/commands.md](../developer/commands.md) |
| 042 | 2026-11-13 | [Inspector, Settings, Standards (U4c)](briefs/042-inspector-standards.md) | Value origin badges; one Standards screen for every default | facts now in [user/styles.md](../user/styles.md), [user/properties.md](../user/properties.md) |
| 043 | 2026-11-13 | [Agent banner, authors in History (U4d)](briefs/043-agent-banner.md) | Banner with pause/undo session; authors and agent sessions in History | facts now in [user/agent-access.md](../user/agent-access.md), [user/history.md](../user/history.md) |
| 044 | 2026-11-13 | [Connector devices](briefs/044-connectors.md) | Mating plug/socket halves; pin halves one net; each half its own BOM row | facts now in [user/connectors.md](../user/connectors.md) |
| 045a | 2026-11-13 | [PLC racks, slots, cards](briefs/045a-plc-rack-model.md) | Rack with numbered slots, card devices per slot | facts now in [user/plc.md](../user/plc.md) |
| 045b | 2026-11-13 | [PLC channels and addressing](briefs/045b-plc-channels.md) | Channel cells, addresses by slot/channel, Spare | facts now in [user/plc.md](../user/plc.md) |
| 045c | 2026-11-13 | [The I/O list](briefs/045c-io-list.md) | Live I/O list: page, CSV, MCP read, Lists column set | facts now in [user/plc.md](../user/plc.md) |
| 046 | 2026-11-13 | [Graphical terminal-strip pages](briefs/046-strip-pages.md) | Strip drawn as wired, second style of the terminals page | facts now in [user/terminal-strips.md](../user/terminal-strips.md) |
| 047 | 2026-11-13 | [BOM completeness](briefs/047-bom-completeness.md) | Internal article numbers, material rows, add-on parts | facts now in [user/reports.md](../user/reports.md) |
| 048 | 2026-11-13 | [Symbol builder](briefs/048-symbol-builder.md) | Project symbols with points, text slots, defaults; edit preview on copies | facts now in [user/library.md](../user/library.md) |
| 049 | 2026-11-13 | [Continuation cross-references](briefs/049-continuation-refs.md) | Live other-end references on arrows; grouped captions | facts now in [user/references.md](../user/references.md) |
| 050a | 2026-11-13 | [Style model](briefs/050a-style-model.md) | Standards and named style catalogues by stable ID, live updates | facts now in [user/styles.md](../user/styles.md), [design/styles.md](../design/styles.md) |
| 050b | 2026-11-13 | [Wire categories](briefs/050b-wire-categories.md) | Category gives colour, size, line look, label rule; conductor overrides | facts now in [user/styles.md](../user/styles.md) |
| 051 | 2026-11-13 | [PLC visuals, rack overview](briefs/051-plc-visuals.md) | Cards as one box sized to channels; rack overview; sheets | facts now in [user/plc.md](../user/plc.md) |
| 052 | 2026-11-14 | [Device-first model](briefs/052-device-first.md) | Device holds all device-level data; symbols equal, typed; locations proposed | facts now in [user/devices.md](../user/devices.md), [user/locations.md](../user/locations.md), [design/devices.md](../design/devices.md) |
| 053 | | Performance batch (v0.2.1) | no brief file; quick start only: [053](quick-starts/053-performance-batch.md) | |
| 054 | 2026-11-14 | [Page types, generated front pages](briefs/054-page-types-front-pages.md) | Page types; numbering from 1, folder start, Renumber pages; cover/legends/contents | facts now in [user/pages.md](../user/pages.md) |
| 055 | 2026-11-14 | [Title block editor (incl. 055c)](briefs/055-title-blocks.md) | Library ▸ Title blocks editor, built-in house border, DXF logos; second part 055c | facts now in [user/pages.md](../user/pages.md) |
| 056 | 2026-11-14 | [PLC signal list and sync](briefs/056-plc-signals.md) | Tag, safe state, drives; tag table export/import | facts now in [user/plc.md](../user/plc.md) |
| 057 | 2026-11-14 | [Wire and cable rating checks](briefs/057-ratings.md) | Ib ≤ In ≤ Iz, panel derating, user rating tables, volt drop | facts now in [user/circuits.md](../user/circuits.md), [design/decisions.md](../design/decisions.md) |
| 058 | 2026-11-14 | Paged reports | Open, not archived: the brief and its design are in [docs/briefs/](../briefs/README.md) | |
| 059 | 2026-11-15 | [Cable manager](briefs/059-cable-manager.md) | Build cables in Project ▸ Cables, assign in the drawing | facts now in [user/cables.md](../user/cables.md) |
| 060 | 2026-11-15 | [Tidy-up guards and check-kinds table](briefs/060-tidy-guards.md) | 0.2.3 wave 1: guards (R0) and check kinds table (R7) | facts now in [developer/guards.md](../developer/guards.md), [user/checks.md](../user/checks.md) |
| 061 | | Crate choices | no brief file; review only: [061](reviews/061-crate-choices.md) | |
| 062 | 2026-11-15 | [Shared test support](briefs/062-test-support.md) | ProjectBuilder behind test-support, common trial harness | facts now in [developer/testing.md](../developer/testing.md) |
| 063 | 2026-11-15 | [UI kit round 2](briefs/063-ui-kit-2.md) | Remaining shared widgets and semantic tokens in ecad_ui | facts now in [developer/ui-components.md](../developer/ui-components.md) |
| 064 | 2026-11-15 | [One update pipeline for derived views (R9)](briefs/064-derived-views.md) | Effect classification, Views::begin/finish on all commit paths | facts now in [design/views.md](../design/views.md) |
| 065 | | Crate map | no brief file; design only: [crate map](designs/065-crate-map.md) | |
| 066 | 2026-11-15 | [Defaults as data](briefs/066-defaults-data.md) | Built-in defaults as TOML data; chosen values stay chosen | facts now in [design/defaults.md](../design/defaults.md), [user/styles.md](../user/styles.md) |
| 067 | 2026-11-15 | [Crate split](briefs/067-crate-split.md) | Electrical domain split from the engine adapter (design crates.md) | facts now in [developer/architecture.md](../developer/architecture.md) |
| 068 | | Crate READMEs | no brief file; done in commits; no brief file | |
| 069 | 2026-11-15 | [One file format](briefs/069-one-format.md) | Remove v1 JSON Lines format and ecad upgrade; redb into ecad-store | facts now in [reference/file-format.md](../reference/file-format.md) |
| 070 | 2026-11-15 | [Typed errors, strict command inputs](briefs/070-errors-params.md) | thiserror per crate, anyhow at edges, typed command params | facts now in [developer/architecture.md](../developer/architecture.md) |
| 071 | | Root README | no brief file; done in commits; no brief file | |
| 072 | 2026-11-16 | [Typed object model](briefs/072-typed-object-model.md) | Lanes A-G: every stored object typed, no property bags | facts now in [design/object-model.md](../design/object-model.md) |
| 073 | | Renames (ecad-engine, dev/xtask) | no brief file; done in commits; no brief file | |
| 074 | 2026-11-17 | [Engine thinning](briefs/074-engine-thinning.md) | ecad-engine is only the CADCraft adapter; rules to ecad-electrical/model | facts now in [developer/architecture.md](../developer/architecture.md) |

## Quick starts

How each piece of work was tried when it landed, and the single quick-start guide used before the user guides
by feature.

| No. | Date | Title | What it was | Facts now in |
| --- | --- | --- | --- | --- |
|  | 2026-10-12 | [ECAD quick start (the single guide)](quick-starts/quick-start-guide.md) | The one guide used before the user guides by feature: setup, each feature's steps, trial records, limitations | facts now in [user/quick-start.md](../user/quick-start.md), [user/projects.md](../user/projects.md), [user/drawing.md](../user/drawing.md), [user/cables.md](../user/cables.md), [user/devices.md](../user/devices.md), [user/history.md](../user/history.md), [user/agent-access.md](../user/agent-access.md), [developer/build.md](../developer/build.md), [developer/testing.md](../developer/testing.md) |
| 003 | 2026-11-13 | [Brief 003: windows trial and docs](quick-starts/003-windows-native.md) | ~ paths, default folder, reserved names, no console | facts now in [user/projects.md](../user/projects.md), [user/quick-start.md](../user/quick-start.md), [developer/build.md](../developer/build.md) |
| 004 | 2026-11-13 | [Brief 004: cable slash](quick-starts/004-cable-slash.md) | change note + owner trial 2026-11-06 | facts now in [user/cables.md](../user/cables.md) |
| 005 | 2026-11-13 | [Brief 005: cable views](quick-starts/005-cable-views.md) | traditional/fan; cable-views.json | facts now in [user/cables.md](../user/cables.md) |
| 008 | 2026-11-13 | [Brief 008: usability mouse tools](quick-starts/008-mouse-tools.md) | click tools, marks, hover | facts now in [user/drawing.md](../user/drawing.md) |
| 009 | 2026-11-13 | [Brief 009: cable look and cores](quick-starts/009-cable-look-and-cores.md) | fan look, Core Label, Cable Join | facts now in [user/cables.md](../user/cables.md) |
| 010 | 2026-11-13 | [Brief 010: cable references](quick-starts/010-cable-references.md) | Cable Reference/Continue/Go to | facts now in [user/references.md](../user/references.md) |
| 011 | 2026-11-13 | [Brief 011: agent access mcp](quick-starts/011-agent-access-mcp.md) | headless ecad-mcp | facts now in [user/agent-access.md](../user/agent-access.md), [agents/mcp.md](../agents/mcp.md) |
| 012 | 2026-11-13 | [Brief 012: load benchmark](quick-starts/012-load-benchmark.md) | cargo xtask bench | facts now in [developer/testing.md](../developer/testing.md) |
| 015 | 2026-11-13 | [Brief 015: startup timing](quick-starts/015-startup-timing.md) | ECAD_STARTUP_TIMING | facts now in [developer/build.md](../developer/build.md) |
| 016 | 2026-11-13 | [Brief 016: project tab](quick-starts/016-project-panel.md) | Project panel, Developer section, one page at a time; records CADCraft 0.19.0 base | facts now in [user/pages.md](../user/pages.md) |
| 017 | 2026-11-13 | [Brief 017: fast tests](quick-starts/017-fast-tests.md) | tests/all, slow set | facts now in [developer/testing.md](../developer/testing.md) |
| 018 | 2026-11-13 | [Brief 018: components and tags](quick-starts/018-devices-and-tags.md) | devices, tags, starter symbols | facts now in [user/devices.md](../user/devices.md), [user/library.md](../user/library.md) |
| 019 | 2026-11-13 | [Brief 019: sheets and properties](quick-starts/019-sheets-and-properties.md) | A3 sheets, grid refs, folders, properties, revisions | facts now in [user/pages.md](../user/pages.md) |
| 020 | 2026-11-13 | [Brief 020: history and branches](quick-starts/020-history-and-branches.md) | 020b merge/update dialog | facts now in [user/history.md](../user/history.md) |
| 021 | 2026-11-13 | [Brief 021: container v2 redb](quick-starts/021-container-v2-redb.md) | redb file, conversion (gone since 069) | facts now in [reference/file-format.md](../reference/file-format.md) |
| 022 | 2026-11-13 | [Brief 022: connectivity and wire numbers](quick-starts/022-connectivity-and-wire-numbers.md) | nets, wire numbers | facts now in [user/wires.md](../user/wires.md) |
| 024 | 2026-11-13 | [Brief 024: outputs](quick-starts/024-outputs.md) | locations, location boxes, reports, BOM | facts now in [user/locations.md](../user/locations.md), [user/reports.md](../user/reports.md) |
| 025 | 2026-11-13 | [Brief 025: device cross references](quick-starts/025-device-cross-references.md) | contact ↔ coil, contact table | facts now in [user/references.md](../user/references.md) |
| 026 | 2026-11-13 | [Brief 026: numbering](quick-starts/026-numbering.md) | Renumber… preview | facts now in [user/devices.md](../user/devices.md) |
| 028 | 2026-11-13 | [Brief 028: terminal strips](quick-starts/028-terminal-strips.md) | terminals on wires, bridges, plan, schedule | facts now in [user/terminal-strips.md](../user/terminal-strips.md) |
| 031 | 2026-11-13 | [Brief 031: text](quick-starts/031-text-styles.md) | text styles, show/hide, placement (§5-031) | facts now in [user/styles.md](../user/styles.md) |
| 032 | 2026-11-13 | [Brief 032: wires nets](quick-starts/032-wires-and-nets.md) | wires join, cables cross pages, labels (§5-032) | facts now in [user/wires.md](../user/wires.md), [user/cables.md](../user/cables.md) |
| 033 | 2026-11-13 | [Brief 033: print project](quick-starts/033-print-book.md) | Print Book PDF, missing-value boxes, project info (§5-033) | facts now in [user/publish.md](../user/publish.md), [user/releases.md](../user/releases.md), [user/reports.md](../user/reports.md), [user/styles.md](../user/styles.md), [user/references.md](../user/references.md), [user/pages.md](../user/pages.md) |
| 034 | 2026-11-13 | [Brief 034: component library](quick-starts/034-built-in-symbols.md) | 21 → 81 built-in symbols (§5-034) | facts now in [user/library.md](../user/library.md), [user/devices.md](../user/devices.md), [user/circuits.md](../user/circuits.md) |
| 035 | 2026-11-13 | [Brief 035: mcp bridge](quick-starts/035-mcp-live-bridge.md) | live agent, --control, ecad-mcp --connect (§7D) | facts now in [user/agent-access.md](../user/agent-access.md), [agents/mcp.md](../agents/mcp.md) |
| 037 | 2026-11-13 | [Brief 037: ui tokens](quick-starts/037-ui-tokens.md) | Plex Sans, Graphite tokens, ui.primitives | facts now in [developer/ui-components.md](../developer/ui-components.md), [design/ui.md](../design/ui.md) |
| 038 | 2026-11-13 | [Brief 038: the shell (U2)](quick-starts/038-ui-shell.md) | shell regions, keys, developer switch | facts now in [user/drawing.md](../user/drawing.md), [user/workspace.md](../user/workspace.md), [user/projects.md](../user/projects.md) |
| 039 | 2026-11-13 | [039: the one table (U3)](quick-starts/039-ui-table.md) | shared table for Checks/Devices/terminals | facts now in [user/workspace.md](../user/workspace.md) |
| 040 | 2026-11-13 | [040: the canvas (U4a), how to try it](quick-starts/040-ui-canvas.md) | canvas look, zoom, selection | facts now in [user/drawing.md](../user/drawing.md), [user/wires.md](../user/wires.md) |
| 041 | 2026-11-13 | [041: the wheel, the intent line and the seven tool groups (U4b)](quick-starts/041-wheel-and-tool-groups.md) | action table, wheel, intent line | facts now in [user/drawing.md](../user/drawing.md) |
| 042 | 2026-11-13 | [042: the inspector, Settings and Standards (U4c)](quick-starts/042-inspector-settings-standards.md) | inspector, Settings as a place, Standards | facts now in [user/workspace.md](../user/workspace.md), [user/projects.md](../user/projects.md), [user/styles.md](../user/styles.md), [user/properties.md](../user/properties.md) |
| 043 | 2026-11-13 | [Brief 043: the agent banner and History authors (U4d)](quick-starts/043-agent-banner.md) | agent banner, authors in History | facts now in [user/agent-access.md](../user/agent-access.md), [user/history.md](../user/history.md) |
| 044 | 2026-11-13 | [044 Connector devices: how to try it](quick-starts/044-connectors.md) | plug/socket halves, mating | facts now in [user/connectors.md](../user/connectors.md) |
| 045 | 2026-11-13 | [Brief 045: PLC racks and cards](quick-starts/045-plc-racks-channels-io.md) | rack/cards, channels/addresses, I/O list (045a–c) | facts now in [user/plc.md](../user/plc.md) |
| 046 | 2026-11-13 | [046: graphical terminal-strip pages](quick-starts/046-terminal-strip-pages.md) | table vs graphic plan pages | facts now in [user/terminal-strips.md](../user/terminal-strips.md) |
| 047 | 2026-11-13 | [Brief 047: BOM completeness](quick-starts/047-bom-completeness.md) | BOM rows for accessories etc. | facts now in [user/reports.md](../user/reports.md) |
| 048 | 2026-11-13 | [048 Symbol builder: how to try it](quick-starts/048-symbol-builder.md) | draw/edit symbols, from page, by command | facts now in [user/library.md](../user/library.md) |
| 049 | 2026-11-13 | [Brief 049: Continuation cross-references](quick-starts/049-continuation-references.md) | continuation refs | facts now in [user/references.md](../user/references.md) |
| 050a | 2026-11-13 | [Brief 050a: the style model](quick-starts/050a-style-model.md) | standards, style catalogues | facts now in [user/styles.md](../user/styles.md), [user/checks.md](../user/checks.md) |
| 050b | 2026-11-13 | [Brief 050b: wire categories](quick-starts/050b-wire-categories.md) | wire categories | facts now in [user/styles.md](../user/styles.md), [user/checks.md](../user/checks.md), [user/reports.md](../user/reports.md) |
| 051 | 2026-11-13 | [Brief 051: PLC visuals and the rack overview](quick-starts/051-plc-visuals.md) | card on the page, rack overview | facts now in [user/plc.md](../user/plc.md) |
| 052 | 2026-11-14 | [052: the device-first model](quick-starts/052-device-first.md) | symbol types, function/location on device, proposals, function boxes | facts now in [user/devices.md](../user/devices.md), [user/locations.md](../user/locations.md) |
| 053 | 2026-11-14 | [053: the performance batch (v0.2.1)](quick-starts/053-performance-batch.md) | timing one group of bench rows | facts now in [developer/testing.md](../developer/testing.md) |
| 054 | 2026-11-14 | [Brief 054: page types, Settings ▸ Pages and sheets, generated cover pages](quick-starts/054-page-types-and-front-pages.md) | page types, cover pages | facts now in [user/pages.md](../user/pages.md) |
| 055 | 2026-11-14 | [Brief 055: Library ▸ Title blocks](quick-starts/055-title-blocks.md) | title-block library; border changed in 0.2.2 | facts now in [user/pages.md](../user/pages.md) |
| 056 | 2026-11-14 | [056: the PLC signal list](quick-starts/056-plc-signal-list.md) | Project ▸ PLC, tags, export/import CSV | facts now in [user/plc.md](../user/plc.md) |
| 057 | 2026-11-14 | [Brief 057: wire and cable rating checks](quick-starts/057-ratings.md) | Ib ≤ In ≤ Iz, volt drop | facts now in [user/circuits.md](../user/circuits.md) |
|  | 2026-11-14 | [Pack re-run fixes (2026-11-14)](quick-starts/pack-rerun-fixes-2026-11-14.md) | 11 bugs from drawing-pack re-run | history only |
|  | 2026-11-14 | [UI rev 3, board 1.01: the frame](quick-starts/uiv3-1-frame.md) | top bar, tool strip, rail, bottom bar, keys | facts now in [user/workspace.md](../user/workspace.md), [user/drawing.md](../user/drawing.md), [user/quick-start.md](../user/quick-start.md) |
|  | 2026-11-14 | [UI rev 3, section A: the left panels](quick-starts/uiv3-a-panels.md) | Book, Devices, History panels, page well | facts now in [user/workspace.md](../user/workspace.md), [user/pages.md](../user/pages.md), [user/history.md](../user/history.md) |
|  | 2026-11-14 | [UI rev 3, section B: the inspector](quick-starts/uiv3-b-inspector.md) | Device/Layout/Checks tabs | facts now in [user/workspace.md](../user/workspace.md), [user/devices.md](../user/devices.md), [user/properties.md](../user/properties.md), [user/checks.md](../user/checks.md) |
|  | 2026-11-14 | [UI rev 3, section C: tools, placing and the wheel](quick-starts/uiv3-c-tools.md) | tool groups, flyouts, keys | facts now in [user/drawing.md](../user/drawing.md), [user/devices.md](../user/devices.md) |
|  | 2026-11-14 | [UI rev 3, section D: the canvas, sheet colour and themes](quick-starts/uiv3-d-canvas.md) | sheet colour, dark | facts now in [user/drawing.md](../user/drawing.md), [user/reports.md](../user/reports.md), [user/history.md](../user/history.md) |
|  | 2026-11-14 | [UI rev 3, section E: the Project workspace](quick-starts/uiv3-e-project-workspace.md) | Project workspace (was Lists) | facts now in [user/workspace.md](../user/workspace.md), [user/reports.md](../user/reports.md), [user/checks.md](../user/checks.md) |
|  | 2026-11-14 | [UI rev 3, section F: Library, Publish and Settings](quick-starts/uiv3-f-library-publish-settings.md) | Library, Publish, Settings layouts | facts now in [user/library.md](../user/library.md), [user/projects.md](../user/projects.md), [user/publish.md](../user/publish.md), [user/styles.md](../user/styles.md), [user/properties.md](../user/properties.md) |
| 059 | 2026-11-15 | [059: the cable manager](quick-starts/059-cable-manager.md) | Project ▸ Cables, assign, glands (059a–d) | facts now in [user/cables.md](../user/cables.md) |
| 060 | 2026-11-15 | [Brief 060: tidy-up guards and the check-kinds table](quick-starts/060-tidy-guards.md) | guards, check kinds | facts now in [user/properties.md](../user/properties.md), [developer/guards.md](../developer/guards.md) |
| 063 | 2026-11-15 | [063: UI kit, round 2](quick-starts/063-ui-kit-round-2.md) | dialogs on ecad_ui | facts now in [developer/ui-components.md](../developer/ui-components.md) |
| 067 | 2026-11-15 | [067: the crate split, steps 9 and 10](quick-starts/067-crate-split.md) | ecad-desktop vs ecad-app | facts now in [developer/architecture.md](../developer/architecture.md), [developer/build.md](../developer/build.md) |
| 066 | 2026-11-16 | [066: defaults as data](quick-starts/066-defaults-as-data.md) | defaults/*.toml, wire labels 2.5 mm, text follows style | facts now in [user/styles.md](../user/styles.md) |
| 069 | 2026-11-16 | [Brief 069: one file format](quick-starts/069-one-file-format.md) | v1 JSONL gone | facts now in [reference/file-format.md](../reference/file-format.md) |
| 070 | 2026-11-16 | [070: one way to report errors, one way to read command inputs](quick-starts/070-errors-and-params.md) | error/params for agents | facts now in [developer/commands.md](../developer/commands.md), [agents/mcp.md](../agents/mcp.md) |
| 064 | 2026-11-17 | [064: one update pipeline for derived views (R9)](quick-starts/064-derived-views.md) | effects, update maps | facts now in [design/views.md](../design/views.md) |
| 072 | 2026-11-17 | [072 Typed object model: lane A1 (foundation types)](quick-starts/072-typed-object-model.md) | lanes A2, B (device types, typed ratings per family, user fields, links), D, E, G, rotations | facts now in [user/properties.md](../user/properties.md), [user/styles.md](../user/styles.md) |
| 072c | 2026-11-17 | [072 C: PLC stations, modules and channels](quick-starts/072c-typed-plc.md) | typed PLC records | facts now in [user/plc.md](../user/plc.md) |
| 072d | 2026-11-18 | [072 Typed object model: lane D](quick-starts/072d-typed-wires.md) | routes vs wires, cable cores | facts now in [user/wires.md](../user/wires.md), [user/cables.md](../user/cables.md) |
| 072f | 2026-11-21 | [072 Typed object model: lane F](quick-starts/072f-drawing-aids.md) | continuation arrows as aids, loose parts | facts now in [user/references.md](../user/references.md), [user/locations.md](../user/locations.md), [user/reports.md](../user/reports.md) |
|  | 2026-11-21 | [R11: parallel work and the UI thread](quick-starts/r11-parallel-loading.md) | rayon load, 0.2.3 refactor lane R11 | facts now in [developer/debugger.md](../developer/debugger.md), [developer/architecture.md](../developer/architecture.md) |
| 036 | 2026-11-22 | [Brief 036: multi level terminals](quick-starts/036-multi-level-terminal-blocks.md) | levels, §5-036 | facts now in [user/terminal-strips.md](../user/terminal-strips.md) |
|  | 2026-11-22 | [D2: first builds off the UI thread…](quick-starts/d2-background-views.md) | Checks/Reports built off UI thread (0.2.3 refactor lane D2) | facts now in [developer/architecture.md](../developer/architecture.md), [developer/testing.md](../developer/testing.md) |
|  | 2026-11-24 | [Report pages of several sheets](quick-starts/report-sheets.md) | one sheet at a time on multi-sheet reports | facts now in [user/pages.md](../user/pages.md), [user/reports.md](../user/reports.md) |

## Designs

The design notes as written for each brief, and the pictures they used. Their reasoning that still holds is in
the short topic notes in [design/](../design/).

| No. | Date | Title | What it was | Facts now in |
| --- | --- | --- | --- | --- |
| 008 | 2026-11-01 | [Mouse tools and interaction principles (008-usability-mouse-tools)](designs/008-mouse-tools.md) | §2 principles for every ECAD interactive form (prompt machine, one commit = one JSON transaction, Esc writes nothing, click-first, words, never crash); glyphs; add point, place, wire, connect | facts now in [design/interaction.md](../design/interaction.md), [user/devices.md](../user/devices.md), [user/wires.md](../user/wires.md) |
| 004 | 2026-11-06 | [Cable slash](designs/004-cable-slash.md) | Two-click slash across wires makes one cable; crossing rule, preview, JSON form | facts now in [design/cables.md](../design/cables.md), [user/cables.md](../user/cables.md) |
| 005 | 2026-11-06 | [Traditional and fan cable view (005-cable-views)](designs/005-cable-views.md) | Per-cable view switch, fan geometry v1, views remembered, not recorded | facts now in [design/cables.md](../design/cables.md), [user/cables.md](../user/cables.md) |
| 009 | 2026-11-07 | [Cable look and core labels (009-cable-look-and-cores)](designs/009-cable-look-and-cores.md) | Fan restyle (bends, spine, tails, label box), cablecore, one-sided fan and reference marker, line weights | facts now in [design/cables.md](../design/cables.md), [user/cables.md](../user/cables.md) |
| 010 | 2026-11-07 | [Cable references across pages (010-cable-references)](designs/010-cable-references.md) | Cable reference pairs, continuation stubs, column text, Go to, page rename/move | facts now in [design/references.md](../design/references.md), [user/cables.md](../user/cables.md), [user/references.md](../user/references.md) |
| 016 | 2026-11-08 | [Project panel, one active page, preview (016-project-tab)](designs/016-project-panel.md) | Page list panel, one active page, read-only preview window, Devices tab, Developer section | facts now in [user/pages.md](../user/pages.md), [user/workspace.md](../user/workspace.md) |
| 018 | 2026-11-08 | [Devices, tags, starter symbols (018-components-and-tags)](designs/018-devices-and-tags.md) | Tag rule, one device many representations, starter symbols, search and place, device properties | facts now in [design/object-model.md](../design/object-model.md), [design/decisions.md](../design/decisions.md), [user/devices.md](../user/devices.md) |
| 019 | 2026-11-08 | [Sheets, title block, properties, folders, revisions (019-sheets-and-properties)](designs/019-sheets-and-properties.md) | Sheet frame and grid, title block, folders, page/project properties, revisions, Properties panel | facts now in [design/decisions.md](../design/decisions.md), [user/pages.md](../user/pages.md), [user/references.md](../user/references.md), [user/properties.md](../user/properties.md), [user/releases.md](../user/releases.md) |
| 022 | 2026-11-08 | [Connectivity, nets, wire numbers (022-connectivity-and-wire-numbers)](designs/022-connectivity-and-wire-numbers.md) | Net derivation from attachments, junctions, incremental algorithm, wire numbers, potentials, labels, Checks | facts now in [design/nets.md](../design/nets.md), [design/decisions.md](../design/decisions.md), [user/wires.md](../user/wires.md), [user/checks.md](../user/checks.md) |
| 020 | 2026-11-10 | [History and branches (020-history-and-branches)](designs/020-history-and-branches.md) | History tab graph, versions, branches, merge and update branch, conflict rules, how merge works, scaling | facts now in [design/history.md](../design/history.md), [design/decisions.md](../design/decisions.md), [user/history.md](../user/history.md), [reference/file-format.md](../reference/file-format.md) |
| 024 | 2026-11-11 | [Locations, PDF book, generated pages, reports (024-outputs)](designs/024-locations-book-reports.md) | Location boxes (containment sets location, live), Print Book, generated pages, Reports windows | facts now in [user/locations.md](../user/locations.md), [user/publish.md](../user/publish.md), [user/reports.md](../user/reports.md), [design/devices.md](../design/devices.md) |
| 025 | 2026-11-11 | [Device cross-references, general reference index (025-device-cross-references)](designs/025-device-references.md) | One reference index for all references, contact mirror tables, update map, cost | facts now in [design/references.md](../design/references.md), [user/references.md](../user/references.md), [reference/tokens.md](../reference/tokens.md) |
| 026 | 2026-11-11 | [Numbering (026-numbering)](designs/026-numbering.md) | One renumber mechanism for tags, wires, terminals, pages; fixed, provisional tags, swap, merge | facts now in [design/numbering.md](../design/numbering.md), [design/decisions.md](../design/decisions.md), [user/devices.md](../user/devices.md), [user/wires.md](../user/wires.md) |
| 027 | 2026-11-12 | [Release (027-release)](designs/027-release.md) | Pre-release check, named revision, stamped folder, re-release, existing folder | facts now in [user/releases.md](../user/releases.md), [user/checks.md](../user/checks.md) |
| 028 | 2026-11-12 | [Terminal strips and cable schedule (028-terminal-strips)](designs/028-terminal-strips.md) | Strips/terminals in the model, placing, bridges, terminal plan, cable cores, checks | facts now in [design/terminal-strips.md](../design/terminal-strips.md), [user/terminal-strips.md](../user/terminal-strips.md), [user/reports.md](../user/reports.md) |
| 029 | 2026-11-12 | [Inspector protocol and dev tree (029-inspector)](designs/029-inspector.md) | Versioned JSON protocol, sources/checks, ecadinspect serve over WebSocket, separate dev/ tree | facts now in [design/decisions.md](../design/decisions.md), [developer/debugger.md](../developer/debugger.md), [developer/guards.md](../developer/guards.md) |
| 030 | 2026-11-12 | [Terminal strip accessories (030-terminal-accessories)](designs/030-strip-accessories.md) | Placed accessories, part data, strip-owned jumpers (model D), Jumper tool, plan/BOM | facts now in [design/terminal-strips.md](../design/terminal-strips.md), [user/terminal-strips.md](../user/terminal-strips.md) |
| 031 | 2026-11-12 | [Text styles, show and hide, placement (031-text)](designs/031-text.md) | Fonts, text kinds and heights, defaults registry, show/hide, Move Text | facts now in [user/styles.md](../user/styles.md) |
| 032 | 2026-11-12 | [Wire join, cable across pages, wire labels (032-wires-nets)](designs/032-wires-nets.md) | Wire join, cable through continuations, wire label rules, looks-connected check | facts now in [design/nets.md](../design/nets.md), [user/wires.md](../user/wires.md), [user/checks.md](../user/checks.md) |
| 033 | 2026-11-12 | [Print quality and project fixes (033-print-project)](designs/033-print-and-project.md) | PDF size, missing data, contact tables clear of title block, pending revision, revision columns, tag style, table | facts now in [user/publish.md](../user/publish.md), [user/releases.md](../user/releases.md), [user/devices.md](../user/devices.md) |
| 034 | 2026-11-12 | [Component library build-up (034-component-library)](designs/034-component-library.md) | Common symbol rules, staged starter symbols (contacts, sensors, connectors, PLC cards), BOM, text anchors | facts now in [user/library.md](../user/library.md) |
| 035 | 2026-11-12 | [Live MCP bridge (035-mcp-bridge)](designs/035-mcp-bridge.md) | Agent access preference, port, app-side write gate, UI-thread calls, agent banner, agent undo, ecad-mcp --connect | facts now in [design/agent.md](../design/agent.md), [design/decisions.md](../design/decisions.md), [user/agent-access.md](../user/agent-access.md), [agents/mcp.md](../agents/mcp.md), [agents/control-protocol.md](../agents/control-protocol.md) |
| 036 | 2026-11-12 | [Multi-level terminal blocks (036-multi-level-terminals)](designs/036-multi-level-terminal-blocks.md) | Positions, levels, types; drawing levels; labels; jumpers per level; internal bridges; row model | facts now in [design/terminal-strips.md](../design/terminal-strips.md), [user/terminal-strips.md](../user/terminal-strips.md) |
| 040 | 2026-11-13 | [canvas](designs/040-canvas.md) | Choices made building the canvas (wire codes, markers, leader line) | facts now in [design/interaction.md](../design/interaction.md), [user/drawing.md](../user/drawing.md), [user/publish.md](../user/publish.md), [user/checks.md](../user/checks.md), [reference/tokens.md](../reference/tokens.md) |
| 043 | 2026-11-13 | [Agent banner (picture)](designs/043-banner.png) | Design picture for the agent banner | picture |
| 043 | 2026-11-13 | [Agent session finished (picture)](designs/043-finished.png) | Design picture for the agent banner | picture |
| 043 | 2026-11-13 | [Agent authors in History (picture)](designs/043-history.png) | Design picture for the agent banner | picture |
| 043 | 2026-11-13 | [Agent session paused (picture)](designs/043-paused.png) | Design picture for the agent banner | picture |
| 044 | 2026-11-13 | [connectors](designs/044-connectors.md) | One device, two mating halves; pin starters; connector, pinlabel; half tag and pin names; mating checks | facts now in [design/devices.md](../design/devices.md), [design/decisions.md](../design/decisions.md), [user/connectors.md](../user/connectors.md) |
| 045 | 2026-11-13 | [plc](designs/045-plc-racks-cards.md) | Racks, slots, cards, card-type catalogue, channel cells, addressing formats, I/O list | facts now in [user/plc.md](../user/plc.md) |
| 046 | 2026-11-13 | [terminal-strips](designs/046-terminal-strip-pages.md) | Graphical terminal-strip page: columns, cable bars and boxes, jumper bars, levels, paging, texts | facts now in [design/terminal-strips.md](../design/terminal-strips.md), [user/terminal-strips.md](../user/terminal-strips.md) |
| 047 | 2026-11-13 | [reports](designs/047-bom-completeness.md) | Part lines, part sets, for lines, kits, BOM items, article numbers, BOM rows and CSV | facts now in [user/reports.md](../user/reports.md) |
| 048 | 2026-11-13 | [symbols / library](designs/048-symbol-builder.md) | Builder in Library, lineage and versions, text slots, Make symbol, update of placed copies, review | facts now in [user/library.md](../user/library.md) |
| 049 | 2026-11-13 | [continuations](designs/049-continuation-references.md) | What an arrow shows, captions (contcaption), go-to, reference index rows | facts now in [design/references.md](../design/references.md), [user/references.md](../user/references.md) |
| 050 | 2026-11-13 | [styles](designs/050-style-model.md) | Five-level style resolution, style IDs, standards, text/location/frame catalogues, wire categories | facts now in [design/styles.md](../design/styles.md), [design/decisions.md](../design/decisions.md), [user/styles.md](../user/styles.md) |
| 051 | 2026-11-13 | [plc](designs/051-plc-visuals.md) | Card drawn whole on the page (cells, bands, frame, rotation); Lists ▸ PLC rack overview; plcpage | facts now in [user/plc.md](../user/plc.md) |
|  | 2026-11-13 | [UI architecture (U0; briefs 037–039)](designs/ui-architecture-u0.md) | Ownership and upstream seams (US1 embedded view, US2 font builder), crates, tokens, fonts, shell slots, table, action table, live updates, tests, perf | facts now in [design/ui.md](../design/ui.md), [design/decisions.md](../design/decisions.md), [developer/ui-components.md](../developer/ui-components.md), [developer/testing.md](../developer/testing.md) |
| 052 | 2026-11-14 | [devices / locations](designs/052-device-first.md) | Representation types from the definition, the head, stored location and function with proposals and conflicts, designations | facts now in [design/devices.md](../design/devices.md), [design/decisions.md](../design/decisions.md), [user/devices.md](../user/devices.md), [user/locations.md](../user/locations.md), [user/checks.md](../user/checks.md), [reference/tokens.md](../reference/tokens.md) |
| 054 | 2026-11-14 | [pages](designs/054-page-types-cover-pages.md) | Page-type catalogue, + Page picker, folder start numbers, Settings ▸ Pages and sheets, generated cover pages | facts now in [user/pages.md](../user/pages.md), [reference/tokens.md](../reference/tokens.md) |
| 055 | 2026-11-14 | [title-blocks](designs/055-title-block-editor-v1.md) | First title-block and frame editor (line-and-field canvas, 055a/b), frame fields, logos, bindings | facts now in [user/pages.md](../user/pages.md) |
| 055c | 2026-11-14 | [title-blocks](designs/055c-title-block-elements.md) | Title block built from elements on a grid; frame separate; two libraries; built-in house block | facts now in [user/pages.md](../user/pages.md) |
| 056 | 2026-11-14 | [plc](designs/056-plc-signal-list.md) | PLC signal list screen, channel table, Tag and Safe state, tag export/import, address matching | facts now in [user/plc.md](../user/plc.md) |
| 057 | 2026-11-14 | [circuits / checks](designs/057-rating-checks.md) | Wire and cable rating checks: datasheet/table/manual basis, overload, volt drop, protective device trace, LoadSource, rating-table CSV | facts now in [design/styles.md](../design/styles.md), [design/decisions.md](../design/decisions.md), [user/circuits.md](../user/circuits.md) |
| 059 | 2026-11-15 | [cables](designs/059-cable-manager.md) | Cable properties and catalogue fields, cablenew/cableedit/cableremove/cableslash/cableadd, Cables list UI | facts now in [design/cables.md](../design/cables.md), [user/cables.md](../user/cables.md) |
| 064 | 2026-11-15 | [views](designs/064-derived-views.md) | One update pipeline: Effect classifier, DerivedView trait, Views, one way in, guards | facts now in [design/views.md](../design/views.md), [design/decisions.md](../design/decisions.md), [developer/architecture.md](../developer/architecture.md) |
| 065 | 2026-11-15 | [developer: layers ("Design 065: the crate map")](designs/065-crate-map.md) | Layer map, module-by-module moves, cuts, OS rule, xtask layers rules, split-lane steps, README template, owner calls | facts now in [developer/architecture.md](../developer/architecture.md), [design/decisions.md](../design/decisions.md), [developer/guards.md](../developer/guards.md) |
| 066 | 2026-11-15 | [defaults](designs/066-defaults-data.md) | Built-in defaults as TOML data, one built-in set, what a saved project means, validation | facts now in [design/defaults.md](../design/defaults.md), [design/decisions.md](../design/decisions.md), [user/styles.md](../user/styles.md), [roadmap.md](../roadmap.md) |
|  | 2026-11-15 | [developer: UI components (UI rev 4)](designs/ui-components-2026-11.md) | Style guide: one component per kind of value; editable vs read; gallery; review checklist | facts now in [developer/ui-components.md](../developer/ui-components.md) |
| 072 | 2026-11-16 | [object model](designs/072-typed-object-model.md) | Typed object model: principles, Device and DeviceType, PLC, wires/routes/cables, pages, format changes F1–F42 | facts now in [design/object-model.md](../design/object-model.md), [design/decisions.md](../design/decisions.md), [design/nets.md](../design/nets.md) |
|  | 2026-11-21 | [commands (owner decision 5)](designs/commands-migration.md) | §1 classification of engine src/cmd/* files G/S; §2 the generic runner (Generic, Cx, Outcome, I/O, gate) | facts now in [developer/commands.md](../developer/commands.md) |
|  | 2026-11-22 | [Source layout: walkthroughs and order of the moves](designs/layout-history.md) | The parts of the layout plan that described the 0.2.3 moves | facts now in [design/layout.md](../design/layout.md) |

## Discussions

Owner discussions and the UI specs.

| No. | Date | Title | What it was | Facts now in |
| --- | --- | --- | --- | --- |
|  | 2026-11-07 | [How the .ecad file grows](discussions/file-container.md) | A 'zip of parts' idea against a log plus snapshots | history only |
|  | 2026-11-08 | [ECAD projects in git](discussions/git.md) | Why a raw .ecad fights git; diff and merge drivers; folder projects | facts now in [design/decisions.md](../design/decisions.md), [design/history.md](../design/history.md) |
|  | 2026-11-08 | [History, versions and branches](discussions/history-and-branches.md) | The agreed history workflow: versions, branches, three-way merge, the words | facts now in [design/history.md](../design/history.md), [reference/glossary.md](../reference/glossary.md), [user/history.md](../user/history.md) |
|  | 2026-11-08 | [Revisions, snapshots and compaction](discussions/revisions.md) | Issued revisions against the log, snapshots, compaction | facts now in [design/history.md](../design/history.md) |
|  | 2026-11-08 | [Storage engine for .ecad v2](discussions/storage-engine-decision.md) | How redb was chosen over a container of our own | facts now in [design/decisions.md](../design/decisions.md), [design/history.md](../design/history.md), [reference/file-format.md](../reference/file-format.md) |
|  | 2026-11-09 | [Upstream CADCraft review](discussions/upstream-review-2026-11.md) | Review of CADCraft 0.15.1 to 0.19.0: changes, seams, patch list, risks | history only |
|  | 2026-11-11 | [QA pass on dev](discussions/qa-dev-2026-11-11.md) | A QA gate run: suite, bench, trial | history only |
|  | 2026-11-12 | [Roadmap review](discussions/roadmap-review-2026-11.md) | Gap analysis against other ECAD tools, milestones to v1, one way to do it per area | facts now in [design/ui.md](../design/ui.md), [design/decisions.md](../design/decisions.md), [developer/testing.md](../developer/testing.md), [roadmap.md](../roadmap.md) |
|  | 2026-11-13 | [ECADCraft UI design spec](discussions/ui-design-spec.md) | Round 2 UI design: four workspaces, seven tool groups, the wheel, one table | facts now in [design/ui.md](../design/ui.md), [design/decisions.md](../design/decisions.md), [user/drawing.md](../user/drawing.md), [user/library.md](../user/library.md), [user/reports.md](../user/reports.md) |
|  | 2026-11-13 | [UI spec: owner answers](discussions/ui-owner-answers.md) | The answers to the UI spec's questions | facts now in [design/ui.md](../design/ui.md), [design/decisions.md](../design/decisions.md), [user/publish.md](../user/publish.md), [user/connectors.md](../user/connectors.md), [user/library.md](../user/library.md), [user/reports.md](../user/reports.md) |
|  | 2026-11-14 | [ECADCraft UI rev 3: build spec](discussions/ui-rev3-build-spec.md) | Transcription of the rev 3 UI handoff | facts now in [design/ui.md](../design/ui.md), [user/drawing.md](../user/drawing.md) |
|  | 2026-11-14 | [ECADCraft UI rev 4: build spec](discussions/ui-rev4-build-spec.md) | Transcription of the rev 4 UI handoff (boards 1.01 to 6.01) | facts now in [design/ui.md](../design/ui.md), [user/workspace.md](../user/workspace.md), [user/drawing.md](../user/drawing.md), [user/pages.md](../user/pages.md), [user/library.md](../user/library.md), [user/circuits.md](../user/circuits.md) |

## Reviews

Reviews, assessments and reports.

| No. | Date | Title | What it was | Facts now in |
| --- | --- | --- | --- | --- |
|  | 2026-10-12 | [CADCraft assessment (phase 0)](reviews/cadcraft-assessment.md) | How ECAD would attach to CADCraft 0.6.2 | history only |
|  | 2026-10-12 | [Environments: where ECAD builds and runs](reviews/environments.md) | Build checks and requirements for Linux and WSL, and where ECAD had run | facts now in [developer/build.md](../developer/build.md) |
|  | 2026-10-12 | [Bounded integration experiment: report](reviews/experiment-report.md) | The October adapter experiment, WSL trial, census, patch footprint | history only |
|  | 2026-10-31 | [Baseline review](reviews/baseline-review-2026-10-31.md) | Windows build check, code review, test audit and doc reconciliation | history only |
| 061 | 2026-11-15 | [Crate choices](reviews/061-crate-choices.md) | Which crates.io crates to adopt and where each may appear | facts now in [design/decisions.md](../design/decisions.md), [developer/architecture.md](../developer/architecture.md) |

## Plans

Proposals, roadmaps, the decision log by date and the format specifications as written. Proposals still open (the upstream sync policy, the binary encoding) are draft briefs in
[docs/briefs/](../briefs/README.md).

| No. | Date | Title | What it was | Facts now in |
| --- | --- | --- | --- | --- |
|  | 2026-10-12 | [Decision log (by date)](plans/decision-log-2026.md) | One row per ruling since 2026-10-12; the rows still in force are in the topic log | facts now in [design/decisions.md](../design/decisions.md), [design/history.md](../design/history.md) |
|  | 2026-10-12 | [ECAD document architecture and integration proposal](plans/document-architecture-proposal.md) | Rev 4.1: principles, document model, file and library contracts, transactions, integration, persistence | facts now in [developer/architecture.md](../developer/architecture.md), [reference/file-format.md](../reference/file-format.md) |
|  | 2026-10-15 | [Deferred work](plans/deferred-work.md) | The deferred list as it stood before the 0.2.3 docs restructure | facts now in [roadmap.md](../roadmap.md) and [briefs/](../briefs/README.md) |
|  | 2026-11-08 | [EEP-0004: container evolution](plans/eep-0004-container-evolution.md) | Index, snapshot chunks and checkpoints on format v1; superseded by EEP-0005 | facts now in [reference/file-format.md](../reference/file-format.md) |
|  | 2026-11-08 | [EEP-0005: .ecad container v2](plans/eep-0005-container-v2.md) | The redb container: tables, branches, versions, chunks, open and save, integrity | facts now in [reference/file-format.md](../reference/file-format.md) |
|  | 2026-11-08 | [Specifications (index)](plans/eep-index.md) | Index of EEP-0002 to EEP-0005 | facts now in [reference/file-format.md](../reference/file-format.md) |
|  | 2026-11-08 | [ECAD features](plans/features.md) | Feature branches and their briefs, up to brief 034 | history only |
|  | 2026-11-08 | [Road to ECAD v0.1](plans/roadmap-v0.1.md) | v0.1 scope, the target panel, lanes, what was left out | facts now in [roadmap.md](../roadmap.md) |
|  | 2026-11-09 | [EEP-0002: .ecad format v1](plans/eep-0002-format-v1.md) | The JSON Lines v1 format: record schema, canonical form, hash chain | facts now in [reference/file-format.md](../reference/file-format.md) |
