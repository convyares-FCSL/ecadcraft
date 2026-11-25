# Commands

Everything a person or an agent can do in ECAD is a command: the command line, scripts, tool buttons, the
wheel, Ctrl+K, the control channel and MCP all reach the same commands by id. This page explains the two kinds
of command, the one runner that runs most of them, and how to add one. The layers they live in are in
[`architecture.md`](architecture.md).

## Two kinds of command

- **Generic commands** are the default. A generic command is a declaration in `ecad-electrical`: a params struct
  and the rules call. The engine's one runner does everything else: reading the parameters, the file reads and
  writes the command declares, taking the project, committing the edit, the JSON answer, the MCP schema, undo
  and the agent gate.
- **Session commands** exist only where a command needs the session: the drawing, the cursor, the selection or
  CADCraft entities (for example `place`, `wire`, `cableslash`, `newpage`, `refgoto`, `release`). A session
  command lives in its feature area's `commands` module in `ecad-engine` (`ecad-engine/src/<feature>/commands.rs`
  or a `commands/` folder). It still reads its parameters with the same typed input and asks
  `ecad-electrical` for every decision; the engine file only commits and answers.

Whichever kind, the JSON form (`engine.execute`, `name {json}`, MCP `execute`) always runs with its parameters
and never prompts. A command may also have a click-based form, started by a menu, a tool button or its bare name;
that form lives in the feature's `interact` module and commits through the JSON form.

## Where the code is

| What | Where |
| --- | --- |
| The `Generic` trait, `Cx`, `Outcome`, `Pages`, `Access`, `Io`, `State` | `ecad-electrical/src/commands/run.rs` |
| Reading parameters, the refusal wording, the schema and the parameters text (`parse`, `schema`, `help`) | `ecad-electrical/src/commands/input.rs` |
| A feature's generic commands | `ecad-electrical/src/<feature>/commands.rs` or `commands/`, beside the rules they call |
| The runner (`Entry`, `run`) | `ecad-engine/src/generic.rs` |
| The registry: `GENERIC`, `specs`, `typed_commands`, `help_of`, `schema_of` | `ecad-engine/src/commands/mod.rs` |
| A session command's typed input (`Command`, `Typed`) | `ecad-engine/src/commands/input.rs` |
| The agent gate's lists for session commands | `ecad-engine/src/agent/policy.rs` |
| Where a person finds a command (tool groups, wheel slots, keys, Ctrl+K words) | `ecad-app/src/commands/actions.rs` |

## The generic runner

A generic command implements `ecad_electrical::commands::run::Generic` on its params struct:

```rust
pub trait Generic: DeserializeOwned + schemars::JsonSchema {
    const ID: &'static str;                 // the command's id
    const TITLE: &'static str;              // its title in menus and the history
    const PAGES: Pages = Pages::Available;  // or Any (also an unavailable page), or ReadAnywhere
    const ACCESS: Access = Access::Edits;   // or Reads: never records a transaction
    const IO: Io = Io::NONE;                // parameters naming files to read or write
    const STATE: State = State::None;       // LastCaption | Conflicts | LastPlaced
    fn check(&self, registry: &Registry) -> Result<(), CmdError> { Ok(()) }   // before the project is taken
    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError>;
}
```

- **`Cx`** holds only what commands share: the committed document and ledger, new ids, the active page, the
  style of new items, the snap tolerance, the change count, the tag high-water marks, where designations are
  used, the derived views, the branch when it is not the release line, this machine's fonts, the `Look`, the
  `Registry`, what the declared files gave (`read`) and the declared state (`state`). The document changes only
  through the `Edit` the command returns.
- **`Outcome`** is the edit to commit (a label and ops, or none), the answer (known now, or read from the
  document once the edit is committed), the files to write, and what to echo on the command line.
- **The run.** `generic::run` parses the parameters, calls `check`, reads the declared files, takes the project
  with `try_lock` (a busy project is refused, never waited for), checks the page, calls `run`, writes the
  returned files, commits the edit under the command's id through `Project::apply_edit` (history, undo, views),
  then echoes and answers. A file that cannot be written commits nothing. An answer read after the commit may
  refuse; the edit stays committed.
