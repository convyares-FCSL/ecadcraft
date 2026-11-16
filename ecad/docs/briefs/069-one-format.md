# Brief 069: one file format

Status: Approved (owner, 2026-11-15: pre-v1, ECAD keeps one `.ecad` format, the redb container).
Branch: `feature/one-format`, from `refactor/0.2.3` at `522acc28`.
Designer: no · Tester: no · Size: L.

## The brief

The survey is `docs/design/crates.md` §3.2 (what goes, what stays, the knock-ons, and the five items
beyond them). Pre-v1 there is no old-format compatibility: the JSON Lines version 1 format and
`ecad upgrade` go, `v2` stops being a module name, and redb storage moves into ecad-store.

Steps, one commit each (`069 step N: …`), pushed after each green step:

1. **Remove the v1 format and `ecad upgrade`** as §3.2 lists. Keep the record types, canonical JSON,
   `definition_hash` (pinned by a known-hash test: existing hashes do not change), chain, crc,
   `ReadOnly` / `JUST_ENDED` / `holder_record` / `utc`, under neutral names (`records`, …).
   `LinkV1` stays as part of the version 2 format.
   - **Test store (owner call):** the v1 in-memory store (`Persistence::TestStore`, `MemoryStore`
     over `ProjectFile`) becomes an in-memory redb store, so tests use the real format.
     `Project::create`, `open_from` and `open_report` keep working for tests.
   - **Debugger:** the file snapshot shows the redb file's equivalents (version, verify and open
     report); rows with no equivalent go.
   - **dev/ecad-inspector:** `export-jsonl` (CLI, protocol call, HTML button) and the v1 sniff /
     `ecad upgrade` offer go; its fixtures move to redb.
   - **Golden fixture:** `tests/golden/project-v1.ecad` is replaced by a golden redb project made by
     our own build; `tester_024_probe` and `lane_057_tester` move onto it.
   - **ecad-bench:** the generator writes the redb format, and the timings are measured on it.
2. **No `v2` folder.** `ecad-file/src/v2/*` moves up into `src/`; the version is the number in the
   file.
3. **redb into ecad-store** as generic storage (open, transactions, handle). ecad-file keeps the
   format (tables, schema, records, verify, pack); ecad-store learns no `.ecad` record types. The
   `lib.rs` docs and Cargo `description` of both crates say what is true.
4. **Docs:** `docs/quick-start.md` loses §7B, the export-jsonl steps and the "version 1" bench and
   limitation lines; `docs/spec/eep-0002.md` is marked history (superseded by EEP-0003); EEP-0003
   gets a one-line note where it relies on v1's canonical JSON (kept) or migration (removed).

## Stops

Anything beyond §3.2 that depends on v1, any change to the redb format itself, or a new op: stop and
report.

## Checks per step

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D
warnings`, `cargo xtask layers`, `cargo xtask test-changed`. Never the whole workspace or the slow
set. When green, the overseer merges `feature/one-format` into `refactor/0.2.3`.
