# Discussion: revisions, snapshots and compaction

Status: **Agreed direction** (owner, 2026-11-08). The specifications follow it:
[EEP-0004](../spec/eep-0004.md) §8.2–§8.5 (snapshots, revisions, compaction); the internal op
is `NameVersion` ([EEP-0002](../spec/eep-0002.md) §7.4).
Raised by: the owner, 2026-11-08. Written by: the overseer, recording the owner's reasoning.

## 1. The question

How should an issued drawing revision (Rev A, B, C) relate to the `.ecad` log, to snapshots and
to compaction, so that the file stays fast and traceable without losing history by accident?

## 2. Options considered

| Option | What a revision is | Gives | Costs |
| --- | --- | --- | --- |
| **A** | a named point in the log (`NameVersion`); nothing removed | full history, undo works as now | the log keeps growing; diffs between revisions read a noisy log |
| **B** | compact at the revision: the working file restarts from a snapshot, with a hash link to the prior history | a small working file | history before the revision leaves the working file |
| **C** | each issued revision is also a frozen, compacted, read-only `Project_RevX.ecad` in the stamped release folder, beside its PDFs and CSVs | an exact record of what was released, outside the working file | a copy per revision |

What large organisations need is **traceability**: what Rev B contained, who issued it, what
changed from B to C. They do not need to undo past a release; a change after release is the next
revision.

## 3. Agreed direction

### 3.1 One user-facing word: "revision"

- The UI and the workflow show only **revisions** (Rev A, B, C). Users never see both "version"
  and "revision": mixing them leaves teams unsure which to bump.
- "Version" stays internal: `container_version`, `schema_version`, the op `v`, and the op name
  `NameVersion`.
- Brief 019 (now): option A, plus a confirmation before undoing past an issued revision.

### 3.2 Issuing a revision writes a snapshot

Once snapshots exist (stage 4, EEP-0004), issuing a revision MUST write a full snapshot (every
part, then an index) immediately after the revision point.

- **Diffs.** The log between Rev A and Rev B is noisy: an engineer may add, move, rename and
  delete a component ten times between releases. The shop floor cares only about the net change
  between released states. With a snapshot at every revision, "what changed since Rev B" is a
  state-to-state diff of the two revisions' parts, and every page whose `state_hash` is unchanged
  is skipped.
- **Experience.** Issuing (F7, Release) is already a deliberate milestone with an expected pause
  (checks; the stamped release folder with PDFs, CSV schedules and the frozen
  `Project_RevB.ecad`), so writing a snapshot then costs the user nothing noticeable.

### 3.3 Intermediate snapshots

- Automatic snapshots keep long sessions fast, and a manual **Create Snapshot** action is
  available.
- Later the trigger becomes a project setting: every X transactions or X MB.
- They are invisible speed caches between revisions and are never shown as revisions.

### 3.4 Compaction ("Defrag"): user-driven, much later

- The EEP rules, for example FILE-05 ("no history is dropped"), are the **baseline to measure
  against**, not a rule that blocks practical tools later.
- History is never dropped automatically because a revision was issued.
- After stage 4: an explicit, confirmed **Defrag / Compact history prior to Revision X**. It
  removes the history before a chosen revision or snapshot, keeps a hash link to the history it
  removed, and resets the working file's baseline.
- It is safe because option C keeps each frozen release outside the working file.

## 4. Rollout order

| When | What |
| --- | --- |
| **Now (brief 019)** | A: a revision is a named point in the log; a confirmation before undoing past an issued revision. |
| **F7 (Release)** | C: the stamped release folder with PDFs, CSV schedules and a frozen `Project_RevX.ecad`. |
| **Stage 4 (snapshots, EEP-0004)** | Issuing a revision writes a snapshot, so comparing revisions is snapshot to snapshot. Intermediate snapshots by project setting (X transactions / X MB) or manual trigger. |
| **After stage 4 (housekeeping)** | The explicit, confirmed Defrag / Compact action. |

## 5. Still open

- The frozen `Project_RevX.ecad`: compacted to a single snapshot plus the link, or the full log
  up to the revision? (EEP-0004 §8.5 leaves its form open.)
- The default intermediate snapshot trigger before the project setting exists (EEP-0004 §8.2).
- What the compaction link must hold so an auditor can match it to a frozen release.