- **Files.** Core touches no disk: a command declares its files as data (`Io { reads, writes }`, a read being
  `Read::Bytes(max)` or `Read::Standard`, another project's standard opened read-only) and the engine alone
  opens them.
- **State.** The few commands that keep state beyond the document declare it (`STATE`) and get it as a `Slot`.
  What they change is kept only when the edit is committed.
- **The agent gate** comes from the same declaration: a command declaring `Access::Reads` runs with writes off,
  and the parameters in `IO` are refused from an agent. Session commands, and generic commands that only read
  with certain parameters (a preview, a dry run), are listed in `agent/policy.rs`.

## Parameters and help text

Each command's parameters are one `#[derive(Deserialize, JsonSchema)] #[serde(deny_unknown_fields)]` struct.
Fields are ordinary types: typed IDs read their hex form, `Point` reads `[x, y]`, `Bounded` is a list with a
length limit, a closed set of words is an enum, and `nullable` tells `null` from absent. Input is strict: a value
of the wrong kind is refused in one uniform wording built from the field's name and kind ("`at` must be a point
[x, y]"), never ignored.

The same struct gives the MCP schema (`schema_of`) and the parameters text that `help` and MCP's
`list_commands` show (`help_of`, made by `input::help`):

- the struct's doc comment is the command's summary and its notes (other forms, units, refusals);
- each field's doc comment is that field's help;
- a closed set of words is an enum, so its words are listed.

Never write the parameters text by hand: edit the doc comments.

```rust
/// Sets the continuation arrows' caption ("TO CONTROL SUPPLY"), or removes it with null. One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContcaptionParams {
    /// The continuation arrows (1 to 2000).
    continuations: Bounded<AidId, 0, MAX_ARROWS>,
    /// The caption, or null to remove it.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "crate::commands::input::OrNull<String>")]
    caption: Option<Option<String>>,
}
```

## Adding a generic command

1. **The params struct and its rules** go in the feature's folder in `ecad-electrical`
   (`src/<feature>/commands.rs`, or a file in `src/<feature>/commands/`). Document the struct and every field:
   that is the help text. Implement `Generic`: `ID` (the command's word, lower case, glossary words), `TITLE`,
   and `PAGES`, `ACCESS`, `IO` and `STATE` where the defaults do not fit. `run` calls the feature's rules and
   returns an `Outcome` (`Outcome::answer` for a read, `Outcome::edit` or `edit_if_any` for an edit).
2. **Register it** with one `Entry` line in the `GENERIC` list of `ecad-engine/src/commands/mod.rs`:
   `Entry::of::<YourParams>()`, adding `.interactive(f)` when it has a click-based form and `.undoable()` when it
   also belongs in the drawing's undo. That one line registers the command, lists it with its schema and gives
   the agent gate its access and file parameters.
3. **Give it agent access.** If it may edit, add a row to the table in
   `ecad/apps/ecad-mcp/tests/all/generic_commands_contracts.rs`: parameters that change the table's small
   project. With writes off they must be refused in the gate's words and record nothing; with writes on they
   must record exactly one transaction by `Author::Agent(<client name>)`. A new edit command fails
   `every_generic_edit_command_has_a_row` until its row is there.
4. **Make it findable.** Add it to an action in `ecad-app/src/commands/actions.rs` (a tool group, a wheel slot,
   a key or Ctrl+K words), or list it in `PROGRAMMATIC_ONLY` with the reason.
   `every_ecad_command_is_in_an_action_or_programmatic_only` checks every command is in one or the other.
5. **Test it.** Rule tests (a `&Document` in, a result out) go in `ecad-electrical/tests/all/<feature>/`; tests
   through the session go in `ecad-engine/tests/all/<feature>/`. A form that only reads, or a refusal in the
   command's own words, also gets an MCP contract test (below).

## Adding a session command

Only when the command needs the session. In the feature's `commands` module in `ecad-engine`:

1. A params struct (the same typed input and doc comments as above) that implements
   `crate::commands::input::Command`, whose `run` takes the session. Put every decision in `ecad-electrical` and
   keep the engine side to taking the project (`on_project` or `on_any_page`), committing through
   `Project::apply_edit` and answering. Errors are `CmdError`, never text: a guard checks the engine's command
   files ([`guards.md`](guards.md#typed-errors)).
2. The module's `specs()` (each a `CommandSpec::new(id, title, typed::<P>)`, with `.interactive(f)` for its
   click-based form) is added to `commands::specs`, and its `TYPED` list to `typed_commands`. Its `mod` line in
   `lib.rs` stays where rustfmt sorts it.
3. If it only reads, or reads with some parameters, add it to the gate's lists in `agent/policy.rs`.
4. Write an MCP contract test that drives it through `ecad-mcp`'s `execute` tool, as
   `ecad/apps/ecad-mcp/tests/all/mcp_contracts.rs` does, and checks the transaction it records, its author, and
   that it is refused with writes off.
5. Make it findable, as for a generic command.

The commands an agent can call, and how to read their reference, are described in
[`../agents/commands.md`](../agents/commands.md).
