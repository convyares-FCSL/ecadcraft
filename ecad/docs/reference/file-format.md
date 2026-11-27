# The `.ecad` file format

What a project file holds and how ECAD reads and writes it, as built. The code is `ecad-file` (the tables,
records, canonical form, hash chain, snapshots, opening, saving and Verify) on top of `ecad-store` (the redb
database handle, the writer lock, the steps of an atomic replace). Where this page and the code differ, the code
is right and this page is wrong.

Words: a **project file** is one `.ecad` file. A **branch** is a line of history; the original one is `main`. A
**transaction** is one recorded step (an edit, an undo, a redo or a merge), made of **ops**. A **version** is a
name a person gives a point in history; a **revision** is an issued one (Rev A, B…). A **snapshot** is the
document's state at a point, stored as **chunks**, so opening does not replay the whole history.

## Versions of the format

A file records four numbers. Each is independent of the others and of the product version.

| What | Where | Value this build writes | Reading another value |
| --- | --- | --- | --- |
| Container version | `meta.container_version` | `2` (`ecad_file::schema::CONTAINER_VERSION`) | refused: the file does not open |
| Codec | `meta.codec` | `"json/1"` (`ecad_file::schema::CODEC`) | refused |
| Record version | `meta.schema_version` | `4` (`ecad_file::SCHEMA_VERSION`, in `ecad-file/src/record.rs`) | opens read-only, named "older format version (file 3, this build 4)" or "newer…", when it reads to its head; refused otherwise, saying how many transactions could be read ("only 4 of its 18 transactions could be read") |
| Op version | `v` in every op | `1` (`ecad_file::records::OP_VERSION`) | the record is refused (`OpVersion`) |

The **record version** says what record contents mean. Every change to the format bumps it: a record's fields
or meaning, an op, a canonical form. The golden test (`ecad-file/tests/golden/`) pins it together with the hash
of the golden project's last transaction, so a changed format with the old number fails there. The versions so
far:

| Record version | Change |
| --- | --- |
| 1 | The first format. |
| 2 | Typed revisions, point names. |
| 3 | A terminal strip level's own label is the property `block_label.<p>.<n>`. |
| 4 | Device data: a potential's supply, a motor's rated voltage and the new rating fields. |

There are no upcasts and no converters: a build reads the record version it writes, and opens other versions
read-only when it can read them to their head. A file of another version that this build cannot read to its head
is refused rather than shown in part: the part would look like the whole project, and a copy of it would keep only
that part. Damage in a file of another version is reported with its version, never hidden by it, and such a file
gets no Save As advice.

## The container

A project file is one **redb** database: pure Rust, crash-safe, one file, nothing to install and no server.
redb's physical file format is redb's; ECAD owns the tables, keys and values below. The redb version is pinned
exactly in the workspace manifest (`redb = "=4.5.0"`): changing it is a deliberate change with its own review and
tests, never a routine update.

### Tables

| Table | Key | Value |
| --- | --- | --- |
| `meta` | name (string) | JSON: container facts only |
| `branches` | branch ID (16 bytes) | JSON: the branch's name, status, parent, fork point and head |
| `transactions` | branch ID ‖ `seq` (24 bytes) | `flag` (1 byte) ‖ chain value (32 bytes) ‖ body |
| `versions` | branch ID ‖ kind byte ‖ name (UTF-8) | JSON: a point, the hashes of its chunks, its undo and redo stacks |
| `chunks` | chunk hash (32 bytes) | `flag` ‖ body |
| `assets` | SHA-256 of the bytes (32 bytes) | media type length (u16, big-endian) ‖ media type ‖ bytes |

