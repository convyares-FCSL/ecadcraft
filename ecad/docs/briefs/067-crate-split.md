# Brief 067: the crate split

Status: Approved (owner, 2026-11-15: design 065, `docs/design/crates.md`).
Branch: `feature/crates`, from `refactor/0.2.3` at `9a27bd5a`.
Designer: no · Tester: yes (step 1c, and steps 4 and 6) · Size: L.

## The brief

The approved design `docs/design/crates.md` is the brief. Follow its §5 step order; §1–§4 and §6
give the details. Behaviour does not change; test changes are limited to paths. If a cut does not
work as designed, or anything would change the `.ecad` format or add an op, stop and report.

## Overrides to §5 (owner process for 0.2.3, 2026-11-15)

- **Precondition.** Lanes 060–063 are merged into `refactor/0.2.3`, not `dev`. There are no PRs.
  When the lane is green, the overseer merges `feature/crates` into `refactor/0.2.3`. If that merge
  conflicts, the worker merges `origin/refactor/0.2.3` into `feature/crates` and resolves it there.
- **Step 10, CI.** `.github/workflows/*` is not edited (house rule 11). CI's ECAD lint job runs
  `cargo xtask lint`, so removing the wasm step from `xtask lint`/`ci` and removing
  `cargo xtask wasm` is enough. The stale `targets: wasm32-unknown-unknown` line and the step name
  in `ci.yml` are a separate CI task.
- **Step 11** (drop the re-exports) is skipped; it is left for the phase-end tidy-up.
- **Commits.** One commit per step, `067 step N: …`, so `git bisect` lands on a step. The branch is
  pushed after each green step.
- **Tester.** After step 1c the worker reports, and the tester covers 1c while steps 2–6 go on.
  The worker reports again after step 6 and at the end.

## Checks per step

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D
warnings`, `cargo xtask layers` and `cargo xtask test-changed`. Never the whole workspace or the
slow set.

## Step 5: closure check (read-only, run 2026-11-15 at c8f8bafb)

The E set (design 065 §2.1 "E" rows and the E halves of the split modules) is:
accessories, bom, cables, checks, connectors, continuations, defaults, designations, device_list,
devices, frames, front, generated, kinds, link_lines, locations, outputs, page_types, plc,
proposals, rating, references, reps, revisions, slash, strips, styles, symbols, terminal_types,
text_style, title_block, wires, words, xref. Every other module of ecad-cadcraft is C.

Two checks, both passed with nothing to report:

1. **Grep.** In the E set's files, outside lines `pub use crate::draw::…;` (the C halves at their
   old paths until step 6's facades), no line names `cadcraft_engine`, `cadcraft_io` or
   `crate::<C module>`. The only hits were doc comments that point at C modules; step 6 turns their
   intra-doc links into plain text.

   ```sh
   cd crates/ecad-cadcraft/src
   E="accessories checks connectors defaults designations link_lines outputs proposals rating reps revisions slash strips styles symbols terminal_types words cables kinds text_style frames bom continuations device_list devices front generated locations page_types plc references title_block wires xref"
   files=$(for m in $E; do [ -f $m.rs ] && echo $m.rs; [ -d $m ] && find $m -name '*.rs'; done)
   C=$( (ls *.rs | sed 's/\.rs$//'; ls -d */ | tr -d /) | while read c; do echo " $E " | grep -q " $c " || echo $c; done | grep -v '^lib$' | paste -sd'|')
   grep -nE "cadcraft_(engine|io)|crate::($C)\b" $files | grep -v ':pub use crate::draw::' | grep -v '//[!/]'
   ```

2. **Build.** The E set copied alone into a scratch crate (dependencies ecad-model,
   ecad-drawing-style, serde, serde_json, chrono; the `pub use crate::draw::` lines dropped) builds
   with no error.
