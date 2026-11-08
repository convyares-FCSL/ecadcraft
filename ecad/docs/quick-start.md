# ECAD quick start: run and try the current app

This is the **one maintained guide** for building, running and manually testing the ECAD trial
app. Other READMEs link here instead of repeating it. It is updated in the same pull request as
any change that affects these instructions (see `ecad/CLAUDE.md`).

> **What you are running.** `ecad-trial` is an experiment, not an ECAD application. It is
> CADCraft's drawing window with the ECAD commands and one ECAD project open at a time. Projects
> are saved as `.ecad` files (persistence stage 2a). The **PROJECT** panel on the far left lists
> the project's pages, shows one of them at a time and holds New, Open, Save, Save As and Close
> (brief 016; there is no top banner any more). An **ECAD** tab in Tool Sets has buttons for the
> ECAD commands; most work by clicking, with prompts at CADCraft's command line. An **ECAD Dev**
> tab beside it holds the read-only debugger panel (stage 2b), snapshot export and the ECAD
> command window for JSON forms; none of them is shown unless you open it.

## Which revision this describes, and what has been tried

**Brief 016 (design `ecad/docs/design/016.md`), on `feature/project-manager`, not yet in `dev`:**
the Project panel, one active page at a time, undo and redo bringing their page to the front, a
read-only preview of another page, and the top banner replaced by the panel's header and an
**ECAD Dev** tool set (design 016, "Top bar removed"). Sections 4, 5E, 6, 7 and 8 describe it.
- *Automated checks:* `ecad/apps/ecad-trial/tests/all/project_panel.rs` (design 016 checks A to J by
  label in a 1280×800 headless window, and the six `016-*.png` screenshots), the one-window and
  undo checks in `ecad/crates/ecad-cadcraft/tests/all/project_pages.rs`, check K in
  `ecad/apps/ecad-mcp/tests/all/mcp_contracts.rs`, and `the_quick_start_trial_runs_headless`
  (sections 5 to 7 below).
- *Agent-observed:* the worker rendered the `016-*.png` reference screenshots headless (wgpu) and
  looked at them; no agent has clicked through the live window.
- *Owner-tried:* not yet.

