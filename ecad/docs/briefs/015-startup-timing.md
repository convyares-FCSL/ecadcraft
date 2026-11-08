# Brief 015: Where the trial's start-up time goes

Status: Approved (owner, 2026-11-08: "Yes, all three", including "draft startup brief 015")
Branch: `ecad/startup-timing`
Designer: no (no visible change unless switched on) · Size: S
Depends on: nothing (branched from `dev` at `bb86e05`).

## Goal

The release trial takes about 2.9 s from launch to an idle window on the owner's machine (native
Windows 11, 2026-11-08, measured with `WaitForInputIdle`, no project, measured clean three times).
Opening a project headless takes milliseconds (brief 012), so almost all of that time is spent
before ECAD's own work. Find out **where** it goes, with numbers, without changing CADCraft.

Already ruled out by the owner's tests:
- the virus scanner checking a freshly built exe (the first launch was not the slow one);
- the GPU backend choice (forcing `WGPU_BACKEND=dx12` made it slower, at 4.2 s).

## Scope

1. **Timing switched on by `ECAD_STARTUP_TIMING=1`.** It's off by default and costs nothing when
   off. Each mark is a line on stderr, and the same marks go to a small JSON file beside the
   snapshots folder or in the temp folder (`ecad-startup-<time>.json`) for comparison. The marks
   are measured from the process creation time where the OS gives it (Windows
   `GetProcessTimes`, Linux `/proc/self/stat`), otherwise from `main`. In order, each with ms since
   process start and since the last mark:
   1. `main` entered (anything before this is the OS loader, DLLs and the scanner);
   2. `ecad_cadcraft::install()` done;
   3. `run_native` called;
   4. the creator closure entered, meaning eframe has created the window, the wgpu instance,
      adapter, device and surface, and egui's context and fonts;
   5. `Trial::new` / `Trial::with_file` split into CADCraft's app and shell construction, and
      ECAD's project open;
   6. `set_wgpu` done (CADCraft's GPU canvas set-up, pipelines and shaders);
   7. the first `update` (first frame built);
   8. the first frame presented, if eframe exposes it; otherwise the second `update`.
2. **The wgpu adapter in use.** Log the backend, the adapter name and the device type once, with
   the marks, so we know what eframe picked.
3. **A short report** in `docs/quick-start.md` ("Measuring start-up"): the switch, what each mark
   means, and the owner's first table of 3 clean runs (the overseer supplies the numbers after
   the owner's run).
4. Code lives in `ecad/apps/ecad-trial` only. Splitting time **inside** steps 4 or 6 (for example
   wgpu instance or adapter, font atlas, shader compilation) would need CADCraft or eframe changes.
   It's out of scope: the worker reports what the marks show, and proposes where a deeper split
   would go, for the owner to decide.

Out of scope: any change to start-up behaviour, CADCraft, eframe or wgpu configuration; GPU or
font tuning (a later brief, if the numbers point there).

## Acceptance criteria

1. With `ECAD_STARTUP_TIMING` unset, behaviour and output are unchanged, and there's no extra file.
   A test checks the switch parsing.
2. With it set to `1`, a release launch prints the 8 marks and the adapter line, and writes the JSON.
3. The marks are monotonic, and each step's time adds up to the total.
4. Runs on Windows and Linux (CI builds it; the marks are checked by a small headless test where
   a window isn't needed, for example marks 1 to 3, 5 and the JSON writer).

## Needs owner check

- Run the release trial three times with `ECAD_STARTUP_TIMING=1`, closing it between runs, and
  send the output.
