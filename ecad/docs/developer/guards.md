# Guards

What each check prevents, so you can tell a real problem from a check to update. `cargo xtask layers` runs every
source guard below in one pass and fails with one line per problem, each naming the file, the line and the fix.
It runs in `cargo xtask lint` and `cargo xtask ci`, and so at the QA gate. Each guard family is one module in
`ecad/dev/xtask/src/`, with its lists at the top; read the module doc before changing a rule.

How the guards are written:

- **Textual.** xtask reads `cargo metadata` and the sources; it has no parser. A guard skips `//` comments and
  stops at a file's trailing `#[cfg(test)]` module unless it says otherwise.
- **Lists with reasons, no counts.** Where a guard allows exceptions, each entry is a file or folder with the
  reason it may. A listed file that no longer needs its entry fails too, so a list only shrinks. Most guards
  are plain bans with no list.
- **Every rule is tested** by unit tests in `ecad/dev/xtask/src/` that feed it a deliberate violation, and by a
  test that runs it on this workspace.

## Layers

`layers.rs`. The crate map: the layer table (`TABLE`), the order inside Core (`CORE_ORDER`), what the UI kit may
use (`UIKIT_ALLOWED`), the CADCraft crates the Engine may use (`ENGINE_CADCRAFT`), and the source checks that go
with them. [`architecture.md`](architecture.md#the-layers) gives the rules in full. It prevents:

- **A crate outside the map.** A workspace member not in `TABLE` fails; a crate is under `ecad/dev/` exactly when
  it is Dev.
- **A dependency on the wrong layer.** Core using CADCraft, a UI toolkit, the UI kit, the Engine or anything
  above; a Core crate using one later in `CORE_ORDER` (a cycle); the Engine using a UI crate or another
  `cadcraft-*` crate; the UI kit using anything but `egui`, `epaint`, `egui_extras` and `ecad-model`; App or
  Programs using `cadcraft-engine`. A dev-dependency is held to the layer order only: no crate depends on a layer
  above it, not even in tests.
- **The engine reached around `ecad-engine`.** App and Programs sources may not name `cadcraft_engine`, because
  `cadcraft-ui-egui` and `cadcraft-mcp` could hand engine types through; they use `ecad_engine::cad`.
  `ecad-electrical`'s sources may not name any `cadcraft_` path.
- **OS access in Core.** Core crates other than `ecad-store` may not reach the file system, environment,
  processes, network, clock or OS randomness (`CORE_OS`), or depend on a crate that finds the user's folders
  (`CORE_OS_CRATES`). A hard zero: Core stays pure data and rules, testable and portable.
- **The wrong crates.** `anyhow` outside Programs, `xtask` and `ecad-bench` (a library returns its own error
  type); `tokio` anywhere (no async runtime beside CADCraft's event loop); `rayon` outside `RAYON_OK`, or as
  anything but a plain dependency.
- **Developer tools mixed into product code.** A product crate depending on a Dev crate other than optionally,
  through its own `dev` feature, off by default (`DEV_BY_DEFAULT` names the exception); a dev feature
  (`DEV_FEATURES`) read outside a crate's `dev` module (the engine's fault-injection and test-ops hooks are
  listed in `DEV_FEATURE_HOOKS`); a product source reading a file under `dev/`. See
  [`debugger.md`](debugger.md#the-developer-tool-tree).
- **A stale layer exception.** `LAYER_EXCEPTIONS` is empty; an entry that is no longer needed fails.

## UI style

`ui_style.rs`, run on App and Programs sources. It prevents:

- **Local styling** (`LOCAL_STYLE`): colours, fonts and corner radii written in place (`Color32::from_rgb`,
  `FontId::new`, `CornerRadius`, egui's named colours, CADCraft's theme colours read directly). Colours, sizes and
  fonts come from the `ecad-widgets` tokens. A plain ban.
- **Raw value controls** (`RAW_CONTROL`): `egui::TextEdit` and `egui::ComboBox`. Panels and dialogs use
  `ecad_widgets::control`, so anything the person can change looks editable and a read value never looks like an
  input ([`ui-components.md`](ui-components.md)). The files made before the components are listed in
  `RAW_CONTROL_FILES`; any other file may not have one.

## Drawn text

`drawn_text.rs`, run on `ecad-engine` and `ecad-electrical`. It prevents a drawn text from ignoring its style: no
literal text height (`height: 2.5`), quoted style name (`style: "…"`, `"Standard"`), style constant
(`STYLE_BOLD`, `STYLE_REGULAR`) or text-size constant (`…_TEXT_MM`, `…_HEIGHT_MM`, `LABEL_HEIGHT`…; bounds named
`MIN_` or `MAX_` are allowed). A text's height, weight, font and colour come from its `TextKind`'s style and the
TOML data. `DRAWN_TEXT_EXEMPT` lists where literal sizes belong: the styles and defaults that define them, symbol
definitions (glyphs as line art), screen-only overlays sized in pixels (`interact`, the pointer, property forms),
an imported title block's own sizes, and commands, which draw nothing.

## Typed errors

`errors.rs`. It prevents errors reported as text: no `Result<_, String>` and no `Err(format!(` in a library
crate, so a caller can tell failures apart. Programs and Dev crates are exempt (a program may return text from
`main`). `STRING_ERROR_EXEMPT` lists the crates still moving to typed errors, each with its reason. The engine's
command files (`ENGINE_COMMAND_FILES`) hold none even so: they return `CmdError`. `ecad-electrical` has no
bridge that turns text into a `CmdError`.

## READMEs

`readme.rs`. It prevents a crate without its card: every workspace member has a `README.md` that starts with
`# <crate name>`, has a `Layer:` line naming its layer as `TABLE` has it, and the headings `## Purpose`,
`## Contents`, `## Must not depend on`, `## Key types` and `## How to test`, in that order. Every file or folder
that `## Contents` names in backticks must exist, so the card cannot drift from the code. `README_ALLOWLIST` is
empty.

## No history in comments

`history_guards.rs`. It prevents comments and docs that say where something came from instead of what it does.
It reads every comment in every `src/` of the workspace, the `#` comments of the built-in data under each
crate's `defaults/` (`.toml`), and every line of the current docs (the Markdown under `ecad/docs` outside
`archive/` and `briefs/`, fenced code blocks left out). It refuses a section mark, who decided something and
when, an issue or pull request number, brief, design, lane or finding numbers, the old requirement ids, and
dates. Such a citation goes stale with the next change, and a reader cannot open most of them: history lives in
git and in `ecad/docs/archive`. A hard zero with no allowlist; rewrite the line to say what is true now. Domain
text that looks similar is not refused: a jumper bar lane, a jumper bar tag (`JB-10`), a PLC address
(`%Q2.2`), a number with a thousands space, a timestamp, a date in a backticked example.

It also refuses a brief in `ecad/docs/briefs/` whose `Status:` line says it is finished (done, delivered,
finished or merged): a finished brief belongs in `ecad/docs/archive/briefs/`, with its banner and its row in
the archive index.

## Engine rule functions

`engine_guards.rs`. It keeps `ecad-engine` an adapter, so rules do not creep back into it. It counts the engine's
**rule functions**: functions whose inputs are all Core or standard types, at least one a type a Core crate
declares (the document, an id, a point), and that return something that is not an engine type. Such a function
needs nothing the engine has, so it belongs in a Core crate, usually `ecad-electrical`. No file may have one,
apart from:

- the areas on `EXEMPT`, each with its reason: `agent/` (the gate's answers), `generic.rs` (the generic runner's
  reply envelope, built at the edge until replies are typed), `dev/` (read-only developer views of a live session)
  and `translate/` (movement, kept beside the movement code);
- the functions on `ADAPTERS`: thin wrappers over something only the engine has (CADCraft's fonts and DXF reader,
  the local time zone, the live ledger), which the textual test cannot see.

Methods with a `self` receiver are not counted, and a name the guard cannot place counts as an engine type, so it
never flags what it cannot place.

## Views

`views_guards.rs`. It keeps one way into every derived view, the project's `Views`. It prevents:

- a direct full build of a view type (`ReportModels::build(`, `RefIndex::build(`, `Checks::build(`…) outside
  `Views` and the view's own module (`OWN_MODULES`; own-module files that build are listed in `OWN_BUILDS` with
  their reason);