This guide describes `dev` with stage 2b (the debugger panel, PR #11) and the ECAD tab in Tool
Sets (PR #12) merged on 2026-10-15, on top of stage 2a, now built on **CADCraft 0.15.1**
(upstream tag `v0.15.1`, commit `660e15c`, brief 007, 2026-10-31; egui 0.37, Rust 1.95 or later).
The steps below did not change with that sync; the Block Editor, REFEDIT and LISP are refused on ECAD pages (section 9). Your trials recorded below were on the
earlier CADCraft 0.6.2 base, except your click-only trial of brief 008 on native Windows, which
was on 0.15.1.

**Brief 008 (design `ecad/docs/design/008.md`)**, merged into `dev` (PR #16), changed how
sections 4 to 7 are done: the ECAD tools work by clicking, terminals and wire ends are marked on
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
an existing device (`-K01`'s coil on one page, its contacts on another). Tags and terminal labels
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
- everything from brief 008: the ECAD tools by clicking (section 5), the terminal and wire-end
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
- **The ECAD command window** (Tool Sets > **ECAD Dev** > **JSON Command**). Use it for ECAD
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
  - **Add Terminal**: click on a component's outline to add a terminal there; repeat; Enter to
    finish.
  - **Find Symbol** (brief 018) opens a small window with one search field: type a few letters
    (`coil`, `cont`, `fuse`, `-Q`, or a device's tag such as `-K01`) and click a row. A symbol row
    starts Place Symbol with it; a device row asks which symbol to add to that device.
  - **Place Symbol** asks `Enter symbol or [List/Rotate/Mirror] <coil>:`. Type a symbol (`no`,
    `mcb1`…; `L` lists them) or press Enter for the one shown. The symbol then follows the cursor
    with its tag and terminal labels; the prompt says what the click makes (`new device -K02` or
    `joins -K01 as 13/14`). `R` then Enter rotates by 90°, `M` then Enter mirrors, `D` then Enter
    adds the next symbol to an existing device, `T` then Enter types the new device's tag. Click
    to place; repeat; Enter to finish. Clicks snap to the 5 mm grid (F9 turns snap off).
  - **Device Properties**: click a device (any of its symbols, its tag or a terminal label), then
    choose Tag, Description, Maker, Part, Location or Function and type the new value.
  - **Delete Device**: click a device; it asks before deleting it with every symbol on every page.
  - **Wire**: click points; an end on a terminal attaches (a green ring shows before you click);
    a click on a terminal after the first point, or Enter, finishes the wire.
  - **Connect**: click a wire near its end, then a terminal. **Disconnect**: click a wire near
    an attached end.
  - These six put their prompts at CADCraft's command line (bottom of the drawing area) and leave
    the ECAD command window alone. Answer by clicking, or type a point (`40,10`) or an option letter and
    press Enter. Enter (or Space, or a right-click) finishes; Esc cancels what is not done yet
    (objects already added stay); `u` afterwards undoes the last object.
  - **Inspect Project** runs at once and shows the project's IDs at the command line.
  - **New Page** and **Rename Page** open a small name box (prefilled with the next number, or
    with the page's title); OK or Enter creates or renames the page in one step, Cancel does
    nothing.
  - **Set Property, Add to Cable, Delete Electrical Object, Open Page, Move Page** only open the
    ECAD command window with an editable line, and a note under the
    field says what to fill in.
    Replace each `<placeholder>`, then press Enter or **Run**. A line that still holds a
    placeholder is refused, never run. IDs come from the current selection where it names them:
    select a wire before **Set Property** or **Delete Electrical Object**, and its ID is filled
    in. The selection is left as it is.
- **Typing a command name** (`wire`, `addterminal` …) in either place starts the same
  click-based form as its button. `name {json}` in the ECAD command window runs the JSON form with
  no prompts, as scripts do.

**What the drawing shows.** Every terminal has a small cyan ring; a wire end attached to a
terminal has a green dot, and a loose wire end a small grey square. They keep their size at every
zoom and are only display: they cannot be selected or printed. With no command running, resting
the pointer on a component, a terminal ring (any terminal, free or connected, of a drawn component
or a symbol; near a ring the terminal wins over the wire ending on it) or a wire for half a second shows a tooltip with its
name or ID and its connections; hovering never changes anything.

Each ECAD command answers at the command line in words, with the full IDs it created. Note IDs
you want to check later.

## 5. Example: two components connected by a wire (by clicking)

Open the **ECAD** tab in Tool Sets first. Prompts appear at CADCraft's command line at the bottom
of the drawing area; type there (or in the ECAD command window) only where a step says so.

1. **Draw a rectangle.** In the ECAD command window: `line 0,0 40,0 40,20 0,20 c`. Then type `zoom e`
   (or zoom with the mouse wheel) so it fills the view; leave room on the right for the coil.
   Expected: four lines; the Project panel's status line shows 1 change not saved.
2. **Make it a component.** Select the four lines with a window selection (click above left of
   the rectangle, then below right of it), then click **Convert to Component**.
   Expected: `Component <id> created from 4 objects.` at the command line. **Note this component
   ID.** The rectangle now selects as one group. (With nothing selected, the button asks
   `Select objects to convert to a component:`; select, then press Enter.)
3. **Give it two terminals.** Click **Add Terminal**. The prompt says
   `Specify point for terminal 1 on a component or [Name]:`. Type `40,10` and press Enter, then
   `0,10` and Enter, then Enter once more to finish.
   Expected: a cyan ring at 40,10 and at 0,10, and `Terminal 1 added to component <id>.`, then
   `Terminal 2 …`. Clicking on an edge works too: the terminal goes onto the outline where you
   click (use Midpoint snap, Shift+right-click > Midpoint, to hit the middle exactly). `N` then
   Enter lets you type a name for the next terminal; Enter alone keeps the number shown.
4. **Place a coil.** Click **Place Symbol**. Type `R` and Enter: the coil follows the cursor,
   turned by 90°, with cyan rings at its two terminals, and the prompt says
   `(new device -K01, rotation 90)`. Type `100,0` and Enter (or click there), then Enter to
   finish.
   Expected: `Placed -K01 (Contactor / relay coil) with terminals A1, A2.`; a rotated coil with
   rings at 90,0 and 110,0, `-K01` above it and `A1`, `A2` beside its terminals.
5. **Draw the wire.** Click **Wire**. Move onto the rectangle's terminal at 40,10: a green ring
   and `-> 1` appear before you click. Click. Then click at about 70,10 and at about 70,0 (bend
   points; F8 for Ortho helps). Move onto the coil's left terminal at 90,0 until the green ring
   and `-> A1` show, and click: the wire is finished.
   Expected: `Wire <id> drawn; end A attached to 1, end B attached to A1.`; green dots on both
   ends, and no grey squares. A click near a terminal but outside the green ring does not attach:
   that end shows a grey square instead.
6. **Look before you type IDs.** With no command running, rest the pointer on the coil, on the
   ring at 40,10 (or the coil's ring at 90,0) or on the wire. Expected: a tooltip with
   `Component (no tag)` and its terminals (on the rectangle; the coil shows
   `Device -K01   Contactor / relay coil`), `Terminal 1` (or `Terminal A1`) and its wire, or `Wire (no tag)` with `End A: terminal 1 of …` and
   `End B: terminal A1 of …`. Nothing changes in the Project panel's change count.

Typing instead of clicking still works, in the ECAD command window:
`convertcomponent` (with the lines selected), `addterminal {"handle":"<handle of the right-hand
edge>","at":[40,10]}`, `place {"at":[100,0],"rotation":90}`,
`wire {"points":[[40,10],[70,10],[70,0],[90,0]]}`. The headless test
`the_quick_start_trial_runs_headless` follows that typed version.

## 5A. Make a cable (by clicking)

A cable groups existing wires; the wires themselves do not change. You draw it as a short line
(the *slash*) across the wires. This example uses a page of its own so the coordinates are easy
to read.

1. **A page with three wires and a plain line.** In the ECAD command window:
   `newpage {"title":"cables"}`. Then click **Wire**, click at `0,0` and `60,0` and press Enter;
   do the same at `0,5` → `60,5` and `0,10` → `60,10` (typing the points at the command line, or
   `wire {"points":[[0,0],[60,0]]}` in the ECAD command window, works too). Draw a plain line with
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

`cableslash {"from":[28,-3],"to":[32,18]}` in the ECAD command window does the same as steps 2 and 3
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
   "select wire" on a fan line gives the same message. Moving a component whose terminals have
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
   `cableslash {"from":[28,-3],"to":[32,18]}` in the ECAD command window). Expected: `Cable <id>
   created with 3 wires: …`. If you skipped step 7 and the cable is still there, skip this step.

1. **Mark where it leaves.** Click **Cable Reference** (Wiring group, after Cable View). The
   prompt says `Select a cable to reference:`. Click the slash. The prompt says
   `Specify reference point or [Undo]:`; a red ring and a red `/?` follow the pointer. Click just
   right of the wire ends, at about `65,5`.
   Expected: `Reference for cable … placed on page 2.` and `It has no other end yet: open the
   other page and click Continue Cable.`; a red ring at the point and, beside it, a magenta flag
   around a small red `/?`. The Project panel's change count goes up by one.
2. **A page where it arrives.** `newpage {"title":"field"}` (page 3). Draw a component with three
   terminals as in section 5: `line 100,-5 110,-5 110,15 100,15 c`, select the four lines, click
   **Convert to Component**, then **Add Terminal** at `100,0`, `100,5` and `100,10` and press
   Enter. Then `zoom e`, and zoom out a little so that `20,5` is on screen.
3. **Continue it.** Click **Continue Cable**. The prompt says
   `Enter cable to continue or [List] <…>:` with your cable as the default (`L` lists the
   references waiting for their other end). Press Enter. Click where the cable arrives, at about
   `20,5` (a magenta ring and `/2.2` follow the pointer: the text this end will show). Move the
   pointer to the right, to about `40,5`.
   Expected before the click: three short wires 5 apart, a slash across them and
   `-> 3 wires continue` beside the pointer. Click.
   Expected: `Cable … continued on page 3 with 3 wires; this end shows /2.2 and the end on page 2
   now shows /3.1.` and `Connect each wire's free end to its terminal with Connect.`; three short
   wires with grey squares at their right ends, a slash with three magenta dots, a magenta ring at
   `20,5` and, to its left, a flag pointing left (away from the wires) with `/2.2`. Switch to page 2: its `/?` is now a magenta `/3.1` (only the page in front is redrawn by a change; another open page it affected is redrawn when you bring it to the front), and the
   wire ends next to the reference lost their grey squares (the wires continue). One change.
4. **Connect.** Click **Connect**, click each short wire near its right end, then the terminal
   at the same height. Expected: each wire stretches to its terminal; green dots there. In the
   debugger the three wires are the same IDs as on page 2.
5. **Hover.** Rest the pointer on a reference (its ring or its text). Expected: a lens such as
   `Cable reference  /3.1`, the cable and the reference's ID, `This end: page 2 "cables",
   column 2, 3 wires`, `Other end: page 3 "field", column 1, 3 wires`, and one line per wire with
   the terminal it reaches there (`wire … -> …:1`).
6. **Go there and back.** Click the reference on page 2 to select it, then click **Go to
   Reference**. Expected: the `field` tab is current, centred on `20,5`, with its reference point
   selected, and `Page 3 "field", at the other end of cable … (/3.1).` Click **Go to Reference**
   again: back on page 2. Without a selection the button asks `Select a cable reference:`.
7. **Move a page.** Click **Move Page** (Pages group). It fills in
   `movepage {"page":"<this page's id>","index":<new position>}`; replace `<new position>` with
   `1` and press Enter. Expected: the page is now first; both references show the new numbers (the other page's when you switch to it); the change count goes up by one only. **Rename Page** works the same way (`"title"`); a
   rename changes the hover lens, not the marker text (it shows numbers, not titles).
8. **Erase one end.** On the `field` page, drag a crossing window (right to left) from about
   `45,15` to `5,-5` and `erase`. Expected: the short wires, the slash and the reference go (the
   wires themselves stay, still connected to the component on that page); the other page's
   marker turns red `/?` (click its tab to see it: an edit in the drawing redraws only the page
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
3. **New page.** Click **+** in the panel's header (or **New Page** in Tool Sets > ECAD > Pages):
   a small name box opens with `3` selected. Type `Spare`, press Enter (or **OK**): page 3 is
   added and shown at once, one `u`. **Cancel** (or Esc) does nothing. **Rename Page** in Tool
   Sets opens the same box for the page shown, prefilled with its title.
4. **Rename.** Double-click `Spare`, type `Spare 2`, press Enter. Type `u` at the command line:
   back to `Spare`. An empty title, or one over 200 characters, is refused under the field and
   nothing is recorded.
5. **Reorder.** Drag `Spare` above `Supply`: it becomes page 1, and the reference texts on the
   other pages change their page numbers (`/2.1` becomes `/3.1`). Click `Spare`, then the down
   arrow in the header twice to put it back (or Alt+Down). Each move is one `u`.
6. **Keyboard.** After clicking a row, Up and Down move an outline, Enter shows that page, F2
   renames it, Space previews it. Any other key goes to the command line as usual.
7. **Undo brings the page.** On `Field`, draw a wire. Click `Supply`. Type `u`: `Field` comes to
   the front with the wire gone, and the command line says `Page 2 "Field" brought to the front.`
   `redo` puts the wire back, still on `Field`.
8. **Preview.** On `Supply`, click the magenta reference point, then **Preview Reference** (Tool
   Sets > ECAD > Wiring). A read-only window over the drawing shows `Field` around the other end,
   with a magenta ring on it and `/1.2` beside it. Scroll to zoom, drag to pan; nothing in it can
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
   Click on the page.
   Expected: `Placed -K01 (Contactor / relay coil) with terminals A1, A2.`; `-K01` left of the
   coil and `A1`, `A2` beside its terminals. Press Enter.
2. **A contact of the same device.** In the Find Symbol window, type `cont` and click the
   **NO contact** row. Type `D` and Enter; the prompt offers `<-K01>`: press Enter. The prompt
   now says `joins -K01 as 13/14`. Click to the right of the coil, then press Enter.
   Expected: `Added NO contact 13/14 to -K01 (now on page 1).`; the contact shows `-K01` and
   `13`/`14`.
3. **Another page.** Make page 2 (**+** in the PROJECT panel, or **New Page**). In
   Find Symbol type `-K01` and click the `-K01` row; at `Enter symbol to add to -K01` type `nc`
   and Enter, then click on page 2 and press Enter.
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

Typed instead, in the ECAD command window: `place {"symbol":"coil","at":[0,0]}`,
`place {"symbol":"no","at":[40,0],"device":"<the coil's component id>"}`,
`setprop {"component":"<id>","key":"tag","value":"K5"}`, `symbols {"query":"-Q"}`,
`devices {}`, `tagsettings {"padding":3}` (three-digit numbers from then on). A tag that exists is refused by `setprop` with the ways forward; add
`"merge":true` to make the device part of that one. `place` with a `tag` that exists adds the
symbol to that device.

## 5G. Devices tab (by clicking)

After 5F (a coil `-K01` on page 1 with contacts on pages 1 and 2). Design 016 §16.

1. **Open the tab.** In the PROJECT panel click **Devices** (beside **Pages**). Expected: `-K (1)`
   and `▸ -K01  Contactor / relay coil`.
2. **Unfold and select.** Click `-K01`. Expected: its symbols appear under it as `/1.1 Contactor /
   relay coil`, `/1.2 NO contact`, `/2.1 NC contact` (page.column), and its properties show at the
   bottom of the panel.
3. **Go to.** Click the `/2.…` row. Expected: page 2 is shown in the drawing tab, centred on that
   contact, which is selected. Nothing is recorded.
4. **Edit.** Click the **Description** field, select all, type `Main contactor`, press Enter.
   Expected: the row reads `-K01  Main contactor`. Type `u` at the command line: it is back.
   **Location** `A1` is stored as `+A1`.
5. **Filter.** Type `q` in the filter: only `-Q` devices remain. Clear it.
6. **A tag that exists.** Select another device and set its **Tag** to `K01`. Expected: a red line
   says the tag is used by `-K01`, with **Make it the same device**; clicking it merges (one `u`), or
   explains why it cannot.

## 6. Check connected movement, undo/redo and pages

1. **Connected movement.** Select the rectangle and its marker with a window selection, type
   `move` at the command line, pick a base point and drag.
   Expected: the wire's end follows while you drag, and stays attached after the move.
2. **Undo/redo.** Type `u` three times, then `redo` three times.
   Expected: the drawing steps back and forward exactly, and the IDs stay the same.
   `undo {"mode":"mark"}` is refused as not supported on ECAD pages.
3. **Multiple pages.** Click **+** in the PROJECT panel and press Enter in the name box (or
   `newpage {"title":"2"}` in the ECAD command window). Expected: page 2 replaces page 1 in the one drawing tab, "… - page 2: 2".
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
   - A file such as `demo.ecad.tail-<offset>-<…>` appears only after a save was interrupted. It
     keeps the incomplete bytes that the next save removed from the end of the project file.

## 7A. Let an AI agent work on a project (MCP, headless)

`ecad-mcp` (brief 011) lets an AI agent such as Claude Code read and change an ECAD project
through the Model Context Protocol. It runs **headless**: no window; the agent works on the
project file in a process of its own. Watching an agent work in the running trial (bridged mode)
is a later brief.

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

- **Read** (always): `ecad_project` (pages, components and terminals, wires and their ends,
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

## 8. Diagnostics and the debugger

### The debugger panel

Click **Debugger** in Tool Sets > **ECAD Dev**; click it again (or **Close** in the panel) to
hide it. It is a
read-only developer view on the right of the window. It follows one trail: **selected drawing
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
  its lock, header and record positions), **Diagnostics** (pages and runtime events, which are
  never saved in the project) and **Checks**.
- **Trail.** Clicking a selected object, a model object or a transaction shows the trail below
  the view: the objects (expand one for its stored value), the transactions that touched them,
  and the chosen transaction's command, ops and record.
- **Transaction states.**
  - `[pending]` (orange): only in memory, not in the file. Save writes it.
  - `[saved]` (green): durably saved, at the file offset shown. **Read bytes** reads that record
    from the file and checks that it holds exactly this transaction.
  - `[written, unconfirmed]`: a Save As wrote it, but could not confirm it is durable; Save
    confirms it.
  - `[pending (save failed)]`: a save of it failed; Save retries it.
- **Checks** run only when you press their button, on copies, and never change the project:
  **Verify replay** (replaying the history gives the live document), **Verify projection** (the
  drawing equals a fresh projection of the document) and **Verify file** (re-read the file:
  checksums, hash chain and history, compared with the project). Each result shows the
  transaction and the time it ran against.

### Worked example: what to inspect when placing, wiring, moving, undoing and saving

Start a new project (`cargo run --release -p ecad-trial`) and click **Debugger** (Tool Sets >
ECAD Dev). Type commands in the ECAD command window (ECAD Dev > JSON Command). IDs and offsets will differ from any shown here.

1. **At the start.** Expected: `live at seq 1`; **History** shows `#1 [pending] New project`.
   The PROJECT panel says no unsaved changes: a new project's own setup is not counted as a change until
   the project has a file, and the panel says so.
2. **Place.** `place {"at":[100,0],"rotation":90}`, then click the coil in the drawing.
   Expected: `live at seq 2`. **Selection** shows
   `Block Reference <handle> → item itm-… → rep-… main of untagged component → component cmp-…`.
   Click that line. The trail lists the item, the representation, the component and its two
   terminals (`…:A1`, `…:A2`), and `#2 [pending] Place symbol`. Click `#2`: it was recorded by
   command `place`; its ops include `CreateComponent`, two `AddTerminal`, `AddItem` and
   `LinkRepresentation`. Expand one: `before: (none)` and the new value after. The record says
   **Pending: only in memory, not in the file.**
3. **Wire.** `wire {"points":[[60,0],[90,0]]}`, then click the wire and its line in
   **Selection**. Expected: the trail lists the wire's item, its route representation, the
   conductor and the coil terminal it is attached to (`…:A1`); the transactions are `#3 Wire`
   and `#2 Place symbol` (which created that terminal). In `#3`, expand `AttachEnd`: before
   `"terminal": null`, after the terminal's ID.
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

### Inspector snapshots

- **Exporting.** Start with `--snapshots <folder>`, then click **Export Inspector Snapshot** in
  Tool Sets > ECAD Dev (shown only with `--snapshots`). Snapshots are also written before a project is discarded, and at exit. They are
  diagnostic files, not saved projects, and cannot be reopened. Their format is
  `ecad-inspect/2`: the design, its history and, new in stage 2b, the file's persistence facts
  (path, header, record offsets, lock).
- **Viewing them.** Open `ecad/tools/inspector/index.html` in a browser (on native Windows,
  double-click `C:\dev\ecadcraft\ecad\tools\inspector\index.html`, or type that path in the
  address bar) and load the files from the folder given to `--snapshots`, for example
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
| move+wires ms | In the same session, move that component together with every wire attached to it, as a window around them selects them: each wire's far end leaves its terminal, so the ends are matched against the page's terminals (brief 014). |
| draw wire ms | In the same session, draw one wire between two terminals on page 1 with the `wire` command, as the mouse wire tool does (brief 014). |
| switch ms | In a headless session with one page active, make another page the active page with `openpage` (the path of the Project panel and Go to Reference), and back to page 1 after each: median over up to 10 pages spread over the project. Target (design 016): at most 5 ms on large, 2 ms on medium. |
| preview ms | Draw the read-only preview of a page: project it, build its display list and rasterise it at 480 × 320. Target: at most 15 ms on large, 8 ms on medium. |
| panel ms | Rebuild the Project panel's rows after one `renamepage` (page order and titles only). |
| devices ms | Rebuild the Devices tab's list (every device from the tag index, with its symbols). |
| goto ms | From the Devices tab, show a device symbol's page and select it. |

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

- **Pages and the Project panel** (brief 016): there is no Delete page (no command exists yet).
  The preview shows no terminal or wire-end marks and cannot be edited. Page views, the panel's
  width and whether it is collapsed are remembered for the session only. The page number shown is
  the page's position; designer-chosen page numbers come later (F6). The JSON form of ECAD
  commands needs the ECAD command window: CADCraft's own command line takes a space as Enter.

- It is an experiment. No native file dialogs (the trial has its own path dialog) and no ECAD
  menu (CADCraft shows no top-level menu supplied by an embedder; use the ECAD tab or type the
  command name). The ECAD tab's buttons are text, not icons.
- **The click-based tools** (brief 008):
  - Terminal capture is ECAD's own: CADCraft's object snap may also show its marker (for example
    an Endpoint square) beside the green ring; the ring decides where the click goes. The
    crosshair itself stays under the pointer; the rubber band ends on the terminal. Tab does
    not cycle through terminals. Only the cursor is captured: a typed point attaches only when it
    is exactly a terminal's point.
  - Rotate and Mirror are options: type `R` or `M` and press Enter (a single key press does not
    rotate). Only steps of 90° by mouse; other angles through the JSON form.
  - Add Terminal shows no pick box (so that object snaps work at its prompt); the cyan ring shows
    where a click would put the terminal.
  - Add Terminal's prompt names the number the next terminal gets for the component under the
    cursor. With the cursor off every component it names a number only when all components on
    the page would get the same one; otherwise it says `Specify point for a terminal on a
    component`, and a typed point gets the smallest unused number of the component it lands on.
  - ECAD objects are drawn in the Model tab only. In a paper-space layout (Layout1 …), also while
    working inside one of its viewports, the six
    tools refuse to start (`Switch to the Model tab to draw ECAD objects.`), a tool already
    running refuses points and picks there, and no terminal marks or terminal tooltips are
    shown.
  - The preview name reads `-> A1` (the drafting font has no arrow character).
  - Terminal and wire-end marks are drawn only in the main drawing area, not in the other tiled
    model-space views (VPORTS), and only on the Model tab.
  - Terminal and wire-end marks use the dark theme's colours; on the light theme cyan and green
    are hard to see.
  - Only the built-in coil can be placed. Set Property, Add to Cable, Delete, Open Page and Move
    Page still open the ECAD command window with a template.
- **Devices and tags** (brief 018):
  - No contact cross-references or mirror tables yet (F3), no terminal strips (F5), no location
    boxes (F6), no `.ecadlib` symbol libraries: the 18 starter symbols are built in.
  - Tag texts and terminal labels are placed automatically (left of a symbol, or above it when
    rotated); they cannot be moved. Editing them alone is refused; erasing one erases its symbol.
  - Tag padding and each symbol's prefix are project settings, changed by typing
    `tagsettings {"padding":3,"prefixes":{"coil":"KA"}}` in the ECAD command window (`null` for a
    symbol goes back to the built-in prefix; `tagsettings {}` lists them). There is no dialog for
    them yet. A project that changes them is refused by older builds.
  - Find Symbol is a small window of the trial's own (Tool Sets holds buttons only); typing at the
    `place` prompt finds the same symbols.
  - Copying a device's symbol makes a new device with the next tag and every terminal of the
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
  - Wires connect to their terminals with **Connect** after continuing; there is no automatic
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
  in the ECAD command window saves only into a folder that already exists.
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
