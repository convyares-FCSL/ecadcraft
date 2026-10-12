# Upstream sync policy (proposal)

> **Status:** proposal for review. Nothing has been deleted, moved or configured. **Written:**
> 2026-10-12 against `dev` at `baee1b8`, which incorporates CADCraft `00c7613`.

## Summary

- Every path outside `ecad/` falls into exactly one of three categories, listed in one machine-read
  file, `ecad/upstream-policy.toml`:
  - **ECAD-owned:** our version is kept during upstream integration;
  - **excluded upstream:** intentionally absent; the removal is reapplied at every sync;
  - **upstream-maintained:** merged normally. This is the default.
- **No `.gitattributes` merge drivers** and no global git configuration. The policy is applied
  only on a dedicated `upstream-sync/<date>` branch, by an explicit script, before pushing. Feature
  branches merge normally.
- CI checks the result on every PR (`cargo xtask policy` in `ecad/`):
  - excluded paths are absent;
  - owned entry points exist;
  - every workflow is classified;
  - licence and attribution files are intact;
  - every difference from the incorporated upstream commit is accounted for.
- Proposed exclusions:
  - CADCraft's AutoCAD-parity planning documents;
  - its README screenshots;
  - its packaging;
  - its release and packaging-lint workflows.
- Kept, because the build, tests or attribution depend on them:
  - `assets/` (the app icon is compiled into the app);
  - `contributors/` (read at build time for the About window credits);
  - `docs/brand/` (named by `NOTICE`);
  - the licence files.

## Why not `merge=ours`

A `merge=ours` attribute applies to every merge, including merges between our own branches, so
legitimate changes on one ECAD branch can be silently dropped when merging another. It also needs
a custom driver configured in each clone, and CI won't have it. It does nothing for files upstream
*adds*, and nothing for files we want absent. The rules below are applied once, on purpose, on a
sync branch, and then checked.

## The three categories

### ECAD-owned: keep ours during upstream integration

