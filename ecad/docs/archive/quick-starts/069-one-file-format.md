Historical record (2026-11-16). Not current; see [reference/file-format.md](../../reference/file-format.md).

# Brief 069: one file format

Part of the [quick start](../quick-start.md). A section number refers to the quick start's
(1–10, 5A…).

## What this brief changed

Pre-v1, ECAD keeps one `.ecad` format: the redb database (container version 2, EEP-0005). The
JSON Lines version 1 format is gone, and with it:

- the app's **Convert this project?** offer when a version 1 file is opened: such a file is now
  refused as "not an ECAD project file";
- `ecad upgrade` (the `ecad` command line has since been removed; headless use goes through
  `ecad-mcp`);
- `ecadinspect … export-jsonl` and the inspector page's **Export JSONL** button. The page's
  **Raw** and **Readable** views (History log, row expand; Tables, row expand) show every stored
  row instead;
- in the debugger, the Persistence view's header, verified bytes and record offsets, and **Read
  bytes**: **Read row** reads a saved transaction's row, also in the automated tests' in-memory
  project.

Nothing a person sees in a project saved as a redb file changes.

## How to try it (by clicking)

1. Open a project saved by this version (quick start §7): it opens as before.
2. Click **Open…** and type the path of a file that is not a redb project, for example a text
   file renamed to `old.ecad`. Expected: `could not open …: it is not an ECAD project file`, and
   no dialog offering a conversion.
3. Inspector (quick start §8, "The inspector"): on the History tab, expand a log row: its record
   shows. On the Tables tab pick `transactions`, expand a row, press **Raw** (the stored bytes in
   hex) and **Readable** (the record). There is no **Export JSONL** button.
4. Debugger (quick start §8): after a save, pick a transaction in History and press **Read row**.

## Checks

- *Automated:* `ecad-file`'s `canonical_contracts::record_and_definition_hashes_are_pinned` (the
  record format, chain and definition hashes, checked against the old golden file before it was
  removed) and `records_contracts::the_golden_project_opens` (`tests/golden/project.ecad`);
  `ecad-inspector`'s `lanes::lane_069::every_op_kind_renders_raw_and_readable` (a project with
  every op kind, read from `Op` itself, renders in Raw and Readable through the protocol).
- *Owner-tried:* not yet.
