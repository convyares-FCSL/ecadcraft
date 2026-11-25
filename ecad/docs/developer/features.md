# Adding a feature

A feature (cables, terminal strips, PLC, references…) has one folder per crate, named the same in each, with
the area's plural name from the glossary ([`../reference/glossary.md`](../reference/glossary.md)). Open the
folder in each crate and you have the whole feature. The full trees are in
[`../design/layout.md`](../design/layout.md); the layers are in [`architecture.md`](architecture.md).

## The folders a feature gets

| Crate | Folder | What goes there |
| --- | --- | --- |
| `ecad-electrical` | `src/<feature>/` | Everything electrical about it: its rules, its command declarations (`commands.rs` or `commands/`, beside the rules they call), its kept view if it has one, its words. Decided from the document alone: a `&Document` in, plain data or ops out |
| `ecad-electrical` | `defaults/<feature>.toml`, `src/defaults/<feature>.rs` | Its built-in values as data and its registry entries |
| `ecad-engine` | `src/<feature>.rs` or `src/<feature>/` | The session half only: drawing (`draw`), click-based forms (`interact`) and the commands that need the session |
| `ecad-app` | `src/<feature>/` | Its screens: lists, editors, panels and their UI state |
| each crate | `tests/all/<feature>/` | Its tests, beside the code they cover |
| docs | `docs/user/<feature>.md` | Its user guide |

`ecad-electrical/src/plc/` and `ecad-engine/src/plc/` show the shape. Take `cables` as an example:
`ecad-electrical/src/cables/` (the planning model, cable slash, layout, words and `commands/`),
`ecad-engine/src/cables/` (`draw.rs`, `views.rs` for the traditional and fan views, `commands.rs` for the
session commands), `ecad-app/src/cables/` (list, editor, manager, detail, view), and
`tests/all/cables/` in each of them.

Rules that keep the folders right:

- **No rules in the engine.** The engine draws, takes clicks and runs session commands. A function that needs
  nothing the engine has belongs in `ecad-electrical` (a guard checks it: [`guards.md`](guards.md)).
- **An engine feature is a file until it has both drawing or interaction code and session commands; then it is a
  folder.** Its names mirror the electrical folder's.
- **No rules, disk, OS or parsing of domain values in `ecad-app`.** The app turns user actions into commands
  and shows their results.
- **The look is not electrical.** Text styles, fonts and frames are `ecad-drawing-style`'s; electrical asks for
  them through `ecad_model::look::Look`. New object types (a new kind of device, a new stored struct) go in
  `ecad-model`.
- **Shared plumbing stays flat and small.** `lib.rs` lists the areas, one line each.
- **A file over about 1,500 lines** is split by job inside its feature folder.
- **Narrow `pub`:** `pub(crate)` unless another crate (its integration tests and doc examples included) uses it,
  modules too. No `pub use` between crates.

## Commands

Every user-visible behaviour is a command. A new command is generic unless it needs the session: its params
struct lives in the electrical feature folder and it is one `Entry` line in the engine's `GENERIC` list. A
session command lives in the engine feature's `commands` module. How to add each, and its agent access, is in
[`commands.md`](commands.md). Every command is either in an action of `ecad-app/src/commands/actions.rs` (a tool
group, a wheel slot, a key, Ctrl+K words) or in `PROGRAMMATIC_ONLY` with a reason.

## Agent access

Every feature ships with its agent access. A generic command that may edit gets a row in
`ecad/apps/ecad-mcp/tests/all/generic_commands_contracts.rs`; a session command, or a generic command's own edge
(a form that only reads, a refusal in its own words), gets an MCP contract test that drives it through
`ecad-mcp`'s `execute` tool and checks the transaction it records, its author, and that it is refused with
writes off. See [`../agents/mcp.md`](../agents/mcp.md).

## Tests

Tests go in `tests/all/<feature>/` of each crate they test, one module per file, inside that crate's one test
executable:

- rule tests (a document in, a result out) in `ecad-electrical/tests/all/<feature>/`;
- session, drawing and click tests in `ecad-engine/tests/all/<feature>/`;
- screen tests in `ecad-app/tests/all/<feature>/`;
- MCP contract tests in `ecad/apps/ecad-mcp/tests/all/`.

Test modules say what they test, never where they came from. A test slower than about 2 s goes in the slow set.
The layout, the slow set and how to run only what changed are in [`testing.md`](testing.md).

## Defaults

A built-in value is data, never a number written where it is used. Each area of `ecad-electrical` owns a TOML
file under `ecad-electrical/defaults/` (embedded at build time) and a file under `src/defaults/` with its
registry entries (`ENTRIES`). Read a value through `ecad_model::registry::resolve` or a typed reader over it. The
look's entries are `ecad-drawing-style`'s (`defaults/text.toml`, `defaults/styles.toml`). The read-only
`defaults` command and Settings ▸ Standards list every entry; `setdefault` writes one at a level. A device's
rating has no registry default: a test checks that no entry is one
([`guards.md`](guards.md#tests-that-guard-the-docs-and-the-data)).

## Tokens and components

- **Screen look** comes from the `ecad-widgets` tokens; UI code never styles in place (a guard checks it).
- **Values in panels and dialogs** use the `ecad-widgets` components ([`ui-components.md`](ui-components.md)). A
  missing component is added to `ecad_widgets` and its gallery, not drawn in the panel.
- **Canvas colours, markers, wire colours, line weights** are the canvas palette, `ecad_model::palette`. Drawn
  text takes its height and style from its text kind's style, never from a literal.
- A new screen token or palette constant gets its row in [`../reference/tokens.md`](../reference/tokens.md) in the
  same change: the doc-sync tests compare the tables with the constants.

## Docs are part of done

Every change updates, in the same commit, the README, crate and module docs and comments it touches, so they
describe the code as it now is, in domain words. Comments in `src/` and in `defaults/*.toml` say what the code
or value is and why, with no history (no brief, design, lane, finding or pull request numbers, no section marks,
no rulings, no dates); `cargo xtask layers` checks it. Only READMEs link into `docs/`.

A feature's user guide, `docs/user/<feature>.md`, describes it as it is now: what a person can do, the labels
they see, what is refused and why. Update it in the same change as the behaviour.

## How a feature lands

A feature is built on one long-lived branch, `feature/<name>`, and reaches `dev` by one pull request when it
works (the owner may take a large one in more than one). `dev` reaches `main` through the QA gate, which runs the
full suite and the bench. Merge `dev` into the feature branch locally. Never merge your own pull request: green CI
is not approval.
