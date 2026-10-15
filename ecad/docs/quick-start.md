# ECAD quick start: run and try the current app

This is the **one maintained guide** for building, running and manually testing the ECAD trial
app. Other READMEs link here instead of repeating it. It is updated in the same pull request as
any change that affects these instructions (see `ecad/CLAUDE.md`).

> **What you are running.** `ecad-trial` is an experiment, not an ECAD application. It is
> CADCraft's drawing window with the ECAD commands and one ECAD project open at a time. Projects
> are saved as `.ecad` files (persistence stage 2a). The live debugger (stage 2b) does not exist
> yet.

## Which revision this describes, and what has been tried

This guide describes branch `ecad/toolsets` (the ECAD tab in Tool Sets, PR #12 into `dev`), built
on stage 2a as merged into `dev` (`4a5a826`). Three kinds of evidence exist, and they are kept
apart:

| Kind | Who | Covers |
| --- | --- | --- |
| **Automated checks** | `cargo xtask ci` and GitHub CI (Linux, Rust 1.97 and 1.99) | Everything in sections 4 to 6 as headless tests (`ecad/apps/ecad-trial/tests/trial.rs`, `the_quick_start_trial_runs_headless`), plus fault injection at every step of Save and Save As, a crash in the middle of a save, a killed process holding the lock, a second opener, external changes and damaged files. **ECAD Tool Sets tab:** every button names an existing command; opening the ECAD tab and clicking Convert to Component converts the selection once, and `u` undoes it; a command waiting at a prompt is cancelled first; templates fill the Command field and run nothing, and an unedited template is refused; IDs come from the selection, and a huge selection is followed only so far; a read-only project behaves as with typed commands (`ecad/apps/ecad-trial/tests/tools.rs`). Without extra tabs, Drafting and Modeling are unchanged (`crates/ui-egui/tests/toolsets_extra.rs`) |
| **Agent smoke run on a virtual display** | the development agent, on Xvfb with software rendering, not a person | The window starts; a typed wire; Ctrl+S opens the Save As dialog and saves; the tab and banner show the file; the file reopens from the command line; a second window on the same file is read-only and says why; a project left by a killed process reports its reclaimed lock; Ctrl+Q with unsaved changes shows Save/Discard/Cancel, and Save writes the file, quits and releases the lock |
| **Agent smoke run on a virtual display, ECAD Tool Sets tab** | the development agent, as above, 2026-10-15 | Tool Sets shows **Drafting**, **Modeling** and **ECAD** tabs; the ECAD tab shows Components, Wiring, Edit, Pages and Inspect; clicking a rectangle's four lines and then **Convert to Component** converted them (the banner's change count went up by one, the lines stayed selected). The first version (a group at the bottom of Drafting) was also run: **Wire** filled the Command field with its template and note, and Enter on it unedited was refused. Not looked at: a real WSLg display |
| **Tried by you (the owner), ECAD Tool Sets** | you, on WSL, 2026-10-15, the first version of PR #12 (a group at the bottom of Drafting) | The ECAD group appeared; you asked for it as a tab of its own beside Drafting and Modeling, which this revision does. The revised tab is not tried yet: see section 4's "ECAD tab in Tool Sets" and section 5 |
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
   `git switch ecad/toolsets`.

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
`git switch ecad/toolsets` and `git merge --ff-only origin/ecad/toolsets`). Then
rebuild by running it (section 3); Cargo rebuilds what changed.

## 3. Running again

```sh
cd ~/ecadcraft/ecad
export CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_INCREMENTAL=0
cargo run --release -p ecad-trial                                    # a new project
cargo run --release -p ecad-trial -- ~/ecad-projects/demo.ecad       # open a saved project
cargo run --release -p ecad-trial -- --snapshots ~/ecad-trial-snapshots   # allow diagnostic snapshots
```

`--snapshots <folder>` and a project file can be given together.

## 4. Where to type

- **The banner's `Command:` field (top of the window).** Use this for ECAD commands, which take
  JSON parameters, for example `wire {"points":[[0,0],[40,0]]}`. It takes a whole line, spaces
  included, so it also accepts CADCraft command lines such as `line 0,0 40,0`. Press Enter or
  **Run**.
- **CADCraft's command line (bottom of the drawing area).** A space there means Enter, so it
  cannot take JSON. Use it for plain CADCraft commands and for the prompts of mouse-driven
  commands such as `move`.
