# 070: one way to report errors, one way to read command inputs

Brief 070 (`docs/briefs/070-errors-params.md`). Nothing changes on screen: the words a person
reads stay the same. What changes is underneath, plus one thing an agent or a script notices.

## What changed

- The wire commands (`wire`, `joinwires`, `junction`, `nets`, `net`, `wirenumber`, `potential`,
  `category`, `wiresettings`, `renumberwires`, `checks`) read their parameters through a typed
  struct. Their electrical rules are in `ecad_electrical::cmd::wire`; the command files only read,
  commit and answer.
- **A parameter a command does not take is now refused**, naming it (`unknown field `colour`, …`).
  Before, it was silently ignored. So far this applies to the commands above.
- Each of those commands has a JSON Schema of its parameters
  (`ecad_cadcraft::cmd::schema_of("wire")`). The `params` text in the command list is unchanged.
- Errors have types: `ecad_model::ParseError` and `Noun`, and `ecad_electrical::CmdError`.
- `cargo xtask layers` counts, per crate, the lines that still report errors as text; the counts
  can only fall.

## How to try it

Clicking: draw wires, join them, add a junction, fix a wire number and set a potential as before;
every message reads as it did.

In the command line: `wire` with an extra parameter, for example
`wire {"points": [[0,0],[20,0]], "colour": "RD"}`, is refused with `unknown field `colour``, and
no wire is drawn.

## Checked

Automated: `cargo test -p ecad-cadcraft lane_070` (the registry, the ratchet of untyped commands,
the refusal wording table, unknown parameters, and the commands still working). Not yet tried by
the owner.
