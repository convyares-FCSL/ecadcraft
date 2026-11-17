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
- `setprop` walks one table of property rules (`ecad_electrical::cmd::prop::PROP_RULES`) for one
  object and for several devices (`components`), so the two give the same refusals. One visible
  difference: `tag_fixed` on several devices now follows the tag rules ("yes" or null, a tag to
  fix, not provisional); before, it was stored as free text.
- `place` (every form), `symbols`, `devices`, `deviceprops`, `devicedelete`, `ecaddelete`,
  `tagsettings`, `convertcomponent` and `addterminal` are typed too. A switch given as something
  other than true or false (`mirror: 1`, `builtins: "x"`) is now refused instead of ignored.
- **Every command is typed now**, with a schema: also `connect`, `disconnect`, the drawn cable
  commands (`cableslash`, `cableadd`, `cablecore`, `cablefan`, `cableview`, `cableref`,
  `cablecontinue`, `cablegoto`, `cablerefs`), the page commands (`newpage`, `openpage`,
  `renamepage`, `movepage`, `ecadinspect`), history and branches (`history`, `steps`,
  `pointinfo`, `version`, `showpoint`, `closepoint`, `restore`, `branches`, `branch`,
  `renamebranch`, `switchbranch`, `archivebranch`, `reopenbranch`), `merge`, `undomerge`,
  `agentaccess` and `canvassheet`. Their rules (connect, slash, add to cable, core labels,
  references, new/rename/move page) are in `ecad_electrical::cmd::{connect, cable_draw,
  reference, sheet}`.
- Input is strict everywhere: a value of the wrong kind (`autos: "yes"`, `save: 1`, a `choices`
  that is not an object) is refused in the uniform wording (`` `save` must be true or false, or
  null ``) instead of being ignored. The wording of a missing or mistyped parameter follows the
  same form (`` `from` (point [x, y]) is required ``); rule refusals read as before.
- Errors have types: `ecad_model::ParseError` and `Noun`, and `ecad_electrical::CmdError`.
- `cargo xtask layers` counts, per crate, the lines that still report errors as text; the counts
  can only fall.

## How to try it

Clicking: draw wires, join them, add a junction, fix a wire number and set a potential as before;
every message reads as it did.

In the command line: `wire` with an extra parameter, for example
`wire {"points": [[0,0],[20,0]], "colour": "RD"}`, is refused with `unknown field `colour``, and
no wire is drawn. `history {"autos": "yes"}` is refused with `` `autos` must be true or false,
or null ``; `cableview {"cable": "zz"}` with `` `cable` must be a hex id, or null ``.

## Checked

Automated: `cargo test -p ecad-cadcraft lane_070` (the registry, the ratchet of untyped commands,
the refusal wording table, unknown parameters, and the commands still working). Not yet tried by
the owner.
