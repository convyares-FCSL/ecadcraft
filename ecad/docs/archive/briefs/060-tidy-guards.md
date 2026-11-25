Historical record (2026-11-15). Not current; see [developer/guards.md](../../developer/guards.md).

# Brief 060: Tidy-up guards (R0) and the check-kinds table (R7)

Status: Approved (overseer, 2026-11-15, for the 0.2.3 tidy-up phase, wave 1).
Branch: `chore/tidy-guards`, from `origin/dev` `445787b8` (059 merged, #116).
Designer: no · Tester: no · Size: S + S, in two parts (R0 pushed first, then R7).
Source: the code review of 2026-11 (not committed; done at `c73f820b`): "Top 10 findings" and the
two defects under it, "Never-crash rules", finding 5 ("Check kinds as one table"), finding 8 (the
`LOCAL_STYLE` part and the label-height question), and the R0 and R7 rows of "Refactor plan as
lanes". Every count below was re-checked against `445787b8`.

No new dependencies (the crate review, lane 061, decides those). No `.ecad` format change: stop and
report if one looks necessary. Persisted words (check kinds' words, property keys) do not change.

## Part 1: R0, guards and the two defects

1. **Multi-component `setprop`.** `run_setprop_many` (`crates/ecad-cadcraft/src/commands.rs`) has
   its own rule list and skips the connector (`pins`, `half.*`), PLC (rack, slot, channels,
   address), strip terminal-default (`terminal.*`), level-label (`term_label.*`) and wire rules
   that the single-object path runs; those keys fall through to free text. Write a failing test
   first, then make both paths (and a device property, design 018 §6.1) walk **one** rule
   function per component. The full `PROP_RULES` table is a later lane (R6).
2. **`ALL_OPS`** in `crates/ecad-model/tests/all/incremental_contracts.rs` covers every `Op`, and
   its coverage list is derived from `Op` itself (an exhaustive `match` with no `_`), so a new op
   cannot be left out.
3. **Never-crash attribute** `#![deny(clippy::unwrap_used, …)]` on the binary roots that lack it
   (`apps/ecad-cli`, `ecad-bench`'s `main.rs`, `dev/ecad-inspector`'s bin, `xtask`), and
   `[lints] workspace = true` in `xtask/Cargo.toml`.
4. **`xtask layers` ratchets.** Widen `LOCAL_STYLE` with `Color32::WHITE`, `GRAY`, `RED`, `YELLOW`;
   add ratcheted counts (per file, can only fall) for literal `Stroke::new(<digit>`,
   `add_space(<digit>` and `vec2(<digit>`, seeded from today's numbers. Add a ratchet on direct
   `cadcraft-engine` dependencies of `ecad-trial`, `ecad-bench` and `ecad-mcp` (owner ruling: apps
   stop depending on the engine directly; app→app dependencies are allowed).
5. ~~Wire label height~~ **Dropped** (overseer, 2026-11-15). The registry built-in is
   `ecad_model::wiring::DEFAULT_LABEL_HEIGHT` (1.6), which ecad-file v1 also uses as the serde
   default and skips writing, so changing it changes what saved files mean. `look::LABEL_HEIGHT`
   (2.5) is the cable look's layout height, not a duplicate. Whether wire labels should default to
   2.5 goes back to the owner, for a defaults-as-data lane.
6. Remove the duplicated `strippage.` doc line in `defaults/mod.rs`.
7. **Dependency rules in `xtask layers`** (owner, 2026-11-15), on ECAD's own manifests only:
   `anyhow` only in the apps (`ecad-cli`, `ecad-mcp`, `ecad-trial`, `ecad-bench`) and xtask;
   `tokio` in no ECAD crate. Each rule says what it prevents. (A rule keeping `rayon` off wasm was
   dropped the same day: ECAD is desktop-only, owner ruling; `xtask wasm` stays until the split
   lane removes it.)
8. **Core OS-access ratchet in `xtask layers`** (owner, 2026-11-15): per-file counts of direct
   file system, environment, process, network and clock access (`std::fs`/`fs::`,
   `std::env`/`env::var`, `std::process`, `std::net`, `SystemTime::now`, `Instant::now`,
   `Utc::now`/`Local::now`) in `ecad-model` and `ecad-file`, test modules aside, seeded today
   (1 and 10 lines); counts only fall. Core stays pure data and rules; disk access belongs to
   `ecad-store`, and the time is passed in. The split lane takes it to zero.

## Part 2: R7, check kinds as one table (after Part 1 is pushed)

- A `check_kinds!` macro (or a const table) generates `checks::Kind`, `Kind::ALL`, its name, words
  and severity from one row per kind.
- The UI lists (`apps/ecad-trial/src/lists/checks.rs` `count_row`, the fix lists) are derived
  from it.
- A small `CheckSource` trait, so each area emits its rows from its own module.
- A guard test that enumerates `Kind::ALL` (words unique and round-trip, every kind has a row).
- Persisted words do not change.

## Checks

fmt, clippy (all targets) and `cargo xtask test-changed` only; never the whole workspace or the
slow set. Tests in each crate's `tests/all/lanes/lane_060.rs`.
