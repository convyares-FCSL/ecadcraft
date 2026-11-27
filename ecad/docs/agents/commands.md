# The command reference

There is no generated reference file. The reference is live: each command describes its own parameters, and
you read that description from the running program.

## Where to read it

- **MCP:** the `list_commands` tool lists every command with its id, label, menu path, aliases and parameters
  text. Pass `filter` (part of an id, a label or an alias) to narrow it: `{"filter": "cable"}`. See
  [mcp.md](mcp.md).
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

## Report rows

`bom`, `wirelist` and `cablelist` answer a page of rows (`rows`, `total`, `flagged`, and `next` while more
follow), or with `path` write the CSV. Each row is a JSON object: the row's ids, then each cell under its
column's key, then `flags`, the keys of its cells marked `?` (given as null). A column's key is its word in the
report's column type (`WireColumn`, `CableColumn`, `BomColumn` in
`ecad/crates/ecad-electrical/src/reports/kept/columns.rs`), the same list that makes the table and the CSV
headers, so the keys and the cells always line up.

- **`wirelist`:** ids `conductor`; cells `wire_number`, `from`, `from_location`, `to`, `to_location`,
  `cross_section`, `colour`, `potential`, `category`, `cable`, `page`.
- **`cablelist`:** ids `cable`, `used` and `spare` (counts; `spare` left out when the cable's cores are not
  known), `over`, `from_devices`, `to_devices`; cells `tag`, `type`, `cores` (a number), `from_location`,
  `to_location`, `used_text`, `spare_text`, `from`, `to`, then the optional columns the listed cables have:
  `maker`, `length`, `designation`, `glands`, `shield`, `planned_from`, `planned_to`, `drawn`.
- **`bom`:** ids `devices`, `entries` (and `accessory` with `strip`, `kit` in the nested layout); cells
  `location` (grouped by location) or `locations` (grouped by part), `qty` (a number), `unit`, `tags`, `line`
  (nested layout), `description`, `maker`, `part_number`, `article_number` (when shown), then one
  `rating.<word>` per rating column.
