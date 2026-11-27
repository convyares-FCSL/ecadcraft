# The command reference

There is no generated reference file. The reference is live: each command describes its own parameters, and
you read that description from the running program.

## Where to read it

- **MCP:** the `list_commands` tool lists the commands the agent may run, with id, label, menu path, aliases
  and parameters text, each marked `"agent": "read"` (runs with writes off) or `"write"`. Pass `filter` (part of
  an id, a label or an alias) to narrow it: `{"filter": "cable"}`; `{"all": true}` lists every command, the
  refused ones marked `"agent": "refused"` and `"enabled": false` with the reason. See [mcp.md](mcp.md).
- **The control channel:** `engine.commands` returns the same list (id, label, menu path, shortcut, aliases,
  parameters text, enabled). See [control-protocol.md](control-protocol.md).

## Where the text comes from

A command's parameters text is made from its params struct, never written by hand
(`commands::help_of` in `ecad/crates/ecad-engine/src/commands/mod.rs`, which calls
`ecad_electrical::commands::input::help` on the struct's JSON Schema):

- the fields come first, required ones before optional ones (`?` marks an optional field), each with its kind;
  a closed set of words is an enum, so its words are listed (`"traditional"|"fan"`);
- then the struct's doc comment: the summary, and its notes (alternative forms, units, refusals);
- then each field's doc comment, as that field's help.

For example:

```
{cable: hex id, view?: "traditional"|"fan"} How a cable is drawn. `cable`: The cable.
```

So the text changes when the struct's docs change, and it is the same everywhere it is shown. The full JSON
Schema of a command's parameters is `commands::schema_of`. A command whose input is not typed has an empty
parameters text.

## Generic and session commands

A **generic command** is a declaration in `ecad-electrical`: a params struct with its access (whether it only
reads), the files it reads or writes, and its edit. It is one `Entry` line in the `GENERIC` list of
`ecad/crates/ecad-engine/src/commands/mod.rs`, and the one generic runner does the rest (read the parameters,
lock the project, commit the edit as one transaction, answer); the agent gate reads the same declaration. A
**session command** needs the CADCraft session (pages, the drawing, the views) and lives in its feature area's
`commands` module of `ecad-engine`, registering its specs in `commands::specs` and its typed input in
`typed_commands`. Both kinds reach the command line, scripts, the control channel and MCP the same way, and both
get their parameters text from their params struct.

How to add a command: [../developer/commands.md](../developer/commands.md).
