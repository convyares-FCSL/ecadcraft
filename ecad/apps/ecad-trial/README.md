# ecad-trial: ECAD desktop trial harness (experiment)

> **This is an experiment, not an ECAD application.**
> - **No project persistence.** Save and Save As only report that saving is unavailable. They
>   write nothing and never mark the project as saved.
> - **Closing an edited project discards its design.** The trial asks first and offers only
>   **Cancel** or **Discard**.
> - **Inspector snapshots (`--snapshots`) are diagnostic files, not saved projects.** They
>   cannot be reopened.
> - **Not yet verified on a real display.** The automated tests run headless. One smoke run used
>   a virtual X display (Xvfb with software Vulkan) and simulated input. It showed the window,
>   the banner, typed commands, the save notice, the Cancel/Discard dialog and discard, with a
>   snapshot written. Mouse interaction, dragging and the connected-wire preview have not been
>   observed on any display. Nobody has run it on WSL yet; that is what this trial is for.

It is CADCraft's own window with the ECAD commands installed and one ECAD project ("Trial")
open. A banner across the top names the experiment and shows the number of open pages and
unsaved changes.

**Not included:**
- file open and save dialogs;
- CADCraft file formats (plain drawings cannot be written either);
- autosave and the control channel;
- saved window or profile settings;
- ECAD menus and interactive ECAD prompts.

## Build and run on WSL

1. Run the checks in [`../../docs/environments.md`](../../docs/environments.md) section 1, and
   keep the output. WSLg is needed for a window: `echo $WAYLAND_DISPLAY $DISPLAY` should print
   something, and `/mnt/wslg` should exist (Windows 11).
2. Install a toolchain if there is none, then get the branch:

   ```sh
   sudo apt-get update && sudo apt-get install -y build-essential pkg-config git curl
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && . "$HOME/.cargo/env"
   git clone https://github.com/convyares-FCSL/ecadcraft.git && cd ecadcraft
   git checkout ecad/desktop-trial
   ```

   In an existing clone: `git fetch origin ecad/desktop-trial && git checkout ecad/desktop-trial`.

   Clone inside the Linux file system (for example `~/ecadcraft`), not under `/mnt/c`, so the
   build runs at normal speed.
3. Build and start (the first build takes several minutes and about 6 GB):

   ```sh
   cd ecad
   export CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_INCREMENTAL=0
   cargo run --release -p ecad-trial -- --snapshots ~/ecad-trial-snapshots
   ```

   Leave out `--snapshots …` to run without snapshot export.
4. If no window appears, or rendering fails:
   - set `WGPU_BACKEND=gl` (or `vulkan`) and try again;
   - if runtime libraries are missing, install `libxkbcommon0 libvulkan1 libegl1 mesa-vulkan-drivers`;
   - report the terminal output together with `vulkaninfo --summary` and `glxinfo -B`.
5. Optional: open `ecad/tools/inspector/index.html` in a Windows browser and load the snapshot
   files from `\\wsl$\<distro>\home\<you>\ecad-trial-snapshots`.

## Manual trial (about 15 minutes)

Type each command in the banner's **Command** field and press Enter (or **Run**). The field
takes a whole line, so it accepts ECAD commands with JSON parameters (`name {json}`) and
CADCraft command lines such as `line 0,0 40,0`. CADCraft's own command line at the bottom
treats a space as Enter, so it cannot take JSON; use it for plain CADCraft commands and for the
prompts of mouse-driven commands. The handles in step 2 come from the drawing; the step explains
how to find them.

1. **Draw.** Type `line 0,0 40,0 40,20 0,20 c`. You can also try drawing with the mouse.
   Check: four lines appear, and the banner shows 1 change.
2. **Convert.** Select the four lines with a window selection, then type `convertcomponent`.
   Click the right-hand edge and type `list` at the command line to see its handle, then type
   `addterminal {"handle":"<that handle>","at":[40,10]}`. Check: a terminal marker appears at
   40,10, and the rectangle now selects as one group.
3. **Place.** Type `place {"at":[100,0],"rotation":90}`. Check: a symbol (coil) appears, rotated.
4. **Wire.** The rotated coil's terminals are at 90,0 (A1) and 110,0 (A2). Type
   `wire {"points":[[40,10],[70,10],[70,0],[90,0]]}`. Check: the wire is drawn from the
   rectangle's terminal to A1, and the reply shows both ends attached.
5. **Connected movement.** Select the rectangle and its marker (a window selection around it)
   and type `move` at the command line, then pick a base point and drag. Check: while you drag, the wire's end follows in
   the preview, and after the move the wire is still attached. Then rotate the symbol
   (`rotate`, base point at its insertion). Check: the wire end stays on the symbol's terminal.
6. **Undo/redo.** Type `u` three times, then `redo` three times. Check: the drawing steps back and
   forward exactly. Try `undo {"mode":"mark"}`. Check: it is refused as not supported on ECAD
   pages.
7. **Save.** Press Ctrl+S, use File ▸ Save As, and type `qsave`. Check: each time, a dialog says
   saving is not available and nothing was written. The tab still shows unsaved changes.
8. **Two pages.** Type `newpage {"title":"2"}`. Check: a second tab "Trial - page 2" opens. Draw
   or wire something on it, then switch tabs. Close page 2's tab. Check: no question, the
   project stays open, and the banner shows 1 page. Reopen it with
   `openpage {"page":"<page id from ecadinspect>"}`. Check: its content is back.
9. **Snapshot** (if started with `--snapshots`). Click **Export inspector snapshot** in the
   banner. Check: the dialog names the file and says it is a diagnostic file, not a saved
   project.
10. **Close.** Close the last page (or the window). Check: the dialog says the design will be
    lost and offers only Cancel and Discard. Cancel keeps everything. Discard closes it (and
    exports a final snapshot when enabled). Then **New trial project** in the banner starts
    again.

Please report:
- what you saw at each check, with screenshots where something looks wrong;
- the terminal output;
- the environment check output.