- working views out from scratch (`.detached(`, `.report_models(`, `Detached::build(`) outside the files
  `FRESH_BUILDS` lists, each with its reason; the bench's probes (`PROBES`) time full rebuilds on purpose;
- in `ecad-electrical`, a match over `Effect` or `EffectKind` ending in a wildcard arm, so a new kind of effect
  needs a decision in every view's update note;
- a view note routing a property key by its prefix (`.starts_with("`): a key's meaning belongs in its
  definition.

## Device kind from the symbol

`starter_guards.rs`. What kind of device something is comes from `Device.device_type`, never from the symbol that
draws it. The one way from a symbol to a kind, `Starter::device_type()`, is `pub(crate)` in `ecad-electrical`, so
the compiler keeps other crates from asking. Inside `ecad-electrical` the guard keeps the calls to the places that
make a definition or a device (`SEED_FILES`, each with its reason): the kind is seeded from the symbol once and
stored from then on. Looking a symbol up for a drawing question is not checked.

## The typed object model

`types_guards.rs`, run on the product crates. Each is a plain ban, so values stay typed:

- boolean words (`"yes"`, `"on"`, `"true"`…) compared or matched: read them with `ecad_model::words::flag`;
- hand-written digit checks (`.all(…is_ascii_digit…)`): use the `words` readers;
- angle conversions (`.to_radians()`, `.to_degrees()`) in Core or the Engine outside `units.rs`: convert with the
  `Degrees` and `Radians` types;
