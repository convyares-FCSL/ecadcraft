# Brief 048: Symbol builder

Status: **Draft** (designer, 2026-11-13). Not approved: FP1–FP8 wait for the overseer and
questions Q1–Q10 of design §14 for the owner.
Branch: `feature/symbol-builder`, from `origin/dev` `f26c8f53`.
Designer: **yes** (`docs/design/048.md`) · Size: **XL**, split into three parts (below)
Roadmap: `docs/discussions/roadmap-review-2026-11.md` M1 item **1.1**, lane **S**, after 1.11
connectors (044). Builds on 018 (devices, Convert to Component, starter symbols), 034 (component
library, the `symbol.<key>.prefix` entries), 036 (relabel and rekey by remove and add of one ID),
044 (starter versions, lookup by content hash), 031 (the defaults registry), 038 (the Library
workspace), 041 (the action table and wheel) and 042 (the inspector and Standards screen). Owner
answer 10 (UI spec §8): Edit symbol pre-selects "update all" and switches to "update selected"
when a placed copy would clash. Proposal §4.1 (promotion) and §4.2 (text slots, deferred until
now). Leaves room for 2.1 `.ecadlib` and 2.2 library viewer, designed next in lane S.

## Goal

Anyone can draw the symbol a real panel needs (a vendor's safety relay, a valve with grouped
pins, a sensor with its colour-coded leads) once, in the project, and place it like a built-in:
with connection points, text slots and its own defaults. Editing it later shows what changes on
every placed copy before anything is saved.

## What exists today (`origin/dev` `f26c8f53`)

- About 70 built-in starters (`symbols.rs`), each a `Definition` with an `Origin` (starter
  library, fixed ID, `library_seq`, content hash). What a starter *is* (kind, prefix, aliases,
  description, text anchors) is catalogue code, never in the file. 044 versioned 24 of them
  (`library_seq` 2) and `symbols::embedded` matches the content hash.
- `convertcomponent` turns drawn geometry into a component of the **drawn form** (proposal §4.1).
  Nothing turns it into a symbol ("promotion" is "not designed here" in the proposal).
- `Definition` has `name`, `origin`, `graphics`, `terminals`, `bridges`. **No text slots**
  (proposal §4.2 deferred them). The content hash is SHA-256 over the canonical record without
  `origin`, checked on read.
- No op changes a component's definition; 036 and 044 change a terminal by removing and adding
  one ID in one transaction.
- The registry has `symbol.<key>.prefix` per starter; ratings and descriptions are not entries.
- The Library workspace (038) hosts today's symbol search. The device wheel's eight slots are
  full (041).

## Scope

1. **New symbol** (Library ▸ New symbol): an empty grid in the Library main view, the builder's
  six tools (Select, Draw, Text, Connection point, Text slot, Origin), and a Details panel on the
  right listing what is missing. Save writes one transaction that embeds the definition.
2. **Text slots** in the definition: where the tag, description, rating, part number and the
  other device texts go (FP1).
3. **The symbol's own defaults**: short key, tag prefix, description, rating, how it joins a
  device. Stored in the definition and read through the registry as `symbol.<key>.<field>`, so a
  project or (later) a standard overrides them in one place (FP2, FP6).
4. **Versions**: a project symbol has a lineage and a version number in its origin; every save
  that changes content is a new version with its own content hash (FP3).
5. **Edit symbol** on a placed symbol: opens its definition. Editing a built-in makes a project
  copy that remembers where it came from (FP4). Before saving, the drawer lists every placed copy
  with a now/after preview and what changes there; choose update all, update selected or save as
  a new symbol. Copies left behind read "update available".