| Path | Notes |
| --- | --- |
| `README.md` | repository front page |
| `AGENTS.md`, `CLAUDE.md` | agent instructions for this repository (rewrite pending; see [Concerns](#dependency-and-licence-concerns), item 5) |
| `.github/workflows/ci.yml` | our CI; if upstream later adds a file of the same name, ours stays and theirs is shown in the sync summary |
| `ecad/**` | all ECAD code and docs (upstream has no such path) |
| `docs/concept-spec.md`, `docs/decisions.md`, `docs/cadcraft-assessment.md`, `docs/document-architecture-proposal.md`, `docs/ecad-architecture-foundations.md` | **transitional**: these move to `ecad/docs/` when the policy is applied, and the entries are then dropped |

Upstream edits to an owned path are not merged. They are listed in the sync summary (as a diff
against the previous upstream commit), so useful changes, such as a new rule in upstream's
`CLAUDE.md`, can be ported by hand.

### Excluded upstream: intentionally absent; removal reapplied at each sync

| Path | Why exclude | Checked references |
| --- | --- | --- |
| `ROADMAP.md` | CADCraft's AutoCAD-parity roadmap; ECAD's roadmap is in the concept spec | text only: upstream `CLAUDE.md`/`AGENTS.md`, other excluded docs |
| `docs/roadmap.md`, `docs/gaps.md`, `docs/target-app-parity.md`, `docs/parity-checklist.md`, `docs/geometry-parity.md`, `docs/ui-parity.md`, `docs/file-format-parity.md`, `docs/hardware-parity.md`, `docs/localization-parity.md` | AutoCAD-parity tracking for CADCraft's own milestones | code **comments** only (`engine/tests/audit.rs`, `autosave.rs` name `docs/gaps.md`); `cargo xtask parity` *writes* `docs/parity-checklist.md` when run by hand (not in CI) |
| `docs/releasing.md` | CADCraft's release procedure (ArtCraft shared workflows and secrets) | none |
| `docs/images/**` | screenshots for CADCraft's README (1.1 MB) | none in code; `ATTRIBUTION.md` rows remain and are harmless; `cargo xtask assets` only checks files that exist |
| `packaging/**` | CADCraft/ArtCraft packaging identities (`ai.storyteller.cadcraft`, signing scripts) | used only by the two excluded workflows; `xtask ico` mentions `packaging/icons.sh` in its help text but writes into `assets/` |
| `.github/workflows/release.yml` | calls `storytold/craft-common-actions` with ArtCraft signing secrets; runs on pushes to `release` or by hand | none |
| `.github/workflows/packaging-lint.yml` | runs a reusable workflow from `storytold/craft-common-actions@main` (unpinned third-party code) on every PR that touches workflows; it already ran on our PRs | none |

ECAD packaging and release workflows will be written under `ecad/` and `.github/` as ECAD-owned
files when the product is named.

### Upstream-maintained: merge normally (the default)

| Path | Why keep |
| --- | --- |
| `crates/**`, `apps/**`, `xtask/**` | the platform; code patches are listed in `ecad/UPSTREAM-PATCHES.md` and marked `ECAD-SEAM(owner)` |
| `Cargo.toml`, `Cargo.lock`, `.cargo/**`, `.gitignore` | the upstream workspace; unchanged so far |
| `rustfmt.toml` (150 columns), `clippy.toml` | **upstream formatting for upstream code**; `ecad/rustfmt.toml` (120 columns) governs `ecad/` because rustfmt uses the nearest file, and each workspace's `cargo fmt` formats only its own members (verified: both `cargo fmt --check` runs are clean) |
| `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, `ATTRIBUTION.md` | licence and attribution notices: always preserved, never owned or excluded |
| `assets/**` | `apps/cadcraft` compiles in `assets/app-icon/*.png` (`include_bytes!`, `main.rs:179-181`); `build.rs` and `xtask ico` use `cadcraft.ico` |
| `contributors/**` | read by `crates/ui-egui/build.rs` for the About window's contributor credits. Without it the build still passes, with a warning, but CADCraft's contributors lose their credit |
| `docs/brand/**` | ArtCraft trademarks named by `NOTICE` and `ATTRIBUTION.md`; see concern 1 |
| `docs/architecture.md`, `docs/control-protocol.md`, `docs/mcp.md`, `docs/plot-styles.md`, `docs/plotters.md`, `docs/localization.md`, `docs/contributors.md` | technical reference for code we use, named in code comments (`plotstyle.rs`, `plotter.rs`, `cmd/plotters.rs`) and in each other |
| `examples/apartment.dxf` | **your choice**: an attributed 52 KB sample drawing, not referenced by the build. I recommend keeping it as a DXF import fixture for adopt (Phase 4); excluding it is equally safe |

Anything not listed is upstream-maintained.

**New upstream paths need classifying.** A sync that brings a new top-level path, or any new file
under `.github/workflows/`, stops until the path is classified in the policy. New workflows are
proposed as excluded until reviewed.

## The policy file (draft, not yet created)

```toml
# ecad/upstream-policy.toml: read by `cargo xtask policy` (CI) and `cargo xtask sync-upstream`.
# Paths are repository-relative; a trailing "/**" means the whole directory.

[owned]          # ours kept on upstream integration; upstream edits reported, not merged
paths = [
  "README.md", "AGENTS.md", "CLAUDE.md", ".github/workflows/ci.yml", "ecad/**",
]
required = ["README.md", "AGENTS.md", "CLAUDE.md", ".github/workflows/ci.yml", "ecad/Cargo.toml"]

[excluded]       # must not exist; removed again at every sync; upstream edits reported
paths = [
  "ROADMAP.md", "docs/roadmap.md", "docs/gaps.md", "docs/target-app-parity.md",
  "docs/parity-checklist.md", "docs/geometry-parity.md", "docs/ui-parity.md",
  "docs/file-format-parity.md", "docs/hardware-parity.md", "docs/localization-parity.md",
  "docs/releasing.md", "docs/images/**", "packaging/**",
  ".github/workflows/release.yml", ".github/workflows/packaging-lint.yml",
]

[protected]      # may never be owned or excluded: build inputs, licences, attribution
paths = [
  "crates/**", "apps/**", "xtask/**", "Cargo.toml", "Cargo.lock", ".cargo/**",
  "rustfmt.toml", "clippy.toml", "assets/**", "contributors/**", "docs/brand/**",
  "LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "ATTRIBUTION.md",
]
notice_must_contain = ["Copyright (c) 2026 ArtCraft Team and the CADCraft contributors."]

[workflows]      # every file in .github/workflows must be listed in owned or upstream_maintained
upstream_maintained = []
```

## Sync process (`cargo xtask sync-upstream`, run from `ecad/`)

It uses plain git only. Every step prints what it did, and the run stops on anything it cannot
decide.

1. Require a clean tree. Fetch `https://github.com/storytold/cadcraft` `main`. Read the previous
   upstream commit from `ecad/UPSTREAM.md`.
2. Create `upstream-sync/<date>` from `origin/dev`. Nothing is done on `dev` or on feature
   branches.
3. `git merge --no-ff --no-commit upstream/main`: an ordinary merge.
4. **Owned paths:** restore our version (`git checkout HEAD -- <path>`, or remove a file upstream
   added at an owned path). Record upstream's diff to that path since the previous upstream
   commit.
5. **Excluded paths:** `git rm -r --ignore-unmatch -- <path>` for anything the merge brought back.
   Record what upstream changed there.
6. **Classification gate:** a new top-level path or a new workflow file stops the run, listing
   them. The person running the sync classifies them in the policy (a separate, reviewed commit
   on the same branch) and reruns.
7. **Upstream-maintained conflicts:** stop for manual resolution. They are expected only in the
   `ECAD-SEAM` files. Resolve keeping the seams, then continue (`--continue`).
8. Update `ecad/UPSTREAM.md` (`Commit:` line) and commit the merge, with the summary in the
   message.
9. Run every gate: the root workspace (`cargo fmt --check`, clippy `-D warnings`, tests,
   `cargo xtask layers|assets|wasm`), then `ecad/` (`cargo xtask ci`, which includes `policy`).
10. Write `target/upstream-sync-summary.md`:
    - the commit range and count;
    - files by category;
    - upstream edits to owned paths (to port by hand) and to excluded paths (dropped);
    - conflicts resolved in `ECAD-SEAM` files;
    - new and changed workflows, with their `uses:` references and triggers;
    - `Cargo.lock` package additions, removals and version changes, flagging new licences for
      review.
11. Push the branch and open a PR into `dev` with the summary as its description. It merges only
    with your approval.

Normal ECAD branches never run steps 4-5, and nothing in `.gitattributes` or git configuration
changes their merges.

## CI check (`cargo xtask policy`, part of `cargo xtask ci`)

1. No excluded path exists.
2. Every required owned path exists.
3. Every file in `.github/workflows/` is listed as owned or upstream-maintained.
4. `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE` and `ATTRIBUTION.md` exist, and `NOTICE` still
   contains upstream's copyright line.
5. No path is in two categories, and no owned or excluded path matches a protected one.
6. Every path that differs from the incorporated upstream commit outside `ecad/` is owned, is an
   excluded removal, or is listed in `ecad/UPSTREAM-PATCHES.md`. This extends the existing patch
   guard.
7. The upstream gates that read these paths stay green: `cargo xtask assets` in the root
   workspace and the CADCraft app build.

## Dependency and licence concerns

1. **`docs/brand/` and `NOTICE`.** `NOTICE` (upstream's attribution) names `docs/brand/` and its
   licence. Excluding the brand files would leave `NOTICE` pointing at missing files, and editing
   upstream's `NOTICE` touches an attribution notice. Proposal: keep `docs/brand/` until the
   product is named. Then make `NOTICE` ECAD-owned, keep upstream's copyright and third-party
   text verbatim, and add an ECAD section saying the ArtCraft marks are not distributed.
2. **`contributors/`.** Removing it would not break the build, but it would remove CADCraft's
   contributors from the About window: a loss of credit. Keep it.
3. **`assets/app-icon/`.** Required by the CADCraft app build. The ECAD app will have its own icon
   under `ecad/`.
4. **Dangling references** after exclusion are confined to code comments (`docs/gaps.md` in two
   engine test headers) and to upstream's `xtask parity` command. If someone runs
   `cargo xtask parity`, it recreates `docs/parity-checklist.md`, and the CI policy check will
   flag the file.
5. **Agent instructions.** Upstream's `CLAUDE.md` and `AGENTS.md` carry rules that still apply to
   the upstream code we patch: never crash, clean-room, assets and attribution. The ECAD-owned
   versions must keep those rules, and each sync summary shows upstream's later edits so new rules
   are not missed. The two files are nearly identical; I propose `AGENTS.md` as the full text and
   `CLAUDE.md` as a short pointer to it plus `ecad/CLAUDE.md`.
6. **`ATTRIBUTION.md` rows** for excluded files (`docs/images/*`, and `examples/apartment.dxf` if
   excluded) stay as written. Editing an upstream attribution file is avoided; the extra rows do
   no harm.
7. **Moving the ECAD docs** from `docs/` to `ecad/docs/` changes links in `README.md`, in PR #1's
   proposal and in PR #4's report. They are updated in the same change; open PRs that are not yet
   merged need a rebase or merge afterwards.
8. **Workflows.** Removing `packaging-lint.yml` also removes the only third-party reusable
   workflow (`storytold/craft-common-actions@main`, unpinned) from our CI. Our `ci.yml` uses only
   `actions/checkout`, `dtolnay/rust-toolchain` and `Swatinem/rust-cache`.

## Decisions needed before anything is deleted

1. Approve the three lists, or adjust them. In particular `examples/apartment.dxf` (keep or
   exclude), and keeping `docs/brand/` until naming.
2. Approve the mechanism: policy file, sync script on a dedicated branch, CI policy check, and no
   merge attributes.
3. Approve moving the ECAD documents into `ecad/docs/`, and the `AGENTS.md` (full) / `CLAUDE.md`
   (pointer) arrangement.

Once approved, the change arrives as one PR: the policy file, `xtask policy` and
`xtask sync-upstream`, the deletions, the document move, and the rewritten agent files. Its CI
proves the deletions break nothing.