- a style catalogue or role held as text: hold the typed value;
- word lists written as `Kind::Choice(&[…])`: declare a `word_enum!`;
- `.word() == "…"` comparisons: compare the typed value;
- catch-all words or bodies (`Unknown(`, `Stored<`, `Raw`): an unknown word is a read error;
- `String` or `&str` fields in stored structs: store a `Text<N>`, `LongText<N>` or a typed value (the fields that
  are stored text by design are listed in `STORED_TEXT_FIELDS`);
- hidden-character checks outside `words.rs`: use `ecad_model::text::Text<N>`.

The word rules are written out in one file, `ecad-model/src/naming/words.rs`.

## Upstream patches

`cargo xtask patches` (in `lint` and `ci`, not part of `layers`). It prevents an unrecorded change to CADCraft:
every file changed outside `ecad/` since the CADCraft commit in `ecad/UPSTREAM.md` must be listed in
`ecad/UPSTREAM-PATCHES.md`. Files listed but unchanged are reported, not refused.

## What the compiler and clippy enforce

- **Visibility.** `[workspace.lints.rust]` denies `unreachable_pub`, `private_interfaces` and `private_bounds`.
  `pub(crate)` is the default for items and modules; `pub` only where another crate (its integration tests and
  doc examples included) uses it. A `pub` item nothing outside the crate can reach fails to compile.
- **No `unsafe`:** `unsafe_code = "forbid"`.
- **Never crash.** Every production crate root denies `clippy::unwrap_used`, `expect_used`, `panic`,
  `unimplemented`, `todo` and `unreachable`; `ecad/clippy.toml` allows them in tests only. Clippy runs with
  `-D warnings`, so a warning fails too.
- Every crate manifest has `[lints] workspace = true`.

## Tests that guard the docs and the data

- **Doc-sync tests.** The token tables in [`../reference/tokens.md`](../reference/tokens.md) and the constants must
  agree: every row names a constant and shows its value, and every constant has a row.
  `ecad-widgets`' `tokens::doc_sync` checks the part from `# Tokens` to the next top-level heading (the screen
  tokens); `ecad-model`'s `palette::doc_sync` checks the part from `# Drawing style` to the next (the canvas
  palette). Both read the file with `include_str!`, so moving or renaming it is a compile error, and renaming
  either heading, or adding a top-level heading inside one, changes what is checked.
- **No default rating.** `symbols::catalogue_api::prefixes_are_registered_defaults` in `ecad-engine` checks that
  no registry entry is a default rating value: no key names a device group together with one of that group's
  rating field words (`motor.current`, `rating.motor.voltage`). A device's rating is the person's to enter.
- **Every generic edit command has an agent row.** `generic_commands_contracts::every_generic_edit_command_has_a_row`
  in `ecad-mcp` ([`commands.md`](commands.md#adding-a-generic-command)).
- **Every command is findable.** `commands::actions::every_ecad_command_is_in_an_action_or_programmatic_only` and
  `every_action_runs_a_registered_command` in `ecad-app`.
- **The file format is pinned.** The golden file in `ecad-file` pins the record format with a hash: a format change
  that does not bump `ecad_file::SCHEMA_VERSION` fails ([`../reference/file-format.md`](../reference/file-format.md)).
