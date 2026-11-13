# ECAD quick start: run and try the current app

This is the **one maintained guide** for building, running and manually testing the ECAD trial
app. Other READMEs link here instead of repeating it. It is updated in the same pull request as
any change that affects these instructions (see `ecad/CLAUDE.md`).

> **What you are running.** `ecad-trial` is an experiment, not an ECAD application. It is
> CADCraft's drawing window with the ECAD commands and one ECAD project open at a time. Projects
> are saved as `.ecad` files (persistence stage 2a). The **PROJECT** panel on the far left lists
> the project's pages, shows one of them at a time and holds New, Open, Save, Save As and Close
> (brief 016; there is no top banner any more). An **ECAD** tab in Tool Sets has buttons for the
> ECAD commands; most work by clicking, with prompts at CADCraft's command line. A **Developer** section at the bottom of the
> panel (folded) holds the ECAD command field for JSON forms, the read-only debugger (stage 2b)
> drawn inline, Inspect Project's summary and snapshot export; none of them is shown unless you
> unfold it.

**Words:** a device's landing point for a wire is a **connection point** (`-K01:A1`); a
**terminal** is only the DIN-rail terminal device (`-X01:1`). See `ecad/docs/glossary.md`.

## Which revision this describes, and what has been tried

**Brief 035 (design `ecad/docs/design/035.md`), on `feature/mcp-bridge`, not yet in `dev`:**
an agent drives the running trial over MCP while you watch (section 7D). Agent access is off by
default and set in the Developer section (or `--control PORT` for one run); `ecad-mcp --connect
47823` is headless until the window answers and live once it does; each agent call is one
transaction recorded as the agent's, and you are the author again after it. A plain strip over
the drawing names the agent, its last change and its steps, with Undo session, Pause and
Disconnect; its look comes with the new UI shell.
- *Automated checks:* see section 7D.
- *Agent-observed:* none yet; no agent has watched the live window with Claude Code.

**Brief 033 (design `ecad/docs/design/033.md`), on `feature/print-project`, not yet in `dev`:**
Print Book's PDF holds on each page only what lies on that page (an approved upstream patch to
CADCraft's PDF writer), so a book grows in step with its sheets: the 25-page bench book went from
7,897 KB to 335 KB. A missing value in a generated table is `?` in a thin box, yellow on screen and
black when printed in either colour mode. An automatic contact table that would run into the title
block goes right of its coil, else is lifted above the title block; a table placed by hand stays
and Checks lists `Contact table over the title block` with **Move table…**. `table` on an ECAD
page is refused, nothing written: `Tables can't be placed on ECAD pages yet. Draw it with lines
and text, or add a generated page for lists (New Page ▸ Generated).` Generated pages number their
continuation sheets in sequence (`400, 401`). Project settings gain **Tags** (leading `-`, case
as typed; changing the dash rewrites every tag in one undoable step) and **Revisions** (pending
changes marker `D*` / `E (draft)` / off, and the revision table's columns with custom fields such
as ECR). The Rev box shows `D*` when the drawing changed after the newest issue. Section 5-033
describes it.
- *Automated checks:* `crates/io/src/tests.rs` (upstream: a page holds only its window's paths,
  20 pages ≤ 2.2 × 10, a culled page equals its sheet plotted alone),
  `ecad/crates/ecad-cadcraft/tests/all/print_project_contracts.rs` (the boxed `?`, its print
  colour), `ecad/apps/ecad-mcp/tests/all/print_project_contracts.rs` (the `contacts` reply's
  `table`, the Checks row, `table` refused, writes off), `print_project_q7_contracts.rs` (pending
  marker with undo and save/reopen, revision columns and custom fields, tag style and its
  rewrite, renumber under `typed`, writes off), unit tests in `revisions.rs` (column grammar,
  hostile settings) and `ecad-model/src/tag.rs` (the tag style table).
- *Agent-observed:* the worker ran the 25-page print bench before and after (release build).
- *Owner-tried:* not yet.

**Brief 034 (design `ecad/docs/design/034.md`), on `feature/component-library`, not yet in
`dev`:** the built-in symbols grow from 21 to 81 so a control-panel pack needs no hand-drawn boxes:
change-over contacts (`co`, `swco`: `11`/`12`/`14`, then `21`/`22`/`24`… when joined), 4-pole power
contacts, RCBO and RCDs, SSR input and output, a fuse terminal and fuse banks of 4 to 12 ways;
proximity, float, roller and interlock switches, single and dual solenoid valves, a thermocouple
and a heater; plugs, sockets, D-sub and M12 halves, RJ45 and USB bulkheads; PLC card bodies
(CPU, PS, DI, DQ, AI, AO, RTD, IO-Link). Find Symbol also finds a family word (`protection`,
`sensors`, `connectors`, `plc`…). Any device takes a `rating`; a fuse bank takes `rating.<n>` per
fuse. Spare connector pins, a fuse bank's `M` and outputs and an I/O card's supply points are not
flagged as unconnected. Ratings are stored and listed but not drawn yet (lane T draws text);
contact tables do not count change-over contacts yet (stage 4, after brief 033). Section 5-034.
- *Automated checks:* `ecad/crates/ecad-cadcraft/src/symbols.rs` (every symbol on the grid, its
  hash, box and labels, the change-over rule, quiet points, anchors, search),
  `ecad/crates/ecad-cadcraft/tests/all/library_contracts.rs` (placing, joining, ratings, nets,
  quiet points, BOM rows) and `ecad/apps/ecad-mcp/tests/all/library_contracts.rs` (every new symbol,
  a change-over join and ratings through `execute` as the agent; writes off; hostile input).
- *Agent-observed:* the worker looked at a drawing of every new definition; no agent has clicked
  through the live window.

**Brief 031 (design `ecad/docs/design/031.md`), on `feature/text-styles`, not yet in `dev`,
steps Q1 (text styles), Q3 (show and hide) and Q2 (placement):** every derived text takes its size and weight from one style table per
project: tags 2.5 mm bold, connection point labels 1.5 mm, location labels 1.6 mm, continuation
names and references 2.0 mm, wire labels 1.6 mm (was 1.8). One project font (`BIZ UDPGothic` by
default) draws them when the build embeds craft-fonts or the machine has it; otherwise the stroke
font draws, and bold is a heavier line. **Project settings ▸ Text** sets the font and each kind's
height, bold and shown state in one transaction; `textstyle` does the same typed, and the
read-only `defaults` lists every registered default and where its value comes from. Section 5-031.
Tags, connection point labels, descriptions and part numbers (new, hidden by default) can be
hidden or shown per symbol, device, page or project (**Show/Hide Text**, Properties ▸ Texts,
`showtext`); a strip's tag prints once per run of adjacent terminals and a PLC device's once per
run of its channels; continuation names are hidden by default. **Show Hidden Text** draws hidden
texts dimmed, never printed. Tags (with any shown description and part number under them) are
placed beside their symbol where they cross no wire, symbol or other text; **Move Text** and
`movetext` store a place that follows the symbol; `text` with `justify` is recorded. The cable look's texts (core labels, label box) are drawn at the cable text height (1.6 mm) and
reference ends at the reference height (2.0 mm); a copied symbol keeps its own text settings.
- *Automated checks:* `ecad/crates/ecad-cadcraft/src/defaults/mod.rs` and `text_style.rs` (the
  registry, hostile values, levels), `ecad/crates/ecad-cadcraft/tests/all/text_style_contracts.rs`
  (drawn sizes and styles, redraw, a font change then an edit),
  `ecad/apps/ecad-mcp/tests/all/text_contracts.rs` (`textstyle`, `defaults`, the `text.` key
  refusals, writes off) and `ecad/apps/ecad-trial/tests/all/text_ui.rs` (the Text section).
- *Agent-observed:* the worker looked at the headless screenshot `031-text-section.png`; no agent
  has clicked through the live window.
- *Owner-tried:* not yet.

**Brief 026 (design `ecad/docs/design/026.md`), on `feature/numbering`, not yet in `dev`:** one
numbering mechanism for devices, cables and wires. **Renumber…** (ECAD tab, new **Numbering**
group; the Devices tab's header; a folder's right-click **Renumber in this folder…**; **Renumber
Wires** opens it with Wires chosen) previews old → new in **drawing order** (page, then column,
then row of the sheet grid) and applies it as one transaction (one `u`). Typing a tag fixes it
(**Fixed** beside Tag releases it); a taken tag offers **Swap** first, then **Make it the same
device**, then **Use -K08**. **Project settings → Numbering → Provisional tags until Finalise**
gives new devices `-K?1`, cables `-W?1` and new nets `24V-?` (orange) until **Finalise
numbering**; Checks lists them and Issue Revision asks **Finalise numbering now** or **Issue
anyway**. New cables are tagged `-W01`… automatically; devices and cables share one tag namespace.
A merge's tag conflict offers **Make them one device**. Section 5M describes it.
- *Automated checks:* `ecad/crates/ecad-model/src/numbering.rs` (drawing order, scopes, fixed,
  provisional, hostile requests), `tag.rs` (provisional form, the high-water reset), `merge.rs`
  (Make them one, provisional duplicates), `ecad/apps/ecad-mcp/tests/all/numbering_contracts.rs`
  (every new or changed write through `execute`, its transaction and author, writes off), and
  `ecad/crates/ecad-cadcraft/tests/all/numbering_contracts.rs` (labels, Checks, save and reopen).
- *Agent-observed:* the worker looked at the headless screenshots `026-*.png`; no agent has
  clicked through the live window.
- *Owner-tried:* not yet.

**Brief 028 (design `ecad/docs/design/028.md`), on `feature/terminal-strips`, not yet in `dev`:**
a terminal clicked on a wire splits it and both halves land on it; **Place Terminals** puts one
terminal on every wire a line crosses; **Draw Bridge** links two terminals of a strip. A strip's
**Terminals offered** (`12+2PE`) gives spare positions; a cable's **Cores** (or its type, `7G1.5`)
gives used, spare and over cores, and a new member takes the next free core. The terminal plan
and the cable schedule are in Reports and on generated pages; the BOM counts a strip per terminal
type; five Checks rows have fixes. Section 5O describes it.
- *Automated checks:* `ecad/crates/ecad-cadcraft/tests/all/strips_contracts.rs` (design 028 §12
  A–H, L, the update map and the FP5 finding; the kept strips equal a rebuild after the edits,
  and in verify mode after every commit), `ecad/apps/ecad-mcp/tests/all/strips_contracts.rs` (the
  brief 011 rule), unit tests of the offer and core grammars in
  `ecad/crates/ecad-cadcraft/src/strips.rs`.
- *Agent-observed:* none yet; no agent has clicked through the live window.
- *Owner-tried:* not yet.

