# Brief 006: Stage 3, libraries (`.ecadlib`): contract first

Status: Draft (overseer, 2026-10-31)
Branch: `ecad/stage3-design`
Designer: no for this brief (yes later, for the library UI) · Size: S (design); the build that
follows is L

## Goal

Settle the stage 3 contract so that it can be approved, before any library code is written. Stage 3
is not approved (`ecad/CLAUDE.md`, `ecad/docs/deferred-work.md`). This brief only produces the
proposal update for the owner.

## Inputs

- Proposal §5.2 (LIB-01 to LIB-04), §5.3, §20.3 (inline embedded definitions, E2), §20.9 and §22.
- The foundations note: "Library contract" in its open decisions.
- The concept spec's "Symbols, parts, search and transfer", read as intent only (review D5-D9).
- `ecad-file`'s current refusal of library files (`crates/ecad-file/src/read.rs:219-221`) and the
  `origin { library_id, definition_id, library_seq, content_hash }` of §20.13.

## Scope

1. A revision of proposal Part E (§20.9 and §22, stage 3) that states:
   - what a `.ecadlib` holds and promises;
   - how a definition gets from a library into a project (an embedded copy with origin, E2);
   - "update available" detection;
   - what happens when a library is missing (P8: nothing);
   - stage 3's acceptance criteria, in the style of stages 1 and 2a.
2. The open decisions, each listed for the owner with options and a recommendation, not settled:
   - parts in the same container or separately;
   - terminal mapping on replacement;
   - same-name conflicts;
   - harvest from a project.
3. A split of the build into briefs, for example format and read-only open first, then embedding,
   then UI.

**Out of scope:** any Rust change; any change to the `.ecad` format.

## Acceptance criteria

1. The proposal revision is reviewable on its own and changes no approved Part A principle.
2. Every open choice is listed with a recommendation and has a decision id (continuing the E
   numbers).
3. The PR touches only `docs/document-architecture-proposal.md` and this brief. It is outside `ecad/`,
   so it is an owner gate.
