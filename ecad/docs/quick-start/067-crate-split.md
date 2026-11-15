# 067: the crate split, step 9 (the program is now `ecad-desktop`)

Lane 067 (design 065, `docs/design/crates.md` §2.2) splits `ecad-trial` in two:

- **`ecad-desktop`** (`apps/ecad-desktop`) is the program you run: the command line, the
  window's size and place, and start-up timing.
- **`ecad-app`** (`crates/ecad-app`) is everything inside the window (the former library of
  `ecad-trial`), and now also holds the action table (menus, icons, search).

Nothing you see or click changes. Only the name of the program does.

## Run it

From `ecad/`:

```sh
cargo run --release -p ecad-desktop                                     # a new project
cargo run --release -p ecad-desktop -- C:\dev\ecad-projects\demo.ecad   # open a saved project (Windows)
cargo run --release -p ecad-desktop -- ~/ecad-projects/demo.ecad        # open a saved project (Linux)
cargo run -p ecad-desktop -- --help                                     # the usage (debug build)
```

The built program is `target/release/ecad-desktop` (`ecad-desktop.exe` on Windows).
`ecad-mcp --launch` now starts `ecad-desktop` (beside `ecad-mcp`, or on `PATH`); the
`ECAD_TRIAL` variable that names its full path keeps its name.

The window keeps its remembered size and position: eframe still stores them under the id
`ecad-trial`.

## Tests

The window's tests moved with the library: `cargo test -p ecad-app <name>` (formerly
`-p ecad-trial`). The tests of the command line and start-up timing stay among them and reach
`ecad-desktop` through a dev-dependency.