6. **Make symbol** from a selection of plain lines, arcs and text, and **promote** a drawn
  component (Convert to Component's result) to a symbol, keeping its component and connection
  point IDs.
7. **Commands for everything** (`symbolnew`, `symboledit`, `symbolimpact`, `symbolupdate`,
  `makesymbol`), JSON form first, so an agent can build a symbol over MCP without the builder.
8. **Update maps** for every view this adds or changes; **MCP contract tests**; **bench** rows.

## Parts (design §15)

| Part | What | Size | Tester |
| --- | --- | --- | --- |
| **048a** Model and commands | FP1–FP8; the five commands in JSON form; placing project symbols; the impact rules and updates; the registry family entries; MCP | L | **Yes** (format and ops) |
| **048b** The builder | Library ▸ New symbol and Edit symbol; the six tools; Details panel; the review drawer; wheel and Ctrl+K actions; Library browser shows project symbols | L | Yes (UI acceptance only) |
| **048c** From the page | Make symbol from a selection; promote a drawn component; "update available" in Properties and Devices with Update and Compare | M | Yes (promotion keeps IDs) |

048a lands first; 048b and 048c can then run one after the other on the same branch.

## Out of scope

- `.ecadlib`, library update from a file, a symbol shared between projects (2.1, 2.2). Copy and
  paste of a project symbol into another project stays as today (embedding by copy).
- Nested symbols (a symbol inside a symbol, proposal §4.2), symbols with internal bridges drawn
  in the builder (the JSON form takes `bridges`; the builder has no tool for them), parametric
  symbols (pin count as a parameter).
- Project symbols that number like contacts (IEC sequence digits) or PLC channels (design §14
  Q4).
- Text sizes, fonts and placement rules beyond the slot's anchor (lane T, 031 and Q2).
- Deleting a project symbol from the library browser (design §14 Q9).
- Import of KiCad symbols (after v1).

## Format and ops (design §13, the overseer decides)

All additive and absent by default, so existing content hashes do not change. Pre-v1: no
compatibility work.

- **FP1** `Definition.texts`: text slots.
- **FP2** `Definition.defaults`: the symbol's own default values.
- **FP3** Project-symbol origins: a fixed library ID for "made in a project", the lineage ID as
  `origin.definition`, the version as `library_seq`.
- **FP4** `Definition.derived_from`: the origin of the definition an edited copy came from.
- **FP5** No new op: a component's definition changes by `DeleteComponent` + `CreateComponent` of
  the same ID in one transaction; a connection point is rekeyed by `RemoveTerminal` +
  `AddTerminal` of one ID (036's pattern).
- **FP6** Registry family entries `symbol.<key>.description`, `.rating`, `.join` (registry code,
  not the file).
- **FP7** One key namespace: a project symbol's key cannot be a starter's.
- **FP8** Superseded project versions that nothing uses are removed in the updating transaction.

## Acceptance criteria

The full table is design §12. In short:

1. `symbolnew` with a name, a key, graphics, two points and a tag slot embeds one definition
   (version 1, origin "project symbols") in one transaction; `place {symbol: <key>}` places it
   with its prefix and description; `u` undoes the save.
2. A symbol with no name, no connection point, no tag slot, a key that is a starter's or
   another project symbol's, a point off the grid or two points on one spot is refused with what
   to do; the builder lists the same as "missing" with a button each.
3. The definition's text slots place the tag and description of every placed copy; a slot for
   `rating` shows the device's rating.
4. `symbol.<key>.prefix`, `.description` and `.rating` of a project symbol resolve from its
   definition, and a project override wins; `defaults` lists them.
5. `symbolimpact` on an edit lists every placed copy with its changes and clashes; a wired point
   removed or left without a counterpart is a clash. `symboledit {update: all}` with no clash
   updates every copy; with a clash it is refused and names the copies; `update: selected`
   updates the named copies only; the others read "update available".
6. An update keeps every component, connection point and wire ID; wire ends on a moved point
   follow it; labels that were the old default take the new one, labels set by hand stay.
7. Editing a starter makes a project copy whose `derived_from` is the starter's origin; the
   starter itself is unchanged; placed starters are updated only if chosen.
8. `makesymbol` from plain lines and a text makes a symbol and, with `replace: true`, replaces the
   drawing with a device of it at the same place. Promoting a drawn component keeps its component
   and connection point IDs and every wire stays attached.
9. Saving the same content twice is refused (`Nothing changed.`); a version's content hash is
   its content's (checked on read).
10. **MCP contract tests** for all five commands: one transaction each, authored
    `Author::Agent(<client name>)`, refused with writes off, head unchanged.
11. Hostile input never panics: 100 000 primitives, 10 000 points, a 10 000-character name or key,
    NaN and huge coordinates, a slot for an unknown property, a cycle of `derived_from`.
12. Every view in design §11 follows its update map; bench rows within design §11.3.

## Tester

**Yes.** The brief adds fields to the definition record and its content hash, a project-symbol
origin, and updates that rewrite placed copies across pages while keeping IDs and wires: each
must be checked from outside, not only by the worker's tests.

## Needs owner check

Design §14 Q1–Q10, and a click-only trial (quick start `048-symbol-builder.md`): draw a vendor
module box with grouped pins, place three copies, edit it to add a pin and move one, and see the
review in the drawer before saving.