- Integers in keys are big-endian, so key order is numeric order.
- `meta`, `branches` and `versions` values are JSON, small enough to read by eye.
- One `transactions` table holds every branch, so the schema stays fixed as branches come and go.
- **Append-only is enforced by ECAD's API**, not by redb. The only write paths (`ecad_file::store::Writer`)
  are: `append_tx` (a `transactions` row directly after the branch's last one; any other `seq` is refused, with
  checked arithmetic, so nothing follows a row at 2^64 − 1), `put_chunk` and `put_asset` (insert only if absent;
  a chunk's hash must match its body), `put_branch`, `put_version` and removing a version row, `set_meta` (only
  the names below) and `copy_all` (every row into an empty file, for Save As). No code path updates or deletes a
  `transactions` row.

### `meta`

| Name | Value |
| --- | --- |
| `format` | `"ecad-project"` |
| `container_version` | `2` |
| `schema_version` | the record version (above) |
| `codec` | `"json/1"`: each transaction row is its canonical JSON record (`bin/1` is reserved for a later codec) |
| `file_id` | the project ID |
| `writer` | the build that last committed |
| `created_ms` | when the file was created, milliseconds since 1970 (informational) |
| `cache` | `{"every": n, "latest": {<branch ID hex>: {"kind", "name", "seq"}}}`: the automatic snapshot interval and, per branch, the snapshot opening uses |
| `active` | the ID (32 hex digits) of the branch the project was last shown on; absent, unknown or not open means `main` |

