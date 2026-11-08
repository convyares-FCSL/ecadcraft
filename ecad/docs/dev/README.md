# ECAD developer documentation

Documentation for people and agents working **on** ECAD's code. Running and trying the app is
covered by the single maintained guide, [`../quick-start.md`](../quick-start.md); this folder
does not repeat it.

The layout follows the split used by ROS and similar projects: concepts explain, how-to guides
instruct, reference describes, and specifications (EEPs) define.

| Section | Answers | Folder |
| --- | --- | --- |
| [Concepts](#concepts) | *Why is it built this way?* Explanations of the model and the main design ideas. | [`concepts/`](concepts/) |
| [How-to guides](#how-to-guides) | *How do I do X?* Short, task-shaped steps for developers. | `how-to/` (created with its first page) |
| [Reference](#reference) | *What exactly is there?* Lists, tables and API pointers, kept close to the code. | `reference/` (created with its first page) |
| [Specifications (EEPs)](#specifications-eeps) | *What must an implementation do?* Numbered, versioned specifications with MUST/SHOULD wording. | [`eep/`](eep/) |

A page belongs to exactly one section. When a page starts to mix them (a concept page that grows
a step-by-step list, a how-to that becomes a field table), split it and link the parts.

## Concepts

- [How ECAD stores a project](concepts/how-ecad-stores-a-project.md): the log, replay, derived
  state, pages, representations and references.

The design itself, with its fixed principles, is the architecture proposal,
[`../../../docs/document-architecture-proposal.md`](../../../docs/document-architecture-proposal.md)
(Part A is fixed; Part B is provisional; Part E covers persistence and the debugger). Concept
pages explain and link to it; they never restate or amend it.

## How-to guides

None yet beyond the quick start. Planned, as they are needed:

- inspect a `.ecad` file by hand (the debugger panel, `jq` on the JSON lines);
- add a new op (record type in `ecad-file/src/v1.rs`, op version, inverse, invariant table, golden
  file);
- run the load benchmark and compare two runs (today in the quick start, section "Measuring load
  time").

## Reference

- Crate documentation: `cargo doc --no-deps -p ecad-model -p ecad-file -p ecad-store --open`
  from `ecad/`. Each crate's `//!` header says what it holds and what it excludes.
- Document invariants I1 to I18: the table in `crates/ecad-model/src/invariant.rs` (summarised in
  [EEP-0002 §11](eep/eep-0002.md#11-replay-and-validation)).
- Decisions: [`../../../docs/decisions.md`](../../../docs/decisions.md). Postponed work:
  [`../deferred-work.md`](../deferred-work.md). Features and their briefs:
  [`../features.md`](../features.md).

## Specifications (EEPs)

An **EEP** (ECAD Enhancement Proposal) is a numbered document that specifies a format, a protocol
or a process precisely enough to implement or check against. The process is defined in
[EEP-0001](eep/eep-0001.md).

| EEP | Title | Type | Status |
| --- | --- | --- | --- |
| [0001](eep/eep-0001.md) | EEP purpose and guidelines | Process | Draft |
| [0002](eep/eep-0002.md) | The `.ecad` project file format, version 1 (current) | Standards Track | Draft |
| [0003](eep/eep-0003.md) | The `.ecad` file format: planned evolution | Standards Track | Draft |

### Numbering and approval, in short

1. **Number.** The author takes the next free four-digit number in the table above, in the same
   commit that adds the EEP. Numbers are never reused, even for rejected or withdrawn EEPs.
2. **Draft.** Anyone (the owner, the overseer or a worker) may write a Draft. A Draft is a
   proposal and binds no code.
3. **Approval.** Only the owner moves an EEP out of Draft: to **Accepted**, **Final**,
   **Rejected** or **Withdrawn**. The decision is recorded in the EEP's header and its revision
   history, and as a row in `docs/decisions.md` when it changes a recorded decision.
4. **Changes.** A Final EEP is changed only by a new EEP that replaces it (status
   **Superseded**, with a link both ways), except for corrections that make it match the code it
   describes, which go in its revision history.

EEP-0001 gives the full rules.
