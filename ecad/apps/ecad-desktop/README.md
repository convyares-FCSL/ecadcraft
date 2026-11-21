# ecad-desktop

Layer: Programs

## Purpose
The ECAD desktop program. It reads the command line, sizes and places the window, times start-up,
and runs `ecad_app::App` in an `eframe` window. What a person sees and clicks inside the window is
`ecad-app`.

## Contents
- `src/main.rs`: the thin `main`.
- `args`: `parse_args` and `Invocation`: the command line (`[<project.ecad>]`, `--snapshots <folder>`, `--control PORT`, `--cad-ui`).
- `window`: window size and place, and whether a window state was saved.
- `startup`: start-up timing.

## Must not depend on
Programs rules: no `cadcraft-engine` directly and no Dev crate. It uses `ecad-app`, `ecad-engine`,
`ecad-model`, `cadcraft-ui-egui` and `eframe`. It holds nothing a person sees inside the window.

## Key types
`args::parse_args`, `args::Invocation`, `window`, `startup`, and `ecad_app::App`.

## How to test
```bash
cargo test -p ecad-desktop
```
Its tests are unit tests in `src/window.rs`. To run the program and try it, see
[`docs/quick-start.md`](../../docs/quick-start.md) (`cargo run --release -p ecad-desktop`).
