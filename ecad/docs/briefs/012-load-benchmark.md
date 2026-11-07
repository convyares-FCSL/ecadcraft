# Brief 012: Project load benchmark, run on demand

Status: Approved (owner, 2026-11-07: "lets build the CI check but lets make it a benchmark test we
can invoke at will, do not want to call it all the time")
Branch: `ecad/load-benchmark`
Designer: no (no UI) · Size: S-M
Depends on: nothing (branched from `dev` at `60c7c35`)

## Goal

Know, with numbers, how opening and saving a `.ecad` project scales as projects grow, so that
checkpoints (persistence stage 4) and any later change to the file's layout are decided from
measurements, not guesses. Today a project opens by replaying its whole transaction log.

## Scope

1. **One command, run at will:** `cargo xtask bench` (from `ecad/`), release build. It is **not**
   part of `cargo xtask ci`, plain `cargo test --workspace`, or the GitHub CI jobs, and adds no time
   to them. `cargo xtask` usage text lists it.
2. **Synthetic projects,** made by a deterministic generator (fixed seed) through the same ops a
   person's commands record (existing ops only; no format change, no new op). Each project has
   pages, components with terminals, wires attached at both ends, cables with core labels, and
   cables continued across pages with references (as `cablecontinue` records them). Three tiers, so
   the trend is visible:

   | Tier | Pages | Components | Wires | Cables |
   | --- | --- | --- | --- | --- |
   | small | 10 | 100 | 500 | 50 |
   | medium | 100 | 1 000 | 5 000 | 500 |
   | large | 500 | 5 000 | 25 000 | 2 500 |

   Each tier is built twice: **lean** (one transaction per object) and **churned** (the same final
   project after realistic editing history: moves, renames, undo and redo, so the log is roughly
   5× longer). The difference shows what history alone costs at open.
   `--tier small|medium|large|all` (default `small,medium`) and `--keep <folder>` (keep the generated
   files to open in the trial) are the only options needed.
3. **Measured per project** (generation itself is not timed), each the median of 5 runs:
   - file size on disk and transaction count;
   - **open**: read from disk, verify records, replay to the document (`ecad-file`, no CADCraft);
   - **derive**: nets and cable references on the opened document (`ecad-model::derive`);
   - **first page**: open the project in a headless CADCraft session and project page 1
     (`ecad-cadcraft`), what a person waits for before seeing a drawing;
   - **save one change**: append one small transaction to the open project and flush;
   - peak memory if it can be read cheaply on Windows and Linux; otherwise left out, not estimated.
4. **Output:** a plain table on stdout, and the same results as JSON in
   `target/bench/load-<date>-<short commit>.json` (with machine, OS, commit and build profile), so
   two runs can be compared by hand. A `--compare <earlier.json>` that prints the change per row is
   welcome if it stays small.
5. **No pass/fail budgets yet.** The first run on the owner's machine sets the baseline; budgets are
   decided after that, with the stage 4 discussion.
6. Where it lives: the generator and timings in a crate or bench target inside `ecad/` that the
   pure crates do not depend on (`ecad-model`, `ecad-store` and `ecad-file` still never depend on
   `cadcraft-*`; the first-page timing is the only part that uses `ecad-cadcraft`).

Out of scope: checkpoints, compaction, any change to the file format or to how projects open;
GUI timing (window start, rendering); criterion or other new heavy dependencies unless the
reviewer agrees they are worth it.

## Acceptance criteria

1. `cargo xtask bench --tier small` runs on Windows and Linux, prints the table and writes the JSON.
2. `cargo xtask ci` and `cargo test --workspace` do not run the benchmark (their test lists and
   times are unchanged apart from the generator's own small unit tests, if any).
3. The generator is deterministic: the same tier and seed give byte-identical files.
4. A generated project opens in the trial (`--keep`), shows its pages, and its nets and references
   match what the generator intended (checked by a small fast test on the small tier's shape, or a
   cut-down tier, that does run in CI).
5. The churned and lean versions of a tier replay to the same document.
6. `docs/quick-start.md` gets a short "Measuring load time" note: the command, what each column
   means, and that results depend on the machine.

## Needs owner check

- Run `cargo xtask bench` once (all tiers) on your machine to set the baseline.
