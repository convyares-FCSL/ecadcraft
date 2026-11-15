# Brief 070: one way to report errors, one way to read command inputs

Status: Approved (overseer, 2026-11-15; lanes R1 and R6 of the code review, design 061 §4 and §6,
design 065 `docs/design/crates.md`).
Branch: `feature/errors-params`, from `refactor/0.2.3` at `522acc28`.
Designer: no · Tester: no (no format or op change) · Size: L.

## Why

Errors are `String` almost everywhere, and the command functions read their JSON by hand, each in
its own way. The same refusal is written out many times, the two `setprop` paths drifted apart, and
an agent gets prose instead of a schema. Code review 2026-11, findings 2 and 3.

Counts on the 0.2.3 layout (after 067, `522acc28`; non-test and test code under `src/`):

| Crate | `Result<_, String>` | `Err(format!(` |
|---|---|---|
| ecad-cadcraft | 317 | 215 |
| ecad-electrical | 140 | 134 |
| dev/ecad-bench | 95 | 22 |
| ecad-file | 76 | 66 |
| ecad-app | 58 | 8 |
| apps/ecad-mcp | 34 | 10 |
| ecad-model | 15 | 15 |
| dev/ecad-inspector | 7 | 5 |
| apps/ecad-desktop | 3 | 5 |
| apps/ecad-cli | 2 | 0 |

162 `fn run_*` command functions; 7 `Deserialize` params structs (all in `cmd/cable.rs`).
Literal refusals: "No such device." 11, "No such wire." 8, "No project is open." 10,
"The project is read-only." 6.

## What

1. **Errors.** One `thiserror` enum per library crate (owner decision in 061: no shared error
   crate).
   - `ecad-model`: `Noun` (device, wire, connection point, terminal, page, folder, cable, symbol…)
     with the words a person reads, and `ParseError` for the model's public parsers
     (`links::parse`, `numbering::plan`, `sheet::parse`, `tag::normalise`,
     `wiring::valid_number`, `merge::join_cables`).
   - `ecad-electrical`: `CmdError` (`NotFound(Noun)`, `ReadOnly`, `NoProject`,
     `Refused(Cow<'static, str>)`, `Model(#[from] ModelError)`, `Parse(#[from] ParseError)`, …)
     and one `msg` module for refusals said in more than one place.
   - The words a person reads do not change: each `#[error(..)]` carries today's text. Errors
     become `String` only at the edge (the command result, MCP, the UI).
   - `anyhow` only in ecad-desktop, ecad-cli, ecad-mcp and `dev/` (061 §6).
2. **Typed command inputs** (R6).
   - Per command a `#[derive(Deserialize, JsonSchema)] #[serde(deny_unknown_fields)]` params
     struct, next to its shim in `ecad-cadcraft/src/cmd/<area>.rs` (schemars only in Engine, 065
     §4).
   - `cmd::typed::<P>` is the `Run` function: `serde_json::from_value` plus error mapping, then
     the command's own `run`.
   - Today's refusal wording is kept: fields read through `cmd::input` helpers that say what
     today's hand-written code said, and a missing required field says the command's own sentence.
   - The electrical work becomes a plain function in **ecad-electrical**: typed input in, ops and
     a result (or `CmdError`) out, no engine and no JSON.
   - `cmd/<area>.rs` keeps a thin shim: register, parse, call, commit, turn the result into JSON.
   - Each command's JSON Schema comes from its struct; today's compact `.params(..)` text stays
     as written (`describe_command` and the menus do not change), and the schema is offered beside
     it (`ecad_cadcraft::cmd::schema_of`, exposed to agents through ecad-mcp).
3. **One property rule table.** `PROP_RULES`, walked by both `setprop` paths (single and many),
   building on 060's minimal unification.
4. **Guards.**
   - `cargo xtask layers` gains a ratchet counting `Result<_, String>` and `Err(format!(` per
     crate: counts can only fall.
   - A contract test checks that every registered command has a params struct and a schema.

## Not in this lane

- `ecad-file` and `ecad-store`: lane 069 is rewriting them; their errors come after 069 lands.
- `defaults/` and `cmd/default.rs`: lane 066 owns them.
- Command ids, behaviour and wording do not change. Tests change only where they assert an error
  type.

## Order

1. This brief.
2. The error types (ecad-model, ecad-electrical).
3. `cmd/wire.rs` end to end, then **stop and report** so the overseer can check the pattern.
4. Then one module per commit: `070 <area>: …`.
5. `PROP_RULES`, the ratchet and the contract test.

## Checks per commit

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D
warnings`, `cargo xtask layers`, `cargo xtask test-changed`; long output to a log file. Never the
whole workspace's tests or the slow set. Push after each green commit.