`meta` never holds project data (title, customer, settings): those are transactions. A `meta` name this build
does not know is an unknown word: the project opens read-only, the reason names it ("unknown meta name
`future_name`"), and nothing writes to the file. Unknown fields inside a `meta` value are refused.

The automatic snapshot interval defaults to 2,000 transactions. It is about the file, not the design, so it is not
a project setting (`Project::set_snapshot_every`).

### `branches`

| Field | Meaning |
| --- | --- |
| `name` | the person's name for it; unique among open branches; `main` for the original branch |
| `status` | `open`, `merged` or `archived` |
| `parent` | absent for `main`; else the parent branch's ID (32 hex digits) |
| `fork_point` | absent for `main`; else `{seq, chain}`: the parent transaction it starts after (`seq` 0 is the parent's own start) |
| `head` | `{seq, chain}`: the branch's last transaction |
| `release_line` | whether revisions may be issued on it; `true` for `main` only |
| `merged` | optional, `{into, seq}`: the target branch and the merge's `seq` there, for a merged branch |
| `created_ms`, `created_by` | optional, for display |

- `main`'s ID is the first 16 bytes of `SHA-256("ecad-branch/main\0" ‖ project ID as 16 big-endian bytes)`, so
  every file of a project names it alike. Every other branch has a random 128-bit ID.
- A new branch's row is written at once, in a write transaction of its own, outside Save, with `head` `{0,
  C(0)}` (see [Hashes](#hashes)). Creating a branch writes no snapshot. Rename, archive and reopen rewrite the
  row the same way; `main` cannot be renamed or archived.
- `head` must equal the branch's last `transactions` row. A reader checks it when it opens the branch; a
  difference is damage.
- Unknown fields are refused.

### `transactions` and the history model

1. **Each branch owns its own append-only log.** Its `seq` starts at 1 and has no gaps.
2. **Only `main` starts from the project root.** Transaction 1's `prev` is the project's chain seed.
3. **Every other branch starts from a fixed reference to a transaction of its parent** (`parent` and
   `fork_point`). Its chain starts at the branch seed.
4. Branches may fork from other branches, to any depth.
5. **Ancestral history is referenced, never copied.** A branch's state at its `seq` *k* is the parent's state at
   `fork_point` (defined the same way, up to the root), followed by the branch's transactions 1 … *k*.
   Example: `main` M1→M2→M3→M4; branch Alt forks at M3 and records A1→A2; Alt's state is M1…M3 then A1, A2,
   and M4 does not affect it.
6. **Branches are never snapshot-only.** A snapshot speeds up opening; the branch is always defined by its
   ancestry and its own log.
7. A row's body is the transaction's [record](#transaction-records), in [canonical JSON](#canonical-json),
   with its `prev`, compressed when that is smaller. The row also stores its own chain value `C(seq)`; a
   reader checks it against the decompressed record and the record's `prev` against the row before. A row
   decodes alone, and its chain value is its content address.

### `versions`

The key is the branch ID, a **kind byte** and a name, so a version `B`, the revision `B` and an automatic
snapshot never collide.

| Kind byte | `kind` in the row | Name (the key) | What it is |
| --- | --- | --- | --- |
| 1 | `{"point": "version"}` | the version's text | a version a person named |
| 2 | `{"point": "revision"}` | `initial`, `letter:<n>` or `number:<n>` | an issued revision |
| 3 | `"auto"` | its `seq` as 20 decimal digits | an automatic snapshot: a cache, never shown as a version |
| 4 | `"merge"` | its step's `seq` as 20 decimal digits | the merge index: one row per live merge into the branch, no chunks |

| Field | Meaning |
| --- | --- |
| `kind`, `name` | as above |
| `point` | `{seq, chain}` on the branch |
| `chunks` | chunk key → chunk hash (64 hex digits) of the state at `point` |
| `release` | a revision only: `revision`, the typed revision record as issued, and `assets`, the hashes of frozen outputs (none are stored yet) |
| `undo`, `redo` | the top of the undo and redo stacks at `point`, as `[seq, transaction ID]`, oldest first |
| `undo_hidden`, `redo_hidden` | how many entries lie below each |
| `stack_cap` | the cap the row was written with (1,024 by this build) |
| `description` | optional: the version's description |
| `merge` | kind `merge` only: `{source, source_head, ancestor}`, copied from the merge transaction |
| `tag_rule` | optional: the tag rule (`ecad_model::naming::tag::TAG_RULE`) the `model:meta` tag marks were computed with; absent means 1 |

- Versions and revisions are transactions too (`NameVersion`, `UnnameVersion`); `versions` indexes them and
  must agree with them. A rev is unique across the project, and revisions are issued only on a release line.
- Versions are branch-local: a branch's rows are its own; the names it inherits are its ancestry's rows at or
  before the fork.
- A reader never uses tag marks of an older rule: it recomputes them from the history. Writers record the
  current rule on every snapshot. It lives here, not in `model:meta`, so older builds still open files that
  carry it.
- **Undo stacks are bounded.** Recording the whole undo stack would grow each row with the history and the
  file quadratically, so a row keeps only the top `stack_cap` entries of each stack. A reader takes the cap from
  the row's own `stack_cap`, never from its build. When undo or redo empties what is held, the next entries are
  found by walking the history backwards only as far as needed, and kept for the session.
- Unknown fields are refused.

### `chunks`

A chunk is a content-addressed piece of a snapshot. Chunks are shared by every version and branch with the same
content.

| Key | Holds |
| --- | --- |
| `page:<PageId>` | the page, the items on it and the representations on it |
| `model:project` | page order, settings, named versions, the display values, folders and folder order; the standard, the project's style records and the device links (each left out while empty) |
| `model:<kind>:<b>` | the objects of one kind whose ID falls in bucket `b`; the kinds are `layer`, `device`, `connection_point`, `wire`, `cable`, `definition`, `aid` and `loose_part` |
| `model:meta` | `{"tag_high": {<prefix>: <highest number>}}`: the highest number each tag prefix has had up to the snapshot's point, deleted and undone tags included |

- **Buckets.** A kind with *n* objects has *B* buckets, the smallest power of two with *B* × 1,024 ≥ *n* (at least
  1), and at most 2^20. An object is in bucket `mix(id) mod B`. An edit therefore rewrites one chunk; all of a
  kind's chunks are rewritten only when its count crosses a power of two. (Object IDs come from per-session
  counters with random starts, so cutting by ID order would shift every later chunk on one insert.)
- **`mix(id)`**, fixed by the format and independent of any crate version: XOR the 128-bit ID's two 64-bit
  halves, then apply the SplitMix64 finaliser, all arithmetic wrapping on 64 bits:
  `z += 0x9E3779B97F4A7C15; z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9; z = (z ^ (z >> 27)) * 0x94D049BB133111EB;
  z ^= z >> 31`.
- **`model:meta` is a cache.** Verify checks it against the history, and a snapshot without it is rebuilt by
  scanning the rows before it. On open, its value plus the tail gives the tag counter, so a tag deleted before
  the snapshot is never given again.
- **Body.** A chunk's body is serde's JSON of the model types (struct fields in declaration order, maps in key
  order): deterministic for a build, so equal content dedupes, but not RFC 8785. Its hash is
  `SHA-256("ecad-chunk/1\0" ‖ uncompressed body)`.
- Snapshots are built incrementally: only the keys the transactions since the previous snapshot touched are
  rebuilt; the rest keep their hashes.
- A chunk that no version or cache entry names is garbage; Save As leaves it out.

### `assets`

Images, PDFs and other binary files, keyed by SHA-256 of their bytes, stored as they are (not recompressed), with
a media type of at most 255 bytes. They are referenced by hash, never embedded in transactions. No product path
stores an asset yet.

### Compression

Transaction bodies and chunk bodies are compressed with **Zstandard** (the `zstd` crate, libzstd, pinned
`=0.13.4`, level 1, no dictionary) when the body is at least 64 bytes and the result is smaller. The flag byte
says which: `0` raw, `1` a Zstandard frame. They are standard frames, so the file does not depend on which encoder
wrote them. Decoding is capped at 64 MiB. Hashes and chain values are always over the uncompressed bytes. Assets
are stored as they are.

## Transaction records

Each transaction is one JSON object. Every field is required and unknown fields are refused.

| Field | JSON | Meaning |
| --- | --- | --- |
| `type` | string | `"tx"` |
| `seq` | integer | position in the branch's history, from 1; at most 2^53 − 1 |
| `id` | string, 32 hex | the transaction's ID |
| `kind` | see below | edit, undo, redo or merge |
| `author` | see below | who made it |
| `time_ms` | integer or `null` | wall-clock milliseconds since 1970 UTC; informational, never used by replay; at most 2^53 − 1 |
| `label` | string | the short description the History tab shows (for undo and redo, the label of the step undone or redone) |
| `prev` | string, 64 hex | the chain link: the previous row's chain value, or the seed for `seq` 1 |
| `ops` | array | the ops, applied in order |

**`kind`:**

| Value | Meaning |
| --- | --- |
| `"edit"` | an ordinary change |
| `{"undo": "<tx id>"}` | inverts that transaction, which must be on top of the undo stack |
| `{"redo": "<tx id>"}` | inverts that undo, which must be on top of the redo stack |
| `{"merge": {"source": <branch ID>, "source_head": {"seq", "chain"}, "ancestor": {"branch", "seq", "chain"}}}` | a merge into this branch |

**`author`:** `"user"` for a person, or `{"agent": "<client name>"}` for an agent, such as an MCP client.

An undo or redo holds **concrete ops**: the inverses of the target's ops, in reverse order. Replay never computes
them; it applies what is recorded.

A **merge** transaction's ops are ordinary ops with `before` values from the target's state, so the target
replays without reading the source's log. On the undo stack it is an edit; its undo and redo are ordinary
records. A build that does not know the kind refuses the record.

A record type `checkpoint` is reserved and refused.

## Ops

Each element of `ops` is a flat object:

```
{"op": "<Name>", "v": 1, …the op's fields…}
```

- `op` is one of the names below. An unknown name is refused (`UnknownOp`).
- `v` is the op's version. Every op is at version 1; any other version is refused (`OpVersion`). There are no
  upcasts.
- The other fields are exactly the op's fields; unknown fields are refused.
- Integer fields (`index`, `at`, a definition origin's `library_seq`) must be at most 2^53 − 1.

| Area | Ops |
| --- | --- |
| Pages and folders | `AddPage`, `RemovePage`, `ModifyPage`, `AddFolder`, `RemoveFolder`, `ModifyFolder` |
| Layers and items | `AddLayer`, `RemoveLayer`, `AddItem`, `ModifyItem`, `RemoveItem` |
| Devices | `CreateDevice`, `DeleteDevice`, `ModifyDevice`, `SetDeviceDefinition`, `AddDeviceLink`, `RemoveDeviceLink` |
| Connection points | `AddConnectionPoint`, `RemoveConnectionPoint` |
| Representations | `LinkRepresentation`, `ModifyRepresentation`, `UnlinkRepresentation` |
| Wires and routes | `CreateWire`, `DeleteWire`, `ModifyWire`, `AddRoute`, `RemoveRoute`, `ModifyRoute` |
| Cables | `CreateCable`, `DeleteCable`, `ModifyCable` |
| Drawing aids and loose parts | `CreateAid`, `DeleteAid`, `ModifyAid`, `CreateLoosePart`, `DeleteLoosePart`, `ModifyLoosePart` |
| Properties | `SetProperty` |
| Definitions | `EmbedDefinition`, `RemoveDefinition` |
| Project, styles and standard | `SetProjectSettings`, `ModifyProject`, `SetStyle`, `SetStandard` |
| Versions and revisions | `NameVersion`, `UnnameVersion` |

The list in code is `OP_NAMES` in `ecad-file/src/record.rs`; the fields of each are the `Op` enum in
`ecad-file/src/records.rs`.

### Recorded values and inverses

Every op carries the value it adds, removes or replaces, so it is its own record of intent and its inverse can be
computed from the op alone:

- **Add and remove pairs** (`Add…` ⇄ `Remove…`, `Create…` ⇄ `Delete…`, `Link…` ⇄ `Unlink…`, `EmbedDefinition` ⇄
  `RemoveDefinition`, `NameVersion` ⇄ `UnnameVersion`) carry the whole value. Removing must name the value that
  is there.
- **Modify and set ops** carry `before` and `after`; the inverse swaps them.
- Replay checks every recorded `before` against the document. Replay never snaps, searches, infers or allocates
  IDs.

### Versions and revisions as data

- A **point name** is `{"version": "<text>"}` or `{"revision": <rev>}`. A version and a revision are told apart
  by the variant, never by a spelling such as `Rev `.
- A **rev** is a position in the project's revision scheme: `"initial"` (the first issue, `–`), `{"letter": n}`
  (A = 1, Z = 26, AA = 27) or `{"number": n}`. The scheme says how revs are written and what comes next; a rev
  outside the scheme, or two rows with one rev, make the project not well formed. The scheme cannot change once a
  revision is issued.
- A **revision record** has `rev`, `date` (`YYYY-MM-DD`), `description`, `drawn`, `checked`, `approved`, `ecr`,
  `custom` (the project's own columns), `issued` and `released`; every field is left out while unset.
- `NameVersion` carries the point name, `at` (the `seq` it names, below this transaction's own) and an optional
  `description`.
- Issuing a revision is one transaction: the Finalise ops (when asked), the `ModifyProject` that marks the row
  issued, and `NameVersion` with the revision's point name and `at` the head before the transaction.

## Values

The record types are `ecad-file`'s own (`records.rs`): they mirror `ecad-model` deliberately, so the model may
change shape freely while the records change only with a record version. The exceptions are stored as the model
writes them: a device's type (`DeviceType`, with its ratings), its tag (`TagSlot`), its location and function
(`Place`) and user values (`UserValue`), whose words and fields are strict already. The golden fixture holds one
device of each type with every field written out, so a field added to a device type does not compile until the
fixture names it, and a changed word or field changes the pinned hash.

| Value | JSON |
| --- | --- |
| any ID (page, layer, item, device, connection point, representation, wire, route, cable, definition, folder, library, transaction, project) | a string of 32 lower-case hex digits |
| content hash, chain value | a string of 64 lower-case hex digits |
| point | `[x, y]`, two finite numbers, in the project's length unit |
| angle | radians, counter-clockwise |
| properties | a string-to-string map; an absent key means unset |
| enums | snake-case words (`"by_layer"`, `{"index": 7}`); an unknown word is a read error |

- **Strict reading:** unknown record types, unknown fields in any record or value, unknown ops, unknown op
  versions and unknown words are refused, never ignored.
- **Optional fields:** most are left out while unset; a few are written as `null`. Which is which is part of the
  record types and pinned by the golden fixture.
- Numbers must be finite: NaN and ±∞ have no canonical form.

## Canonical JSON

The canonical form is RFC 8785, the JSON Canonicalization Scheme (`ecad-file/src/canonical.rs`). It is the bytes
stored in a transaction row and the input to the chain and definition hashes.

1. **Objects:** keys sorted by their UTF-16 code units; no whitespace anywhere.
2. **Strings:** `"` → `\"`, `\` → `\\`, U+0008 → `\b`, U+0009 → `\t`, U+000A → `\n`, U+000C → `\f`, U+000D →
   `\r`, other characters below U+0020 → `\u00xx` (lower-case hex); everything else as literal UTF-8. `/` is not
   escaped.
3. **Numbers:** every number is an IEEE 754 double, written as ECMAScript `Number.prototype.toString` writes
   it: the shortest digits that round-trip; plain notation for 10⁻⁶ ≤ |x| < 10²¹, exponent notation otherwise
   (`1e-7`, `1e+21`); `-0` as `0`; integers without a fraction (`25`, not `25.0`).
4. NaN and ±∞ have no canonical form. Neither has an integer a double cannot hold exactly (above 2^53 − 1).
5. `true`, `false` and `null` as literals. Arrays keep their order.

Examples: `0.1` → `0.1`, `1e-7` → `1e-7`, `1e21` → `1e+21`, `-0.0` → `0`, `100.0` → `100`, `12.5` → `12.5`.

## Hashes

All hashes are SHA-256 with a domain-separation prefix (`ecad-file/src/chain.rs`, `schema.rs`).

| Hash | Formula |
| --- | --- |
| Project chain seed | `SHA-256("ecad-chain/1\0" ‖ "project" ‖ "\0" ‖ project ID as 16 big-endian bytes)` |
| Branch seed `C(0)` (a branch other than `main`) | `SHA-256("ecad-chain/2 branch " ‖ project ID ‖ branch ID ‖ C_parent(fork_point))`, the IDs as big-endian bytes |
| Transaction `C(seq)` | `SHA-256("ecad-tx/1\0" ‖ canonical(transaction record))` |
| Definition content hash | `SHA-256("ecad-definition/1\0" ‖ canonical(definition record with origin cleared))` |
| Chunk | `SHA-256("ecad-chunk/1\0" ‖ uncompressed body)` |
| Asset | `SHA-256(bytes)` |
| `main`'s branch ID | first 16 bytes of `SHA-256("ecad-branch/main\0" ‖ project ID as 16 big-endian bytes)` |

### The chain

```
prev(1)    = hex(seed)            (main: the project seed; another branch: its branch seed)
prev(k+1)  = hex(C(k))
C(k)       = SHA-256("ecad-tx/1\0" ‖ canonical(record k))
```

- `canonical(record)` is the canonical form of the **whole record as stored**, including its own `prev`.
- The seed depends only on the project ID (and, for a branch, its ID and its fork point): not on the codec, the
  writer or the time.
- **What the chain detects:** a changed, missing, duplicated or reordered transaction within the history; a record
  from another project; two copies whose histories diverge after a shared prefix.
- **What it does not do:** it is unkeyed, so it does not resist deliberate rewriting; and it cannot detect
  truncation at a record boundary, because a valid prefix of a history is itself a valid history. The branch
  head check covers the latter inside a file.

### Definition content hash

An embedded symbol definition from a library records the hash of its content in its `origin`. The hash is over the
definition record with `origin` cleared, because the origin records the hash itself. A content hash is computed,
never trusted: on reading, `EmbedDefinition` and `RemoveDefinition` with an origin are refused when the recorded
hash does not match the content (`ContentHash`).

## Limits

A reader refuses these before allocating or looping for them.

| Value | Limit |
| --- | --- |
| A string in a record | 1 MiB (`MAX_STRING`) |
| Elements in one array (ops per transaction, vertices per polyline…) | 1,000,000 (`MAX_ARRAY`) |
| Entries in one object (properties per object…) | 100,000 (`MAX_OBJECT`) |
| Nesting of arrays and objects | 32 deep (`MAX_DEPTH`) |
| `seq`, `time_ms` and integer op fields | at most 2^53 − 1 |
| A decompressed body | 64 MiB |
| `meta.cache.every` | 16 to 1,000,000 transactions (a writer refuses others too) |
| A version name | 4,096 bytes |
| An asset's media type | 255 bytes |
| Buckets per model kind | 2^20 |
| A snapshot's recorded stacks | at most `stack_cap` entries each; entries below only with a cap; no more entries in all than transactions up to the snapshot |
| A sequence number | arithmetic on it is checked: nothing follows a row at 2^64 − 1 |

## Opening

Open reads only what it needs:

1. `meta`, checked: `format` must be `ecad-project`, `container_version` 2, `codec` `json/1`, and a
   `schema_version` must be present. A failure here refuses the file. A record version other than this build's
   opens read-only, or is refused when replay does not reach the head.
2. The active branch's `branches` row and its head transaction (it must be the last row, with the head's chain
   value).
3. The branch's latest snapshot at or before its head: the whole chunk set, each chunk checked against its hash.
4. The transactions after the snapshot, each checked against its chain value and replayed with every check.

It never reads other branches (beyond the ancestry it needs), older versions, transactions before the snapshot,
or assets. Older transactions are read on demand: when undo or redo reaches past the snapshot (the tops of both
stacks are read at once, so labels never wait), or when an earlier state is asked for (the nearest snapshot, then
the rows after it). Open time therefore follows the active branch's tail, not the file's size, the number of
branches or the history's length.

- **A branch with no snapshot of its own** opens from its parent's state at the fork point (the nearest snapshot
  at or before it, plus the rows after, up the ancestry), with empty undo and redo stacks: undo is per branch. A
  branch's first save writes an automatic snapshot.
