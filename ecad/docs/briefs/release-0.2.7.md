# Release 0.2.7: the format release

Status: Draft, not started
Release ladder: [0.2.4 tidy-up](release-0.2.4.md) → [0.2.5 features](release-0.2.5.md) →
[0.2.6 UI](release-0.2.6.md) → 0.2.7 format release → 0.3.0, the functional core done well: good structure,
a good build checked against the reference drawing pack, and the solid foundation `.ecadlib` builds on
([roadmap](../roadmap.md)).

## Goal

Make the `.ecad` format ready to freeze at 0.3.0: the codec and compression, migration, snapshots decoupled
from the model, versioned built-in defaults, field-level ops and one written statement of what "frozen"
covers. The codec waits until here because the model may still move in 0.2.4 and 0.2.5.

## Decided

- **The container stays redb.** `.ecad` is ECAD's own structure built on redb; redb is pinned (`=4.5.0`) and
  ships upgrade paths, so a pinned version is a known format. A file of ECAD's own or SQLite (A2) are not
  pursued. The crash on damaged redb pages (B1) is fixed by 0.2.4's panic guard; this release adds the
  salvage test: a file with damaged redb pages opens read-only at what verifies, or is refused, and never
  crashes.
- **The CADCraft seam (A3).** From 0.2.5, new electrical interactions are ECAD tools that emit ops directly;
  existing tools are not rewritten. A later architecture review asks whether to build on the core drawing
  library directly instead of CADCraft, if CADCraft grows 3D or rendering weight: ECAD stays a focused 2D
  app, and any 3D comes through a SolidCraft plugin with drawings coming back here.
- **Architecture items placed elsewhere:** A4 (where command declarations live) in 0.2.4's typed-commands
  phase; splitting merge and connectivity out of ecad-model at 0.3.0 if build times justify it; A5 (the agent
  tool set) in 0.2.5, its allowlist in 0.2.4 (B9, B11).

## 1. Measure, then decide the codec

Today ([reference/file-format.md](../reference/file-format.md)): one redb file; transactions are canonical
JSON (RFC 8785, codec `json/1`), snapshots are serde JSON chunks, both Zstandard level 1 when that is
smaller; hashes and chain values are over the uncompressed canonical JSON; `bin/1` is reserved, not built.
At the 0.2.3 end gate (release build) the large project churned to 128.5 MB for 170,000 transactions, ten
times that history to 1 GB (690 MB compacted); opening took 200–240 ms (the latest snapshot plus the tail, so
open time does not grow with history).

1. A bench column first: for medium and large, split file size and open time into JSON encode and decode,
   Zstandard, redb and the rest; the same for saving one transaction. Also try Zstandard at higher levels and
   with a trained dictionary on the same data (the cheapest change, no new codec).
2. Decide with the numbers: keep `json/1` (perhaps with a better Zstandard setting), or build `bin/1`.
   `bin/1` needs its own canonical form for hashing, a spec section, a codec flag on open, and loses files
   readable by eye; weigh that against size on long histories. The case is file size, not open speed. The
   binary-encoding proposal is the seed: [eep-0003-binary-encoding.md](eep-0003-binary-encoding.md).
3. Whatever is chosen is settled before the freeze. Before it, older files need not open.

## 2. Freeze readiness

The ledger model is right; the format is not freezable yet. These are the entry criteria for the freeze at
0.3.0, all in this release:

- **B3 Migration.** Every record-version bump strands existing files today. An upcast layer (a version `v` on
  each record and each op), a golden corpus of every released version, each of which opens fully and
  verifies after upcasting; the chain is checked on the stored bytes before upcasting (L).
- **B4 Snapshot chunks.** Chunks are serde's form of the live model types, so the model's shape is the file
  format; some model types lack `deny_unknown_fields`, and 314 fields carry `serde(default)`. Either chunk
  record types of their own, or chunks declared a rebuildable cache with a `chunk_format` number (cheaper,
  and fits "derived is never read back as fact") (M–L).
- **B5 The loaded-snapshot check.** Release builds skip the invariant check of a loaded snapshot. Measure it
  on large, then run it always or in the background (M).
- **B6 Versioned built-in defaults,** recorded in the project, and unparsable stored values shown as Checks
  rows rather than silently falling back: [versioned-defaults.md](versioned-defaults.md) (M).
- **A1 Field-level modify ops** (`SetDeviceField`) as the stored form for typed fields. Branches and merge are
  frozen deliberately, or marked experimental in the format.
- **Forward compatibility of stored words:** decided for v1
  ([forward-compatibility.md](forward-compatibility.md)).
- **The salvage test** for damaged redb pages (above).
- **One written statement of what "frozen" covers:** records and ops, chunks, meta, and the `.ecadlib` seams.
