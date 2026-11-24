# ECAD crates: the overall structure

ECAD is a Cargo workspace in `ecad/` (`crates/`, `apps/`, `dev/`). Every crate belongs to
one of six **layers**, and a crate may use only crates in the layers above it. The full reasoning
is in [`docs/design/crates.md`](../docs/design/crates.md); the binding decisions on names, crates, layers and
source layout are in [`docs/design/decisions.md`](../docs/design/decisions.md). ECAD is desktop-only: there is no web
build.

## The layers

| Layer | What it is |
|---|---|
| **Core** | What an ECAD project *is* and what it *means*, including how its drawing is defined, with no screen and no drawing engine. |
| **Engine** | The link to CADCraft: draws pages from the model, turns edits into transactions, runs commands. |
| **UI kit** | ECAD's widgets and screen look: how a value, button, table or panel looks. Knows nothing about projects. |
| **App** | The ECAD application: shell, workspaces, panels, dialogs, lists and tabs, on a CADCraft window. |
| **Programs** | Things a person or agent starts: one thin `main` each. |
| **Dev** | Developer and test tools under `dev/`. Never shipped. |

## Who may use whom

```text
Programs   ecad-desktop   ecad-mcp                 (may also use each other)
   |            |
   v            v
  App        ecad-app
   |     \        \
   v      v        v
Engine  ecad-engine      UI kit: ecad-widgets             <- Engine and UI kit are siblings:
   |            |                       |                      neither uses the other
   v            v                       v
Core   model -> store -> file -> electrical -> drawing-style
Dev    ecad-bench, ecad-inspector, xtask   (may use anything; nothing uses them)
```

Inside Core the order is fixed: `ecad-model`, `ecad-store`, `ecad-file`, `ecad-electrical`,
`ecad-drawing-style`. A Core crate uses only Core crates before it in that list, so a
cycle cannot be written. The model defines what things are, electrical what they mean, and the
drawing style how they look. The UI kit may use `ecad-model` (for the canvas palette) and nothing
else of ours.

## Every crate

| Crate | Layer | Purpose |
|---|---|---|
| [`ecad-model`](ecad-model/README.md) | Core | The document, ops, transactions, history, merge, numbering, nets, the canvas palette. |
| [`ecad-store`](ecad-store/README.md) | Core | Bytes under the file: media, the writer lock, atomic replace. The only Core crate that touches the OS. |
| [`ecad-file`](ecad-file/README.md) | Core | The `.ecad` file: records, integrity chain, read, save, recover. |
| [`ecad-electrical`](ecad-electrical/README.md) | Core | The electrical rules, one folder per feature: devices, wires, cables, terminal strips, checks, reports. |
| [`ecad-drawing-style`](ecad-drawing-style/README.md) | Core | The look of a page on the model's definitions and the electrical rules. |
| [`ecad-engine`](ecad-engine/README.md) | Engine | The session: draws pages from the model, interaction, the generic command runner and the session commands, in feature folders. |
| [`ecad-widgets`](ecad-widgets/README.md) | UI kit | Widgets (controls, primitives, panels, tables, dialogs) and the screen tokens (`tokens`). |
| [`ecad-app`](ecad-app/README.md) | App | The application: shell, panels, dialogs, lists, the action table. |
| [`ecad-desktop`](../apps/ecad-desktop/README.md) | Programs | The desktop program: command line, window, start-up timing. |
| [`ecad-mcp`](../apps/ecad-mcp/README.md) | Programs | ECAD for agents over MCP, with a write gate. |
| [`ecad-bench`](../dev/ecad-bench/README.md) | Dev | The load benchmark (`cargo xtask bench`). |
| [`ecad-inspector`](../dev/ecad-inspector/README.md) | Dev | `ecadinspect`: look inside a `.ecad` file, text or web page. |
| [`ecad-xtask`](../dev/xtask/README.md) (`xtask`) | Dev | The project's own commands: `ci`, `layers`, `bench`, `test-changed`. |

## The rules `cargo xtask layers` checks

- **Core has no UI and no CADCraft**: no `cadcraft-*` crate, no UI crate, nothing from a layer above it.
- **OS access only in `ecad-store`**: other Core crates may not use files, environment, processes, network, clocks or randomness.
- **Layer order**: each crate uses only the layers (and the Core order) above; nothing outside `dev/` uses a Dev crate, and a crate is under `dev/` exactly when it is Dev.
- **Engine and UI kit stay apart**; App and Programs reach CADCraft's engine only through `ecad-engine`, never `cadcraft-engine` directly.
- **`anyhow` only in Programs, `xtask` and `ecad-bench`'s binary**; libraries use their own error type.
- **No `tokio`** in any ECAD crate.
- **UI code uses `ecad-widgets` components**: no new raw `egui::TextEdit` or `ComboBox`.
- **Every crate has a README** with the six headings and the right `Layer:` line, and every file or folder its
  Contents names exists.
- **Comments carry no history**: no brief, design, lane, finding or pull request numbers, section marks, rulings or
  dates in the comments of any `src/` or of a crate's built-in data (`defaults/*.toml`). History lives in git and
  `docs/`.

To change a rule, edit the table in `dev/xtask/src/main.rs` and the matching section of
[`docs/design/crates.md`](../docs/design/crates.md) (§4), and tell the owner: layers are a recorded
decision.

## Working on a crate

Each README ends with how to test that crate. Everyday checks are scoped to what you changed
(`cargo xtask test-changed`); see [`CLAUDE.md`](../CLAUDE.md) for the full process and
[`docs/quick-start.md`](../docs/quick-start.md) to run the app.