- **Switching branch** saves the shown branch first, loads the other branch as opening does, and writes
  `meta.active`. A read-only file switches and writes nothing; with unsaved changes it does not switch.
- **The invariant check of a loaded snapshot** runs in debug builds and after a crash repair, not on every release
  open. Chunk hashes are checked on every open.
- **After a crash**, redb's quick-repair is on for every write transaction, so reopening is fast.

## Saving

**Save** writes, in **one redb write transaction** committed before Save reports success:

- the pending transactions of the active branch;
- for each `NameVersion` in them, a `versions` row with its snapshot (the row's kind is the kind of the point the
  op names; a revision's row carries the revision record as `release`), built at the version's point by undoing
  the later transactions on the live document;
- the removal of any version an `UnnameVersion` names;
- the merge index row of a merge (and its removal with the save of its undo);
- an automatic snapshot when the head is at least `cache.every` transactions past the latest snapshot;
- the branch head, `meta.writer`, and `meta.cache` when it changed.

After a crash, all of it is in the file or none. A save writes only new rows, chunks and assets not already
stored; it never rewrites stored rows or chunks, never reads the history back to check it, and never compacts.
Its cost follows what is written, not the file's size.

Before writing, the disk binding checks that the file at the path is still the one opened (its identity: device
and inode on Unix, volume serial number and file index on Windows). On Windows it reads the identity through a
handle opened for attributes only, so the malware scanner does not re-read the whole file on every save.

**Save As** (and a project's first save) writes a temporary file beside the target: a copy of the current file's
rows, then the pending transactions. It compacts it (`Database::compact()`, which returns free pages and leaves out
garbage chunks), renames it over the target and flushes the folder; the previous file's lock is released only
after that. Nothing compacts on save or on close.

### Merges

- A merge into a branch is a merge transaction on the target (above) plus one `versions` row of kind `merge`.
  The History graph and the merge's common ancestor are read from these rows, never from the log.
- A merge saved with **Close branch** sets the source's `status` to `merged` and `merged` to `{into, seq}` in the
  same write transaction; the save of its undo sets it back to `open`. A merge with nothing to merge may close the
  source at once, outside Save, as other branch actions do.

## Locking

One writer at a time. Open takes the writer lock, `<file>.lock` beside the project file, and redb's own lock.
If another session holds the lock, the file opens read-only and reads the last committed state; where the system
allows (not on Windows), the holder is named. A lock left by a session that crashed is reclaimed and reported.
The redb handle runs in single-writer mode.

## Damage and Verify

redb's crash safety does not replace ECAD's checks: every transaction is checked against its chain value on
replay, every chunk against its hash on load, every head against its log.

**Damage found while opening** (a failed hash, chain or replay check, a head that is not the last row, a missing
chunk) opens the project **read-only at the last state that verifies**, with a report; it is not refused. The file
is kept as it is. **Save As** rescues what verifies: the copy leaves out rows after the verified head and points
the cache at the snapshot that loaded.

**Damage below the records** (a damaged redb page) can make redb panic on an ordinary read; redb does not promise
to survive damaged input. Every call into redb runs under a guard (`ecad_store::guard`) that catches the panic,
reports it as `StoreError::Halted` ("the file is damaged and cannot be read: the storage engine stopped at a damaged
page (redb: …, at …)") and halts that handle: nothing more is read or written through it. While opening, this
**refuses the file**: nothing more can be read to open it at an earlier state. Opened read-only, the file is kept
exactly as it is; a writable open may let redb update its own header, as it does for every file. After open (an
earlier state, undo past the snapshot, a branch switch, Verify), the read fails with that error and the session goes
on: what is shown stays, edits work in memory, and nothing more is read from or written to that file, so Save fails
and Save As cannot copy it either.

**Verify** checks a whole file, on request only (never as part of open):

1. **Chain:** every row of every branch decoded and checked against the one before, each head against its last
   row, each version's and merge row's chain value, and every chunk a version names present and matching its
   hash.
2. **Replay** of each branch, parents before children, from the root or from the parent's state at the fork,
   with the commit-time check per transaction, so a broken invariant names the transaction that broke it.
3. **Full check** at each stored snapshot point and at each branch's end, comparing the replayed state with the
   stored snapshot, its tag numbers and its undo stacks.

## Tests that guard the format

- **Golden file** (`ecad-file/tests/golden/`): the record version and the hash of the golden project's last
  transaction are pinned together.
- **Crash tests**, in the slow set: power loss (a storage backend, `ecad_store::power`, that lands any subset of
  unsynced writes in any order, each possibly torn), torn writes, a process killed during saves, versions and
  assets, and a kill during Save As and first saves. Every acknowledged save must reopen as saved.
  `ECAD_CRASH_ROUNDS` sets the number of rounds.
- **Hostile input:** limits and damage at rest (stored values corrupted in the file image) open read-only or are
  refused, never crash.
- **Damaged pages** (`ecad-file/tests/all/container/damaged_pages.rs`): a corpus of damaged copies of the golden
  file, one per redb panic site seen (`ecad-file/tests/damaged/redb-panics.txt`, kept as byte patches to
  `tests/damaged/base.ecad`), is refused
  in words; a panic in a read after open leaves the session going. In the slow set, seeded damage (flipped bits,
  random blocks, zeroed 4 KiB, truncation) to the golden file is opened, read back and verified without a panic
  escaping; `ECAD_DAMAGE_ROUNDS` sets the rounds.

## Not in the format

- Library files (`ecad-library` and library chain seeds exist in code; no library file is written).
- Checkpoint records (the type is reserved and refused).
- Another codec (`bin/1` is reserved in `meta.codec`).
- History compaction (dropping history) does not exist; engine compaction on Save As drops none.
- Reading a file from bytes in memory (`Store::from_bytes`) exists for tests; ECAD is desktop-only.
