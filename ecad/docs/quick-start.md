# ECAD quick start: run and try the current app

This is the **one maintained guide** for building, running and manually testing the ECAD trial
app. Other READMEs link here instead of repeating it. It is updated in the same pull request as
any change that affects these instructions (see `ecad/CLAUDE.md`).

> **What you are running.** `ecad-trial` is an experiment, not an ECAD application. It is
> CADCraft's drawing window with the ECAD commands and one ECAD project open at a time. Projects
> are saved as `.ecad` files (persistence stage 2a). A read-only debugger panel (stage 2b)
> opens from the banner; it is never shown unless you open it. An **ECAD** tab in Tool Sets has
> buttons for the ECAD commands; eight of them (Convert to Component, Add Terminal, Place Symbol,
> Wire, Connect, Disconnect, Cable Slash, Cable View) work by clicking, with prompts at CADCraft's
> command line.

## Which revision this describes, and what has been tried

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
transaction and never changes the banner's change count. Each cable's view is remembered for you
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
nobody has run it on a display yet. The reference end's marker text `->` is a placeholder: it
becomes the live reference when brief 010 is merged.

Three kinds of evidence exist, and they are kept apart:

| Kind | Who | Covers |
| --- | --- | --- |
| **Automated checks** | `cargo xtask ci` and plain `cargo test --workspace`, in GitHub CI on Linux and natively on Windows (jobs `ecad` and `ecad-windows`), and locally on Windows with Rust 1.99.1 | The file tests run on Windows with none ignored (brief 002): a file renamed over or deleted and recreated behind an open project is refused; Save As is confirmed only after a real folder flush; read-only files, a lock that cannot be taken, the unnamed lock holder on Windows, retries while another program briefly holds a file, and a killed process's lock released within 2 s (`ecad/crates/ecad-store/tests/disk_contracts.rs`, `ecad/crates/ecad-file/tests/disk_contracts.rs`, `disk_edge_contracts.rs`). The model outcomes of sections 5 to 7 (what the project holds, not how it looks), across the trial, tools, mouse and files tests together: `the_quick_start_trial_runs_headless` (`ecad/apps/ecad-trial/tests/trial.rs`) follows the typed version of sections 5 to 7, saving through the dialog as `~/ecad-projects/demo` under a temporary home folder and checking the banner line and tab title quoted in section 7; the click-based steps are in `mouse.rs` and `tools.rs` (next row and below); plus fault injection at every step of Save and Save As, a crash in the middle of a save, a killed process holding the lock, a second opener, external changes and damaged files. **Stage 2b:** section 8's worked example (`ecad/apps/ecad-trial/tests/debugger.rs`, `the_quick_start_debugger_example_runs_headless`); the trail from a selected wire to its record bytes, compared with the file; pending, unconfirmed and failed saves never shown as saved; a busy project not waited for; history views and checks leaving the live project unchanged (`ecad/crates/ecad-cadcraft/tests/debugger.rs`) **ECAD Tool Sets tab:** every button names an existing command; opening the ECAD tab and clicking Convert to Component converts the selection once, and `u` undoes it; a command waiting at a prompt is cancelled first; templates fill the Command field and run nothing, and an unedited template is refused; IDs come from the selection, and a huge selection is followed only so far; a read-only project behaves as with typed commands (`ecad/apps/ecad-trial/tests/tools.rs`). Without extra tabs, Drafting and Modeling are unchanged (`crates/ui-egui/tests/toolsets_extra.rs`) |
| **Automated checks, brief 008 (ECAD tools by mouse)** | `cargo xtask ci` and plain `cargo test --workspace` locally on Windows (Rust 1.99.1), 2026-11-01; CADCraft's own tests for the overlay change (`cargo test -p cadcraft-engine -p cadcraft-ui-egui`) | Section 5 by clicks and keys in a 1280×800 window (`ecad/apps/ecad-trial/tests/mouse.rs`, `the_quick_start_example_is_built_by_clicks_and_keys`): window selection, **Convert to Component**, **Add Terminal** with `40,10` and `0,10`, **Place Symbol** with `R`, **Wire** by four clicks ending within 5 pixels of the coil's terminal; Esc while drawing a wire writes nothing, and `u` undoes one object; hovering names the coil and the wire and writes nothing. Every interactive form records exactly the transaction its JSON form records for the same values; Esc at every prompt; capture inside and outside the aperture; error messages; hostile input (`ecad/crates/ecad-cadcraft/tests/interactive_contracts.rs`). Marks keep their size at every zoom (`crates/ui-egui/tests/owner_overlay.rs`). Not checked: how it looks (reference screenshots are taken by the tester) |
| **Automated checks, brief 004 (cable slash)** | written with the branch; run by the tester and by CI on Linux and Windows | Section 5A: **Cable Slash** clicked by its label, the first point clicked, the crossing count shown before the second point, one transaction, three magenta dots, `u` in one step, and a refused second slash that keeps its first point (`ecad/apps/ecad-trial/tests/cable_slash.rs`). The JSON and click forms record the same transaction; the crossing rules C1 to C6; every refusal leaves the history unchanged; undo, redo, save and reopen; erasing the slash keeps the cable and `ecaddelete {"cable":…}` keeps the wires; `cableadd` refuses a wire already in a cable; hover and the debugger trail; 10 000 wires accepted and 10 001 refused (`ecad/crates/ecad-cadcraft/tests/cable_slash_contracts.rs`). Not checked: how it looks (reference screenshots are taken by the tester) |
| **Automated checks, brief 005 (cable views)** | written with the branch; run by the tester and by CI on Linux and Windows | Section 5B: **Cable View** clicked by its label, the slash clicked (fan: a trunk and six fan lines), the trunk clicked (back to traditional), no transaction and no change in the change count (`ecad/apps/ecad-trial/tests/cable_view.rs`). Ten switches by JSON and by clicks leave the document, history, head, IDs and nets unchanged; the fan of design 005's fixture is its worked example on layer `ECAD-CABLES`; `u` and `redo` skip the switch; moving, erasing, copying, Connect, Convert to Component and Add Terminal on a fan line are refused with the spec's message, while JSON forms still act on the wires; moving equipment beside a fan moves the hidden routes; views remembered per cable across save, close and reopen; an unreadable, oversized or unwritable settings file never fails anything; fans of 1, 2 and 20 wires, bent and right-to-left routes, loose ends and hostile coordinates; hover; selection-based switching; the JSON form's errors (`ecad/crates/ecad-cadcraft/tests/cable_view_contracts.rs`). Not checked: how it looks, and whether the switch feels instant (reference screenshots and the feel are for the tester and you) |
| **Automated checks, brief 003 (the trial on native Windows)** | local checks from `ecad/CLAUDE.md` on native Windows 11 (Rust 1.99.1), 2026-11-01, branch `ecad/windows-trial`; CI runs them on Linux and Windows | Typed paths (`ecad/apps/ecad-trial/tests/paths_contracts.rs`): `~`, `~/` and `~\` from the home folder, also with `HOME` unset (a child process); `~` with no home folder known is refused and no folder named `~` is ever made; drive letters, backslashes, spaces, non-ASCII and UNC paths; saving and reopening in folders with spaces and non-ASCII names; a reserved name (`CON.ecad`, `nul`, `LPT1`), `.`, `..`, a drive-relative path (`C:`, `C:demo`) or a trailing dot refused with a message; one pair of surrounding double quotes (Explorer's "Copy as path") removed; a path normalised once; `--snapshots ~/…` under the home folder, never a `~` folder (also `windows_trial_contracts.rs`, the tester's); a case-variant spelling of the project's own file saved without a replace question (one file on Windows); the platform's default folder in the dialog, created by the first save into it on Linux (a test must not create `C:\dev\ecad-projects`). Arguments (`args_contracts.rs`): a project path, `--snapshots`, both, `--help`, `--version`, usage errors, and a name that is not valid Unicode. Not checked: the console window of a release build, and anything on a display |
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

- **The banner's `Command:` field (top of the window).** Use this for ECAD commands, which take
  JSON parameters, for example `wire {"points":[[0,0],[40,0]]}`. It takes a whole line, spaces
  included, so it also accepts CADCraft command lines such as `line 0,0 40,0`. Press Enter or
  **Run**. Copy and paste (Ctrl+C, Ctrl+V) work with other programs such as Notepad.
- **CADCraft's command line (bottom of the drawing area).** A space there means Enter, so it
  cannot take JSON. Use it for plain CADCraft commands and for the prompts of mouse-driven
  commands such as `move`.
- **The mouse** works as in CADCraft: drawing, selecting and dragging.
- **The ECAD tab in Tool Sets** (the left panel: click **ECAD** beside Drafting and Modeling),
  with one button per implemented ECAD command, in groups (Components, Wiring, Edit, Pages,
  Inspect). A button cancels a command waiting at a prompt first, as CADCraft's own Tool Sets
  buttons do.
  - **Convert to Component** converts the current selection at once; with nothing selected it
    asks you to select objects (click or window), and Enter converts them.
  - **Add Terminal**: click on a component's outline to add a terminal there; repeat; Enter to
    finish.
  - **Place Symbol**: the coil follows the cursor; `R` then Enter rotates it by 90°, `M` then
    Enter mirrors it; click to place; repeat; Enter to finish.
  - **Wire**: click points; an end on a terminal attaches (a green ring shows before you click);
    a click on a terminal after the first point, or Enter, finishes the wire.
  - **Connect**: click a wire near its end, then a terminal. **Disconnect**: click a wire near
    an attached end.
  - These six put their prompts at CADCraft's command line (bottom of the drawing area) and leave
    the Command field empty. Answer by clicking, or type a point (`40,10`) or an option letter and
    press Enter. Enter (or Space, or a right-click) finishes; Esc cancels what is not done yet
    (objects already added stay); `u` afterwards undoes the last object.
  - **Inspect Project** runs at once and shows the project's IDs at the command line.
  - **Set Property, Add to Cable, Delete Electrical Object, New Page, Open Page** only fill in
    the Command field with an editable line, and a note under the field says what to fill in.
    Replace each `<placeholder>`, then press Enter or **Run**. A line that still holds a
    placeholder is refused, never run. IDs come from the current selection where it names them:
    select a wire before **Set Property** or **Delete Electrical Object**, and its ID is filled
    in. The selection is left as it is.
- **Typing a command name** (`wire`, `addterminal` …) in either field starts the same
  click-based form as its button. `name {json}` in the banner's field runs the JSON form with no
  prompts, as scripts do.

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
of the drawing area; type there (or in the banner's field) only where a step says so.

1. **Draw a rectangle.** In the Command field: `line 0,0 40,0 40,20 0,20 c`. Then type `zoom e`
   (or zoom with the mouse wheel) so it fills the view; leave room on the right for the coil.
   Expected: four lines; the banner shows 1 change not saved.
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
4. **Place a coil.** Click **Place Symbol**. The coil follows the cursor, with cyan rings at its
   two terminals. Type `R` and Enter: it turns by 90° and the prompt says `rotation 90`. Type
   `100,0` and Enter (or click there), then Enter to finish.
   Expected: `Coil placed: component <id>, terminals A1, A2.`; a rotated coil with rings at 90,0
   and 110,0.
5. **Draw the wire.** Click **Wire**. Move onto the rectangle's terminal at 40,10: a green ring
   and `-> 1` appear before you click. Click. Then click at about 70,10 and at about 70,0 (bend
   points; F8 for Ortho helps). Move onto the coil's left terminal at 90,0 until the green ring
   and `-> A1` show, and click: the wire is finished.
   Expected: `Wire <id> drawn; end A attached to 1, end B attached to A1.`; green dots on both
   ends, and no grey squares. A click near a terminal but outside the green ring does not attach:
   that end shows a grey square instead.
6. **Look before you type IDs.** With no command running, rest the pointer on the coil, on the
   ring at 40,10 (or the coil's ring at 90,0) or on the wire. Expected: a tooltip with
   `Component (no tag)` and its terminals, `Terminal 1` (or `Terminal A1`) and its wire, or `Wire (no tag)` with `End A: terminal 1 of …` and
   `End B: terminal A1 of …`. Nothing changes in the banner's change count.

Typing instead of clicking still works, in the banner's Command field:
`convertcomponent` (with the lines selected), `addterminal {"handle":"<handle of the right-hand
edge>","at":[40,10]}`, `place {"at":[100,0],"rotation":90}`,
`wire {"points":[[40,10],[70,10],[70,0],[90,0]]}`. The headless test
`the_quick_start_trial_runs_headless` follows that typed version.

## 5A. Make a cable (by clicking)

A cable groups existing wires; the wires themselves do not change. You draw it as a short line
(the *slash*) across the wires. This example uses a page of its own so the coordinates are easy
to read.

1. **A page with three wires and a plain line.** In the Command field:
   `newpage {"title":"cables"}`. Then click **Wire**, click at `0,0` and `60,0` and press Enter;
   do the same at `0,5` → `60,5` and `0,10` → `60,10` (typing the points at the command line, or
   `wire {"points":[[0,0],[60,0]]}` in the Command field, works too). Draw a plain line with
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
   banner's change count goes up by one. The wires are listed bottom to top: the order is the
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

`cableslash {"from":[28,-3],"to":[32,18]}` in the Command field does the same as steps 2 and 3
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
   `Cable <first 8 hex digits>… now shown in fan view.` The banner's change count does **not**
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
5. **A reference end (brief 010's model).** Brief 010 adds the command that places a cable's
   reference end. When a cable has one on the page (a `reference` drawing of the cable: one
   point), the fan stops at that end: no spine, bends or tails there; the trunk runs to a
   magenta flag beside the point, and the flag shows `->` until brief 010 makes it the live
   reference. The loose-end squares at that end are not drawn. Erasing the point fans that end
   again; `u` brings it back.

Typed: `cablecore {"conductor":"<id>","core":"1"}` sets one label (`"core":null`, `""` or spaces
clear it); `cablecore {"cable":"<id>","cores":["1","2",null]}` sets every member at once, one
label (or `null`) per member in member order, as one transaction.
`cablefan {"cable":"<id>","end":"A","at":[x,y]}` moves where the trunk joins end A (the point is
put onto the spine); `"at":0.25` places it a quarter of the way along the spine; `"at":null`
returns it to the middle. Without `end`, a point picks the nearer end.

## 6. Check connected movement, undo/redo and pages

1. **Connected movement.** Select the rectangle and its marker with a window selection, type
   `move` at the command line, pick a base point and drag.
   Expected: the wire's end follows while you drag, and stays attached after the move.
2. **Undo/redo.** Type `u` three times, then `redo` three times.
   Expected: the drawing steps back and forward exactly, and the IDs stay the same.
   `undo {"mode":"mark"}` is refused as not supported on ECAD pages.
3. **Multiple pages.** `newpage {"title":"2"}`.
   Expected: a second tab, "… - page 2". Draw or wire something on it, then switch tabs.
   Closing page 2's tab asks nothing, and the project stays open with 1 page.
   To reopen it, type `ecadinspect` and find page 2's ID in the reply, then type
   `openpage {"page":"<page id>"}`. Expected: its content is back.

## 7. Save, close and reopen

1. **Save.** Press Ctrl+S, or click **Save** in the banner. A new project asks for a file name
   in the trial's own dialog: type `~/ecad-projects/demo` (on native Windows
   `C:\dev\ecad-projects\demo`) and press Enter. `.ecad` is added.
   Expected:
   - the banner shows `file /home/<you>/ecad-projects/demo.ecad, no unsaved changes` (on native
     Windows `file C:\dev\ecad-projects\demo.ecad, no unsaved changes`; the whole line reads
     `Project "demo": 2 pages open, file …, no unsaved changes.` after section 6);
   - the tab is called "demo - page 1";
   - the command line says `Saved: …`.
2. **Save again.** Make an edit, then Ctrl+S.
   Expected: no dialog; the banner returns to no unsaved changes.
3. **Close.** Make one more edit, then close the last page, or the window.
   Expected: "Save changes to project "demo"?" with **Save**, **Discard** and **Cancel**:
   - Cancel keeps everything open;
   - Save writes the change, then closes;
   - Discard closes, and the file keeps its last saved state.
4. **Reopen.** Start the app with the file, or click **Open project…** in the banner when no
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

## 8. Diagnostics and the debugger

### The debugger panel

Click **Debugger** in the banner; **Hide debugger** (or **Close** in the panel) hides it. It is a
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

Start a new project (`cargo run --release -p ecad-trial`) and click **Debugger**. Type commands in
the banner's Command field. IDs and offsets will differ from any shown here.

1. **At the start.** Expected: `live at seq 1`; **History** shows `#1 [pending] New project`.
   The banner says no unsaved changes: a new project's own setup is not counted as a change until
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

- **Exporting.** Start with `--snapshots <folder>`, then click **Export inspector snapshot** in
  the banner. Snapshots are also written before a project is discarded, and at exit. They are
  diagnostic files, not saved projects, and cannot be reopened. Their format is
  `ecad-inspect/2`: the design, its history and, new in stage 2b, the file's persistence facts
  (path, header, record offsets, lock).
- **Viewing them.** Open `ecad/tools/inspector/index.html` in a browser (on native Windows,
  double-click `C:\dev\ecadcraft\ecad\tools\inspector\index.html`, or type that path in the
  address bar) and load the files from the folder given to `--snapshots`, for example
  `C:\dev\ecad-trial-snapshots`; under WSL, from `\\wsl$\<distro>\home\<you>\<folder>`. It shows
  whether each transaction was saved, and where.

## 9. Current limitations

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
  - Only the built-in coil can be placed. Set Property, Add to Cable, Delete, New Page and Open
    Page still fill in the Command field.
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
  in the Command field saves only into a folder that already exists.
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