- **The mouse** works as in CADCraft: drawing, selecting and dragging.
- **The ECAD tab in Tool Sets** (the left panel: click **ECAD** beside Drafting and Modeling).
  A convenience for testing, with one button per implemented ECAD command, in groups
  (Components, Wiring, Edit, Pages, Inspect). Buttons never start anything of their own: they go through the same command path as
  the Command field, so validation, undo and saving work exactly as when you type.
  - **Convert to Component** runs at once on the current selection (select the objects first).
  - **Inspect Project** runs at once and shows the project's IDs at the command line.
  - **Every other button** (Add Terminal, Place Symbol, Wire, Connect, Disconnect, Set Property,
    Add to Cable, Delete Electrical Object, New Page, Open Page) only fills in the Command field
    with an editable line, and a note under the field says what to fill in. Replace each
    `<placeholder>`, then press Enter or **Run**. A line that still holds a placeholder is
    refused, never run.
  - IDs and handles come from the current selection where it names them: select a wire before
    **Set Property** or **Delete Electrical Object**, and its ID is filled in. The selection is
    left as it is. If a CADCraft command is waiting at a prompt, a button cancels it first, as
    CADCraft's own Tool Sets buttons do.

Each ECAD command answers at the command line, for example with the IDs it created. Note IDs
you want to check later.

## 5. Example: two components connected by a wire

1. **Draw a rectangle.** In the Command field: `line 0,0 40,0 40,20 0,20 c`.
   Expected: four lines; the banner shows 1 change not saved.
2. **Make it a component.** Select the four lines with a window selection (drag from the top
   left to the bottom right), then type `convertcomponent`, or click **Convert to Component** in
   the ECAD tab of Tool Sets.
   Expected: the reply shows `"component":"<id>"`. **Note this component ID.** The rectangle now
   selects as one group.
3. **Give it a terminal.** Click the rectangle's right-hand edge and type `list` at the command
   line to see its handle, then type `addterminal {"handle":"<that handle>","at":[40,10]}`.
   Expected: a terminal marker at 40,10.
4. **Place a second component.** `place {"at":[100,0],"rotation":90}`. (With the ECAD tab:
   click **Place Symbol**, replace `<x>` and `<y>` with `100` and `0`, set `"rotation":90`, and
   press Enter.)
   Expected: a coil symbol, rotated, with its terminals at 90,0 and 110,0.
5. **Connect the terminals.** `wire {"points":[[40,10],[70,10],[70,0],[90,0]]}`.
   Expected: a wire from the rectangle's terminal to the coil, and a reply listing two attached
   ends (`"attached":[…]` with both terminals).

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
   in the trial's own dialog: type `~/ecad-projects/demo` and press Enter. `.ecad` is added.
   Expected:
   - the banner shows `file /home/<you>/ecad-projects/demo.ecad, no unsaved changes`;
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

   Expected: it opens read-only and says that another session is editing it. Save is refused
   there; Save As can save a copy under another name.
7. **Files beside the project.**
   - `demo.ecad.lock` is normal. It marks which session may write the file, and it is empty when
     nothing holds it. If the app is killed or crashes, the next open reports and reclaims the
     leftover lock; nothing stays locked.
   - A file such as `demo.ecad.tail-<offset>-<…>` appears only after a save was interrupted. It
     keeps the incomplete bytes that the next save removed from the end of the project file.

## 8. Diagnostics

- **Inspector snapshots.** Start with `--snapshots <folder>`, then click
  **Export inspector snapshot** in the banner. Snapshots are also written before a project is
  discarded, and at exit. They are diagnostic files, not saved projects, and cannot be reopened.
- **Viewing them.** Open `ecad/tools/inspector/index.html` in a Windows browser and load the
  files from `\\wsl$\<distro>\home\<you>\<folder>`.
- **The live debugger.** Not available yet: it is stage 2b, built after your trial of this stage.
  This section will describe how to open and use it once it exists.

## 9. Current limitations

- It is an experiment. No native file dialogs (the trial has its own path dialog), no ECAD
  menus, no interactive ECAD prompts: ECAD commands are typed with JSON parameters. The ECAD
  tab in Tool Sets runs or prefills those same commands; it has no picking or prompting of its
  own, and its buttons are text, not icons.
- One project at a time. Only `.ecad` projects are saved; plain CADCraft drawings cannot be
  written.
- **Where files may live.** The Linux home folder under WSL (for example `~/ecad-projects`) is
  the supported and tested location. Other file systems, including Windows drives under `/mnt`,
  network drives and native Windows or macOS builds, are **unverified**: file locking and the
  durability guarantees are not known to hold there.
- The `.ecad` format is version 1, a development format. Files from a later format version are
  refused rather than converted, and nothing rewrites older files yet.
- No library files (`.ecadlib`), no checkpoints, no live debugger.

## 10. Reporting what you saw

Please send:
- what you saw at each "Expected" above, with screenshots where something looks wrong;
- the terminal output;
- the output of `git log --oneline -1` and the environment checks from section 1.