**Brief 024 (design `ecad/docs/design/024.md`), on `feature/outputs`, not yet in `dev`:** a
device's **location** (`+CP1`) is its own value, else the **location box** its main symbol sits
in, else its page's, else the project's; Properties says where it comes from (`+JB1 (from box on
3)`). Location boxes are dashed rectangles or closed polylines labelled `+…`, drawn with
**Location Box** (ECAD tab) or made from an existing closed polyline; what they hold updates live,
and nothing is written on the devices. A symbol drawn away from its device's location shows the
prefix (`+CP1-K01`). **Reports** (BOM, wires, cables, each exported as Excel-safe CSV) and
**Print Book** (one PDF of every page, a folder or a page list) are buttons in the PROJECT panel's
header; its **v** beside **+** adds generated pages: Cover, Contents, Revision list, BOM by
location. Section 5K describes it.
- *Automated checks:* `ecad/crates/ecad-cadcraft/tests/all/outputs_contracts.rs` (precedence, live
  boxes, innermost wins, own value kept and flagged with its fix, the prefix, reports and CSV,
  generated pages and overflow, Print Book; every edit's kept models equal a fresh build),
  `ecad/apps/ecad-mcp/tests/all/outputs_contracts.rs` (MCP writes as the agent, refused with writes
  off; reports and printing read), `ecad/apps/ecad-mcp/tests/all/demo.rs`
  (`the_demo_locations_reports_and_generated_pages`), and `ecad/apps/ecad-trial/tests/all/outputs_ui.rs`
  (the windows, headless, with screenshots).
- *Agent-observed:* the worker looked at the headless screenshots `024-*.png`; no agent has
  clicked through the live window.
**Brief 025 (design `ecad/docs/design/025.md`), on `feature/cross-references`, not yet in `dev`:**
a contact shows where its coil is (`100-B-3`) and a coil shows a **contact table** under it: one
row per pole, power then auxiliary, with each pole's place, the poles the part offers that nobody
drew (grey, `—`) and those drawn beyond the offer (red, and a Checks warning). A device's
**Contacts offered** (`3P+1NO+1NC`) and **Contact table** placement are set in Properties; Move
Mirror Table moves the table; Go to Reference follows contacts and coils both ways. Cable
references now run on the same reference index with no visible change. Section 5L describes it.
- *Automated checks:* `ecad/crates/ecad-cadcraft/tests/all/xref_contracts.rs` (design 025 §10
  checks A–G, J, K, and the index against a rebuild after every commit in verify mode),
  `ecad/apps/ecad-mcp/tests/all/xref_contracts.rs` (the brief 011 rule), unit tests of the
  `contacts` grammar and `mirror` values in `ecad/crates/ecad-cadcraft/src/xref.rs`.
- *Agent-observed:* the worker looked at the headless screenshots `025-coil-table.png`,
  `025-contacts.png`, `025-props.png` and `025-checks.png`; no agent has clicked through the live
  window.
- *Owner-tried:* not yet.

**Brief 020b (design `ecad/docs/design/020.md` §4–§6), on `feature/history-merge`, not yet in
`dev`:** a branch is **merged into main** (or into the branch it came from), and a branch is
**updated from main**, through a dialog that previews the changes and lists each conflict with an
explicit choice (nothing preselected; **Keep main for all** / **Keep branch for all**; **Edit**
for text). The merge is one step on main, drawn as a join in the History graph; a plain `u` of a
merge asks first (**Undo merge**), and undoing it reopens the branch. The branch bar's **Merge
into main…** and **Update from main…** buttons work now (they were disabled in 020a); so do
Revisions' **Merge into main…**, the History tab's **⋯** entries and a Now row's right-click.
A branch's first save writes its first snapshot, so switching to it costs about an open. The
History graph reads 200 rows and the next 200 when scrolled to the end; a version's details name
the devices it changed (`-K05 added`); **F2** renames the branch shown. Section 5J steps 3, 5 and
9–11 describe it.
- *Automated checks:* `ecad/crates/ecad-file/tests/all/merge_store_contracts.rs` (the merge record,
  the merge index and the merged status through merge, undo and redo across saves, the first-save
  snapshot), `ecad/apps/ecad-mcp/tests/all/merge_contracts.rs` (merge, update, a conflict refused
  without a choice and merged with one, undo merge, writes off), the merge engine's unit tests in
  `ecad/crates/ecad-model/src/merge.rs`, and `ecad/apps/ecad-trial/tests/all/merge_ui.rs` (the
  branch bar's Merge into main…, the dialog with one conflict, Merge, the join, Undo merge asked).
- *Agent-observed:* the worker looked at the headless screenshots `020-merge.png` and
  `020-undomerge.png`; no agent has clicked through the live window.
- *Owner-tried:* not yet.

**Brief 020a (design `ecad/docs/design/020.md`), merged into `dev` (PR #48):** the
project's history is a **History** tab in the PROJECT panel: a graph of the main line with its
versions and issued revisions, and branches drawn from where they start. Select a row to see it,
**Show** it read-only, **Restore** it (one new step, undoable), make a **New version**, and work
on **branches** (create, rename, switch with **Save and switch**, archive, reopen). A project
reopens on the branch you last used; the window title and a branch bar say which branch is shown
once there is more than one. Section 5J describes it.
- *Automated checks:* `ecad/crates/ecad-file/tests/all/branches_contracts.rs` (branch rows,
  switching, reopening on the last branch, snapshots on a branch, deep branches, Verify, readers
  ignoring unknown optional fields), `ecad/apps/ecad-mcp/tests/all/history_contracts.rs` (every
  history and branch command through MCP, refused with writes off), and the unit tests of the
  graph's order and lanes in `ecad/crates/ecad-cadcraft/src/history.rs`.
- *Agent-observed:* not yet clicked through by an agent in the live window.
- *Owner-tried:* not yet.

**Brief 021 (container v2 on redb, EEP-0005), on `feature/container-v2` (built on
`dev`, brief 022 included), not yet in `dev`:** a `.ecad` file is now one **redb** database. Saving writes
one transaction in a few milliseconds and opening reads the latest snapshot and the changes after
it, so both stay fast as the history grows. A version 1 file is **offered** for conversion when
you open it (a new `<name>.v2.ecad`, the original untouched), or converted with `ecad upgrade`.
`ecadinspect` shows a version 2 file's tables, branches, log, versions and chunks, and exports the
history as version 1 JSON Lines. The debugger gains **Storage** and **Branches** views. Sections
7, 7B, 7C, 8 and "Measuring load time" describe it.
- *Automated checks:* `ecad/crates/ecad-file/tests/all/container_v2.rs` (reopen, saves and
  snapshots, versions and revisions, the append-only API, damage),
  `ecad/crates/ecad-file/tests/all/container_crash.rs` (power loss, torn writes and process kills,
  in the slow set), `ecad/crates/ecad-model/tests/all/ledger_base.rs` (a project opened from a
  snapshot undoes and rebuilds exactly as a replayed one), `ecad/apps/ecad-cli/tests/all/cli_contracts.rs`
  (`ecad upgrade`), `ecad/dev/ecad-inspector/tests/all/cli_contracts.rs` (every `ecadinspect`
  view, `export-jsonl` read back by version 1; moved there by brief 029), and the
  debugger checks in `ecad/crates/ecad-cadcraft/tests/all/debugger.rs` and
  `ecad/apps/ecad-trial/tests/all/debugger.rs`.
- *Agent-observed:* the worker ran the crash suite and the benchmark; no agent has clicked through
  the live window.
- *Owner-tried:* not yet.

**Brief 022 (design `ecad/docs/design/022.md`), on `feature/connectivity`, not yet in `dev`:**
ECAD knows what is connected. Nets come from the attachments (a T joins only at a junction dot,
crossings never join, continuations with one name join across pages); every wire shows its net's
wire number with its cross-section and colour (`24V-04 0.5 BU`); adding a wire never renumbers
another net; the Project panel has a **Checks** tab. Section 5I describes it, on the demo project.
- *Automated checks:* `ecad/crates/ecad-model/tests/all/connectivity_contracts.rs` (net rules,
  numbering rules, a random-op test against a fresh build after every step),
  `ecad/crates/ecad-cadcraft/tests/all/wiring_contracts.rs` (T, crossing, labels, stable numbers,
  one row per seeded fault, the kept Checks list against a fresh build),
  `ecad/apps/ecad-mcp/tests/all/demo.rs` (the demo's numbers) and `mcp_contracts.rs` (every new
  command through MCP), `ecad/apps/ecad-trial/tests/all/checks_ui.rs` (the Checks tab by label,
  go-to, Add junction, and the `022-*.png` screenshots).
- *Agent-observed:* the worker rendered the `022-*.png` screenshots headless (wgpu) and looked at
  them; no agent has clicked through the live window.
- *Owner-tried:* not yet.

**Brief 019 (design `ecad/docs/design/019.md`), on `feature/sheets` (built on
`feature/project-manager`), not yet in `dev`:** every page is an A3 sheet with a lettered and
numbered grid and a title block; references read `page-column-row` (`105-C-4`); pages have their
own numbers and sit in folders; project and page properties are edited in CADCraft's
**Properties** palette and a **Project Properties** dialog; revisions are issued and opened
read-only. Section 5H describes it; 5D, 5E and 5G changed with it (texts such as `/2.1` now read
`2-A-6`, and moving a page no longer renumbers it).
- *Automated checks:* `ecad/crates/ecad-cadcraft/tests/all/sheets_contracts.rs` (design 019 checks
  A–G and J), `ecad/crates/ecad-model/tests/all/sheet_contracts.rs` (folders and their
  invariants), the round trip in `ecad/crates/ecad-file/tests/all/records_contracts.rs`, check I in
  `ecad/apps/ecad-mcp/tests/all/mcp_contracts.rs`, and
  `ecad/apps/ecad-trial/tests/all/sheets_ui.rs` (the tree, the Properties section, both dialogs,
  Rev A and the undo question, by the UI in a headless 1280×800 window, with the eight `019-*.png`
  screenshots).
- *Agent-observed:* the worker rendered the `019-*.png` screenshots headless (wgpu) and looked at
  them; no agent has clicked through the live window.
- *Owner-tried:* not yet.

**Brief 016 (design `ecad/docs/design/016.md`), on `feature/project-manager`, not yet in `dev`:**
the Project panel, one active page at a time, undo and redo bringing their page to the front, a
read-only preview of another page, and the top banner replaced by the panel's header and an
folded **Developer** section at the bottom of the panel (design 016, "Top bar removed"). Sections 4, 5E, 6, 7 and 8 describe it.
- *Automated checks:* `ecad/apps/ecad-trial/tests/all/project_panel.rs` (design 016 checks A to J by
  label in a 1280×800 headless window, and the six `016-*.png` screenshots), the one-window and
  undo checks in `ecad/crates/ecad-cadcraft/tests/all/project_pages.rs`, check K in
  `ecad/apps/ecad-mcp/tests/all/mcp_contracts.rs`, and `the_quick_start_trial_runs_headless`
  (sections 5 to 7 below).
- *Agent-observed:* the worker rendered the `016-*.png` reference screenshots headless (wgpu) and
  looked at them; no agent has clicked through the live window.
- *Owner-tried:* not yet.

This guide describes `dev` with stage 2b (the debugger panel, PR #11) and the ECAD tab in Tool
Sets (PR #12) merged on 2026-10-15, on top of stage 2a, now built on **CADCraft 0.19.0**
(upstream tag `v0.19.0`, commit `227e3c38`, brief 023, 2026-11-09; egui 0.37, Rust 1.95 or later;
before that CADCraft 0.15.1, brief 007). The steps below did not change with either sync; the
Block Editor, REFEDIT and LISP are refused on ECAD pages, and BREAK, TRIM and OOPS are refused
where they would split or bring back an object with an electrical meaning (section 9). Your trials
recorded below were on the earlier CADCraft 0.6.2 base, except your click-only trial of brief 008
on native Windows, which was on 0.15.1.

**Brief 008 (design `ecad/docs/design/008.md`)**, merged into `dev` (PR #16), changed how
sections 4 to 7 are done: the ECAD tools work by clicking, connection points and wire ends are marked on
the drawing, and hovering an ECAD object names it. It adds one approved CADCraft change (overlay
marks drawn by a document's owner, listed in `ecad/UPSTREAM-PATCHES.md`). For it there are
automated checks and your first click-only trial on native Windows (below); no agent has run it on
a display.

**Branch `ecad/windows-trial` (brief 003)**, cut from `dev` at `0afaf1d`, makes the trial behave
natively on Windows: `~` in a typed path is your home folder (on Windows your user profile, such
as `C:\Users\<you>`, whether or not `HOME` is set) and may be written `~/` or `~\`; the Save As
dialog suggests `C:\dev\ecad-projects` on Windows (`~/ecad-projects` on Linux) and creates that
folder the first time you save into it; a reserved device name such as `CON.ecad` is refused;
typing the project's own file in another letter case saves it without a "replace?" question;
release builds on Windows open no console window. Until it is merged, `dev` behaves as the
previous version of this guide said. For this branch there are automated checks only (below),
run on native Windows 11 Pro (10.0.26200) with Rust 1.99.1, 2026-11-01; nobody has tried it on a
display yet.

**Branch `ecad/cable-slash` (brief 004, design `ecad/docs/design/004.md`)** adds **Cable Slash**
to the ECAD tab: a short line drawn across wires makes them one cable (section 5A). Nothing
outside `ecad/` changes, and the file format is unchanged. For this branch there are automated
checks and your trial on native Windows (2026-11-06, below); no agent has run it on a display.

**Branch `ecad/cable-views` (brief 005, design `ecad/docs/design/005.md`)**, cut from `dev`
after brief 004 was merged, adds **Cable View** to the ECAD tab: one click switches a cable
between the traditional view (every wire and the slash) and a fan view (one magenta trunk that
fans out to the wire ends), and back (section 5B). The switch is presentation only: it writes no
transaction and never changes the project's change count. Each cable's view is remembered for you
outside the `.ecad` file, in `cable-views.json` in your ECAD settings folder (section 9). Nothing
outside `ecad/` changes, and the file format is unchanged. For this branch there are automated
checks only (below), written with the branch and run by the tester and CI; nobody has run it on a
display yet.

**Branch `ecad/cable-look` (brief 009, design `ecad/docs/design/009.md`)**, cut from `dev` after
brief 005 was merged, restyles the fan to read like a cable on an electrical drawing (a label box,
a thick trunk and spines, curved bends and thin tails with each core's label and wire number),
adds **Core Label** to the ECAD tab (a core number or colour per wire in a cable, shown just past
the slash in traditional view and at the tail start in fan view), and draws brief 010's cable
reference end as a marker where the fan stops (section 5C). At your request of 2026-11-07 the
trunk joins the middle of each spine, and **Cable Join** slides it up or down the spine. Nothing outside `ecad/` changes, and
the file format is unchanged: a core label is the existing member `core` field, set with the
existing member ops. For this branch there are automated checks only, written with the branch,
and reference screenshots rendered headless for the owner to approve
(`ecad/apps/ecad-trial/tests/reference/009-*.png`, and `005-fan.png` redrawn in the new look);
nobody has run it on a display yet. In this combined trial with brief 010 the marker shows 010's
live reference text (such as `/2.1`, or a red `/?` while the end has no other end).

**Branch `ecad/cable-references` (brief 010, design `ecad/docs/design/010.md`)**, cut from `dev`
after brief 005 was merged, lets one cable be drawn on two pages (section 5D). **Cable Reference**
marks where a cable leaves a page; **Continue Cable** draws the same cable on another page (short
wires that are the same wires, and a slash); each end shows the other end's page and column, such
as `/2.1`, worked out every time the page is drawn, so renumbering or moving pages never breaks
it. **Go to Reference** jumps between the ends, and **Rename Page** and **Move Page** are new in the
Pages group. Nothing outside `ecad/` changes, and the file format is unchanged. The decisions the
overseer took on the design's open questions while you were away (one conductor drawn on two
pages is one core, two references per cable, no continuation on the same page) are listed in the
pull request for you to confirm. For this branch there are automated checks only (below); nobody
has run it on a display yet. In this combined trial the marker is brief 009's flag around that
text, with 010's ring on the point.

**Branch `ecad/agent-mcp` (brief 011)**, cut from `ecad/cable-views` (PR #24) with `dev`
merged, adds `ecad-mcp`: an AI agent such as Claude Code can read and change a project through
MCP, headless (section 7A). Nothing outside `ecad/` changes, and the file format is unchanged. For
this branch there are automated checks only, written with the branch and run by the tester and
CI; nobody has connected Claude Code to it yet.

**Branch `ecad/load-benchmark` (brief 012)**, cut from `dev` at `60c7c35`, adds the on-demand load
benchmark `cargo xtask bench` (section 8, "Measuring load time"). The app, the file format and CI
are unchanged. Evidence: the generator's shape tests (`ecad/crates/ecad-bench/tests/all/
generator_contracts.rs`), written with the branch and run by the tester and CI; one agent run of
`cargo xtask bench --tier small` on native Windows 11 Pro, 2026-11-07, which printed the table and
wrote the JSON. You have not run it yet.

**Branch `ecad/startup-timing` (brief 015)**, cut from `dev` at `bb86e05`, adds start-up timing
to the trial, switched on by `ECAD_STARTUP_TIMING=1` (section 8, "Measuring start-up"). With the
switch off nothing changes. Evidence: `ecad/apps/ecad-trial/tests/all/startup_contracts.rs`, written
with the branch; three agent launches of the release trial on native Windows 11 Pro, 2026-11-08
(section 8). You have not run it yet.

**Branch `feature/fast-tests` (brief 017)** changes how the tests are built and run, not the app:
nothing you run or see changes. Each crate's integration tests are one executable
(`tests/all/`), so the test paths below are `…/tests/all/<file>.rs`. Tests that take more than
about 2 s are marked slow: plain `cargo test` and `cargo xtask test-changed` skip them, and
`cargo xtask ci` (the QA gate, Linux and Windows CI) runs them with `--include-ignored`. To run
the whole set by hand: `cargo test --workspace --all-features -- --include-ignored`.

**Branch `feature/components` (brief 018, design `ecad/docs/design/018.md` with your answers of
2026-11-08)** makes placed symbols real devices with tags (section 5E). **Place Symbol** now asks
which symbol first (18 built-in IEC-style starter symbols: coil, contacts, MCBs, fuse, PSU, push
buttons, lamp, motors, transformer, terminal, PLC channels); each new device gets the next tag
of its prefix (`-K01`, `-Q01`…, highest + 1, two digits by default), and a contact can be added to
an existing device (`-K01`'s coil on one page, its contacts on another). Tags and connection point labels
are drawn beside every symbol. **Find Symbol**, **Device Properties** and **Delete Device** are
new in the Components group. Nothing outside `ecad/` changes. Existing files are written as before, but
one model rule is relaxed (I14, your decision of 2026-11-08): a project with a contact added to a
device is refused by older builds, and so is one whose tag settings were changed (two optional
project settings, your decision of 2026-11-08). For this branch there are automated checks only (below),
written and run by the worker; nobody has run it on a display yet.

Three kinds of evidence exist, and they are kept apart:

| Kind | Who | Covers |
| --- | --- | --- |
| **Automated checks** | `cargo xtask ci` and plain `cargo test --workspace`, in GitHub CI on Linux and natively on Windows (jobs `ecad` and `ecad-windows`), and locally on Windows with Rust 1.99.1 | The file tests run on Windows with none ignored (brief 002): a file renamed over or deleted and recreated behind an open project is refused; Save As is confirmed only after a real folder flush; read-only files, a lock that cannot be taken, the unnamed lock holder on Windows, retries while another program briefly holds a file, and a killed process's lock released within 2 s (`ecad/crates/ecad-store/tests/disk_contracts.rs`, `ecad/crates/ecad-file/tests/all/disk_contracts.rs`, `disk_edge_contracts.rs`). The model outcomes of sections 5 to 7 (what the project holds, not how it looks), across the trial, tools, mouse and files tests together: `the_quick_start_trial_runs_headless` (`ecad/apps/ecad-trial/tests/all/trial.rs`) follows the typed version of sections 5 to 7, saving through the dialog as `~/ecad-projects/demo` under a temporary home folder and checking the banner line and tab title quoted in section 7; the click-based steps are in `mouse.rs` and `tools.rs` (next row and below); plus fault injection at every step of Save and Save As, a crash in the middle of a save, a killed process holding the lock, a second opener, external changes and damaged files. **Stage 2b:** section 8's worked example (`ecad/apps/ecad-trial/tests/all/debugger.rs`, `the_quick_start_debugger_example_runs_headless`); the trail from a selected wire to its record bytes, compared with the file; pending, unconfirmed and failed saves never shown as saved; a busy project not waited for; history views and checks leaving the live project unchanged (`ecad/crates/ecad-cadcraft/tests/all/debugger.rs`) **ECAD Tool Sets tab:** every button names an existing command; opening the ECAD tab and clicking Convert to Component converts the selection once, and `u` undoes it; a command waiting at a prompt is cancelled first; templates fill the Command field and run nothing, and an unedited template is refused; IDs come from the selection, and a huge selection is followed only so far; a read-only project behaves as with typed commands (`ecad/apps/ecad-trial/tests/all/tools.rs`). Without extra tabs, Drafting and Modeling are unchanged (`crates/ui-egui/tests/toolsets_extra.rs`) |
| **Automated checks, brief 008 (ECAD tools by mouse)** | `cargo xtask ci` and plain `cargo test --workspace` locally on Windows (Rust 1.99.1), 2026-11-01; CADCraft's own tests for the overlay change (`cargo test -p cadcraft-engine -p cadcraft-ui-egui`) | Section 5 by clicks and keys in a 1280×800 window (`ecad/apps/ecad-trial/tests/all/mouse.rs`, `the_quick_start_example_is_built_by_clicks_and_keys`): window selection, **Convert to Component**, **Add Terminal** with `40,10` and `0,10`, **Place Symbol** with `R`, **Wire** by four clicks ending within 5 pixels of the coil's terminal; Esc while drawing a wire writes nothing, and `u` undoes one object; hovering names the coil and the wire and writes nothing. Every interactive form records exactly the transaction its JSON form records for the same values; Esc at every prompt; capture inside and outside the aperture; error messages; hostile input (`ecad/crates/ecad-cadcraft/tests/all/interactive_contracts.rs`). Marks keep their size at every zoom (`crates/ui-egui/tests/owner_overlay.rs`). Not checked: how it looks (reference screenshots are taken by the tester) |
| **Automated checks, brief 018 (devices and tags)** | written with the branch and run by the worker on native Windows 11 (Rust 1.99.1), 2026-11-08: `cargo xtask test-changed` and the medium bench | Section 5E without a display (`ecad/crates/ecad-cadcraft/tests/all/device_contracts.rs`): all 18 starter symbols get their prefix and the next number and their definitions' terminal names, each definition embedded once with its origin; a coil and two contacts on two pages are one `-K01` with `13`/`14` and `21`/`22`; renaming the tag is one `Set tag` transaction that `u` undoes; a duplicate tag names the device and the ways forward; placing with an existing tag adds to that device; merging a renamed device moves its symbols, terminals and wires in one undoable transaction; deleted tags are not reused; save, close and reopen keep it all; the interactive Place Symbol (search, Device, Tag, the Yes/Choose another question), Device Properties and Delete Device; refusals and hostile input; erasing one symbol, then the last one; a moved contact's wire follows; a copy gets the next tag; the hover lens. MCP: `place`, the join and `setprop` tag record the JSON forms' transactions as the agent, a duplicate is refused, writes off refuse device writes (`ecad/apps/ecad-mcp/tests/all/mcp_contracts.rs`). The model rule (`ecad/crates/ecad-model/tests/all/device_contracts.rs`). Not checked: anything on a display, and the Find Symbol window by clicks |
| **Automated checks, brief 004 (cable slash)** | written with the branch; run by the tester and by CI on Linux and Windows | Section 5A: **Cable Slash** clicked by its label, the first point clicked, the crossing count shown before the second point, one transaction, three magenta dots, `u` in one step, and a refused second slash that keeps its first point (`ecad/apps/ecad-trial/tests/all/cable_slash.rs`). The JSON and click forms record the same transaction; the crossing rules C1 to C6; every refusal leaves the history unchanged; undo, redo, save and reopen; erasing the slash keeps the cable and `ecaddelete {"cable":…}` keeps the wires; `cableadd` refuses a wire already in a cable; hover and the debugger trail; 10 000 wires accepted and 10 001 refused (`ecad/crates/ecad-cadcraft/tests/all/cable_slash_contracts.rs`). Not checked: how it looks (reference screenshots are taken by the tester) |
| **Automated checks, brief 005 (cable views)** | written with the branch; run by the tester and by CI on Linux and Windows | Section 5B: **Cable View** clicked by its label, the slash clicked (fan: a trunk and six fan lines), the trunk clicked (back to traditional), no transaction and no change in the change count (`ecad/apps/ecad-trial/tests/all/cable_view.rs`). Ten switches by JSON and by clicks leave the document, history, head, IDs and nets unchanged; the fan of design 005's fixture is its worked example on layer `ECAD-CABLES`; `u` and `redo` skip the switch; moving, erasing, copying, Connect, Convert to Component and Add Terminal on a fan line are refused with the spec's message, while JSON forms still act on the wires; moving equipment beside a fan moves the hidden routes; views remembered per cable across save, close and reopen; an unreadable, oversized or unwritable settings file never fails anything; fans of 1, 2 and 20 wires, bent and right-to-left routes, loose ends and hostile coordinates; hover; selection-based switching; the JSON form's errors (`ecad/crates/ecad-cadcraft/tests/all/cable_view_contracts.rs`). Not checked: how it looks, and whether the switch feels instant (reference screenshots and the feel are for the tester and you) |
| **Automated checks, brief 010 (cable references)** | written with the branch (the worker ran only the new tests once); run by the tester and by CI on Linux and Windows | Section 5D: **Cable Reference**, **Continue Cable** and **Go to Reference** clicked by their labels, their first prompts, the click-only path (reference, continue with Enter and two clicks, jump back), and the **Rename Page** and **Move Page** templates filling in the current page (`ecad/apps/ecad-trial/tests/all/cable_references.rs`). One cable on two pages keeps its ID, members and nets; continuing is one transaction with the design's ops and geometry; the texts `/2.1` and `/1.2` and where they sit; no end mark where a wire leaves toward a reference; moving or renaming a page is one transaction and the texts follow with none of their own; erasing one drawing makes the other end a red orphan with a diagnostic and a message, and `u` restores it; save, reopen and replay; the hover lens; `cablegoto` there and back; the interactive and JSON forms record the same transactions; refusals R1 to R6; hostile input; the debugger trail; fan view keeps the reference (`ecad/crates/ecad-cadcraft/tests/all/cable_reference_contracts.rs`, whose JSON-form table is written to move to the MCP harness of brief 011). Not checked: how it looks (reference screenshots are taken by the tester) |
| **Automated checks, brief 011 (agent access through MCP)** | written with the branch; run by the tester and by CI on Linux and Windows | Section 7A without a client (`ecad/apps/ecad-mcp/tests/all/mcp_contracts.rs`): the tools and every ECAD command with its parameter docs are listed; the cable examples of sections 5A and 5B through `execute` record exactly the JSON forms' transactions, each by `agent <client name>`, and `u` undoes each in one step; with writes off, or on a project opened read-only, every change and `save` is refused and the history is unchanged; the Block Editor, REFEDIT and LISP are refused with your refusal; save only with `--write`, and reopening shows the agent as author; malformed, oversized and non-UTF-8 requests, long batches and unknown tools get JSON-RPC errors; the built `ecad-mcp` over real stdin and stdout. Briefs 009 and 010's commands through MCP (`ecad/apps/ecad-mcp/tests/all/cable_command_contracts.rs`): `cablecore`, `cablefan`, `cableref`, `cablecontinue`, `renamepage` and `movepage` record exactly their JSON forms' transactions by the agent, `u` undoes each in one step, and refusals change nothing; `cablerefs` and `cablegoto` run with writes off, record nothing, and `cablegoto` only switches the open page. Not checked: Claude Code itself connected to it |
| **Automated checks, brief 003 (the trial on native Windows)** | local checks from `ecad/CLAUDE.md` on native Windows 11 (Rust 1.99.1), 2026-11-01, branch `ecad/windows-trial`; CI runs them on Linux and Windows | Typed paths (`ecad/apps/ecad-trial/tests/all/paths_contracts.rs`): `~`, `~/` and `~\` from the home folder, also with `HOME` unset (a child process); `~` with no home folder known is refused and no folder named `~` is ever made; drive letters, backslashes, spaces, non-ASCII and UNC paths; saving and reopening in folders with spaces and non-ASCII names; a reserved name (`CON.ecad`, `nul`, `LPT1`), `.`, `..`, a drive-relative path (`C:`, `C:demo`) or a trailing dot refused with a message; one pair of surrounding double quotes (Explorer's "Copy as path") removed; a path normalised once; `--snapshots ~/…` under the home folder, never a `~` folder (also `windows_trial_contracts.rs`, the tester's); a case-variant spelling of the project's own file saved without a replace question (one file on Windows); the platform's default folder in the dialog, created by the first save into it on Linux (a test must not create `C:\dev\ecad-projects`). Arguments (`args_contracts.rs`): a project path, `--snapshots`, both, `--help`, `--version`, usage errors, and a name that is not valid Unicode. Not checked: the console window of a release build, and anything on a display |
| **Agent smoke run on a virtual display, stage 2a** | the development agent, on Xvfb with software rendering, not a person | The window starts; a typed wire; Ctrl+S opens the Save As dialog and saves; the tab and banner show the file; the file reopens from the command line; a second window on the same file is read-only and says why; a project left by a killed process reports its reclaimed lock; Ctrl+Q with unsaved changes shows Save/Discard/Cancel, and Save writes the file, quits and releases the lock |
| **Agent smoke run on a virtual display, stage 2b** | the development agent, as above, 2026-10-13 | Section 8's example: the panel opens from **Debugger** on the right; clicking the coil and then the wire listed their handles, IDs and trails; the transactions showed `[pending]` in orange, then `[saved]` in green after Save As; **Read bytes** showed the wire's record at its offset with "valid record holding exactly this transaction"; the move and its undo appeared as pending `#4` and `#5`; after Save, Verify replay, Verify projection and Verify file all passed. Not looked at: the panel on a real WSLg display |
| **Agent smoke run on a virtual display, ECAD Tool Sets tab** | the development agent, as above, 2026-10-15 | Tool Sets shows **Drafting**, **Modeling** and **ECAD** tabs; the ECAD tab shows Components, Wiring, Edit, Pages and Inspect; clicking a rectangle's four lines and then **Convert to Component** converted them (the banner's change count went up by one, the lines stayed selected). The first version (a group at the bottom of Drafting) was also run: **Wire** filled the Command field with its template and note, and Enter on it unedited was refused. Not looked at: a real WSLg display |
| **Tried by you (the owner), brief 004 (cable slash)** | you, on native Windows, 2026-11-06, branch `ecad/cable-slash` (exact commit and build not restated) | You reported that every check passed: the steps of section 5A; the preview text readable past the Dynamic Input box; the red refusal text readable near the right edge of the drawing; the red `-> no wires`; and the crossing rules behaving as designed. You approved the three reference images `004-slash.png`, `004-slash-preview.png` and `004-slash-refused.png` (`ecad/apps/ecad-trial/tests/reference/`) |
| **Tried by you (the owner), brief 008 (ECAD tools by mouse)** | you, on native Windows 11, 2026-11-01, branch `ecad/usability-mouse-tools` at `8c64c08` (release build of `ecad-trial`, CADCraft 0.15.1) | By clicking: a drawn component with terminals (cyan rings), a second component with terminal A1, and a wire whose ends attached to both (green rings). Hovering terminal A1 showed "Terminal A1", its component's ID, its own ID and its wire ("end B"). You reported it as a big improvement that starts to feel like an ECAD. Not reported as tried: save and reopen, Esc and undo, connect and disconnect, and the paper-space refusal; the three reference screenshots are not yet approved. Noted for the later tidy-up (not a separate brief): the tooltip shows long IDs where tags or names would read better, along with default sizes and feel |
| **Tried by you (the owner), stage 2b and the ECAD Tool Sets tab** | you, on WSL, 2026-10-15; you accepted both for merging (PRs #11 and #12) | You observed the debugger panel's appearance, the revised **ECAD** tab with its five groups (Components, Wiring, Edit, Pages, Inspect), and a successful **Convert to Component** from that tab. Nothing else was reported as tried: see "Not yet tried by you" below |
| **Tried by you (the owner), first ECAD Tool Sets version** | you, on WSL, 2026-10-15, the first version of PR #12 (a group at the bottom of Drafting) | The ECAD group appeared; you asked for it as a tab of its own beside Drafting and Modeling, which the merged version does |
| **Tried by you (the owner), stage 2a** | you, on WSL, 2026-10-13, branch `ecad/disk-persistence` (exact commit and environment not restated; the branch head was `fec06f7`, app code `0e6dfe6`) | The core of section 7: Save, close and reopen; connected movement after reopening; undo and redo across the save; writer locking; Save As; Cancel and Save on close. The exact checks are listed below |
| **Tried by you (the owner), earlier** | you, on WSL (Ubuntu 26.04, WSLg, Rust 1.99), 2026-10-12, on the earlier trial at `239d07f` | Drawing, conversion to a component, a terminal, PLACE, WIRE, connected movement, undo/redo, a second page and its tab close, snapshot export, and the old "persistence unavailable" behaviour. Details: [`experiment-report.md`](experiment-report.md) ("Manual desktop trial on WSL") |

### Your stage 2a trial (2026-10-13): what you observed

Recorded as you reported them:

1. Saved `untitled.ecad`; the banner showed no unsaved changes.
2. Closed and reopened; both components and the wire came back.
3. Moving a component after reopening kept the wire attached.
4. Undo restored the move, then removed the wire created before saving and closing. Redo
   restored both.
5. `u` and `redo` work in the top Command field; Ctrl+Z also works.
6. Saving again after undo and redo succeeded.
7. A second window opened the same file read-only, with a writer-lock warning.
8. Save As to `copy-test.ecad` cleared read-only and allowed editing.
9. Closing after an edit offered Save, Discard and Cancel. Cancel kept the work open; a later
   close with Save saved and closed.

**Not manually verified** (covered by automated tests only, or not at all):
- crash recovery, including reclaiming a lock left by a crashed session;
- interrupted saves and the `.tail-…` sidecar;
- **Discard** on close;
- stable-ID inspection (the `setprop` check in section 7, step 5);
- saving and reopening a project with more than one page;
- what failed saves look like (for example a folder that does not exist).

### Your acceptance of stage 2b and the ECAD tab (2026-10-15): what you observed

Recorded as you reported them:

1. The debugger panel's appearance.
2. The revised **ECAD** tab in Tool Sets, beside Drafting and Modeling, with its five groups:
   Components, Wiring, Edit, Pages and Inspect.
3. A successful **Convert to Component** from the ECAD tab.

**Not yet tried by you** (covered by automated tests or the agent's virtual-display runs only):
- everything from brief 008: the ECAD tools by clicking (section 5), the connection point and wire-end
  marks and the hover tooltip beyond your first click-only trial;
- everything from brief 003: the native Windows steps of section 1W as now written, `~\` paths,
  the default folder `C:\dev\ecad-projects` being created by Save As, a reserved name refused,
  and no console window beside a release build;
- section 8's worked example beyond the panel's appearance: following a selection into the trail,
  pending versus saved transactions, **Read bytes**, history at an earlier transaction, and the
  three checks;
- the debugger on a reopened project or a second, read-only window;
- whether the panel stays responsive while you draw;
- the ECAD tab's template buttons (for example **Place Symbol**, **Wire**, **Set Property**),
  refusal of an unedited template, and IDs filled in from the selection;
- that Drafting and Modeling look as before with the ECAD tab present.

## 1. First-time setup (WSL on Windows)

1. Check the environment, and keep the output for any report. The checks are in
   [`environments.md`](environments.md) section 1. A window needs WSLg: this should print
   something, and `/mnt/wslg` should exist.

   ```sh
   echo $WAYLAND_DISPLAY $DISPLAY
   ls /mnt/wslg
   ```

2. Install the build tools and Rust (skip what you already have):

   ```sh
   sudo apt-get update && sudo apt-get install -y build-essential pkg-config git curl
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
   . "$HOME/.cargo/env"
   ```

3. Clone into the **Linux** file system (your home folder), not under `/mnt/c`:

   ```sh
   cd ~
   git clone https://github.com/convyares-FCSL/ecadcraft.git
   cd ecadcraft
   git switch dev
   ```

   To try a branch before it is merged, switch to it instead, for example
   `git switch <branch-name>`.

4. Make a folder for projects. The Linux home folder is the supported place for `.ecad` files
   under WSL.

   ```sh
   mkdir -p ~/ecad-projects
   ```

5. Build and start. The first build takes several minutes and 6-10 GB of disk.

   ```sh
   cd ~/ecadcraft/ecad
   export CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_INCREMENTAL=0
   cargo run --release -p ecad-trial
   ```

6. If no window appears, or rendering fails:
   - set `WGPU_BACKEND=gl` (or `vulkan`) and try again;
   - if runtime libraries are missing, install `libxkbcommon0 libvulkan1 libegl1 mesa-vulkan-drivers`;
   - report the terminal output together with `vulkaninfo --summary` and `glxinfo -B`.

## 1W. First-time setup (native Windows)

The same trial runs natively on Windows 11 (no WSL), which is supported alongside Linux. Use
**PowerShell**. Saving on Windows differs from Linux in a few ways listed in section 9; keep
projects in a local NTFS folder such as `C:\dev\ecad-projects` (the trial's default), not in
OneDrive or the OneDrive-redirected Documents folder (unverified).

1. Install Git for Windows, the Visual Studio Build Tools with the "Desktop development with C++"
   workload (MSVC and a Windows SDK), and Rust from <https://rustup.rs> (the default
   `x86_64-pc-windows-msvc` toolchain). Open a **new** PowerShell window afterwards so that
   `cargo` is on the `PATH`. Check, and keep the output for any report:

   ```powershell
   rustc --version          # 1.95 or later; tested with 1.99.1
   cargo --version
   git --version
   [System.Environment]::OSVersion.VersionString
   ```

2. Clone and choose the branch:

   ```powershell
   cd C:\dev
   git clone https://github.com/convyares-FCSL/ecadcraft.git
   cd ecadcraft
   git config core.autocrlf false
   git switch dev        # or: git switch <branch-name>
   ```

3. The projects folder `C:\dev\ecad-projects` is created by Save As the first time you save into
   it (on `dev` before brief 003, make it yourself with `mkdir C:\dev\ecad-projects`). No other
   missing folder is created: saving into one fails with a message and changes nothing.

4. Build and start. The first build takes several minutes and 6-10 GB of disk.

   ```powershell
   cd C:\dev\ecadcraft\ecad
   $env:CARGO_PROFILE_RELEASE_DEBUG = "0"; $env:CARGO_INCREMENTAL = "0"
   cargo run --release -p ecad-trial
   ```

   A release build opens only the trial's window, with no console window beside it, and shows no
   console output, also when run from PowerShell or through `cargo run`. For `--help` and usage
   errors use a debug build (`cargo run -p ecad-trial -- --help`), or redirect the output
   (`... > out.txt`).
   To open a saved project: `cargo run --release -p ecad-trial -- C:\dev\ecad-projects\demo.ecad`.
   To update later, use section 2's commands in `C:\dev\ecadcraft`.

Everything from section 4 on is the same on Windows. Only file paths differ: where this guide says
`~/ecad-projects/demo`, type `C:\dev\ecad-projects\demo` (the Save As dialog suggests
`C:\dev\ecad-projects\untitled.ecad`). `~\` and `~/` also work and mean your user profile, for
example `~\Desktop\demo` is `C:\Users\<you>\Desktop\demo.ecad`. Forward slashes, spaces and
non-ASCII letters in paths are fine. Names Windows reserves for devices (`CON`, `PRN`, `AUX`,
`NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, with any extension) are refused with a message, as are
drive-relative paths such as `C:demo` (type `C:\…`) and a name ending in a dot. A path pasted
from Explorer's "Copy as path" may keep its double quotes. `--snapshots ~\snaps` means the
`snaps` folder in your user profile.

## 2. Updating an existing checkout

```sh
cd ~/ecadcraft
git status                     # must be clean; commit or stash local changes first
git fetch origin --prune       # also drops branches deleted on GitHub
git switch dev
git merge --ff-only origin/dev # refuses rather than merging if your dev has diverged
git log --oneline -1           # compare with the commit you were told to test
```

For a branch under review, use its name instead of `dev` in the last three commands (for example
`git switch <branch-name>` and `git merge --ff-only origin/<branch-name>`). Then
rebuild by running it (section 3); Cargo rebuilds what changed.

## 3. Running again

**Start from the demo:** `cargo xtask demo` writes a small motor control panel (three pages:
Supply, Motor control, Field; devices such as `-K01` across pages 2 and 3; cable `-W01` with
`/page.column` references) to `C:\dev\ecad-projects\demo.ecad` (`~/ecad-projects/demo.ecad` on
Linux); open it as below. It is rebuilt from `ecad/examples/demo/commands.jsonl` through the MCP
`execute` tool, so it never goes stale; an existing file is replaced only with
`cargo xtask demo --force` (or give another path: `cargo xtask demo C:\dev\ecad-projects\try.ecad`).

```sh
cd ~/ecadcraft/ecad
export CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_INCREMENTAL=0
cargo run --release -p ecad-trial                                    # a new project
cargo run --release -p ecad-trial -- ~/ecad-projects/demo.ecad       # open a saved project
cargo run --release -p ecad-trial -- --snapshots ~/ecad-trial-snapshots   # allow diagnostic snapshots
```

On native Windows (PowerShell):

```powershell
cd C:\dev\ecadcraft\ecad
$env:CARGO_PROFILE_RELEASE_DEBUG = "0"; $env:CARGO_INCREMENTAL = "0"
cargo run --release -p ecad-trial                                           # a new project
cargo run --release -p ecad-trial -- C:\dev\ecad-projects\demo.ecad         # open a saved project
cargo run --release -p ecad-trial -- --snapshots C:\dev\ecad-trial-snapshots   # allow diagnostic snapshots
```

`--snapshots <folder>` and a project file can be given together.

## 4. Where to type

- **The PROJECT panel (far left).** Its header shows the project's status line (name, pages,
  file, unsaved changes, and any read-only or unconfirmed-save warning), the buttons **New**,
  **Open…**, **Save**, **Save As…** and **Close**, and one small line saying this is an
  experiment. Below it, the project's pages: click one to show it (section 5E).
- **The ECAD command field** (PROJECT panel > unfold **Developer**: the field and **Run** are at
  the top of the section, the reply under them). Use it for ECAD
  commands with JSON parameters, for example `wire {"points":[[0,0],[40,0]]}`. It takes a whole
  line, spaces included, so it also accepts CADCraft command lines such as `line 0,0 40,0`. Press
  Enter or **Run**; close it with its ×. Copy and paste (Ctrl+C, Ctrl+V) work with other
  programs such as Notepad.
- **CADCraft's command line (bottom of the drawing area).** A space there means Enter, so it
  cannot take JSON with spaces. Use it for plain CADCraft commands, for the prompts of
  mouse-driven commands such as `move`, and for `u` and `redo`.
- **The mouse** works as in CADCraft: drawing, selecting and dragging.
- **The ECAD tab in Tool Sets** (the left panel: click **ECAD** beside Drafting and Modeling),
  with one button per implemented ECAD command, in groups (Components, Wiring, Edit, Pages,
  Inspect). A button cancels a command waiting at a prompt first, as CADCraft's own Tool Sets
  buttons do.
  - **Convert to Component** converts the current selection at once; with nothing selected it
    asks you to select objects (click or window), and Enter converts them.
  - **Add Connection Point**: click on a component's outline to add a connection point there; repeat; Enter to
    finish.
  - **Find Symbol** (brief 018) opens a small window with one search field: type a few letters
    (`coil`, `cont`, `fuse`, `-Q`, or a device's tag such as `-K01`) and click a row. A symbol row
    starts Place Symbol with it; a device row asks which symbol to add to that device.
  - **Place Symbol** asks `Enter symbol or [List/Rotate/Mirror] <coil>:`. Type a symbol (`no`,
    `mcb1`…; `L` lists them) or press Enter for the one shown. The symbol then follows the cursor
    with its tag and connection point labels; the prompt says what the click makes (`new device -K02` or
    `joins -K01 as 13/14`). `R` then Enter rotates by 90°, `M` then Enter mirrors, `D` then Enter
    adds the next symbol to an existing device, `T` then Enter types the new device's tag. Click
    to place; repeat; Enter to finish. Clicks snap to the 5 mm grid (F9 turns snap off).
  - **Device Properties**: click a device (any of its symbols, its tag or a connection point label), then
    choose Tag, Description, Maker, Part, Location or Function and type the new value.
  - **Delete Device**: click a device; it asks before deleting it with every symbol on every page.
  - **Wire**: click points; an end on a connection point attaches (a green ring shows before you click);
    a click on a connection point after the first point, or Enter, finishes the wire.
  - **Connect**: click a wire near its end, then a connection point. **Disconnect**: click a wire near
    an attached end.
  - These six put their prompts at CADCraft's command line (bottom of the drawing area) and leave
    the ECAD command field alone. Answer by clicking, or type a point (`40,10`) or an option letter and
    press Enter. Enter (or Space, or a right-click) finishes; Esc cancels what is not done yet
    (objects already added stay); `u` afterwards undoes the last object.
  - Pages are made, opened, renamed and moved in the PROJECT panel (**+**, a click, double-click
    or the row's right-click menu, drag or the arrows; section 5E), not in Tool Sets. **New page…**
    and **Rename…** in the row's menu open a small name box; OK or Enter acts in one step.
  - **Inspect Project** is in the panel's **Developer** section; it shows a readable, copyable
    summary there (file and transaction, pages, devices, cables, counts).
  - **Set Property, Add to Cable, Delete Electrical Object** only open the
    ECAD command field with an editable line, and a note under the
    field says what to fill in.
    Replace each `<placeholder>`, then press Enter or **Run**. A line that still holds a
    placeholder is refused, never run. IDs come from the current selection where it names them:
    select a wire before **Set Property** or **Delete Electrical Object**, and its ID is filled
    in. The selection is left as it is.
- **Typing a command name** (`wire`, `addterminal` …) in either place starts the same
  click-based form as its button. `name {json}` in the ECAD command field runs the JSON form with
  no prompts, as scripts do.

**What the drawing shows.** Every connection point has a small cyan ring; a wire end attached to a
connection point has a green dot, and a loose wire end a small grey square. They keep their size at every
zoom and are only display: they cannot be selected or printed. With no command running, resting
the pointer on a component, a connection point ring (any connection point, free or connected, of a drawn component
or a symbol; near a ring the connection point wins over the wire ending on it) or a wire for half a second shows a tooltip with its
name or ID and its connections; hovering never changes anything.

Each ECAD command answers at the command line in words, with the full IDs it created. Note IDs
you want to check later.

## 5. Example: two components connected by a wire (by clicking)

Open the **ECAD** tab in Tool Sets first. Prompts appear at CADCraft's command line at the bottom
of the drawing area; type there (or in the ECAD command field) only where a step says so.

1. **Draw a rectangle.** In the ECAD command field: `line 0,0 40,0 40,20 0,20 c`. Then type `zoom e`
   (or zoom with the mouse wheel) so it fills the view; leave room on the right for the coil.
   Expected: four lines; the Project panel's status line shows 1 change not saved.
2. **Make it a component.** Select the four lines with a window selection (click above left of
   the rectangle, then below right of it), then click **Convert to Component**.
   Expected: `Component <id> created from 4 objects.` at the command line. **Note this component
   ID.** The rectangle now selects as one group. (With nothing selected, the button asks
   `Select objects to convert to a component:`; select, then press Enter.)
3. **Give it two connection points.** Click **Add Connection Point**. The prompt says
   `Specify point for connection point 1 on a component or [Name]:`. Type `40,10` and press Enter, then
   `0,10` and Enter, then Enter once more to finish.
   Expected: a cyan ring at 40,10 and at 0,10, and `Connection point 1 added to component <id>.`, then
   `Connection point 2 …`. Clicking on an edge works too: the connection point goes onto the outline where you
   click (use Midpoint snap, Shift+right-click > Midpoint, to hit the middle exactly). `N` then
   Enter lets you type a name for the next connection point; Enter alone keeps the number shown.
4. **Place a coil.** Click **Place Symbol**. Type `R` and Enter: the coil follows the cursor,
   turned by 90°, with cyan rings at its two connection points, and the prompt says
   `(new device -K01, rotation 90)`. Type `100,0` and Enter (or click there), then Enter to
   finish.
   Expected: `Placed -K01 (Contactor / relay coil) with connection points A1, A2.`; a rotated coil with
   rings at 90,0 and 110,0, `-K01` above it and `A1`, `A2` beside its connection points.
5. **Draw the wire.** Click **Wire**. Move onto the rectangle's connection point at 40,10: a green ring
   and `-> 1` appear before you click. Click. Then click at about 70,10 and at about 70,0 (bend
   points; F8 for Ortho helps). Move onto the coil's left connection point at 90,0 until the green ring
   and `-> A1` show, and click: the wire is finished.
   Expected: `Wire <id> drawn; end A attached to 1, end B attached to A1.`; green dots on both
   ends, and no grey squares. A click near a connection point but outside the green ring does not attach:
   that end shows a grey square instead.
6. **Look before you type IDs.** With no command running, rest the pointer on the coil, on the
   ring at 40,10 (or the coil's ring at 90,0) or on the wire. Expected: a tooltip with
   `Component (no tag)` and its connection points (on the rectangle; the coil shows
   `Device -K01   Contactor / relay coil`), `Connection point 1` (or `Connection point A1`) and its wire, or `Wire (no tag)` with `End A: connection point 1 of …` and
   `End B: connection point A1 of …`. Nothing changes in the Project panel's change count.

Typing instead of clicking still works, in the ECAD command field:
`convertcomponent` (with the lines selected), `addterminal {"handle":"<handle of the right-hand
edge>","at":[40,10]}`, `place {"at":[100,0],"rotation":90}`,
`wire {"points":[[40,10],[70,10],[70,0],[90,0]]}`. The headless test
`the_quick_start_trial_runs_headless` follows that typed version.

## 5A. Make a cable (by clicking)

A cable groups existing wires; the wires themselves do not change. You draw it as a short line
(the *slash*) across the wires. This example uses a page of its own so the coordinates are easy
to read.

1. **A page with three wires and a plain line.** In the ECAD command field:
   `newpage {"title":"cables"}`. Then click **Wire**, click at `0,0` and `60,0` and press Enter;
   do the same at `0,5` → `60,5` and `0,10` → `60,10` (typing the points at the command line, or
   `wire {"points":[[0,0],[60,0]]}` in the ECAD command field, works too). Draw a plain line with
   `line 0,15 60,15` and press Enter, then `zoom e`.
   Expected: three wires with grey squares at their loose ends, and a line without marks.
2. **Draw the slash.** Click **Cable Slash** (Wiring group). The prompt says
   `Specify first point of cable slash:`. Click below the wires, at about `28,-3`. Move the
   pointer up across them to about `32,18`, above the plain line.
   Expected before the second click: a rubber band, a green ring where it crosses each wire, and
   `-> 3 wires, 1 other object ignored` beside the pointer. Nothing is written yet.
3. **Click.** Expected: `Cable <id> created with 3 wires: wire …, wire …, wire ….` and
   `1 object that is not a wire was ignored.` at the command line; the slash stays as a plain
   line in the wire colour, with a small magenta dot on each wire it holds and no end marks; the
   Project panel's change count goes up by one. The wires are listed bottom to top: the order is the
   direction you drew the slash.
4. **Look.** With no command running, rest the pointer on the slash. Expected: a tooltip with
   `Cable (no tag)`, its ID, `View: traditional`, `Wires: 3` and the three wires. On a wire:
   its usual lines plus `In cable …`.
5. **Undo and redo.** Type `u`: the slash, its dots and the cable go in one step. `redo` brings
   them back with the same IDs.
6. **A refusal.** Click **Cable Slash** again and slash across the middle wire only (click at
   about `40,3`, move to `40,7`). Expected before the click: a **red** ring and
   `-> refused: 1 wire already in a cable` in red. Click: `Wire … is already in cable …; a wire
   can be in only one cable. Nothing was created.`; the prompt for the second point stays, so you
   can try another point. Press Esc: nothing was written.
7. **Delete.** Erasing the slash (select it, `erase`) removes only the drawing: the cable stays
   (the debugger still lists it). To delete the cable itself, select the slash first and click
   **Delete Electrical Object**: it fills in `ecaddelete {"cable":"<id>"}`; press Enter.
   Expected: `Cable <id> deleted; its 3 wires are kept.` in the reply; the wires stay.

`cableslash {"from":[28,-3],"to":[32,18]}` in the ECAD command field does the same as steps 2 and 3
(add `"tag":"-W1"` to name the cable). In the debugger, selecting the slash shows its trail:
item → representation `slash` → cable → its wires in order.

## 5B. Switch the cable's view (by clicking)

Continue on the page of section 5A, with the cable made in step 3 (redo it if you deleted it).
A cable can be shown **traditional** (every wire, and the slash) or as a **fan** (one cable line
that fans out to the wire ends). Switching changes only what is drawn: nothing is written to the
project.

1. **Switch to fan.** Click **Cable View** (Wiring group, after Cable Slash). The prompt says
   `Select a cable to switch its view:`. Click the slash.
   Expected on the same frame (brief 009's look): the three wires, the slash and its magenta dots
   go; in magenta, a label box (the cable's tag, `?` without one) on a thick trunk that joins the
   middle of a thick vertical spine at `x = 22.5` and of another at `x = 37.5` (along the middle
   wire, `y = 5`), a small curved bend where each outer wire leaves a spine (the middle wire leaves
   it straight), and a thin tail to each wire end. The grey
   squares at the wire ends stay where they were, at the ends of the tails. The command line says
   `Cable <first 8 hex digits>… now shown in fan view.` The Project panel's change count does **not**
   change. The prompt repeats.
2. **Switch back.** Click the magenta trunk (between a spine and the label box). Expected: the three wires and the slash are back,
   `… now shown in traditional view.`, the change count still unchanged. Press Enter (or Esc) to
   end the command.
3. **Undo skips it.** Switch to fan again and end the command. Type `u`: the last real edit is
   undone (if it was the slash, the cable goes), never the switch. `redo` brings it back; the
   cable is still a fan.
4. **Editing the fan is refused.** Select the trunk, type `move`, pick a base point and a second
   point. Expected: `This cable is shown in fan view; switch to traditional view to edit.`; the
   drawing is as before and the change count unchanged. Selecting fan lines is allowed; Connect's
   "select wire" on a fan line gives the same message. Moving a component whose connection points have
   fan-view wires attached is allowed: the fan follows.
5. **Hover.** Rest the pointer on a fan line. Expected: `Cable (no tag)`, its ID, `View: fan`
   and the three wires.
6. **Remembered.** Save the project (Ctrl+S), close it and reopen it. Expected: the cable opens in
   the view you left it in. Two cables in different views keep their own views. The view is not
   in the `.ecad` file: someone else opening the file sees every cable traditional.

With objects selected, **Cable View** switches every cable they name at once (a slash, a fan line
or a wire in a cable): if all of them are fans they all go back to traditional, otherwise they all
become fans. Typed: `cableview {"cable":"<id>"}` switches one cable, with `"view":"fan"` or
`"view":"traditional"` to set it; `cableview {}` lists every cable's view and changes nothing.

## 5C. Cores, label box and a reference end (by clicking)

Continue with the cable of sections 5A and 5B, shown traditional (switch it back if needed).

1. **Name the cable.** Select the slash and click **Set Property** (Edit group); edit the template
   to `setprop {"cable":"<id>","key":"tag","value":"C1"}` and press Enter. Do the same with
   `"key":"type","value":"3/C #10"` (and `"designation"` if you like: it becomes the box's second
   line). Give the wires numbers the same way, `"conductor":"<id>","key":"tag","value":"W1"`.
2. **Core labels.** Click **Core Label** (Wiring group, after Cable View). The prompt says
   `Select a wire in a cable:`. Click the bottom wire. It turns green and the prompt becomes
   `Core number or colour for W1 <none>:`; type `1` and press Enter. Expected:
   `Wire W1 is now core 1 of cable C1.`, a small magenta `1` just past the slash, above the wire;
   the change count goes up by one. The prompt asks for the next wire: click the middle wire and
   type `2`, the top wire and type `3`. Press Enter to end the command.
   - Enter with nothing typed leaves the label as it was; a single `.` (or only spaces) clears
     it; on a wire with no label that says `W1 has no core label.` and writes nothing.
   - Clicking the slash says `That is the cable; click one of its wires.`; a label longer than 32
     characters is refused and the same wire's prompt repeats.
   - `u` undoes one label at a time; `redo` brings it back.
3. **Fan view.** Click **Cable View** and the slash. Expected: the label box shows `C1` and
   `3/C #10`; each tail starts with its core label next to the spine, then its wire number
   further out, one text height apart; the trunk and spines are thick and the bends and tails
   thin (the trial turns **Show/Hide Lineweight** on when it opens or creates a project, unless
   you switched it yourself since the trial started).
   **Slide the join.** The trunk joins the middle of each spine. Click **Cable Join** (Wiring
   group), click the fan near its left end, then click higher or lower: the trunk now joins that
   end there (kept on the spine, between the outermost wires), and the change count goes up by
   one. At the second prompt, `Middle` puts it back. `u` undoes it.
4. **Hover.** Rest the pointer on a tail: `Cable C1`, then `Core: 1 (W1)`, its ID, its type and
   its wires with their cores (`  W1  core 1`). On a wire in traditional view: `In cable C1, core 1`.
5. **A reference end.** Place one with **Cable Reference** (section 5D). The fan stops at that
   end: no spine, bends or tails there; the trunk runs to a magenta flag beside the point,
   pointing away from the wires, and the flag shows the other end's page and column (such as `/3.1`; a red `/?` while there is no
   other end). The loose-end squares at that end are not drawn. Erasing the point fans that end
   again; `u` brings it back.

Typed: `cablecore {"conductor":"<id>","core":"1"}` sets one label (`"core":null`, `""` or spaces
clear it); `cablecore {"cable":"<id>","cores":["1","2",null]}` sets every member at once, one
label (or `null`) per member in member order, as one transaction.
`cablefan {"cable":"<id>","end":"A","at":[x,y]}` moves where the trunk joins end A (the point is
put onto the spine); `"at":0.25` places it a quarter of the way along the spine; `"at":null`
returns it to the middle. Without `end`, a point picks the nearer end.

## 5D. Continue a cable on another page (by clicking)

Continue on the page of section 5A (`cables`, page 2 when you followed section 5 first), with its
three wires. In this section the cable is named by the first 8 hex digits of its ID, as in the
replies (`setprop {"cable":"<id>","key":"tag","value":"C1"}` gives it a name instead).

0. **Make the cable again.** Section 5A step 7 deleted the cable (its wires stayed). Slash them
   again: click **Cable Slash**, click at about `28,-3` and then `32,18` (or type
   `cableslash {"from":[28,-3],"to":[32,18]}` in the ECAD command field). Expected: `Cable <id>
   created with 3 wires: …`. If you skipped step 7 and the cable is still there, skip this step.

1. **Mark where it leaves.** Click **Cable Reference** (Wiring group, after Cable View). The
   prompt says `Select a cable to reference:`. Click the slash. The prompt says
   `Specify reference point or [Undo]:`; a red ring and a red `?` follow the pointer. Click just
   right of the wire ends, at about `65,5`.
   Expected: `Reference for cable … placed on page 2.` and `It has no other end yet: open the
   other page and click Continue Cable.`; a red ring at the point and, beside it, a magenta flag
   around a small red `?`. The Project panel's change count goes up by one.
2. **A page where it arrives.** `newpage {"title":"field"}` (page 3). Draw a component with three
   connection points as in section 5: `line 100,-5 110,-5 110,15 100,15 c`, select the four lines, click
   **Convert to Component**, then **Add Connection Point** at `100,0`, `100,5` and `100,10` and press
   Enter. Then `zoom e`, and zoom out a little so that `20,5` is on screen.
3. **Continue it.** Click **Continue Cable**. The prompt says
   `Enter cable to continue or [List] <…>:` with your cable as the default (`L` lists the
   references waiting for their other end). Press Enter. Click where the cable arrives, at about
   `20,5` (a magenta ring and `2-B-6` follow the pointer: the text this end will show). Move the
   pointer to the right, to about `40,5`.
   Expected before the click: three short wires 5 apart, a slash across them and
   `-> 3 wires continue` beside the pointer. Click.
   Expected: `Cable … continued on page 3 with 3 wires; this end shows 2-B-6 and the end on page 2
   now shows 3-A-6.` and `Connect each wire's free end to its connection point with Connect.`; three short
   wires with grey squares at their right ends, a slash with three magenta dots, a magenta ring at
   `20,5` and, to its left, a flag pointing left (away from the wires) with `2-B-6`. Switch to page 2: its `?` is now a magenta `3-A-6` (only the page in front is redrawn by a change; another open page it affected is redrawn when you bring it to the front), and the
   wire ends next to the reference lost their grey squares (the wires continue). One change.
4. **Connect.** Click **Connect**, click each short wire near its right end, then the connection point
   at the same height. Expected: each wire stretches to its connection point; green dots there. In the
   debugger the three wires are the same IDs as on page 2.
5. **Hover.** Rest the pointer on a reference (its ring or its text). Expected: a lens such as
   `Cable reference  3-A-6`, the cable and the reference's ID, `This end: page 2 "cables",
   column 2, 3 wires`, `Other end: page 3 "field", column 1, 3 wires`, and one line per wire with
   the connection point it reaches there (`wire … -> …:1`).
6. **Go there and back.** Click the reference on page 2 to select it, then click **Go to
   Reference**. Expected: the `field` tab is current, centred on `20,5`, with its reference point
   selected, and `Page 3 "field", at the other end of cable … (3-A-6).` Click **Go to Reference**
   again: back on page 2. Without a selection the button asks `Select a cable reference:`.
7. **Move a page.** In the PROJECT panel drag this page's row to the top (or click it and the
   up arrow in the header). Expected: the page is now first; both references show the new numbers (the other page's when you switch to it); the change count goes up by one only. Renaming (double-click the row) changes the hover lens, not the marker text (it shows numbers, not titles).
8. **Erase one end.** On the `field` page, drag a crossing window (right to left) from about
   `45,15` to `5,-5` and `erase`. Expected: the short wires, the slash and the reference go (the
   wires themselves stay, still connected to the component on that page); the other page's
   marker turns red `?` (click its tab to see it: an edit in the drawing redraws only the page
   in front, and another page whose references it changed is redrawn when you bring it to the
   front); the reply adds `Cable …'s reference on page … now has no other end.`;
   the debugger's Diagnostics lists it under **Cable references**. Type `u`: everything is back
   with the same IDs.

Typed: `cableref {"cable":"<id>","at":[65,5]}`, `cablecontinue {"cable":"<id>","at":[20,5],
"toward":[40,5]}` (optionally `"members":[…]` to continue only some wires),
`cablegoto {"reference":"<id>"}` and `cablerefs {}` (lists every reference with its text, partner
and whether it is an orphan; read only).

## 5E. Project panel (by clicking)

On `demo.ecad`, or after 5D (which leaves two pages, `Supply` and `Field`). Brief 016, design
`ecad/docs/design/016.md`. Everything here is by clicking.

1. **See the pages.** The **PROJECT** panel on the far left lists `1 Supply`, `2 Field`; the page
   shown is highlighted (a bar in the accent colour at its left) and named in the single drawing tab, for example
   `demo - page 1: Supply`.
2. **Switch.** Click `Field`: it replaces `Supply` in the same tab, at the view you left it. Click
   `Supply` again.
3. **New page.** Click **+** in the panel's header (or **New page…** in a row's right-click menu):
   a small name box opens with `3` selected. Type `Spare`, press Enter (or **OK**): page 3 is
   added and shown at once, one `u`. **Cancel** (or Esc) does nothing. **Rename…** in a row's
   right-click menu opens the same box, prefilled with its title; the menu also has Open,
   Preview, Move up and Move down.
4. **Rename.** Double-click `Spare`, type `Spare 2`, press Enter. Type `u` at the command line:
   back to `Spare`. An empty title, or one over 200 characters, is refused under the field and
   nothing is recorded.
5. **Reorder.** Drag `Spare` above `Supply`: it moves to the top of the list. Page numbers are
   stored (brief 019), so `Spare` keeps its number `3` and no reference text changes. Click `Spare`, then the down
   arrow in the header twice to put it back (or Alt+Down). Each move is one `u`.
6. **Keyboard.** After clicking a row, Up and Down move an outline, Enter shows that page, F2
   renames it, Space previews it. Any other key goes to the command line as usual.
7. **Undo brings the page.** On `Field`, draw a wire. Click `Supply`. Type `u`: `Field` comes to
   the front with the wire gone, and the command line says `Page 2 "Field" brought to the front.`
   `redo` puts the wire back, still on `Field`.
8. **Preview.** On `Supply`, click the magenta reference point, then **Preview Reference** (Tool
   Sets > ECAD > Wiring). A read-only window over the drawing shows `Field` around the other end,
   with a magenta ring on it and `1-B-6` beside it. Scroll to zoom, drag to pan; nothing in it can
   be selected and nothing is recorded. Click **×** to close it. Click `Field`'s row and press
   Space: a preview of the whole page; **Open page** shows it in the drawing tab.
9. **Closed tab.** Close the drawing tab (its ×): the panel still lists every page and none is
   highlighted; click one to show it again. To close the project itself, click **Close** in the
   panel's header (it asks to save changes).
10. **Collapse.** The `<` at the right of the panel's header folds it to a thin strip; click the
    strip to open it again.

## 5F. Devices and tags (by clicking)

A device is a real electrical part with a tag (`-K01`). It can be drawn by several symbols on
several pages: a contactor's coil on one page and its contacts on others are the same `-K01`.
Tags are numbered with two digits by default (`-K01`, `-Q03`); `-K1` and `-K01` are the same tag.
Any letters are accepted (`-SR01`, `-TB01`, `-N01`): IEC 81346 is the default, never the law.
Open the **ECAD** tab first.

1. **A coil.** Click **Find Symbol**, type `coil` and click the **Contactor / relay coil** row.
   The window asks `Place Contactor / relay coil as:`; click **New device (-K01)**. Click on the
   page (move the window aside if it covers the spot).
   Expected: `Placed -K01 (Contactor / relay coil) with connection points A1, A2.`; `-K01` left of the
   coil and `A1`, `A2` beside its connection points. Each further click places another coil; press Enter
   or Esc when done.
2. **A contact of the same device, by mouse only.** In the Find Symbol window type `cont` and
   click the **NO contact** row. The window lists **New device (-K02)** first, then the devices
   the contact can join, those with a coil first: click the `-K01` line (it shows `13/14`). The
   prompt now says `joins -K01 as 13/14`. Click to the right of the coil.
   Expected: `Added NO contact 13/14 to -K01 (now on page 1).`; the contact shows `-K01` and
   `13`/`14`. (Typing `D` at the prompt still works: the default device is shown as an option you
   can click, `[-K01/New]`.)
3. **Another page.** Make page 2 (**+** in the PROJECT panel). In
   Find Symbol type `-K01` and click the `-K01` row: the window lists what `-K01` can take, with
   the next labels. Click **NC contact … 21/22**, then click on page 2.
   Expected: `21`/`22` on page 2, still `-K01`.
4. **Hover** the contact on page 2. Expected: `Device -K01   Contactor / relay coil`, then
   `This symbol: NC contact 21/22, page 2 …` and an `Also:` line naming the coil and the NO
   contact on page 1.
5. **Rename.** Click **Device Properties**, click the contact, type `Tag` and Enter (or `T`),
   then `K5` and Enter. Expected: `-K01 is now -K05 (3 symbols on pages 1, 2).` All three show
   `-K05` (page 1 when you go back to it). Press Enter to finish, then type `u` and Enter: back to
   `-K01` everywhere.
6. **A tag that exists is never a dead end.** Place another coil (`-K02`). With **Device
   Properties** set its tag to `K01`: the prompt says
   `-K01 exists: -K01 (Contactor / relay coil, page 1 …). Make this the same device?
   [Yes/Choose another] <Choose another>:`. Enter keeps `-K02` and asks for another tag; `Y`
   makes it part of `-K01` (refused with a reason when `-K01` already has a coil). The same
   question comes while placing: at Place Symbol type `T`, Enter, `K01`: `Add this … to it?`.
7. **Erase and delete.** Erase the NC contact on page 2: `Removed -K01's NC contact 21/22
   (page 2); -K01 is still shown on page 1.` Erasing a device's last symbol deletes the device
   (`U` brings it back). **Delete Device** asks first: `Delete -K01 and its 2 symbols on page 1?
   [Yes/No] <No>:`.
8. **Save, close and reopen** (section 7): tags, properties and the device's symbols are all
   still there.

Typed instead, in the ECAD command field: `place {"symbol":"coil","at":[0,0]}`,
`place {"symbol":"no","at":[40,0],"device":"<the coil's component id>"}`,
`setprop {"component":"<id>","key":"tag","value":"K5"}`, `symbols {"query":"-Q"}`,
`devices {}`, `tagsettings {"padding":3}` (three-digit numbers from then on). A tag that exists is refused by `setprop` with the ways forward; add
`"merge":true` to make the device part of that one. `place` with a `tag` that exists adds the
symbol to that device.

## 5G. Devices tab (by clicking)

After 5F (a coil `-K01` on page 1 with contacts on pages 1 and 2). Design 016 §16.

1. **Open the tab.** In the PROJECT panel click **Devices** (beside **Pages**). Expected: `-K (1)`
   and `▸ -K01  Contactor / relay coil`.
2. **Unfold and select.** Click `-K01`. Expected: its symbols appear under it, each with its place
   on the sheet (`1-A-6  Contactor / relay coil`, `1-B-6  NO contact`, `2-A-6  NC contact`, page,
   column and row; brief 019), and its properties show at the bottom of the panel.
3. **Go to.** Click the `2-…` row. Expected: page 2 is shown in the drawing tab, centred on that
   contact, which is selected. Nothing is recorded.
4. **Edit.** Click the **Description** field, select all, type `Main contactor`, press Enter.
   Expected: the row reads `-K01  Main contactor`. Type `u` at the command line: it is back.
   **Location** `A1` is stored as `+A1`.
5. **Filter.** Type `q` in the filter: only `-Q` devices remain. Clear it.
6. **A tag that exists.** Select another device and set its **Tag** to `K01`. Expected: a red line
   says the tag is used by `-K01`, with **Make it the same device**; clicking it merges (one `u`), or
   explains why it cannot.

## 5H. Sheets and properties (by clicking)

On the demo project (`cargo xtask demo <file>.ecad`, then open it; pages `Supply`, `Motor control`
and `Field` with cable `-W01` continued from page 2 to page 3). Design 019 §13, with the owner's fix
round of 2026-11-08. Every write is one transaction: one `u` takes it back.

1. **The sheet.** Each page sits on an A3 sheet: the paper's bottom-left corner is the drawing's
   origin (0,0). A frame with columns `A`–`H` and rows `1`–`6` labelled on all four sides; the
   zones run over the whole frame, title block included (row `6` is the bottom strip with the
   title block). Type `z` Enter `e` Enter (zoom extents): the whole sheet fits. On page 2 the
   `-W01` reference reads `3-C-4`, on page 3 it reads `2-C-5` (page, column, row). The frame cannot
   be picked or edited: a click on one of its lines selects nothing and the command line says `The
   sheet frame is derived; change it in Project settings or the page's Sheet.`
2. **Number a page.** On page 2, click an empty part of the drawing (nothing selected). In the
   **Properties** palette (right), under `ECAD · PAGE 2`, click **Number**, select all, type `105`,
   press Enter. Expected: the title block's Page box says `105`, the panel's row says `105`, and
   page 3's reference now reads `105-C-5`. Type `u`: back to `2`. Set it again. A number another
   page has (`1`) is refused under the field. You can also double-click a page's number in the
   PROJECT panel.
3. **Project info.** Click **Project info** under the PROJECT panel's header. Fill **Title**,
   **Drawing no.** and **Customer**, pick **Status** `Draft` from the **v** list, click **OK**.
   Expected: the title block fills in; one `u` takes all of it back. **Cancel** writes nothing.
4. **Project settings.** Click **Project settings**. Under **References** choose `/105.C`, set
   **Digits** to `3`, click **OK**: the references read `/105.C` style and new tags get three
   digits; one `u` takes both back. Changing the sheet (Size `A2`) says it changes reference texts
   on every page.
5. **Folders.** On the **Pages** tab, click the folder icon at the right of the `Pages | Devices`
   row (**New folder**): a folder appears with its name ready to type; type `Schematic layouts`,
   Enter. Or right-click a page (**New folder**), the empty space under the pages (**New folder**)
   or a folder (**New sub-folder**). Drag `Motor control` onto the folder row: it moves inside and
   the folder reads `Schematic layouts (1)`. An empty folder shows `(empty)`. Click a folder row to
   fold or unfold it. Right-click a folder for **New page here**, **New sub-folder**, **Rename** and
   **Delete folder** (its pages move up a level, one `u`).
6. **Revision A.** Click **Revisions** under the PROJECT panel's header. In the pending row type
   Description `First issue` and Drawn your initials, pressing Tab or clicking away after each.
   Click **Issue Rev A…**, then **Issue**. Expected: the title block's Rev box says `A` and the table
   lists it with today's date. Save.
7. **Look back.** Draw a wire on page `1`. In Revisions click **Reopen** on Rev A, then close the
   Revisions window. Expected: the PROJECT panel shows a bar `Rev A, <date>: read-only` with
   **Back to current**; click page `1` in the panel: a preview titled `Rev A · page 1: Supply`
   shows the page without the wire. Click **Back to current**.
8. **Undo is careful with an issue.** Type `u` (the wire goes), then `u` again: a window asks
   `Rev A was issued on <date>. Undo the issue? This removes the release mark.` **Cancel** keeps
   it; **Undo the issue** takes it back.
9. **Device properties.** On page `105`, click the coil `-K01`: Properties shows
   `ECAD · DEVICE -K01` with its Tag, Description, Maker, Part number, Location, Function, This
   symbol and Where (`105-A-5`). Change its Description, Enter. Type `u`.

## 5I. Connectivity and wire numbers (by clicking)

On the demo project (`cargo xtask demo`, then open `C:\dev\ecad-projects\demo.ecad`; on Linux
`~/ecad-projects/demo.ecad`). Design 022. Select one wire to see its number, potential,
cross-section, colour and its net in the **Properties** palette (`ECAD · WIRE`); hovering a wire
shows the same facts.

1. **Labels.** Show page 2 (**Motor control**). Expected: every wire is labelled beside it, e.g.
   `24V-04 0.5 BU` on the three wires joining `-S02:14`, `-K01:14`, `-K01:A1` and the lamp's
   `X1`; vertical labels read bottom to top; a dot where the lamp's 0V wire meets the coil's, and
   `0V-01 0.5 BU` on both; the cable cores below the contactor show their number only
   (`L1-02`). Page 1 shows `L-01 2.5 BN`, `N-01 2.5 BU`, `PE 2.5 GNYE`,
   `24V-01 0.5 BU`, `0V-01 0.5 BU`. Colours are IEC 60757 codes (`BN`, `BU`, `BK`, `GY`,
   `GNYE`…); labels are small (1.6 mm, brief 031) and no two touch. Switch cable `-W01` to fan view
   (**Cable View**, click it): its cores' labels hide with the cores; back to traditional, they
   return. Nothing is recorded.
2. **Hover** the wire under the coil (the 0V wire). Expected, among the lens lines:
   `Wire number 0V-01`, `Potential 0V (continuation)`, `Cross-section 0.5 (0V) · Colour BU (0V)`
   and `Net 0V-01 · 5 wires · -G01:-, -H01:X2, -H02:X2, -K01:A2`.
3. **A wire into a net.** Click **Wire** (ECAD tab, Wiring) and draw from the lamp's `X1` to an
   empty spot, Enter. Expected: `Wire 24V-04 drawn; …`, and the new wire shows `24V-04 0.5 BU`;
   no other label changes. Type `u`.
4. **A new net, and a potential.** Draw a wire between two empty spots. Expected: a new number
   with no prefix, `01` (nets with no potential have none). Click **Wire** again, click
   **Potential** (or type `P`), type `24V`, then draw. Expected: `24V-06 0.5 BU`
   (24V-05 is on page 3). The choice stays for new wires in this session (`auto` clears it).
5. **A junction.** Draw a wire ending on the middle of another wire. Expected: while you point at
   the wire, the ring says `junction`; a dot appears and both wires show one number. Select the
   dot and erase it. Open the **Checks** tab (PROJECT panel, beside **Devices**): a row
   `▲ Looks connected, no junction`. Click the row (the page centres on it with a yellow ring),
   then **Add junction**: the dot is back. `u` takes it away again.
6. **Fix a number.** Select a 24V wire. In **Properties** (`ECAD · WIRE`) click **Wire number**,
   type `24V-50`, Enter (or click **Fix Wire Number**, click the wire, type it). Expected: every
   wire of that net shows `24V-50` and **Fixed** says `yes`; setting **Fixed** to `no` keeps the
   number as an automatic one. **Potential** and **Cross-section**/**Colour** are edited the same
   way: Potential sets the whole net, the others this wire only; empty returns to the default shown
   faint with its potential (`0.5 (24V)`). Now fix another net to `24V-50`: it is never a dead end;
   the prompt asks `24V-50 is used by another net. Swap, use another or merge? [Swap/Another/Merge]
   <Another>:`. Enter gives
   it the next free number (`24V-51`); `S` swaps the two numbers; `M` joins the nets with a pair
   of continuations named `24V-50` when that is electrically sensible, otherwise it says why (for
   example two different potentials).
7. **Checks.** In the **Checks** tab: `· Connection point not connected 3-C-5 -M01:PE`, and three
   `· Continuation without partner` rows (`L1`, `L2`, `L3` on page 2: the supply side of the motor
   circuit is not drawn). Click the `-M01:PE` row. Expected: page 3 is shown, the motor centred
   and selected, a grey ring on `PE`. Esc, or a click on empty space, takes the ring away. A
   continuation whose name is not in the potentials table (say `MOTOR-RUN`) is placed as usual
   and shows a yellow `Continuation name not a potential` row. **Show** lets you untick kinds of
   rows.
8. **Save, close and reopen** (section 7). Expected: the same numbers (they are stored on the
   wires, so they never change by themselves).

Typed instead, in the ECAD command field: `nets {"number":"24V-04"}`,
`wirenumber {"number":"24V-04","value":"24V-10"}`, `potential {"conductor":"<id>","value":"24V"}`,
`wiresettings {"scheme":"page"}` then `renumberwires {"scope":"all"}` (numbers become
`24V-2.01`…; fixed ones stay), `checks {}`, `junction {"at":[57.5,52.5]}` (on page 2),
`place {"symbol":"continuation","at":[0,0],"name":"24V"}`,
`wiresettings {"colour_style":"three-letter"}` (labels read `BLU`, `BRN`, `GRN/YEL`; back with
`"iec"`), `wiresettings {"label_height":2.5}` (the default is 1.6 mm since brief 031). By clicking, both are in
**Project settings** (under the PROJECT panel's header), section **Wires**: **Colours** `IEC  BN BU
GNYE` or `BRN BLU GRN/YEL`, and **Label height** in mm; OK writes them in one transaction. A colour may also be free
text (`setprop {"conductor":"<id>","key":"colour","value":"Violet stripe"}`).
`setprop` refuses `wire_number` and `potential` on a wire (use `wirenumber` and `potential`, which
change the whole net); it takes `cross_section` and `colour`, which only that wire shows.

## 5J. History and branches (by clicking)

On the demo project, opened from its file (section 7; branches live in the file, so a project
never saved says `Branches live in the project file. Save it first.` with **Save As…**).
Design 020 (020a: history, versions and branches; 020b: merge and update).

1. **The History tab.** In the PROJECT panel, click **History** (after **Checks**). Expected: the
   graph, newest at the top: `Now · main` (a ring on it: the branch shown), any revision issued in
   section 5H as a filled diamond (`Rev A`), and `Project created` at the bottom. Click **New
   version**, type `Before change`, **Create**. Expected: the echo `Version "Before change"
   created. Save to keep it.`, and a hollow circle `Before change` under `Now`. `u` takes it away
   without asking; redo brings it back. **Save**.
2. **Details and Show.** Click the `Before change` row. Expected, under the graph: `── Version
   "Before change"`, `Version on main`, its time and `you`, and `Since …: n steps`. Click
   **Show**. Expected: the bar `Version "Before change" · main · read-only` with **Back to
   current**; the Pages tab lists that state's pages; click a page: its preview opens read-only.
   **Back to current**. **Steps ▾** lists the steps before it, newest first (`#12  Wire  you …`);
   a step can be selected and shown the same way.
3. **A branch.** With `Before change` selected, click **Branch from here…**, name `Option B`,
   keep **Switch to the new branch** ticked, **Create**. Expected: `Branch "Option B" created from
   Before change. Now on branch "Option B".`; the window title ends `demo.ecad · Option B`; the
   header line starts `Branch Option B`; a bar `Branch "Option B" · from Before change` with
   **Merge into main…** and **Update from main…** (steps 9–10); in the graph
   the Option B line starts at `Before change`, drawn in the accent colour down to the root (the
   line you are on). Select `-K01` and change its description in **Properties**.
4. **Switch back.** In the History tab's **Branch** list, pick `main`. Expected: `Option B has 1
   unsaved change. They are saved, then the project shows "main". [Save and switch] [Cancel]`.
   **Save and switch**. Expected: `Now on branch "main". Undo works on this branch's own steps.`;
   `-K01`'s description is as before; `u` undoes main's own last step, never Option B's.
5. **Revisions on a branch.** Switch to `Option B` again and open **Revisions**. Expected:
   **Issue Rev …** is disabled with `Revisions are issued on main. Merge "Option B" into main
   first.`, the pending row's fields are read-only, and **Merge into main…** opens the merge
   dialog (step 9).
6. **Restore.** On main, make two edits (draw two wires), then select `Before change` and click
   **Restore…**. Expected: `Make the project on main as it was at Version "Before change"? 2
   objects change, on page 1. Nothing is deleted from the history: this is one new step, and Undo
   takes it back.` (and `It lands after Rev A…` when a revision lies between). **Restore**.
   Expected: `Restored "Before change" (2 changes). Undo takes it back.`; the two wires are gone,
   the graph still has every row; `u` brings them back.
7. **Rename and archive.** On Option B, **⋯ > Rename branch…** (`Option C`), then **⋯ > Archive
   branch…**: it says the project first shows main; **Archive**. Expected: the branch is hidden;
   **⋯ > Show merged and archived branches** shows it faint, `Archived "Option C"`; select its row
   and **Reopen branch**.
8. **Save, close and reopen** (section 7). Expected: the project opens on the branch you were on,
   and the History tab shows the same graph. **⋯ > Show automatic snapshots** adds faint
   `Auto · <time>` rows (one every 2 000 steps, so few on the demo, plus one at a branch's first
   save).
9. **Merge into main.** Make a branch `Option B` from Now (step 3), change `-K01`'s description
   there (`Pump 1`) and draw a wire. Click **Merge into main…** in the branch bar. Expected: a
   dialog `MERGE "Option B" INTO main`: `This switches to main (saving "Option B" first) and
   merges into it.`, `Based on …` (the point the branch started from, and `main has since issued
   Rev …` when a revision was issued on main after it), `2 changes from the branch · page … · 0
   conflicts`, the changes (`~ -K01 · Description "…" → "Pump 1"`, `+ Wire … added`), `Close
   branch "Option B" after merging` ticked. Nothing is written yet: **Cancel** leaves everything
   as it was. **Merge**. Expected: the project shows main, the echo `"Option B" was saved; now on
   main. Merged "Option B" into main (2 changes). Save to keep it.`, `-K01`'s description is
   `Pump 1`, and the graph has a row `Merged "Option B"` on main's line with a line from Option B's
   head into it. **Save**: Option B is closed (it shows only with **Show merged and archived**).
   Issue Rev B now if you like (section 5H): the merge is in it.
10. **A conflict.** Make a branch `Option C`, set `-K01`'s description to `Branch pump`; switch to
    main (**Save and switch**), set it to `Main pump`; switch back to Option C and click **Merge
    into main…**. Expected: `1 conflict`, the row `▲ -K01 · Description`, `main: Main pump
    branch: Branch pump`, choices **Keep main**, **Keep branch**, **Edit [Branch pump]**, none chosen,
    and **Merge** disabled with the hover `Choose for 1 conflict first, or use Keep main for all /
    Keep branch for all.` Pick **Keep branch** (or type another value in Edit and pick Edit).
    **Merge**: `-K01` is `Branch pump` on main.
11. **Undo the merge.** Press `u` right after the merge. Expected: `Undo the merge?` with `The last
    step merged "Option C" into main (1 change). Undo the merge? "Option C" is reopened.`
    **Undo merge**: main's `-K01` is back to `Main pump`; after **Save**, Option C is open again.
    Redo merges again without asking.
12. **Update from main.** On a branch, after main has changed, click **Update from main…** in the
    branch bar. Expected: `UPDATE "<branch>" FROM main`, `Brings main's changes since … (… steps)
    into this branch. Issued revisions are not changed.`, sides named `main` and `this branch`, no
    Close box. **Update**: one step on the branch, `Updated "<branch>" from main (…). Save to keep
    it.` A later Merge into main compares against that update, so main's changes are not
    conflicts again.
13. **Rename with F2.** On a branch, press **F2** with the History tab shown (or double-click the
    Branch list's name): the Rename dialog opens. A Now row's right-click offers Switch, Merge
    into…, Update from… (the branch shown), Rename and Archive.

Typed instead, in the ECAD command field: `version {"name":"Before change"}` (or
`version {"name":"Before change","description":"before the cable rework"}`),
`history {}`, `steps {"limit":10}`, `pointinfo {"version":"Before change"}`,
`showpoint {"version":"Before change"}` / `closepoint {}`,
`restore {"version":"Before change","preview":true}` (the Restore question, nothing recorded),
`restore {"version":"Before change"}`,
`branch {"name":"Option B","from":{"version":"Before change"}}`,
`switchbranch {"branch":"main","save":true}` (without `save` it is refused while changes are
unsaved), `renamebranch {"branch":"Option B","name":"Option C"}`,
`archivebranch {"branch":"Option C"}`, `reopenbranch {"branch":"Option C"}` (when another open
branch took its name: `reopenbranch {"branch":"Option C","name":"Option C (2)"}`; by clicking, the
dialog offers that name), `branches {}`. Typed branch commands update the title, the branch bar,
Revisions and the History tab exactly as clicks do.
Merging typed: `merge {"from":"Option B","preview":true}` (the dialog's preview as JSON, nothing
recorded; works with writes off), `merge {"from":"Option B"}` (into its parent: switches there
first, saving; refused with each conflict's ID when one has no choice),
`merge {"from":"Option C","choices":{"<id>":"branch"}}` (`main`, `branch`, `loose`, `bringback`,
`renumber_branch`, `renumber_main` or `{"value":"text"}`; `"close":false` keeps the branch open),
`merge {"from":"main"}` on a branch (update from main), `undomerge {}` (what **Undo merge** runs;
a plain `u` of a merge is refused, saying so), `history {"limit":400}` (more rows).
On a branch, **Revisions** has **Switch to main** (Save and switch applies); archiving the branch
shown when its parent is archived too shows main, saying so.

## 5K. Outputs (by clicking)

On the demo project (`cargo xtask demo`, then open it, section 7). Design 024. The demo's project
location is `+CP1`, page 3 (**Field**) is `+FIELD`, the motor terminals `-X02` sit in location box
`+JB1`, cable `-W01` is a `4G1.5`, and the book ends with a generated **Revision list** and **BOM
by location**. The trial has no file dialogs: where a file is asked for, type its path.

1. **Where things are.** Show page 2 and click `-K01`'s coil. Expected: **Properties**
   (`ECAD · DEVICE -K01`) shows **Location** faint, `+CP1 (project)`. Type `+CP2`, Enter: it shows
   `+CP2` (one step); `u` returns it to `+CP1 (project)`. Hover the coil: a line
   `Location: +CP1 (project)`. Show page 3: `-K01`'s NC contact reads `+CP1-K01` (it is drawn in
   the field; the device stays in `+CP1`, from its coil), and `-X02` shows `+JB1 (from box on 3)`.
2. **A location box.** On page 3 click **Location Box** (ECAD tab, Components), type `+JB2`,
   Enter, and click two corners round the motor `-M01` (below the terminals). While you drag, the
   motor gets a green ring. Expected: `Location box +JB2: 1 device inside, now in +JB2.`; click the
   motor: **Location** `+JB2 (from box on 3)`. Select the box's dashed outline and drag a corner
   grip so the motor is outside: at once `+FIELD (from page 3)`. Type `u` twice. Again with
   **Location Box**, `+JB3`, then `P` at the first prompt: click five points round the motor, then
   `C`. A box drawn smaller than 10 x 10 mm is refused and the prompt stays open.
3. **An own location in a box.** Click `-X02`, type `+CP9` in **Location**, Enter. Open the
   **Checks** tab: `▲ Own location differs from its box  -X02 is +CP9 but sits in box +JB1`, with
   **Use the box's location**. Click it: `-X02` is `+JB1 (from box on 3)` again (one step).
4. **Reports.** Click **Reports** (the table button in the PROJECT panel's header). Expected: tabs
   `BOM (n)`, `Wires (n)`, `Cables (1)`. The BOM lists devices only, grouped by location
   (`+CP1`, `+FIELD`, `+JB1`); `-S01`'s Maker and Part number are yellow `?`, and `n ?` counts
   them. Click the `-S01` row: page 2 is shown with the stop button selected; set its **Maker** in
   Properties: the `?` goes at once. **Group v By part** groups the whole project. **Wires** shows
   each conductor with both ends (`-Q02:2`), their locations, cross-section and colour, the cable
   cores with their cable (`-W01 core 1`). **Cables**: `-W01  4G1.5  3  +CP1  +JB1  —  —`.
5. **Export.** On the BOM tab click **Export CSV…**, check the path (`…\demo-bom.csv`), **Save**.
   Expected: `Wrote n rows to … (m cells marked ?).` Open it in a spreadsheet: tags read `-K01`
   (written `'-K01`, so the spreadsheet does not take them for formulas).
6. **Generated pages.** In the PROJECT panel click **v** beside **+** and choose **Cover**, then
   **Contents**; with each new page selected, click the up arrow until it is first (cover) and
   second (contents). Show **BOM by location** (the last page): the table of step 4, one block
   per location. Try to draw a line on it: `Page 5 is generated (BOM by location). Draw on
   another page, or add one after it with +.`
7. **The book.** Click **Print Book** (the plot button), keep **All pages**, check **File**, then
   **Print**. Expected: `Printed n sheets (4 generated) to …\demo.pdf (… KB).` Open the PDF:
   cover, contents, the three drawing pages, the revision list and the BOM, each an A3 sheet with
   its frame and title block, black on white. **Pages** `1, 3` prints just those; an unknown
   number is listed under the field and left out. A device without a part number shows `?` in a
   box in the BOM, black in the PDF (brief 033); the file is a few hundred KB, not megabytes.

Typed instead, in the ECAD command field: `locationbox {"location":"+JB2","corners":[[110,60],[150,90]]}`,
`setprop {"component":"<id>","key":"location","value":null}` (back to inherited),
`bom {}`, `bom {"group":"part"}`, `wirelist {}`, `cablelist {}`, `bom {"path":"C:/tmp/bom.csv"}`,
`newpage {"generated":"contents"}`, `printbook {"path":"C:/tmp/demo.pdf","scope":{"pages":"1, 3"}}`.

## 5L. Cross-references (by clicking)

A contact says where its coil is, and a coil lists its contacts in a **contact table** under it,
including those the part offers that nobody has drawn yet. The texts are derived and stay live:
nothing is stored for them. Brief 025, design `ecad/docs/design/025.md`. Start a new project (or
use any project; the demo's `-K01` already shows them on pages 1 and 2). Open the **ECAD** tab.

1. **Numbers and a coil.** With nothing selected, set page 1's **Number** to `100` and its
   **Title** to `Supply` in the Properties palette. With **Find Symbol**, place a `coil` as a new
   device `-K01` at about the middle of the sheet's upper half. Click **Wire** and draw from the
   coil's `A2` straight down 20 mm, then a horizontal rail across under it.
   Expected: no table yet (the coil has no contacts and no offer).
2. **Contacts on two pages.** Find `power3`, click the `-K01` line and place it right of the coil.
   Make a page with **+**, number it `105`, title `Motor control`; place a `no` and an `nc`, each
   joined to `-K01`. Expected: under each contact, just right of its lower wire, `100-B-3` (the
   coil's page, column and row; yours may differ with where you clicked). Show page 100: under
   the rail a table of five rows: `1`/`2`, `3`/`4`, `5`/`6` with the power contacts' place,
   then (a little gap) `13`/`14` and `21`/`22` with their places on 105.
3. **Follow a reference.** Click **Go to Reference** (ECAD tab, Wiring), then the `100-B-3` text
   under the NC on page 105. Expected: page 100 comes up, centred on the coil, which is ringed and
   selected; `Page 100 "Supply", -K01's coil (100-B-3).` Select the coil: the Properties palette
   lists **Contacts** with a **Go to 21/22** button; click it: back to the NC on 105.
4. **The part's contacts.** With the coil selected, set **Contacts offered** to `3P+2NO+1NC` and
   press Enter. Expected: a grey row `23`/`24` with `—` appears (offered, not drawn). Type `u`:
   it goes; set it again. Typing `3X` is refused under the field:
   `"3X" is not a contact list. Write it like 3P+1NO+1NC, or 13/14 NO, 21/22 NC.`
5. **One too many.** On 105 place two more `no` contacts joined to `-K01`. Expected: the first
   takes `23`/`24` (the spare row turns into a drawn one); the second takes `43`/`44`, its row is
   red, the echo says `… its part offers 2 auxiliary NO, now 3 drawn (see Checks).` and the
   **Checks** tab lists `More contacts than the part offers` (`-K01: 3 auxiliary NO drawn, part
   offers 2 (3P+2NO+1NC)`). Click that row: page 105, the extra contact ringed. Type `u`.
6. **Move the table.** Click **Move Mirror Table** (ECAD tab, Components), click the coil (or its
   table), then click a new place: the table follows the cursor while you choose. Move the coil
   with MOVE: the table moves with it. In Properties set **Contact table** to `Off` (the table
   goes; the texts under the contacts stay), then to `Project default (auto)`. Each is one `u`.
   **Project Properties → References → Contact tables** sets the default for every coil.
7. **Renumber.** Set page 105's **Number** to `106`. Expected: the two auxiliary rows on page 100
   read `106-…`; the texts on 106 still read `100-B-3`. Type `u`.
8. **An MCB with an auxiliary contact.** Place an `mcb1` (`-Q01`) and join an `nc` to it: the NC
   shows the MCB's place and the MCB shows a one-row table.

Hovering a contact adds `Coil: 100-B-3  (page 100 "Supply")` to its lens; hovering a coil adds
`Contacts: 5 drawn, 1 spare (3P+2NO+1NC)` and its rows; hovering a table row describes that pole.
The texts and the table cannot be edited or erased on their own (`The contact table is derived;
move it with Move Mirror Table, or set it in Properties.`); a window move of a coil and its table
moves the coil and draws the table again.

Typed instead, in the ECAD command field: `setprop {"component":"<id>","key":"contacts","value":"3P+1NO+1NC"}`,
`setprop {"component":"<id>","key":"mirror","value":"below"}` (`auto`, `below`, `right`, `off`,
`at dx,dy` in millimetres from the coil; `null` follows the project),
`mirrormove {"component":"<id>","at":[x,y]}` (or `"placement":"off"`), `contacts {"component":"<id>"}`
(the table as JSON), `xrefs {"component":"<id>"}` (every reference, one per pole; also `cable`,
`page`), `refgoto {"source":"<representation id>"}` (a contact goes to its coil; from a coil add
`"target"`), `projectprops {"contact_mirror":"off"}`.

## 5M. Numbering (by clicking)

On the demo project (`cargo xtask demo`, then open `C:\dev\ecad-projects\demo.ecad`). Design 026.

1. **Renumber in drawing order.** Show page 2 (**Motor control**). Place two coils near its right
   edge and one near its left edge (**Place Symbol**, `coil`). Click **Renumber…** (ECAD tab,
   **Numbering**), choose **Dev** and **Prefix** `-K`. Expected: the list shows `Old → New At` for
   every `-K` device, page by page and column by column (`page · col · row`): the left coil gets the
   lower number. **Apply**: one step; `u` puts every tag back; redo.
2. **Fixed.** Select `-K01`'s coil. In **Properties** (`ECAD · DEVICE -K01`) set **Fixed** to `yes`
   (or tick **Fixed** in the Devices tab). Renumber again: `-K01` shows `F` and keeps its tag; the
   others are numbered around it. Typing a tag in Properties fixes it too.
3. **Swap.** Select `-K03`'s coil, type `-K02` as its **Tag**, Enter. Expected: under the field,
   `-K02 is used by -K02 (…)` and three buttons: **Swap** (first), **Make it the same device**, **Use
   -K0n**. Click **Swap**: both tags change in one step; `u` undoes both.
4. **Provisional tags.** **Project settings** (PROJECT panel header) → **Numbering** → tick
   **Provisional tags until Finalise**, OK. Place a coil: it is `-K?1`, drawn orange. Draw a wire
   between two empty spots with potential `24V`: it reads `24V-?`, orange. Slash two wires into a
   cable: `-W?1`. The **Checks** tab shows `▲ Provisional tag` per device and cable, and one row
   `▲ n wire numbers provisional`, each with **Finalise**.
5. **Issue with provisional items.** Open **Revisions…**, fill Rev, Description and Drawn, **Issue
   Rev …**, **Issue**. Expected: a window lists the provisional items with **Finalise numbering
   now**, **Issue anyway** and **Cancel**. **Finalise numbering now** opens Finalise numbering (all
   kinds, Prov, Start `next`); **Apply 3** numbers them after the highest in use, then the issue
   follows. Two steps: `u` twice takes both back.

Typed instead, in the ECAD command field: `renumber {"kind":"devices","scope":"all","preview":true}`,
`renumber {"kind":"devices","scope":{"prefix":"-K"},"start":1,"digits":2}`, `finalise {}`,
`numbering {"provisional":true}`, `setprop {"component":"<id>","key":"tag","value":"-K02","conflict":"swap"}`
(`"merge"`, `"next"`), `setprop {"component":"<id>","key":"tag_fixed","value":null}`,
`issuerevision {"provisional":"finalise"}` (or `"issue"`), and in a merge `choices: {"<id>": "one"}`.

## 5N. Release (by clicking)

One action checks the project, issues the pending revision and writes a folder that can be sent:
the PDF book and every list, stamped with that revision. Brief 027, design
`ecad/docs/design/027.md`. On the demo project (`cargo xtask demo`, then open it, section 7); the
project must have been saved once, since the folder goes beside it (`…\release\Rev A\`). (Section
5M is brief 026's numbering; the design calls this section 5L.)

1. **The check.** Click **Release** in the PROJECT panel (beside **Revisions**; once the Checks
   tab has been shown it carries a badge: red `●n` for errors, else yellow `▲n` for warnings).
   Expected: a window `RELEASE REV A` with the pending revision's fields, the folder, a **Writes**
   line (`demo Rev A.pdf · bom · wires · cables · terminal-plan · cable-schedule · checks`; a
   project with no strips adds `terminal-plan: No terminal strips in this project.`), then the
   check by group: **Open ends**, **Orphans**, **?
   parts** (`-S01` has no part number) and **Other** (folded). Click a row: its page comes up with
   the place ringed; the window stays open. To see an orphan, place a **Cable Reference** on `-W01`
   (section 5D) without continuing it: at once **Orphans** lists `▲ Cable reference with no other
   end … -W01 /?`; type `u` and it goes.
2. **Release.** Fill **Description** (`First issue`) and **Drawn** (your initials) in the window;
   **Release Rev A** is greyed until both are there (hover: `Fill in Rev, Description and Drawn
   first.`). Click **Release Rev A**. Expected: the window closes and the command line says
   `Rev A released to …\release\Rev A: n sheets, 5 lists, … listed.` The project is saved. Open the
   folder: `demo Rev A.pdf` (the title blocks' Rev box reads `A`, and the Revision list page lists
   Rev A), `demo Rev A BOM.csv`, `… wires.csv`, `… cables.csv`, `… terminal plan.csv` and `… cable
   schedule.csv` (brief 028), `… checks.csv` (every row of the
   check, with its group) and `release.json` (the revision, the log point, the app and each file's
   SHA-256).
3. **Revisions.** Open **Revisions**: Rev A's new **Folder** cell reads **Open** (opens the folder in
   the file manager). Beside **Issue Rev B…** there is **Release Rev B…**.
4. **Again.** In Revisions click Rev A's **Folder** cell after renaming its folder away, or click
   **Write…** on a revision issued without a folder: `WRITE REV A'S FOLDER` writes it from Rev A as
   issued and records nothing. Doing it while the folder exists asks first: **Add missing files**
   (only when the rest is the same), **Replace** (the old folder is moved to
   `release\replaced\Rev A <date time>`, never deleted), **Keep both** (`Rev A (2)`), **Cancel**.
5. **An error.** Join a `24V` and a `0V` continuation with one wire: the check shows **Errors (1)**
   and Release is greyed; tick `Release with 1 error (it is listed in the folder's checks.csv)` to
   go on. `u` after a release undoes the issue (it asks first); the folder stays on disk.

Typed instead, in the ECAD command field: `prerelease {}` (the check as JSON, with what `release`
would write or refuse), `release {}`, `release {"accept":"errors"}`, `release {"rev":"A"}` (write an
issued revision's folder again; records nothing), `release {"rev":"A","existing":"keep-both"}`
(`add-missing`, `replace`), `release {"folder":"C:/tmp/out/Rev B"}`.

## 5O. Terminal strips and cables (by clicking)

A terminal strip `-X01` is one device whose terminals `-X01:1…n` are placed on wires; a cable
knows its cores. The terminal plan and the cable schedule are read off the drawing, live, and
never stored. Brief 028, design `ecad/docs/design/028.md`. Use any project (the demo after 5N, or
a new one); the steps make their own page so its numbers do not matter.

1. **Some wires.** Add a page with **+**. Place two `coil`s (`-K..`) near the top and a `motor3`
   near the bottom, then draw three vertical wires with **Wire** from the coils' `A2` and from a
   free point down to the motor's `U1`, `V1`, `W1`. With **Cable Slash**, cut the three wires low
   down, near the motor.
2. **One terminal.** **Find Symbol**, type `terminal`, and click on the first wire, between the
   coil and the slash. Expected: the wire is cut and both halves land on `-X01:1`
   (`Placed -X01:1 on wire … (split in two).`); both halves keep one wire number. Hover the
   terminal: `-X01:1  feed-through  …`, an `inside  -K..:A2` line and an
   `outside  -W.. core … → -M..:U1` line. Type `u`: one wire again and no `-X01`; place it again.
   Clicking the wire's end at the coil is refused: `That is the end of the wire at -K..:A2. Click
   along the wire, away from its ends, or on empty space.`
3. **A row of terminals.** Click **Place Terminals** (ECAD tab, Components), press Enter (the last
   strip, `-X01`), and draw a line across the other two wires at the height of terminal 1.
   Expected: a ring and its label at each crossing while you draw; then `Placed -X01:2…3 on
   2 wires.`, one `u`.
4. **A bridge.** Click **Draw Bridge**, click terminal 2, then terminal 3: a staple 4 mm above
   them (`Bridged -X01:2–3.`). The next click would continue from 3; press Enter to end.
5. **Spares.** Click a terminal; in Properties set **Terminals offered** to `8+1PE`. Expected:
   **Terminals** reads `3 drawn, 6 spare` (8 feed-through and 1 PE: 9 positions). Typing `8X` is refused under the field:
   `"8X" is not a terminal count. Write 12, or 12+2PE.`
6. **Cores.** Click the cable slash; set its **type** to `7G1.5`. Expected: **Cores** shows
   `from type: 7G`, **Used / Spare** `3 / 4 (4, 5, 6, GNYE)`. Hover the slash:
   `-W..  7G1.5: 3 used, 4 spare (4, 5, 6, GNYE)`.
7. **Reports.** Open **Reports**. **Terminals**: every position of `-X01` (pick `-X01` or `All`
   above the table), with the spare ones; click a row to go to its terminal. **Cables**: Cores 7,
   Used 3, Spare `4 (4, 5, 6, GNYE)`, From `… -X01`, To `… -M..`; double-click it for its core
   list. **Show plan** in Properties (strip) and **Show cores** (cable) open the same tabs.
8. **Pages.** **v** beside **+** ▸ **Terminal plan** and **Cable schedule**: generated pages with
   the plan (terminal boxes, bridge dots and line, the cable written once per run of its cores)
   and the schedule.
9. **Checks.** Set **Terminals offered** to `2`: the Checks tab lists `More terminals than the
   strip offers` (`-X01: 3 terminals drawn, strip offers 2`) with **Offer drawn**, which sets it
   back to `3`.
10. **Renumber.** Drag terminal 3 (its ring) left of terminal 1; click **Renumber…** (ECAD tab,
    Numbering), choose **Trm**, **All**. Expected: the list shows `-X01:3 → -X01:1` and the
    others shifted, in drawing order; **Apply**: one step, the wires stay on their terminals;
    `u` puts the labels back.

Typed instead, in the ECAD command field: `place {"symbol":"terminal","at":[x,y]}` (on a wire: it
is split), `terminals {"from":[x,y],"to":[x,y]}` (add `"strip":"<id>"`, `"pe":true`),
`bridge {"from":"<terminal id>","to":"<terminal id>"}`,
`setprop {"component":"<strip id>","key":"terminals","value":"12+2PE"}`,
`setprop {"cable":"<id>","key":"cores","value":"7G"}`, `stripplan {}`, `cablecores {"cable":"<id>"}`,
`newpage {"generated":"terminals","strip":"<strip id>"}`, `newpage {"generated":"cables"}`,
`renumber {"kind":"terminals","preview":true}`, `renumber {"kind":"terminals","scope":"selection","strips":["<strip id>"]}`.

## 5P. Strip accessories and jumpers (by clicking)

End stops, end plates, partitions, strip labels and markers are placed by you, where you want
them; jumpers are links owned by the strip, never wires. They appear in the terminal plan in
position order and on the BOM. Brief 030, design `ecad/docs/design/030.md`. Continue from 5O
(strip `-X01`, terminals 1–3, offer `8+1PE`).

1. **Parts.** Open **Project info** and add the properties `strip_end_stop_part_number` `ES-1`
   and `strip_end_plate_part_number` `EP-1` (Properties of a strip also shows them, faint, under
   **End stop part number** and **End plate part number**).
2. **Ends.** Open **Reports ▸ Terminals**, pick `-X01`, **Accessory v ▸ At start: end stop**, then
   **At end: end plate** and **At end: end stop**. Expected: an `end stop` row before position 1
   and `end plate`, `end stop` rows after the last position; their Place column shows `ES-1`/`EP-1`.
   Untick **Accessories** to hide those rows (view only).
3. **A partition.** Click **Strip Accessory** (ECAD tab, Components), click terminal 2, press Enter
   (Partition). Expected: `Partition after -X01:2.`; a `partition` row after 2.
4. **Markers.** **Accessory v ▸ Marker on every position**: each position shows `▭`. Without a
   marker part number the BOM lists none, and Checks counts them (`… strip accessories have no part
   number`).
5. **Jumpers.** Click **Jumper**, click terminal 1, then terminal 3, press Enter: a dashed line
   through 1–3 (`Linked -X01 1-3.`), and 1–3 share one wire number. Hover the line:
   `-X01 link 1-3 (3 positions, 1 bar …)`. Click it: Properties shows **Link** with its
   **Positions**; type `1-5` to extend it (positions 4 and 5 are spare: a short stub past 3 says
   the link goes on). Erase the line to delete the link; `u` brings it back.
6. **BOM.** **Reports ▸ BOM**: end stops `ES-1 ×2`, end plate `EP-1 ×1`, partition (`?`), and
   `Jumper bar ×10` rows per strip.

Typed instead: `accessory {"strip":"<id>","at":"start","items":["end_stop"]}`,
`accessory {"strip":"<id>","at":["1","2","3"],"on":true,"items":["marker"]}`,
`accessory {"strip":"<id>","at":"2","remove":["partition"]}`,
`jumper {"terminals":["<terminal id>","<terminal id>"]}` (`"skip":true` for only those),
`jumper {"strip":"<id>","positions":"7, 9, 11","skip":true}`,
`jumper {"link":{"strip":"<id>","n":1},"add":"5"}` (also `remove`, `split_at`, `delete`),
`setprop {"component":"<id>","key":"jumper.bar","value":"4"}`, `stripplan {"accessories":false}`.

## 5-031. Text styles (by clicking)

Brief 031, design `ecad/docs/design/031.md`. Sizes and the font are one table per project.

**The font.** The default font, BIZ UDPGothic, comes from the craft-fonts repository and is built
in only when the build is told where it is. To see it, clone `storytold/craft-fonts` next to this
checkout and build with `CRAFT_FONTS_DIR` set to that folder (PowerShell:
`$env:CRAFT_FONTS_DIR = "C:\dev\craft-fonts"`; bash: `export CRAFT_FONTS_DIR=~/craft-fonts`).
Without it, texts are drawn in CADCraft's stroke font and bold tags are drawn with a heavier line;
nothing else changes, and the project's stored font is kept.

1. Place a `coil` (5F). Expected: its tag `-K01` is small and bold (2.5 mm) and its `A1`/`A2`
   are smaller still (1.5 mm).
2. **Project panel ▸ Project settings**, scroll to **Text**. The line under **Font** says `drawn
   with BIZ UDPGothic`, or, in yellow, why another font draws.
3. Set **Tag** to `3`, untick its **Bold**, OK. Expected: the tag redraws at once, larger and
   regular; one `u` puts both back. A height of `11` shows `A text height is 0.5 to 10 mm.` and
   OK is disabled. **Defaults** puts every row back (written only on OK).
4. **Once per run.** Place a `terminal`, then nine more of the same strip in a row 10 mm apart
   (**Place**, answer the device prompt with the strip). Expected: `-X01` is drawn once, over the
   leftmost terminal. A group of the same strip further along the line (more than 15 mm between
   boxes) prints it once more.
5. **Show/Hide Text** (ECAD tab ▸ **Text**): click a coil's tag. Expected: it disappears and the
   command line says `Tag -K01 hidden on this symbol.`; type `D` (Device), `P` (Page) or `PR`
   (Project) first to hide it wider. Click the symbol to show its hidden tag again. Hover the
   symbol: the lens lists `Tag -K01 (hidden here)`. Esc or Enter ends the tool.
6. **Show Hidden Text** (ECAD tab ▸ **Text**): hidden tags and labels are drawn dimmed (grey);
   click it again to stop. It records nothing and is never printed. Properties of a device, a
   symbol or a page has a **Texts** group (`Default (shown)`, Shown, Hidden, Once per run).
7. **Placement.** Place two `no` contacts 10 mm apart and draw a wire through where a tag sits:
   the tags move to the next free place (left top, left middle, right, above, below) and never
   overlap each other while a free place exists.
8. **Move Text** (ECAD tab ▸ **Text**): click a tag, then click where it goes (`J` first to type
   its alignment, e.g. `LT`). Expected: `Tag -K01 moved. Reset puts it back.` Move the symbol: the
   tag follows. Start Move Text again, type `R` (Reset) and click the tag: placed automatically
   again. `text {"at":[100,100],"text":"X","justify":"MC"}` now draws X centred (older builds
   cannot open a project with such a text).

Typed instead: `textstyle {}` (reads: the font, what draws it, every kind),
`textstyle {"kind":"tag","height":3,"weight":"regular"}`, `textstyle {"font":"Arial"}`,
`textstyle {"kind":"wire_label","height":2}` (the same value as **Wires ▸ Label height**),
`defaults {"group":"Text"}` (every registered default, its value and where it comes from).
`showtext {"kind":"tag","show":"off"}` (the project; add `"page"`, `"component"` or
`"representation"` for narrower), `showtext {"kind":"description","show":"on","component":"<id>"}`,
`hiddentext {}` (toggles), `pagetexts {}` (every derived text on the page, hidden ones too, with
`moved` and `overlaps`), `movetext {"representation":"<id>","kind":"tag","to":[130,170],"justify":"LT"}`
(`"reset":true` instead of `to` puts it back).
`projectprops`, `pageprops` and `setprop` refuse `text.` keys.

## 5-032. Wires join, cables cross pages, wire labels (by clicking)

Brief 032, design `ecad/docs/design/032.md`. Checked by the automated tests named in the design
(§12); the steps below have not been tried by the owner yet.

1. **Wire** tool: draw a wire from `0,0` to `50,0`, Enter; then start **Wire** again and click
   on the first wire's open end, then `50,-20`, Enter. Expected: the echo says `end A joins wire …
   (now one wire)`; the two parts are one wire with one label; Checks has no Open end row for the
   corner. While the cursor is over an open end the Wire tool shows a green ring and `join`.
2. Type `J` while drawing: the prompt reads `Specify next point (join off) [Undo/Join]:` and ends
   that meet another wire stay open. Expected after the wire: a yellow ring on the open end, a
   **Looks connected** row in Checks with **Join**; **Join** makes them one wire.
3. **Join Wires** (Wiring tools): click where two open ends meet. Expected: one wire; on an open
   end lying on a wire's middle, a junction.
4. A wire drawn from a `0V` wire's open end with potential `24V` is refused: `That wire carries
   0V and this one 24V; joining them would short them. …`; nothing is drawn.
5. Cable across pages: on one page wires from a terminal to continuations `A1…A3`, slashed as
   `-W04`; on another page continuations `A1…A3` and wires on. Slash them there with tag `-W04`.
   Expected: no new cable; Reports ▸ Cables shows `-W04` with 3 cores from the first page's
   terminals to the second's.
6. **Project panel ▸ Project settings ▸ Wires**: **Labels** `One per net per page` (default) or
   `Every wire`, and **Shortest labelled wire** (enabled with Every wire). Expected: with the
   default a net is labelled once per page; Every wire with 10 mm leaves short drops unlabelled.

Typed instead: `wire {"points":[[0,0],[50,0]]}`, `wire {"points":[[50,0],[50,-20]],"join":false}`,
`joinwires {"at":[50,0]}`, `cableslash {"from":[50,-5],"to":[50,25],"tag":"-W04"}`,
`wiresettings {"label_rule":"each","label_min_length":10}`. `defaults {"group":"Wires"}` lists
`wire.join`.

## 5-033. Print and project (by clicking)

Brief 033, on `feature/print-project`. Start from the demo project of section 5K (it has a BOM
page and devices without part numbers).

1. **Book size.** Click **Print Book**, keep **All pages**, check **File**, **Print**. Expected:
   `Printed n sheets (… generated) to … (… KB).`: a few hundred KB, not megabytes. Each PDF page
   looks as before.
2. **Missing values.** Show the BOM page: a missing part number is `?` in a thin yellow box. Print
   Book with **Colour: Screen**: in the PDF the `?` and its box are black.
3. **Contact table and the title block.** On a drawing page place a coil low on the sheet (just
   above the title block) with two contacts on another page. Expected: its contact table sits to
   the right of the coil, or is lifted above the title block; never over it. Move the table into
   the title block with **Move Mirror Table**: it stays there, and Checks lists `Contact table over
   the title block` with **Move table…**.
4. **Continuation numbers.** Number the BOM page `400` (Pages list, double-click its number). With
   enough devices for two sheets, the second sheet's Page box reads `401` and the first sheet's
   Next reads `401`; Contents lists `400–401`. Typing `401` as another page's number is refused:
   `Number 401 is sheet 2 of page 400 (BOM by location).` Print Book **Pages** `401` prints the
   whole BOM page and says why.
5. **`table`.** Type `table` on a drawing page and place it: refused, `Tables can't be placed on
   ECAD pages yet. Draw it with lines and text, or add a generated page for lists (New Page ▸
   Generated).` Nothing is added.

6. **Tag style.** Project settings ▸ Tags: untick **Leading - on tags** (the line under it reads
   `k5 is written K05`), **OK**. Every device and cable tag loses its `-` in one step (`Tag style:
   no leading -. n tags rewritten.`); one **u** puts them all back. **Case ▸ As typed**: tags
   typed from then on keep their letters (`M02a`); existing tags stay upper case.
7. **Revision columns.** Project settings ▸ Revisions ▸ **Columns**: type
   `rev, date, ecr:ECR, description, drawn`, **OK**. The title block's revision table now has an
   ECR column and no Checked or Approved. In the Revisions window the pending row has an ECR
   input. A wrong list says why under the field and is not written.
8. **Pending changes.** Issue Rev A, then draw a line: the Rev box reads `A*`, the cover
   `Revision A, … (changed since)`, and the Print Book dialog (and its result) notes `The drawing
   has changed since Rev A; the title block shows A*.` Undo the line: `A` again. Filling in the next revision's fields
   does not count. **Pending changes ▸ E (draft)** shows `B (draft)`; **Off** shows `A`.

Typed instead: `projectprops {"tag_dash":false}`, `projectprops {"tag_case":"typed"}`,
`projectprops {"revision_columns":"rev, date, ecr:ECR, description, drawn"}`,
`revision {"fields":{"ecr":"ECR-114"}}`, `projectprops {"revision_pending":"draft"}`,
`contacts {"component":"<id>"}` (its `table` says where the table went and why),
`pageprops {"page":"<id>"}` (`sheet_numbers`), `printbook {"scope":{"pages":"401"}}` (`notes`).

## 5-034. Component library (by clicking)

Brief 034, design `ecad/docs/design/034.md`. Any project; open the **ECAD** tab first.

1. **Change-over contacts.** Place a new coil. In Find Symbol type `change`, click
   **Change-over contact**, then that coil's device line, and click beside the coil. Expected: it
   joins as `11/12/14`; a second one joins as `21/22/24`, and a NO contact after that as `33/34`.
2. **Families.** In Find Symbol type `protection`. Expected: the RCBO, the RCDs, the fuse terminal
   and the five fuse banks. `sensors`, `connectors` and `plc` list theirs.
3. **A fuse bank.** Place a **Fuse bank, 8-way** as a new device. Expected: `-F01` with connection
   points `L+`, `M`, `1`…`8`. Typed: `setprop {"component":"<its id>","key":"rating.3","value":"2A"}`
   stores fuse 3's rating; `rating.13` is refused with `-F01 has no fuse 13.` Checks does not list
   its unwired `M` and outputs, only `L+`.
4. **Prefixes are registered defaults.** Typed: `defaults {"group":"Tag prefixes"}` lists every
   symbol's built-in prefix (`symbol.fusebank8.prefix` = `F`, from `builtin`). Set the fuse bank's
   prefix to `FB` in **Project settings ▸ Tags** (or `tagsettings {"prefixes":{"fusebank8":"FB"}}`).
   Expected: `defaults` now says `FB`, from `project`, and the next fuse bank placed is `-FB01`.

## 6. Check connected movement, undo/redo and pages

1. **Connected movement.** Select the rectangle and its marker with a window selection, type
   `move` at the command line, pick a base point and drag.
   Expected: the wire's end follows while you drag, and stays attached after the move.
2. **Undo/redo.** Type `u` three times, then `redo` three times.
   Expected: the drawing steps back and forward exactly, and the IDs stay the same.
   `undo {"mode":"mark"}` is refused as not supported on ECAD pages.
3. **Multiple pages.** Click **+** in the PROJECT panel and press Enter in the name box (or
   `newpage {"title":"2"}` in the ECAD command field). Expected: page 2 replaces page 1 in the one drawing tab, "… - page 2: 2".
   Draw or wire something on it, then click page 1 in the panel, then page 2 again: its content
   is there. Closing the drawing tab asks nothing; the panel still lists both pages and one click
   shows a page again (section 5E).

## 7. Save, close and reopen

1. **Save.** Press Ctrl+S, or click **Save** in the PROJECT panel. A new project asks for a file name
   in the trial's own dialog: type `~/ecad-projects/demo` (on native Windows
   `C:\dev\ecad-projects\demo`) and press Enter. `.ecad` is added.
   Expected:
   - the PROJECT panel's status line shows `file /home/<you>/ecad-projects/demo.ecad, no unsaved
     changes` (on native Windows `file C:\dev\ecad-projects\demo.ecad, no unsaved changes`; the
     whole line reads `Project "demo": 2 pages, file …, no unsaved changes.` after section 6);
   - with page 1 shown, the tab is called "demo - page 1: 1";
   - the command line says `Saved: …`.
2. **Save again.** Make an edit, then Ctrl+S.
   Expected: no dialog; the status line returns to no unsaved changes.
3. **Close.** Make one more edit, then click **Close** in the PROJECT panel, or close the window.
   (Closing the drawing tab only hides the page; the project stays open.)
   Expected: "Save changes to project "demo"?" with **Save**, **Discard** and **Cancel**:
   - Cancel keeps everything open;
   - Save writes the change, then closes;
   - Discard closes, and the file keeps its last saved state.
4. **Reopen.** Start the app with the file, or click **Open…** in the PROJECT panel when no
   project is open:

   ```sh
   cargo run --release -p ecad-trial -- ~/ecad-projects/demo.ecad
   ```

   Expected: the same pages, drawing and wire.
5. **Check that the saved project kept its work.**
   - **Connectivity:** move the rectangle again. Expected: the wire end follows.
   - **IDs:** type `setprop {"component":"<the ID you noted>","key":"tag","value":"-K1"}`.
     Expected: accepted. A component ID that does not exist is refused, so this shows the ID
     survived.
   - **History:** type `u` repeatedly. Expected: it steps back through edits made before the
     save; `redo` brings them back. Each undo and redo is a new change to save.
6. **Optional: a second window.** While the project is open, start a second copy from another
   terminal, using the binary the first run built:

   ```sh
   ~/ecadcraft/ecad/target/release/ecad-trial ~/ecad-projects/demo.ecad
   ```

   On native Windows (PowerShell):

   ```powershell
   C:\dev\ecadcraft\ecad\target\release\ecad-trial.exe C:\dev\ecad-projects\demo.ecad
   ```

   Expected: it opens read-only and says that another session is editing it. Save is refused
   there; Save As can save a copy under another name.
7. **Files beside the project.**
   - `demo.ecad.lock` is normal. It marks which session may write the file, and it is empty when
     nothing holds it. If the app is killed or crashes, the next open reports and reclaims the
     leftover lock; nothing stays locked.
   - Since brief 021 there is nothing else: `demo.ecad` is one redb database. (A version 1 file
     could leave `demo.ecad.tail-<offset>-<…>` after an interrupted save; version 2 never does.)
8. **Kill it mid-edit (brief 021).** Make a few edits, save, make one more edit, then end the app
   from Task Manager (or `kill -9` on Linux) without saving. Start it again with the same file.
   Expected: it opens at once at the last save; the status says a leftover lock was reclaimed and,
   if the app was killed inside a save, that the file was recovered to its last save. The unsaved
   edit is gone; nothing saved is lost.

## 7B. Open a version 1 project: convert it (brief 021)

Projects saved before brief 021 are version 1 files (JSON Lines). This version opens only
version 2 files, and never converts one silently.

1. Click **Open…** in the PROJECT panel (no project open) and type the path of a version 1 file,
   for example `C:\dev\ecad-projects\demo.ecad`, then Enter.
   Expected: a dialog, **Convert this project to the new file format?**, naming the new file it
   would write, `C:\dev\ecad-projects\demo.v2.ecad`, and saying the original is left untouched.
2. Click **Convert and open**.
   Expected: the project opens from `demo.v2.ecad`, with the same pages, devices and history; a
   notice reports the conversion (transactions, named versions, sizes, the final hash carried
   over, and that it was verified by replaying both). `demo.ecad` is unchanged.
   **Cancel** opens and writes nothing.
3. Opening the same version 1 file again offers again; the new file is never overwritten (rename
   or delete `demo.v2.ecad` first, or open it directly).

The same converter on the command line (build once with `cargo build --release -p ecad-cli`):

```sh
cargo run --release -p ecad-cli --bin ecad -- upgrade ~/ecad-projects/demo.ecad
```

`--out <file>` chooses another name. It refuses a damaged version 1 file (rescue it first with
Save As in a build before brief 021) and an output that exists.

## 7C. Look inside a version 2 file: `ecadinspect` (brief 021, moved by brief 029)

A developer tool: it is in `ecad/dev/ecad-inspector` and never ships (`ecad inspect` now only
says where it went). Read-only; run it on a saved file (the app may have it open). The page that
shows the same file in a browser is under "The inspector" below.

```sh
cargo run --release -p ecad-inspector --bin ecadinspect -- ~/ecad-projects/demo.v2.ecad tables
```

Views: `tables` (rows and bytes per table, and `meta`), `branches` (each branch, its head and
fork point; only `main` exists for now), `log main --last 20` (the newest transactions,
decompressed, one JSON record each with its stored size), `versions` (named versions, issued
revisions and the automatic snapshots taken every 2 000 transactions), `chunk <hash>` (one
snapshot chunk, decompressed), `verify` (the chain, then the replay with the commit-time check,
then the full check at every snapshot and branch end; reads everything, about 2 s on the large
bench file), and `export-jsonl [--out <file>]` (the whole history as a version 1 JSON
Lines file: the HTML inspector and any version 1 reader open it). `export-jsonl --with-raw
--out <file>` writes every stored row instead, with its raw bytes (hex) beside the readable form;
that file is not a version 1 project.

## 7A. Let an AI agent work on a project (MCP, headless)

`ecad-mcp` (brief 011) lets an AI agent such as Claude Code read and change an ECAD project
through the Model Context Protocol. It runs **headless**: no window; the agent works on the
project file in a process of its own. To watch an agent work in the running trial, see section
7D.

Every step below uses the **same project file**: `C:\dev\ecad-projects\agent.ecad` on native
Windows, `~/ecad-projects/agent.ecad` on Linux or WSL. Use the line for your system.

1. **Build it once** (from `ecad/`):

   ```sh
   cargo build --release -p ecad-mcp
   ```

   The program is `target/release/ecad-mcp` (`target\release\ecad-mcp.exe` on Windows), or under
   `$CARGO_TARGET_DIR` if you set it.
2. **Create the project file.** The folder must exist: `ecad-mcp` does not create folders (the
   trial's Save As creates the default folder the first time; otherwise make it yourself). The
   file must not exist yet, and the trial must not have it open (a project open elsewhere opens
   read-only, and the agent can then only read it). `--create` makes the file and saves it; the
   server then waits for an agent on its input, so stop it with Ctrl+C. Native Windows:

   ```powershell
   mkdir C:\dev\ecad-projects -Force
   target\release\ecad-mcp.exe --write --create C:\dev\ecad-projects\agent.ecad
   ```

   Linux or WSL:

   ```sh
   mkdir -p ~/ecad-projects
   target/release/ecad-mcp --write --create ~/ecad-projects/agent.ecad
   ```

   A missing folder is refused with `the folder … does not exist; create it first.`
3. **Add it to Claude Code** (one line, from the folder where you run Claude Code). Native
   Windows:

   ```powershell
   claude mcp add ecad -- C:\dev\ecadcraft\ecad\target\release\ecad-mcp.exe --write C:\dev\ecad-projects\agent.ecad
   ```

   Linux or WSL:

   ```sh
   claude mcp add ecad -- ~/ecadcraft/ecad/target/release/ecad-mcp --write ~/ecad-projects/agent.ecad
   ```

   Or put it in the project's `.mcp.json`:
   `{"mcpServers": {"ecad": {"command": "<path to ecad-mcp>", "args": ["--write", "<project.ecad>"]}}}`.
   Leave out `--write` to let the agent only read.
4. **Ask the agent**, for example: *"With the ecad tools, draw three wires from 0,0 to 60,0,
   0,5 to 60,5 and 0,10 to 60,10, make them a cable with a slash from 28,-3 to 32,18, show it as
   a fan, then save."*
   Expected: the agent calls `execute` with `wire`, `cableslash` and `cableview`, and `save`;
   `ecad_history` lists one transaction per wire and one for the cable, each by
   `agent <its name>` (Claude Code gives its own name), and nothing for the view switch.
5. **Look at it in the trial.** Stop the agent's session (or quit Claude Code) so the file is
   released, then open the same file. Native Windows:

   ```powershell
   cargo run --release -p ecad-trial -- C:\dev\ecad-projects\agent.ecad
   ```

   Linux or WSL:

   ```sh
   cargo run --release -p ecad-trial -- ~/ecad-projects/agent.ecad
   ```

   Expected: the wires and the slash on page 1; the debugger (section 8) shows each transaction
   with `agent <name>` as its author, and `u` undoes the cable in one step. The cable opens **as a
   fan**: the view is not in the file (section 5B), but `ecad-mcp` remembers views in your own
   settings folder, the one the trial reads (`cable-views.json`). To keep the agent's views apart
   from yours, start the server with `ECAD_SETTINGS_DIR` set to another folder (section 9); the
   cable then opens traditional in your trial.

What the agent can and cannot do:

- **Read** (always): `ecad_project` (pages, components and connection points, wires and their ends,
  cables and their members, nets), `ecad_history` (transactions with their authors),
  `ecad_inspect`, and CADCraft's `inspect_drawing`, `query_entities`, `find_text`, `render` and
  `list_commands` (every ECAD command with its parameters). Through `execute`, the commands that
  record nothing also run without `--write`: `openpage`, `cablerefs` (the cable references and
  their texts) and `cablegoto` (opens the page of a reference's other end and selects it).
- **Write** (only with `--write`, and only when the project did not open read-only): one command
  per `execute` call, with JSON parameters, through the same checks and refusals as yours; each
  change is one transaction by the agent and one step of `u`. A call is one request line, batches
  included: a line may record at most one transaction, and a second one in the same batch is
  refused and taken back. `undo`, `redo` and `mredo` go one step per call (no counts above 1,
  no `undo` back, no `mredo` all). `save` writes the project to its file.
- **Never:** Save As, open, new or close (you choose the file when you start it), other files
  (export, import, plot, scripts, AUDIT and its log), CADCraft's command line, or anything ECAD
  refuses for you (the Block Editor, REFEDIT, LISP on ECAD pages). Without `--write` every
  change and `save` is refused with
  `Refused: this ECAD MCP server is read-only (started without --write)...`.
- `ecad-mcp --help` lists the options; `--agent <name>` records another author name. The author
  name is taken from the first `initialize` only.
- Quitting the server cleanly (the client closes it) releases the writer lock; the
  `agent.ecad.lock` file stays beside the project, empty, as the trial leaves it.

## 7D. Watch an agent draw (live, brief 035)

The same `ecad-mcp`, started with `--connect`, drives the **running** trial: you watch the
drawing appear, every agent change is recorded as `agent <name>` and every change of yours as
`user`. One registered server does both: it is headless while no ECAD window answers on the port
and live once one does (`ecad_connection` tells the agent which). Agent access is **off** until
you turn it on; when off, nothing listens.

1. **Build once** (from `ecad/`):

   ```sh
   cargo build --release -p ecad-mcp -p ecad-trial
   ```
2. **Turn agent access on in the trial.** Open the trial, unfold **Developer** at the bottom of
   the Project panel, and set **Agent access** to **Read and change** (port 47823). The status
   says `● Listening on 127.0.0.1:47823`. Once only: it is remembered in `agent-access.json` in
   your ECAD settings folder (never in a project). `ecad-trial --control 47823` turns it on for
   one run instead, without remembering it. **Read only** lets an agent read but not change or
   save.
3. **Register the server for every folder** (user scope). Native Windows:

   ```powershell
   claude mcp add --scope user ecad -- C:\dev\ecadcraft\ecad\target\release\ecad-mcp.exe --connect 47823 --write
   ```

   Linux or WSL:

   ```sh
   claude mcp add --scope user ecad -- ~/ecadcraft/ecad/target/release/ecad-mcp --connect 47823 --write
   ```

   Add `--launch` to let Claude open ECAD when it is closed (Windows, and Linux with a display;
   WSL needs WSLg). Leave out `--write` for read-only. A project path at the end chooses what the
   server opens headless when ECAD is closed (without one, headless works on an unsaved project).
4. **Ask:** *"With the ecad tools, draw three wires from 0,0 to 60,0, 0,5 to 60,5, 0,10 to
   60,10."* Expected: the wires appear one by one; a small strip over the top of the drawing
   reads `Agent claude-code · changing · last: #3 Wire … · 3 steps`; History shows each by
   `agent claude-code`; the command history has a line `Agent claude-code: WIRE  #… "Wire …"`
   per call.
5. **Start `wire` yourself and leave it half done**, then ask for another wire. Expected: the
   strip says `waiting for you to finish WIRE`; after 5 s the agent reports *"Refused: the
   designer is in the middle of WIRE; nothing was changed. Try again when they finish."*. Finish
   or press Esc, then ask again.
6. **Pause** on the strip refuses agent changes (*"paused by the designer"*) until **Resume**;
   reads go on. **Disconnect** ends this agent's session (restart the MCP server to connect
   again).
7. **Undo session** takes back the agent's steps; History has the undos by `user`. If you made
   changes after the agent's, it asks first and names them (**Undo all**, **Undo the agent's
   steps after my last one**, **Cancel**).
8. **Close ECAD and ask again:** the agent says *"The ECAD window at 127.0.0.1:47823 is not
   answering (closed?)…"* (once live, the server stays live). Reopen ECAD and it reconnects.

What the agent may do is 7A's: the same tools, checks, caps and refusals, one transaction per
call. Over the port it can also take a `screenshot` of the window. It can never turn agent
access on, widen it or move the port (`agentaccess` is refused to agents), never use your mouse,
keyboard or command line, and never open, save as or close files. An agent's `u` and `redo`
take back only its own steps. One agent changes the project at a time; others read.

The strip is deliberately plain: its look and place come with the new UI shell.

- *Automated checks:* `ecad/apps/ecad-trial/tests/all/bridge_contracts.rs` (a live trial and an
  `ecad-mcp --connect` server: authors per call, refusals byte-identical to headless, the person
  restored after refused, failing and panicking calls, the cap with batches, Read only and no
  `--write`, the busy rule, raw methods and the token, the agent-undo rule, one writer, modes,
  off by default), `agent_ui.rs` (the Developer row, the settings file, Pause, Disconnect, Undo
  session with and without your steps on top), `ecad/apps/ecad-mcp/tests/all/connect_contracts.rs`
  and `ecad/apps/ecad-mcp/src/launch.rs` (path order, arguments, no display).
- *Agent-observed:* none yet; registering with Claude Code and watching it draw (steps 3 to 8) is
  the owner's check on Windows.

## 8. Diagnostics and the debugger

### The debugger panel

Unfold the PROJECT panel's **Developer** section and click **Debugger** there; click it again (or **Close** in the debugger) to
hide it. It is a
read-only developer view drawn inside the Developer section, which takes the lower part of the
panel (drag its top edge to make it taller, and the panel's right edge to make it wider, up to
720 px). It follows one trail: **selected drawing
objects → ECAD IDs → model objects → the transactions that touched them, with the values each
change replaced (before) and set (after) → where each transaction's record is in the saved file,
and its bytes.**

- **Heading.** `Snapshot: live at seq N` is the transaction the panel shows. The panel copies the
  project only when something changes (an edit, a save, the selection) or when you press
  **Refresh**, and draws from that copy. `Saved through transaction S; P pending (in memory
  only)` separates saved history from changes that a crash would lose. If it ever says
  **Stale**, the project was busy; the previous copy is shown until the next one succeeds.
- **Views.** **Selection** (what you clicked, and its IDs), **Model** and **Definitions**
  (searchable lists), **History** (every transaction, newest first), **Persistence** (the file,
  its lock, header and record positions), **Storage** (brief 021: the version 2 file's head, the
  snapshot the project was opened from, the latest snapshot and the interval, what the last save
  wrote, the version 1 link; **Read table sizes** reads each table's rows and bytes and the version
  rows, on request), **Branches** (brief 021: each branch, its head, rows and fork point, read
  again after each save), **Diagnostics** (pages and runtime events, which are never saved in the
  project) and **Checks**.
- **Trail.** Clicking a selected object, a model object or a transaction shows the trail below
  the view: the objects (expand one for its stored value), the transactions that touched them,
  and the chosen transaction's command, ops and record.
- **Transaction states.**
  - `[pending]` (orange): only in memory, not in the file. Save writes it.
  - `[saved]` (green): durably saved. For a version 2 file (brief 021) **Read row** reads its row
    from the file's transactions table, decompressed, and checks that it holds exactly this
    transaction. (A version 1 file showed its offset and **Read bytes**.)
  - `[written, unconfirmed]`: a Save As wrote it, but could not confirm it is durable; Save
    confirms it.
  - `[pending (save failed)]`: a save of it failed; Save retries it.
- **Checks** run only when you press their button, on copies, and never change the project:
  **Verify replay** (replaying the history gives the live document), **Verify projection** (the
  drawing equals a fresh projection of the document) and **Verify file** (for a version 2 file:
  replay the history from the root with every check, compare every snapshot with the replay, and
  compare the rows with the project's history). Each result shows the
  transaction and the time it ran against.

### Worked example: what to inspect when placing, wiring, moving, undoing and saving

Start a new project (`cargo run --release -p ecad-trial`) and click **Debugger** (PROJECT panel >
Developer). Type commands in the ECAD command field at the top of the Developer section. IDs and offsets will differ from any shown here.

1. **At the start.** Expected: `live at seq 1`; **History** shows `#1 [pending] New project`.
   The PROJECT panel says no unsaved changes: a new project's own setup is not counted as a change until
   the project has a file, and the panel says so.
2. **Place.** `place {"at":[100,0],"rotation":90}`, then click the coil in the drawing.
   Expected: `live at seq 2`. **Selection** shows
   `Block Reference <handle> → item itm-… → rep-… main of untagged component → component cmp-…`.
   Click that line. The trail lists the item, the representation, the component and its two
   connection points (`…:A1`, `…:A2`), and `#2 [pending] Place symbol`. Click `#2`: it was recorded by
   command `place`; its ops include `CreateComponent`, two `AddTerminal`, `AddItem` and
   `LinkRepresentation`. Expand one: `before: (none)` and the new value after. The record says
   **Pending: only in memory, not in the file.**
3. **Wire.** `wire {"points":[[60,0],[90,0]]}`, then click the wire and its line in
   **Selection**. Expected: the trail lists the wire's item, its route representation, the
   conductor and the coil connection point it is attached to (`…:A1`); the transactions are `#3 Wire`
   and `#2 Place symbol` (which created that connection point). In `#3`, expand `AttachEnd`: before
   `"terminal": null`, after the connection point's ID.
4. **Save.** Click **Save As…** and save as `~/ecad-projects/debug-demo`. Expected: everything
   turns green. **History**: `Saved durably (…/debug-demo.ecad): transactions 1–3.` and
   `Nothing pending.` Click `#3`: `Saved: offset … in the file, … bytes.` Click **Read bytes**:
   `The bytes are a valid record holding exactly this transaction.`, then the record itself,
   `{"crc":"…","rec":{… "label":"Wire" …}}`. **Persistence** shows the file, the writer lock
   `…/debug-demo.ecad.lock`, the header, 3 records and the last record's offset.
5. **Move.** Click the coil, then `move 100,0 100,-10`. Expected: `live at seq 4`; **History**:
   `Pending (in memory only; lost unless saved): transactions 4–4.` and `#4 [pending] MOVE`.
   Click the wire and its **Selection** line: its trail now includes `#4`, because moving the
   coil moved the wire's end. In `#4`, the wire's `ModifyItem` shows different vertices before
   and after.
6. **Undo.** `u`. Expected: `#5 [pending] MOVE (undo of #4, …)`. Undo is a new transaction that
   applies the opposite change; `#4` stays in the history. Its ops show before and after swapped.
7. **Look back without changing anything.** In **History**, type `2` beside
   **State at transaction** and click **Rebuild**. Expected: `History at seq 2 (rebuilt from a copy
   of the log; the live project is unchanged)` with 1 component and no conductor. The drawing and
   the heading still show seq 5.
8. **Save again.** Ctrl+S. Expected: `#4` and `#5` turn `[saved]`, with offsets after `#3`'s.
   In **Checks**, press **Verify replay**, **Verify projection** and **Verify file**: each shows
   `PASS … at seq 5` and the time.

### The inspector (brief 029)

A developer and test tool in `ecad/dev/ecad-inspector`; it never ships. One page, two modes.

**Server mode: look inside a `.ecad` file.** From `ecad/`:

```sh
cargo run -p ecad-inspector --bin ecadinspect -- serve C:\dev\ecad-projects\demo.v2.ecad
```

1. Expected: `ecadinspect: serving demo.v2.ecad (324 KB) read-only` and
   `open http://127.0.0.1:<port>/?t=<token>`. The server listens on 127.0.0.1 only, and the
   address carries a token: the page and its WebSocket refuse requests without it. Ctrl+C stops
   it. `--port N` picks the port; without a file, drop one on the page.
2. Open the printed address. Expected: the file name, size, `container 2 · schema 1 · json/1`
   and the writer; pills `local only`, **chain PASS** and **verifying…**, then **verified** (or
   **Verify FAIL**); the Project tab with the pages.
3. Visit Devices, Wires and nets, History (branches, log with **Export JSONL**, versions) and
   Tables (pick a table, expand a row, then **Raw**, then **Readable**). Lists show 200 rows at a
   time with a search box. Times are UTC.
4. **Checks**: the open report, the chain, the invariants of the opened state, and Verify's
   result with its time.
5. Drop `demo.v2.ecad.lock` (it says it is the lock file), then `demo-old.ecad` (version 1: it
   offers `ecad upgrade`). Drop `demo.v2.ecad` again to go back. A dropped file is held in memory
   only; **Reload** re-opens the file named on the command line (also after the app saved it:
   the page then says the file changed).
6. If the server stops, the page says so and offers **Reconnect**.

The page talks to the server through a versioned JSON protocol (design 029 §2); the same views
are available to tests and scripts. Nothing on the page writes, and the protocol has no op that
writes.

- *Automated checks:* `ecad/dev/ecad-inspector/tests/all/protocol_contracts.rs` (every op, every
  mismatch message, hostile files, never writes) and `serve_contracts.rs` (token, `Host`,
  `Origin`, one page at a time, dropped files, limits).
- *Agent-observed (worker, 2026-11-12, `feature/inspector`):* `serve` on a copy of
  `demo.v2.ecad`; every tab answered, Readable/Raw on a transaction row, no console errors. On
  that file (written 2026-11-10, before brief 026), Verify reports that the tag numbers of its
  three snapshots differ from the history's: brief 026 made cable tags count towards the tag
  numbers, so snapshots written before it carry the older numbers.
- *Owner-tried:* not yet.

**Snapshots mode: exported snapshots.**

- **Exporting.** Start with `--snapshots <folder>`, then click **Export Inspector Snapshot** in
  the PROJECT panel's Developer section (shown only with `--snapshots`). Snapshots are also written before a project is discarded, and at exit. They are
  diagnostic files, not saved projects, and cannot be reopened. Their format is
  `ecad-inspect/2`: the design, its history and, new in stage 2b, the file's persistence facts
  (path, header, record offsets, lock).
- **Viewing them.** Open `ecad/dev/ecad-inspector/page/index.html` in a browser (on native
  Windows, double-click `C:\dev\ecadcraft\ecad\dev\ecad-inspector\page\index.html`, or type
  that path in the address bar; it was `ecad/tools/inspector/index.html`) and load the files from the folder given to `--snapshots`, for example
  `C:\dev\ecad-trial-snapshots`; under WSL, from `\\wsl$\<distro>\home\<you>\<folder>`. It shows
  whether each transaction was saved, and where.

### Measuring load time (brief 012)

A benchmark measures how opening and saving scale as projects grow. You run it when you want
numbers; it is never part of `cargo xtask ci`, `cargo test --workspace` or GitHub CI. From
`ecad/`:

```sh
cargo xtask bench                    # small and medium (release build), about half an hour
cargo xtask bench --tier small       # about two minutes
cargo xtask bench --tier large --runs 1
cargo xtask bench --tier small --keep C:\dev\ecad-bench-projects
cargo xtask bench --tier small --compare target/bench/load-<date>-<time>-<commit>.json
```

It generates synthetic projects (the same tier always gives the same file), writes them as
`.ecad` files and times each one. Tiers: small (10 pages, 100 components, 500 wires, 50 cables),
medium (×10) and large (×50). Each has a **lean** history (one transaction per object) and a
**churned** one (the same project after moves, renames, undo and redo, about five times as many
transactions). Columns, each the median of 5 runs (`--runs N` to change):

| Column | What it times |
| --- | --- |
| file MB, txs | The file's size and its number of transactions. |
| open ms | Open the file as the app does: take the writer lock, read it, check every record and replay the history (no drawing). |
| derive ms | Work out the **nets** of the opened project (cable references are the next column). |
| xref ms | Work out the cable references (brief 010's derivation, `ecad_model::derive::cable_references`) on the opened project. |
| 1st page ms | Open the file in a headless drawing session and draw page 1: what you wait for before you see a drawing. |
| edit ms | Record one new wire in the open project (one transaction, checked as any edit is). |
| save ms | Save that wire (append one transaction and flush). |
| draw move ms | In a headless drawing session with page 1 open, move a component that has wires attached, as MOVE does it on its symbol: the drawing change is turned into ECAD ops (the wires follow), recorded, and page 1 is drawn again (brief 014). |
| move+wires ms | In the same session, move that component together with every wire attached to it, as a window around them selects them: each wire's far end leaves its connection point, so the ends are matched against the page's connection points (brief 014). |
| draw wire ms | In the same session, draw one wire between two connection points on page 1 with the `wire` command, as the mouse wire tool does (brief 014). |
| switch ms | In a headless session with one page active, make another page the active page with `openpage` (the path of the Project panel and Go to Reference), and back to page 1 after each: median over up to 10 pages spread over the project. Target (design 016): at most 5 ms on large, 2 ms on medium. |
| preview ms | Draw the read-only preview of a page: project it, build its display list and rasterise it at 480 × 320. Target: at most 15 ms on large, 8 ms on medium. |
| panel ms | Rebuild the Project panel's rows after one `renamepage` (page order and titles only). |
| devices ms | Rebuild the Devices tab's list (every device from the tag index, with its symbols). |
| goto ms | From the Devices tab, show a device symbol's page and select it. |
| devupd ms | Bring the Devices tab's list up to date after one device property edit (only that device is refreshed). |

**Container version 2 (brief 021).** The generated version 1 file is converted to a version 2 file
(`<tier>-<history>.v2.ecad`), and since the app opens only version 2, every column from **1st
page** on is timed on the version 2 file; **open**, **edit** and **save** above stay the version 1
timings, for comparison. A second table follows:

| Column | What it times |
| --- | --- |
| v1 MB, v2 MB | The version 1 file and the converted version 2 file. |
| convert ms | `ecad upgrade`: convert, write and verify by replaying both. |
| v2 open ms | Open the version 2 file as the app does: the latest snapshot and the transactions after it. Target (EEP-0005): about 100 ms or less on large. |
| v2 save ms | Save one new wire: one redb write transaction, durable before it returns. Target: about 1–1.4 ms on large (the bake-off's redb range). |
| crash open ms | Open the file after a process saved and exited without closing it (as a killed app leaves it), with redb's quick repair. |
| open x10 ms | Open it after the history grew tenfold (property changes saved 1 000 at a time): it should stay within the run's noise of **v2 open**. |

Below it, **Zstandard** compares the pure-Rust `ruzstd` the container uses with the `zstd` crate
(levels 1 and 3) on the file's records and chunks: compressed sizes and the time to compress and
decompress them all.

Open and first page open the file afresh in every run; edit and save all run on one copy of the
file opened once; the three draw columns run on another copy, opened once in one drawing session
(the moves go alternately down and back up). Every open reads from a warm file cache (the file was just written), so a cold
open after a restart can be slower.

**How long it takes** (one run on native Windows 11, 2026-11-07, which is also the first result
below): small about 1 minute, medium about 30 minutes, both together 32 minutes. Opening grows
with the number of transactions times the size of the project, so the large tier is estimated
(not measured) at 10 to 12 hours with the default 5 runs, or 1 to 1.5 hours with `--runs 1`.

| tier | history | file MB | txs | open ms | derive ms | 1st page ms | edit ms | save ms |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| small | lean | 0.86 | 678 | 238 | 0.14 | 230 | 0.81 | 30 |
| small | churned | 3.24 | 3 386 | 1 050 | 0.14 | 1 064 | 0.92 | 26 |
| medium | lean | 8.69 | 6 798 | 27 772 | 1.98 | 27 975 | 9.6 | 40 |
| medium | churned | 32.62 | 33 986 | 138 928 | 1.96 | 140 887 | 15.8 | 62 |

The draw columns were added by brief 014 (branch `ecad/drawing-edit-speed`), which made an edit
in the drawing cost in proportion to what it touches. Native Windows 11, release, 2026-11-08,
median of 3 runs. **Before** is `f095edf` (brief 013 with its fix round merged, before 014
changed the translation; the bench columns were applied on top to measure it), **after** is
`4ca788c` (014 with its fix round). Large was measured after only.

| tier | history | draw move ms before | after | move+wires ms before | after | draw wire ms before | after |
| --- | --- | --- | --- | --- | --- | --- | --- |
| small | lean | 3.74 | 0.22 | 4.09 | 0.25 | 0.13 | 0.13 |
| small | churned | 2.74 | 0.19 | 2.81 | 0.20 | 0.12 | 0.10 |
| medium | lean | 78.04 | 0.36 | 66.76 | 0.38 | 0.49 | 0.25 |
| medium | churned | 54.61 | 0.43 | 52.02 | 0.43 | 0.44 | 0.26 |
| large | lean | — | 1.17 | — | 1.52 | — | 1.21 |
| large | churned | — | 1.19 | — | 1.39 | — | 1.32 |

The before run was on a busier machine than the after run (its open times are about 1.5 times
higher), so compare the draw columns by their growth with the tier rather than to the
millisecond. What still grows from medium to large is redrawing the page, which goes through the
whole document to find what is on it (`docs/deferred-work.md`), not the translation.

After briefs 009 to 011 were merged into the branch (`c5dfb21`), every commit derived all cable
references twice and the cable look went through every representation, so medium draw edits rose
to about 7.5 ms. The 014 fix round derives references only for the cables an edit touches, finds a
page's objects through the page index, and redraws only the page in front (another open page whose
references changed is redrawn when it comes to the front). Medium after it, same machine, median
of 5: lean 0.53 / 0.59 / 0.38 ms and churned 0.54 / 0.66 / 0.39 ms (draw move / move+wires /
draw wire), against 0.25 / 0.27 / 0.21 and 1.49 / 1.68 / 1.03 before the merge (`9045d27`) and
7.61 / 7.57 / 7.20 and 7.43 / 7.26 / 7.19 at `c5dfb21`.

The other columns were not changed by brief 014. Not yet tried by you: open the medium project
(`--keep`) in the trial and say whether moving a component feels instant.

The table is printed and also written as JSON to
`ecad/target/bench/load-<date>-<time>-<commit>.json` (UTC; an existing file is never
overwritten), with the machine, OS, commit and build. `--compare` reads the earlier file before
the run starts and prints the change per row after it. `--keep <folder>` keeps the projects so
you can open them in the trial (**Open project…**); it refuses to replace a project that is open
in another session. Results depend on the machine and what else it is doing; compare runs from
the same machine. There are no pass or fail limits yet, and peak memory is not measured. The
generated cable references (each page's first cable continued on the next page) are recorded as
brief 010 records them; on a revision without brief 010 they open as ordinary points and lines.

### Measuring start-up (brief 015)

To see where the time goes between launching the trial and its first frame, set
`ECAD_STARTUP_TIMING=1` before starting it. Each step is printed on stderr as it is reached, then
one line with the GPU adapter eframe picked and the total, and the same marks are written to a new
file `ecad-startup-<time>.json` (the time is `main`'s, in milliseconds since 1970; `-1`, `-2`, ...
is added rather than replace a file). It goes in the folder given to `--snapshots` (created if
missing, `~` meaning your home folder), or else, or if that fails, in your temp folder; the last
stderr line says where. If you close the window before its first frame, the marks reached so far
are still written, marked `"complete": false`. Unset, `0` or `off`: no output, no file.

A release build on Windows has no console, so its stderr is lost unless you redirect it; the JSON
file is always written. From Git Bash, in `ecad/`:

```sh
ECAD_STARTUP_TIMING=1 target/release/ecad-trial.exe 2> startup-1.txt
```

In PowerShell (stderr is not captured; read the JSON file):

```powershell
$env:ECAD_STARTUP_TIMING = "1"; .\target\release\ecad-trial.exe; Remove-Item Env:ECAD_STARTUP_TIMING
# then open the newest ecad-startup-*.json in $env:TEMP
```

Close the window after it has drawn, and repeat for three runs. Each mark shows the time since the
start and, in brackets, the step: the time since the previous mark. The steps add up to the total.

| Mark | Reached when (the step before it covers) |
| --- | --- |
| 1 main entered | The program's own code starts. Before it: the OS loader, DLLs and any virus scan. |
| 2 install() done | ECAD's commands are registered with CADCraft. |
| 3 run_native called | The window options are built and eframe takes over. |
| 4 creator entered | eframe has made the event loop, the window (still hidden), the wgpu instance, adapter, device and surface, and egui's context. Not the fonts: egui builds its font atlas in the first frame (step 7). |
| 5a shell built | CADCraft's app and shell exist. |
| 5b project opened | ECAD's new project is created (or the file is opened). |
| 6 set_wgpu done | CADCraft's GPU canvas is set up (pipelines and shaders). |
| 7 first update built | The first frame's user interface has been built, including egui's font atlas (font loading and glyphs). |
| 8 first frame presented | The next frame starts after the first full one, so that one was painted and presented (eframe has no call-back after presenting). An upper bound: it includes tessellation, the font texture upload, painting, presenting (which can wait for vsync), showing the window and the wait until the next frame. |

**Where the start is.** On Linux the marks count from the process's creation (from `/proc`), so
mark 1 includes the loader; the kernel gives the creation time in clock ticks, so mark 1 there is
accurate to about ±10 to 20 ms. On Windows they count from mark 1, `main`: the creation time would
need `GetProcessTimes`, which needs `unsafe` (forbidden in this workspace) or a new dependency.
To measure the time before `main` on Windows, while the trial is still open run, in PowerShell:

```powershell
([DateTimeOffset](Get-Process ecad-trial).StartTime).ToUnixTimeMilliseconds()
```

and subtract that number from `main_unix_ms` in the run's JSON file: the difference is the
milliseconds between Windows creating the process and `main`.

**Agent runs (not yours),** native Windows 11 Pro, 2026-11-08, release build after the review
fixes, no project, launched from Git Bash and closed after 8 s. "Before main" is `main_unix_ms`
minus the process start time from PowerShell, as above. Columns 4 to 8 are steps (time since the
previous mark):

| run | before main | 4 creator | 5a + 5b | 6 set_wgpu | 7 first update | 8 presented | total from main |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 244 ms | 757 ms | 0.6 ms | 2.5 ms | 40 ms | 63 ms | 863 ms |
| 2 | 56 ms | 723 ms | 0.4 ms | 2.9 ms | 39 ms | 56 ms | 821 ms |
| 3 | 75 ms | 736 ms | 0.4 ms | 2.7 ms | 39 ms | 62 ms | 841 ms |

Marks 2 and 3 took under 0.5 ms. Adapter: Vulkan, "NVIDIA RTX 3500 Ada Generation Laptop GPU",
discrete. Run 1 was the first launch of a freshly built exe. An earlier agent run of the first
version of this branch, on a freshly built exe, spent about 2.6 s before `main` on its first
launch and about 0.15 s on the next two (measured from Git Bash's clock, so approximate).

Your table of three clean runs goes here once you have run it.

## 9. Current limitations

- **Live agent access (brief 035):** one agent changes the project at a time (others read); the
  port is loopback only (`127.0.0.1`); `--launch` opens the trial on Windows and on Linux with a
  display (WSL needs WSLg), not on macOS; the banner is a plain strip until the UI shell.
- **Outputs (brief 024):** one built-in table layout for generated pages (44 rows per A3 sheet);
  custom sheets print fitted on the smallest ISO A paper; no XLSX. The trial has no file dialogs, so
  Print Book's file and the CSV path are typed. The Devices tab does not show the location prefix
  yet. **Make location box** is the `S` option of Location Box (no right-click entry yet).
- **Strip accessories (brief 030):** accessories are placed by hand (no automatic rule yet); no
  level jumpers, test plugs or rail parts; marker text is not printed; links join whole positions
  (no levels). The Terminals tab's Accessory menu acts on a strip's ends and all its markers (place
  a partition or a single marker with **Strip Accessory**); a link is extended, shortened or split
  through **Jumper**, its Properties **Positions** or `jumper`, not from a context menu. The
  generated plan page writes the marks as text (`||`, `|`, `==`, `[]`, `o` per link lane).
  Merging two branches that each added a link with the same number on one strip shows one conflict
  on that `link.<n>` key: keep one side, then add the other link again with **Jumper** (design
  030 §9.4 "Keep both" is later work).
- **Terminal strips (brief 028):** no multi-level, disconnect or fuse terminals; no bridge parts
  (end plates, markers) on the BOM; no reference texts under terminals; cable lengths are typed,
  not measured. A terminal on a wire drawn on several pages is refused (place it where the wire
  is on one page). Draw Bridge continues from the second terminal by default (the engine passes
  no Shift state); bridges get no end dots yet. **Set cores…** selects the cable's slash rather
  than putting the cursor in the Cores field. A new cable member takes the next free core when
  added with `cableadd` to a cable with an offer; a new slash always makes a new cable.
  New Page ▸ **Terminal plan** shows every strip (one strip: `newpage` with `strip`).

- **Release (brief 027).** No transmittal, zip or upload, and one folder layout. The folder is a typed path (no folder
  dialog in the trial). The PDF is written in one go: Esc does not stop a release while it writes.
  The History tab has no Release button yet (Revisions and the PROJECT panel have). Provisional
  tags (brief 026) are released as they are until 026's Finalise is wired in.
- **Cross-references (brief 025).** No PLC card symbol yet (a card with channels is a later step);
  no change-over (`CO`) contacts; a contact's text sits at a fixed place under it and only a
  device's whole table can be moved (Move Mirror Table); continuations are not on the reference
  index yet. Going to a reference is **Go to Reference** or a Properties **Go to** button:
  double-clicking a reference text does nothing yet.

- **History and branches (briefs 020a, 020b):** a branch merges into, and updates from, its
  parent only (owner answer 14). The merge dialog has no **Show ▾** preview of an object's page
  as on main or on the branch yet, and its "After the merge" line points at the Checks tab instead
  of counting the new Checks entries. A duplicate tag offers renumbering either device (not "make
  them one device"); two sibling folders with one name offer renaming either (not "combine").
  A redo of a merge whose undo was saved in an earlier session merges again but leaves the branch
  open (close it from History). Times are shown in local time (the file keeps UTC). New versions
  are made at Now only (owner answer 13).

- **Connectivity and wire numbers (brief 022).** Labels are placed on each route's longest
  segment with no collision avoidance and cannot be dragged (owner answer Q11), so a label can
  cross a tag or a reference text. Properties shows wires only; junctions and continuations show as
  devices without a tag (their own sections are later work). The go-to
  ring of the Checks tab clears at the next change or the next go-to, not at a click or Esc.
  Projects made before brief 022 open with no numbers (`?`); the Checks tab offers **Number them**,
  and any net an edit touches is numbered then. Wire numbers on cable core labels in fan view,
  terminal strips and wire lists come later (F5, F6).

- **Pages and the Project panel** (brief 016): there is no Delete page (no command exists yet).
  The preview shows no connection point or wire-end marks and cannot be edited. Page views, the panel's
  width and whether it is collapsed are remembered for the session only. The JSON form of ECAD
  commands needs the ECAD command field: CADCraft's own command line takes a space as Enter.
- **Sheets and properties** (brief 019): one built-in title block; landscape A3–A0 and custom sizes
  only (A4 and portrait later); the revision-list page, cover and contents are F6. A click exactly
  on a frame line selects nothing and does not start a window selection: start the window off the
  line. Title-block texts are fitted by an estimate of their width, so a long value may be squeezed
  or cut a little early. The revision view is read-only and shows pages one at a time in the
  preview. Folders are remembered as folded for the session only.

- It is an experiment. No native file dialogs (the trial has its own path dialog) and no ECAD
  menu (CADCraft shows no top-level menu supplied by an embedder; use the ECAD tab or type the
  command name). The ECAD tab's buttons are text, not icons.
- **The click-based tools** (brief 008):
  - Connection point capture is ECAD's own: CADCraft's object snap may also show its marker (for example
    an Endpoint square) beside the green ring; the ring decides where the click goes. The
    crosshair itself stays under the pointer; the rubber band ends on the connection point. Tab does
    not cycle through connection points. Only the cursor is captured: a typed point attaches only when it
    is exactly a connection point's point.
  - Rotate and Mirror are options: type `R` or `M` and press Enter (a single key press does not
    rotate). Only steps of 90° by mouse; other angles through the JSON form.
  - Add Connection Point shows no pick box (so that object snaps work at its prompt); the cyan ring shows
    where a click would put the connection point.
  - Add Connection Point's prompt names the number the next connection point gets for the component under the
    cursor. With the cursor off every component it names a number only when all components on
    the page would get the same one; otherwise it says `Specify point for a connection point on a
    component`, and a typed point gets the smallest unused number of the component it lands on.
  - ECAD objects are drawn in the Model tab only. In a paper-space layout (Layout1 …), also while
    working inside one of its viewports, the six
    tools refuse to start (`Switch to the Model tab to draw ECAD objects.`), a tool already
    running refuses points and picks there, and no connection point marks or connection point tooltips are
    shown.
  - The preview name reads `-> A1` (the drafting font has no arrow character).
  - Connection point and wire-end marks are drawn only in the main drawing area, not in the other tiled
    model-space views (VPORTS), and only on the Model tab.
  - Connection point and wire-end marks use the dark theme's colours; on the light theme cyan and green
    are hard to see.
  - Set Property, Add to Cable and Delete still open the ECAD command field with a template.
- **Devices and tags** (brief 018):
  - No contact cross-references or mirror tables yet (F3), no terminal strips (F5), no location
    boxes (F6), no `.ecadlib` symbol libraries: 81 symbols are built in (21 since brief 018, 60
    more since brief 034).
  - Tag texts and connection point labels are placed automatically (left of a symbol, or above it when
    rotated); they cannot be moved. Editing them alone is refused; erasing one erases its symbol.
  - Tag padding and each symbol's prefix are project settings, changed by typing
    `tagsettings {"padding":3,"prefixes":{"coil":"KA"}}` in the ECAD command field (`null` for a
    symbol goes back to the built-in prefix; `tagsettings {}` lists them). There is no dialog for
    them yet. A project that changes them is refused by older builds.
  - Find Symbol is a small window of the trial's own (Tool Sets holds buttons only); typing at the
    `place` prompt finds the same symbols.
  - Copying a device's symbol makes a new device with the next tag and every connection point of the
    original.
  - Components converted from lines, and coils placed by older builds, have no tag and keep
    today's behaviour (erasing their drawing leaves the component).
- **Cable Slash** (brief 004, owner answers of 2026-11-06):
  - A wire counts when the slash crosses it, passes through one of its bends, or ends on it. A
    slash that meets a wire only at the wire's own end, or runs along it, does not take it; the
    reply says how many such wires were left out.
  - Each wire can be in only one cable: a slash across a wire already in a cable is refused, and
    so is `cableadd` with such a wire.
  - Any crossed wires may form a cable; the member order is the crossing order and means
    nothing else (no core numbers, no spares).
  - The slash is drawn in the same style as a wire (no cable layer or colour of its own).
  - Moving a wire does not change the cables it is in; if the slash no longer crosses it, only
    its magenta dot goes (no warning). Deleting a cable's last wire leaves an empty cable.
  - On a project opened read-only, a slash works as the other ECAD edits do: edits work in
    memory; use Save As to keep them.
  - Only straight two-point slashes; arcs and circles are never taken; one cable per slash.
- **Cable View** (brief 005, owner answers of 2026-11-06):
  - The fan is ordinary drawing content on the reserved layer `ECAD-CABLES` (magenta): it snaps,
    selects, plots and appears in a DXF export of the page as shown. Drawing new objects on that
    layer is refused.
  - There is no single-key shortcut: use the button, a click on the cable, or type `cableview`.
  - Editing the fan itself is refused; anything else is allowed, including moving equipment with
    fan-view wires attached (the hidden wires' ends follow and the fan is redrawn). Typed and
    scripted JSON commands act on IDs and behave the same whatever any cable's view is.
  - Brief 009's fan: a label box (tag, else `?`; designation; type) on a thick trunk, a thick
    spine per fanned end, a quarter-circle bend per wire and a thin tail at least 20 drawing units
    long to the wire's end. The trunk joins the middle of each spine unless moved with
    **Cable Join** (stored in the project as a hidden point of the cable, one per end and page;
    no format change); a join is always kept on the spine between the outermost wires, also when
    wires move later. Sizes are fixed drawing units for now (text 2.5, tails 20, bends 2.5),
    so the labels are small when zoomed out. The trunk's corners are sharp. Thick and thin
    show only with **Show/Hide Lineweight** on, which the trial turns on when it opens or creates
    a project unless you switched it yourself since the trial started;
    plots and DXF exports carry the weights either way. A cable with one wire is not offered a fan
    view; a fan-view cable with only one of its wires on a page is drawn traditional there.
- **Core labels and reference ends** (brief 009, overseer decisions of 2026-11-07, to be confirmed
  by the owner):
  - A core label is free text, at most 32 characters, no control characters, trimmed; duplicates
    in one cable are allowed with a note. ECAD never numbers cores or checks them against colours.
  - In traditional view a core label sits just past each slash crossing; wire numbers are shown
    only in fan view, at the tail start.
  - A cable's reference end is brief 010's: this branch only draws its marker and stops the fan
    there (at most two per cable per page, each taking the nearer end); its text is `->` until
    brief 010 is merged. One cable across pages is brief 010.
  - Views are remembered per user and per project in `cable-views.json`: on Windows in
    `%APPDATA%\ECAD\` (for example `C:\Users\<you>\AppData\Roaming\ECAD\cable-views.json`), on
    Linux in `$XDG_CONFIG_HOME/ecad/` or `~/.config/ecad/`. The environment variable
    `ECAD_SETTINGS_DIR` names another folder. If the file cannot be read, every cable opens
    traditional and opening says `Cable views file could not be read; all cables start in
    traditional view.`; if it cannot be written, the switch still happens and says
    `(not remembered: <reason>)`. A file that is not JSON is replaced at the next switch; one that
    is locked by another program, has another format (for example from a newer version) or is
    over 1 MiB is left alone, and switches are not remembered until it is fixed. The file never
    grows past 1 MiB: the projects used least recently are dropped from it first.
  - The same project in two windows: the window that switches last decides what is remembered.
    Two different projects lose a switch only if they write at the same moment. Views are
    presentation only, so the file is not locked.
  - At most 25 000 cables of one project are remembered in fan view.
- **Cable references** (brief 010; the overseer's decisions, for you to confirm):
  - A cable has at most two references (one continuation). A third is refused; a chain over more
    pages needs a later brief.
  - A cable cannot be continued on the page it is already drawn on (no second area of one page).
  - A wire drawn on two pages is one wire (one core), connected at one end on each page. Only
    wires connected at one end at most are continued; a wire connected at both ends is skipped and
    named in the reply.
  - Columns are a fixed 40 mm grid counted from x = 0 until drawing frames exist; the page
    number is the page's position, so renaming a page does not change the marker text.
  - The hover lens is text, not a picture of the other page. There is no jump on double-click:
    select the reference and click **Go to Reference**.
  - The reference text is drawn on the reserved layer `ECAD-CABLES`; erasing it erases its
    reference; moving or editing it is refused (move the reference point instead). The marker is
    a ring and a text until brief 009's marker is merged.
  - Wires connect to their connection points with **Connect** after continuing; there is no automatic
    routing.
- One project at a time. Only `.ecad` projects are saved; plain CADCraft drawings cannot be
  written.
- **Where files may live.** Native Windows and Linux are supported. Keep projects in a local
  folder: `C:\dev\ecad-projects` (NTFS) on Windows, the dialog's default, and the Linux home
  folder on Linux and under WSL (for example `~/ecad-projects`). On native Windows, as on Linux,
  a file replaced or deleted behind an open project is detected and Save is refused. Saving
  differs from Linux in these ways:
  - A read-only file (the read-only attribute) opens read-only and says the file cannot be
    written; Save As saves a copy.
  - If no writer lock can be taken at all, the project opens read-only and says so; this is not
    reported as another session editing.
  - A second window says another session is editing the file but cannot name it.
  - After a crash, or after closing the other window, the lock may take a moment to be released;
    open again shortly.
  - A save waits up to about half a second while another program (antivirus, the indexer, a sync
    client) briefly holds the file.
  - If the folder cannot be flushed, Save As reports the save as unconfirmed, and later Saves
    write nothing (they say so) until a Save As to a folder that can be flushed, such as one on a
    local NTFS drive, rescues the project.

  Other file systems, including FAT and exFAT drives (USB sticks), network shares, ReFS
  (including Windows 11 Dev Drives), OneDrive folders, Windows drives under `/mnt`, and macOS builds, are **unverified**: file locking and the
  durability guarantees are not known to hold there.
- **Windows release builds** open no console window and show no console output, however they
  are started; `--help`, `--version` and usage errors print only when the output is redirected to
  a file or pipe. Use a debug build (`cargo run -p ecad-trial -- --help`) to see them.
- Only the trial's Save As dialog creates the default projects folder. `saveas {"path": …}` typed
  in the ECAD command field saves only into a folder that already exists.
- The `.ecad` format is version 1, a development format. Files from a later format version are
  refused rather than converted, and nothing rewrites older files yet.
- No library files (`.ecadlib`) and no checkpoints.
- **Refused on ECAD pages** (CADCraft 0.15.1 features that change a page outside the commands
  ECAD records): the Block Editor (BEDIT, BSAVE, BSAVEAS, BCLOSE, also from Tools, the context
  menu and Tool Sets), in-place reference editing (REFEDIT, REFSET, REFCLOSE), and LISP:
  expressions typed at the command line such as `(command "line" …)`, APPLOAD, the LISP command
  and commands defined in LISP. Each is refused with a message before the drawing changes (a
  command still running is cancelled first, as for any new command); the page, its history and
  the project stay as they were.
- **Since CADCraft 0.19.0 (brief 023):** BREAK and TRIM that cut a wire, a component's line or
  another object with an electrical meaning into pieces are refused (on plain drawing they work,
  and the object keeps its handle). OOPS brings back plain drawing removed by the last erase on
  the page you are on; for a wire, a device or anything else ECAD records, and after UNDO, REDO or
  a page switch, it is refused with a message: use UNDO. `-ARRAY` copies devices and wires as COPY
  does. MINSERT of a symbol is refused.
- **Window size and position (brief 023):** the trial's window opens centred the first time; after
  that it reopens at the size and position (and maximized state) it had when you closed it, as
  CADCraft's own app does. The state is kept by eframe in its `app.ron` under the id `ecad-trial`
  (on Windows `%APPDATA%\ecad-trial\data\app.ron`); delete that file to start centred again.
- **Update checks.** CADCraft 0.15.1 brings an update checker (`craft-update`). The trial never
  checks at start. CADCraft's **Help > Check for Updates…** item is still in the menu: choosing
  it asks GitHub (`api.github.com`) for CADCraft's releases and may offer a newer CADCraft
  release. It cannot install anything into the trial (a build run from Cargo's `target` folder is
  never updated in place); ignore that item.
- **The debugger** only reads. It shows the active page's selection (at most 500 objects); long
  lists are paged 100 at a time; record bytes are shown up to 16 KiB (longer records are still
  checked in full); **Verify file** reads files up to 1 GiB. Each snapshot copies the project's
  document, which is fine at trial sizes but not designed for very large projects.

## 10. Reporting what you saw

Please send:
- what you saw at each "Expected" above, with screenshots where something looks wrong (for the
  debugger, a screenshot of the whole window, panel included);
- the terminal output;
- the output of `git log --oneline -1` and the environment checks from section 1 (WSL) or
  section 1W step 1 (native Windows).
