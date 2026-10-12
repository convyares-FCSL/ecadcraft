# `.ecad` and `.ecadlib` persistence: design proposal

> **Status:** proposal for review (revision 1, 2026-10-12). **Nothing here is implemented.** No
> persistence code is to be written until the owner approves this proposal and its decisions
> ([§12](#12-decisions-for-the-owner)).
> **Builds on:** `docs/document-architecture-proposal.md` revision 3.1, especially P1-P10,
> §5 (file contracts), §6 (transactions, history, unsaved state). Those contracts stay
> authoritative; this document says how to meet them.
> **Owner's direction:** use JSON for the initial development implementation, because it can be
> read and understood directly. Keep encoding concerns separate from document semantics, so more
> efficient serialisation formats can be evaluated later.

## Summary

- **Three layers, kept apart:**
  1. **Semantics:** what a project or library file *means*. These are versioned record types:
     a header, transactions, ops, definitions and named versions. They are defined once,
     independent of bytes.
  2. **Encoding:** how one record becomes bytes and back (a *codec*), and how records are
     framed in a file. The first codec is **JSON Lines**: one JSON object per line, readable in
     any editor.
  3. **Storage:** where the bytes go and how they are made durable. This covers append, flush to
     disk, atomic Save As, a writer lock, and a test medium in memory.

  Replacing JSON later changes layer 2 only. A conformance suite proves that any new codec gives
  back exactly the same records.
- **A project file is its history.** It is a header followed by an append-only list of
  transactions (P1, P4, P9). Opening a project replays the history. Saving appends what is not
  stored yet and flushes it. Save As writes the whole history to a temporary file and renames it
  into place. These are the contracts FILE-01 to FILE-07 and SAVE-01 to SAVE-06, unchanged.
- **The file schema is not the in-memory model.** The file uses explicit version-1 record types
  with explicit conversions to and from `ecad-model`. This way, refactoring the model can never
  silently change the file format, and every old op version has a named upcast (FILE-06).
- **Integrity is checked at two levels:**
  - *Frame checksums* catch torn or damaged records. They are encoding level.
  - A *history hash chain* links each transaction to the one before. It is computed over a
    **canonical semantic form**, so it is the same whichever codec wrote the file. This makes
    converting a file between codecs verifiable.
- **A library file is the same kind of container.** It has its own history of definition edits,
  a stable library ID that survives renaming the file, and content-hashed definition versions.
  Projects embed copies (P8). A library change only ever *offers* an update (LIB-01 to LIB-05).
- **Not in the first implementation:**
  - checkpoints (opening replays everything; fine at development sizes);
  - a crash journal;
  - compaction;
  - binary codecs;
  - library editing UI;
  - DWG/DXF.

## Contents

1. [Scope and principles](#1-scope-and-principles)
2. [Layering: semantics, encoding, storage](#2-layering-semantics-encoding-storage)
3. [Semantic schema, version 1](#3-semantic-schema-version-1)
4. [The `.ecad` project file](#4-the-ecad-project-file)
5. [The `.ecadlib` library file](#5-the-ecadlib-library-file)
6. [JSON Lines, the first codec](#6-json-lines-the-first-codec)
7. [Integrity: checksums, hash chain, canonical form](#7-integrity-checksums-hash-chain-canonical-form)
8. [Storage: durability, Save As, locking, platforms](#8-storage-durability-save-as-locking-platforms)
9. [Opening, failures and hostile input](#9-opening-failures-and-hostile-input)
10. [Evaluating other encodings later](#10-evaluating-other-encodings-later)
11. [Code structure and staged plan](#11-code-structure-and-staged-plan)
12. [Decisions for the owner](#12-decisions-for-the-owner)
13. [Model gaps this proposal exposes](#13-model-gaps-this-proposal-exposes)

## 1. Scope and principles

**In scope:** the project file (`.ecad`), the library file (`.ecadlib`), how they are versioned,
written, opened and verified, and how projects take definitions from libraries.

**Out of scope here:**
- the product UI for libraries;
- part and catalogue data beyond symbol definitions;
- DWG/DXF exchange (architecture proposal §16);
- multi-user editing and merging;
- the crash journal;
- compaction.

Each item outside this scope is named where it touches the design.

Principles carried over, with what they mean for files:

| From | Meaning for persistence |
| --- | --- |
| P1, FILE-02 | The file holds **transactions**, not a drawing. Opening means replaying them. Replay never allocates IDs, snaps or infers. |
| P4, P9, FILE-05 | Append only. Nothing rewrites or drops history. Save As writes all of it. |
| P5 | No derived results, UI state, CADCraft handles or drawing copies are stored. |
| P8, FILE-01 | A project opens with no library, network or other file. |
| FILE-03, SAVE-04 | Whole records are appended, then flushed. A torn tail is ignored on open and cut off before the next save. The persisted position moves only after the flush succeeds. |
| FILE-06 | Every op has a version; old versions are upcast on read. |
| P10 | The persistence code has no CADCraft dependency (Core layer in `ecad/xtask layers`). |

## 2. Layering: semantics, encoding, storage

```
  ecad-model (in memory)        Document, Ledger, Transaction, Op            ← changes freely
        │  explicit conversion, versioned (to_v1 / from_v1 + upcasts)
        ▼
  1. semantic schema            FileHeader, TxRecord, OpRecord, DefinitionRecord, …  (v1)
        │  Codec: encode_record / decode_record     ← JSON Lines now; others later
        ▼
  2. encoding + framing         one frame per record: bytes + frame checksum
        │  Medium: append / flush / truncate / replace_atomically / lock
        ▼
  3. storage                    FileMedium (disk), MemoryMedium (tests), later browser storage
```

**Why three layers:**

- **Semantics stay put when encodings change.** The record types are the contract. A JSON file
  and a future binary file of the same project hold the *same records*. Tests can show this:
  decode from one codec and re-encode into another, and the records and the history hash are
  identical.
- **The model can evolve without breaking files.** Today the test store serialises
  `ecad_model::Transaction` directly through `serde`. That is acceptable only for a test store.
  With that approach a renamed field or a reshaped op changes the file format without anyone
  deciding to. Version-1 records are written out explicitly, and the model converts to and from
  them. When the model changes, either the conversion absorbs it, or a schema version 2 is
  introduced with an upcast from 1.
- **Storage stays boring and testable.** Durability rules (flush, rename, lock) live in one
  place. Tests use the in-memory medium with fault injection (fail on append or on flush, tear a
  write). Fault injection already exists in the test store.

**Cost:** about 25 op variants are mirrored as version-1 record types, with conversions both
ways. That is mechanical code and fully covered by round-trip tests. Inspector snapshots
(`ecad-inspect/1`) keep using the model's own `serde` output, because they are diagnostics, not
persisted projects (decision D2).

## 3. Semantic schema, version 1

Each record type below is part of schema version 1. Field names are part of the contract. Every
record carries `"type"`, and every op carries its name and its own version (`"v"`).

### 3.1 Common value rules

- **IDs:** 128-bit, written as 32 lowercase hex digits (as today). Each ID type is a distinct
  type in code. Files never contain CADCraft handles.
- **Numbers:** coordinates and lengths are in the project's length unit (§13). Angles are in
  **radians**, counter-clockwise (architecture §4.2). Only finite values are allowed. Values are
  written in the shortest form that reads back to the identical `f64`.
- **Strings:** UTF-8, NFC not enforced. Length caps apply on read (§9).
- **Unset versus set** (P7): an absent property means unset. An explicit empty string is a set
  value.
- **Times:** `time_ms` (milliseconds since the Unix epoch, UTC) is optional and informational.
  Replay never reads it (FILE-07).
- **Author:** `"user"` or `{"agent": "<name>"}`.
- **Transaction kind:** `"edit"`, `{"undo": "<tx id>"}` or `{"redo": "<tx id>"}`.

### 3.2 Project records (`.ecad`)

| Record | Fields | Notes |
| --- | --- | --- |
| `header` (first record, exactly once) | `container`: `"ecad-project"`; `container_version`: 1; `schema_version`: 1; `codec`: `"jsonl/1"`; `project_id`; `created_ms`; `writer`: product and version, e.g. `"ecad 0.1.0-dev (cadcraft 00c7613)"` | identifies the file regardless of its name |
| `tx` | `id`, `seq` (1, 2, …), `kind`, `author`, `time_ms?`, `label`, `ops: [op]`, `prev`: history hash of transaction `seq-1` (§7) | one transaction = one record; all-or-nothing |
| op (inside `tx`) | `{"op": "AddItem", "v": 1, …fields…}` | the 24 ops of `ecad-model` today, plus `NameVersion` and `SetProjectSetting` (§13) |

Named versions (bookmarks) are an op inside a transaction (`NameVersion { name, at_seq }`), so
they are history like everything else and are never lost (FILE-05). Checkpoints are **not**
records in the project file (§4.4).

### 3.3 Library records (`.ecadlib`)

| Record | Fields |
| --- | --- |
| `header` | `container`: `"ecad-library"`; `container_version`; `schema_version`; `codec`; `library_id`; `created_ms`; `writer` |
| `tx` | as for projects, with library ops |
| library ops | `AddDefinition { id, definition }`, `ModifyDefinition { id, before, after }`, `RemoveDefinition { id, definition }`, `SetLibraryProperty { key, before, after }`, `NameVersion { name, at_seq }` |

A **definition** is stored with:
- `name`;
- `graphics` (primitives in the local frame);
- `terminals` (key, default label, local position);
- `properties` (default component properties, all optional);
- later, text slots.

Its **version** is not stored: it is the `seq` of the library transaction that last changed it.
Its **content hash** is computed, never stored as fact (§7.3).

### 3.4 Embedded definitions in projects

Projects keep the existing `EmbedDefinition` op. Its `origin` becomes:
- `library_id`;
- `definition_id` (the library's ID);
- `library_seq`;
- `content_hash`.

This replaces today's untyped `{library, definition, version}` strings (§13). A definition
created in the project itself has no origin. Editing an embedded copy creates a new local
definition marked "modified from" its origin (LIB-04).

## 4. The `.ecad` project file

### 4.1 Layout

```
{"type":"header","container":"ecad-project","container_version":1,"schema_version":1,"codec":"jsonl/1","project_id":"…","created_ms":…,"writer":"ecad 0.1.0-dev (cadcraft 00c7613)","crc":"…"}
{"type":"tx","seq":1,"id":"…","kind":"edit","author":"user","time_ms":…,"label":"New project","prev":"…","ops":[{"op":"AddPage","v":1,…},…],"crc":"…"}
{"type":"tx","seq":2,…}
```

(Line breaks shown only between records. A record never spans lines, and `crc` is the frame
checksum of §7.1.)

### 4.2 Saving

- **Save** (the file exists and holds this project's history up to `persisted`):
  1. Cut back any torn tail found when the file was opened or last written.
  2. Append the records for transactions `persisted+1 … head`.
  3. Flush them to disk.
  4. Only then advance `persisted` (SAVE-04).

  If writing or flushing fails, `persisted` stays put and the error is reported. The project
  stays unsaved, and nothing in memory is lost.
- **Save As, or first save:**
  1. Write the header and *all* transactions to `name.ecad.tmp-<random>` in the same folder.
  2. Flush it, then rename it over the target.
  3. Flush the folder where the platform supports it.

  A failure never damages an existing file. The project's identity (`project_id`) stays the
  same: Save As is a new location, not a new project. The open question is a "Save a Copy As"
  that issues a new `project_id`; see D7.
- **Never:** rewrite, reorder, compact or truncate good history. The only truncation is cutting
  an incomplete tail back to the last good record.

### 4.3 Opening

1. Read the header. Check `container`, `container_version`, `schema_version` and `codec`. An
   unknown or newer value refuses to open, with a message naming the version this build
   supports.
2. Read the records in order, checking for each:
   - its frame checksum;
   - that `seq` is consecutive;
   - that the `prev` hash links to the previous transaction;
   - that each op decodes, upcasting old op versions.
3. Replay through `Ledger::replay`. This checks preconditions (TX-04) and every document
   invariant after each transaction (TX-03).
4. Results:
   - **Clean:** the project opens writable, with `persisted = head`.
   - **Torn last record:** an incomplete, unterminated or checksum-failing *final* record is
     ignored (FILE-03) and reported. The project opens writable. The tail is cut off before the
     next save.
   - **Damage before the end:** a bad record in the middle, a broken chain, a precondition
     failure or an invariant failure. The project opens **read-only** at the last good
     transaction, with a report naming the first bad `seq` (TX-04). Nothing is written. The user
     may Save As to a new file, which writes only the good history.

### 4.4 Checkpoints (later, and disposable)

Replaying is linear in the history. For development projects that is milliseconds. When
checkpoints are needed they will:
- live in a **sidecar cache** (`name.ecad-cache/` or one cache file), never inside the project
  file;
- record `{project_id, seq, history hash at seq, document}`;
- be used only when the project's history hash at that `seq` matches.

Deleting the cache only changes how long opening takes (FILE-04, P9). See D5.

## 5. The `.ecadlib` library file

- **Same container**, framing, codec, integrity rules and durability rules as projects, with
  library records (§3.3).
- **Stable identity:** `library_id` comes from the header, so renaming or moving the file
  changes nothing. Two files with the same `library_id` but diverging histories (one copied and
  edited separately) are detected by their hash chains, and reported as the same library with
  different history.
- **Definition versions and hashes:**
  - A definition's version is the `seq` of the library transaction that last changed it.
  - Its content hash is computed over its canonical form (§7.3).
  - A project's embedded copy records both values.
  - "Update available" is derived (P5) by comparing the embedded copy's `content_hash` with the
    library's current definition. Nothing changes until the user updates, as one undoable
    transaction (LIB-03).
- **Placing from a library** (LIB-01) works as today: the definition is embedded (if the project
  lacks that exact content) and the component, terminals and symbol instance are created, all in
  one transaction. The project never refers to the library file at run time (P8).
- **Same name, different content or origin:** both are kept (LIB-05). Identity is by ID and hash,
  never by name.
- **Open, as in architecture §5.3:**
  - how terminals map when a definition is replaced;
  - whether parts and symbols share a container;
  - library editing UI.

  None of these blocks the file format.

## 6. JSON Lines, the first codec

**Why JSON Lines:**
- every record is one line of standard JSON, readable in any editor and in `jq`;
- appending is a plain write of one more line;
- a torn write is visible as an incomplete last line;
- it matches the existing test store and inspector snapshots, so tools carry over.

**Rules for `jsonl/1`:**

- **UTF-8**, with `\n` as the record terminator. No BOM. CR LF is not accepted on read.
- **One record per line.** No pretty-printing in the file. For reading, `ecad-cli dump` will
  pretty-print with indentation, and `ecad-cli check` will verify a file (§11). The inspector may
  later open project files directly, read-only.
- **Deterministic output:** the writer emits keys in a fixed order (struct field order; maps
  sorted by key, already `BTreeMap`). Floats use the shortest round-trip form
  (`serde_json` with `float_roundtrip`). Writing the same records always gives the same bytes.
- **Frame checksum** as a trailing `"crc"` member of each record (§7.1).
- **Size:** JSON is roughly 3-6 times larger than a compact binary form. The development
  projects of the coming milestones measure in kilobytes to a few megabytes. §10 says when to
  revisit.

## 7. Integrity: checksums, hash chain, canonical form

### 7.1 Frame checksum (encoding level)

- **What it checks:** that each record's bytes are as written.
- **How:** CRC-32C of the record's encoded bytes without the `crc` member, written as 8 hex
  digits. In JSON Lines the record is serialised without `crc`, the CRC is computed, and
  `,"crc":"…"}` is spliced before the closing brace.
- **What it catches:** disk and transfer damage and torn writes. It does not protect against
  deliberate edits.
- **Other codecs:** each codec defines its own frame checksum.

### 7.2 History hash chain (semantic level)

- **What it checks:** that the history is complete, ordered, and is the history this project
  had.
- **How:** each `tx` record's `prev` is the SHA-256 (hex) of the canonical form (§7.3) of
  transaction `seq-1`, *including that transaction's own `prev`*. Transaction 1's `prev` is the
  hash of the header's `project_id`.
- **What it catches:**
  - a missing, reordered or duplicated transaction;
  - two files claiming the same project but holding diverging histories;
  - a damaged record that still parses but differs.
- **What it enables:**
  - **verifiable codec conversion:** the chain is computed over semantics, so a file converted
    to another codec keeps every hash;
  - **checkpoint validation** (§4.4);
  - **named versions** that name a hash as well as a `seq`.

### 7.3 Canonical form

The canonical form of a record is its semantic content serialised as JSON with:
- the RFC 8785 rules (JSON Canonicalization Scheme): sorted keys, fixed number formatting and no
  insignificant whitespace;
- the frame `crc` excluded.

It is used **only for hashing**. It does not depend on which codec stored the record. Definition
content hashes (§5) use the same canonical form of the definition alone, so the same symbol has
the same hash in every project and library.

**Why JCS and SHA-256:**
- both are standard and inspectable: you can recompute a hash by hand with common tools;
- both have small, permissively licensed Rust implementations (MIT/Apache), which need a line
  in the ECAD dependency notes;
- BLAKE3 would be faster, and is an option if hashing ever shows up in profiles (D4).

## 8. Storage: durability, Save As, locking, platforms

- **The `Medium` trait** (already in `ecad-store`) gains `replace_atomically` (write a temporary
  file, then rename) and `lock`. Implementations:
  - `FileMedium` (std file I/O; `sync_data` after appends; folder flush after rename on Unix);
  - `MemoryMedium` (tests, with fault injection);
  - later, browser storage for the web build. File access is not available on wasm; the codec
    and schema crates stay wasm-clean.
- **Writer lock:** opening a project for writing takes an OS file lock on the file (advisory on
  Unix, mandatory on Windows). A second writer opens **read-only** and says so. A stale lock
  cannot outlive its process, because OS locks are released when the process ends.
- **Platform notes:**
  - On **Windows**, `std::fs::rename` replaces an existing target. Flushing a folder is not
    available and is skipped.
  - On **WSL**, keep projects in the Linux file system. Files under `/mnt/c` work, but they are
    slower, and their locking and flushing go through the Windows bridge. The trial README
    already advises the Linux file system for the build.
  - On **network and cloud-synced folders** (OneDrive, Dropbox), appends and locks are only as
    reliable as the sync client. These folders are documented as unsupported for writing during
    development.
- **Never-crash rules apply** (`ecad/CLAUDE.md`): every I/O error becomes a `Result` with the
  path and the operation, and is shown to the user. Nothing panics.

## 9. Opening, failures and hostile input

Project files can come from anywhere, so the reader treats them as hostile:

| Threat | Rule |
| --- | --- |
| Huge record or line | refuse lines over a cap (initially 64 MiB) before parsing |
| Huge counts | caps on transactions per file, ops per transaction, vertices per polyline, string length; checked while decoding |
| Deep nesting | the schema has fixed, shallow nesting; the JSON parser's recursion limit stays on |
| Non-finite or out-of-range numbers | refused at decode (in addition to `Document::check`) |
| Duplicate or colliding IDs | refused by op preconditions on replay (TX-04) |
| Unknown record type, op, op version or field | refused, naming it. Unknown fields are refused, not ignored, so nothing is lost silently by an older reader (D6) |
| Inconsistent history | read-only at the last good transaction (§4.3) |

**Tests:**
- a fuzz target (`cargo fuzz`, or a proptest corpus) over the reader;
- golden files for every schema version ever released;
- round trips (model → records → bytes → records → model);
- fault-injection tests for every SAVE-04 failure point.

## 10. Evaluating other encodings later

Changing codec is a layer-2 change. To make it a measured decision rather than a guess:

- **Codec trait:** `encode(&Record) -> Vec<u8>`, `decode(&[u8]) -> Result<Record>`, plus
  framing (`jsonl`: newline; binary: length prefix and checksum). The schema types derive
  `serde`, so serde formats are cheap to try:
  - CBOR, a standard and self-describing format;
  - MessagePack;
  - postcard, which is compact but not self-describing.
- **Conformance suite** that every codec must pass:
  - golden projects decode to identical records;
  - the history hash is identical;
  - torn-tail and damaged-record behaviour as in §4.3;
  - hostile-input caps.
- **Benchmark corpus:** generated projects of 1k, 10k and 100k transactions and libraries of
  100 and 1,000 definitions. The measures are file size, open (replay) time, append time and
  peak memory.
- **When to revisit:**
  - when opening the largest realistic project takes noticeably long (say over 1 second) with
    checkpoints in place;
  - when file size becomes a practical problem.

  JSON stays available as an export and inspection format either way (`ecad-cli convert`).
- **Not planned:** zero-copy formats (FlatBuffers, Cap'n Proto). The file is replayed into the
  model, so zero-copy reads buy little.

## 11. Code structure and staged plan

**Crates (all in the ECAD workspace):**

| Crate | Layer | Contents |
| --- | --- | --- |
| `ecad-file` (new) | Core | schema v1 record types; conversions to and from `ecad-model`; upcasts; canonical form and hashes; the `Codec` trait and the `jsonl` codec; the container reader and writer; caps. No I/O beyond the `Medium` trait. wasm-clean. |
| `ecad-store` | Core | the `Medium` trait, `MemoryMedium` with fault injection, `FileMedium` (native only). The current test store is retired once `ecad-file` passes its tests. |
| `ecad-cadcraft` | Adapter | `Persistence::File` alongside `TestStore` and `Unavailable`; open, save and save-as through `ecad-file`; open projects read-only on damage |
| `ecad-cli` (new, small) | UI | `ecad-cli dump`, `check`, `convert` |
| `ecad-trial` | UI | gains Open, Save and Save As only once stage 2 lands; its "no persistence" labelling changes then, and not before |

**Stages.** Each stage is one PR into `dev`, and none starts before approval.

1. **Schema and JSON codec, in memory.**
   - Covers `ecad-file`: records, conversions, canonical form, hash chain, the `jsonl` codec, and
     reading and writing over `MemoryMedium`.
   - Tests: golden files, round trips, torn tail, damage in the middle opening read-only,
     hostile caps, fuzzing.
   - Adds the model gaps from §13 (project settings and units, `NameVersion`, typed origin).
2. **Files on disk.**
   - Covers `FileMedium`: append and `sync_data`, atomic Save As, the writer lock, the
     read-only path.
   - Wires in the adapter's `Persistence::File` and adds Open, Save and Save As to the trial
     harness.
   - Tests: a real temporary folder, plus a failure on every flush and rename step.
   - Ends with a manual WSL trial: save, quit, reopen, verify with the inspector.
3. **Libraries.**
   - Covers the `.ecadlib` container, placing from a library file, typed origin and content
     hash, and "update available" as a derived result.
   - The update op is a separate small design (LIB-03, terminal mapping).
4. **Opening speed:** checkpoints in a sidecar cache, only if measurements call for them.
5. **Codec evaluation:** the conformance suite run against one binary codec, with the benchmark
   corpus.

## 12. Decisions for the owner

| # | Decision | Recommendation |
| --- | --- | --- |
| D1 | Three layers (semantics / encoding / storage) with JSON Lines as the first codec | **Yes**. This is the owner's stated direction, made concrete. |
| D2 | Separate version-1 record types in `ecad-file`, rather than serialising `ecad-model` types directly | **Yes.** Model refactors cannot silently change files. Inspector snapshots keep using the model's serde output. |
| D3 | One record per transaction, append-only, a single file per project, no inline checkpoints | **Yes** (FILE-03, FILE-05) |
| D4 | Frame checksum CRC-32C (encoding level); history hash chain SHA-256 over JCS canonical form (semantic level) | **Yes.** BLAKE3 only if profiles demand it. |
| D5 | No checkpoints in stages 1-3; later only as a sidecar cache validated by the history hash | **Yes** |
| D6 | Strict reading: unknown records, ops, versions and fields are refused, never ignored | **Yes** for development. Revisit before the first release that must open newer files. |
| D7 | Save As keeps `project_id`; add "Save a Copy As" (new `project_id`) later | **Keep the ID.** Decide whether a copy command is needed. |
| D8 | Damaged history opens read-only at the last good transaction; Save As can rescue the good part | **Yes** (TX-04) |
| D9 | `.ecadlib` uses the same container, with a stable `library_id`, version = `seq`, and content hash computed over the canonical form | **Yes**. Parts versus symbols stays open. |
| D10 | WSL: Linux file system recommended; cloud-synced folders unsupported for writing during development | **Yes**. Document it in the trial README and the environment notes. |
| D11 | Length unit stored in project settings, mm by default; the unit is fixed at creation for now | **Yes**. Changing units is a later explicit transaction. |
| D12 | Staging as in §11, each stage a PR into `dev`, starting with stage 1 | Approve, or reorder |

## 13. Model gaps this proposal exposes

These are small model additions that stage 1 would make. They are listed here so the format
decisions do not hide model decisions.

- **Project settings and units.** `Document` has no project-level settings today, although
  architecture §3.2 lists units on `Project`. Add `ProjectSettings { length_unit, … }` and
  `SetProjectSetting { key, before, after }`, recorded at creation (D11).
- **Named versions.** Architecture §6.1 lists `NameVersion`; the model does not have it yet.
- **Typed definition origin.** Today `Origin { library: String, definition: String, version }`.
  Proposed: `Origin { library_id, definition_id, library_seq, content_hash }` (§3.4).
- **Definition default properties.** These are listed in architecture §5.2 but not modelled yet.
  They are needed for libraries (stage 3), not for stage 1.
- **Agent identity.** `Author::Agent(String)` is free text. A structured agent identity (name,
  version, session) can wait. The schema keeps the field open as a string for now.
