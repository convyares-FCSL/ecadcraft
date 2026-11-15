# 067: the crate split, steps 9 and 10 (the program is now `ecad-desktop`)

Lane 067 (design 065, `docs/design/crates.md` §2.2) splits the former `ecad-trial` in two:

- **`ecad-desktop`** (`apps/ecad-desktop`) is the program you run: the command line, the
  window's size and place, and start-up timing.
- **`ecad-app`** (`crates/ecad-app`) is everything inside the window, and now also holds the action table (menus, icons, search).

Nothing you click changes. The program's name does, and so do a few names around it (below).

## Run it

From `ecad/`:

```sh
cargo run --release -p ecad-desktop                                     # a new project
cargo run --release -p ecad-desktop -- C:\dev\ecad-projects\demo.ecad   # open a saved project (Windows)
cargo run --release -p ecad-desktop -- ~/ecad-projects/demo.ecad        # open a saved project (Linux)
cargo run -p ecad-desktop -- --help                                     # the usage (debug build)
```

The built program is `target/release/ecad-desktop` (`ecad-desktop.exe` on Windows).
`ecad-mcp --launch` now starts `ecad-desktop` (beside `ecad-mcp`, or on `PATH`); the variable
that names its full path is now `ECAD_DESKTOP`.

Step 10 renamed what still said "trial" (owner, 2026-11-15):

- **The window forgets its size and place once.** eframe now stores them under the id
  `ecad-desktop` (on Windows `%APPDATA%\ecad-desktop\data\app.ron`), so the first start opens
  centred again; after that it remembers as before. The old `ecad-trial` folder can be deleted.
- The window title, the experiment line and the notice dialog say **ECAD desktop (experiment)**.
- Diagnostic snapshot files are named `ecad-desktop-…json`.
- The headless quick-start test is `cargo test -p ecad-app the_quick_start_runs_headless` (its
  module is `app`).

## Tests

The window's tests moved with the library: `cargo test -p ecad-app <name>` (formerly
`-p ecad-trial`). The tests of the command line and start-up timing stay among them and reach
`ecad-desktop` through a dev-dependency.
