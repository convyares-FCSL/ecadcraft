# Storage engine for `.ecad` v2: earlier arguments against the findings

Status: **Decided: redb** (owner, 2026-11-08). The earlier recommendation of our own container is
**withdrawn**; §9 explains why. Sections 1–8 are kept as the record of how the decision was reached.

Related: `discussions/history-and-branches.md` (the model: one log per branch, versions as
snapshots, content-addressed chunks and assets), EEP-0005 (container v2, draft), and the spike report
`worker` notes `spike-storage-report.md`.

---

## 1. What the container must do

These requirements were agreed before any engine was compared:

- **One self-contained `.ecad` file**, with nothing to install and no server.
- **The log stays the truth:** one append-only log per branch, starting with a fork transaction and
  ending with merge transactions.
- **Versions are snapshots**, built from **content-addressed chunks** (one per page plus the shared
  model tables) and stored once, so branches cost only what differs.
- **Assets** (images, PDFs) are stored once each, by hash.
- **Opening loads only what's needed:** `meta`, then the active branch's latest snapshot, then the
  tail after it. **Saving writes only what is new.**
- **Crash-safe:** a save interrupted at any point never loses or half-writes a committed save.
- **The owner's priorities:** 1. fast; 2. Rust; 3. easy management. Plus: never be stuck behind
  someone else's 20-year-old decisions. Majors may break, with a converter.

### The logical layout: the same "tables" in every engine

This is the structure agreed with the owner. The bake-off stored exactly this in all three
engines; only the physical form differs.

| Logical table | One row per | Holds | Written |
|---|---|---|---|
| `meta` | file | Format and container version, the file's ID, the ECAD build that wrote it, and cache bookkeeping. **No design data.** | rarely |
| `branches` | branch | ID, name, status (open / merged / archived), parent branch and fork point, current head, release-line flag | on fork, merge and each save (head) |
| `log:<branch>` | transaction | This branch's **own linear log**, today's v1 rules: `seq`, `prev` hash chain, author (user or agent), time and ops. Row 1 is the **fork transaction** (parent branch, point, snapshot). **Merge transactions** name the source branch and head and carry the resolved changes as ordinary ops. Append-only, never edited. | every save (new rows only) |
| `versions` | named point | Name, branch, point in that branch's log, and the chunk hashes of its snapshot. A **revision** is a version plus release data (rev, date, drawn, checked, approved) and a release-line flag. | on create version or issue revision |
| `chunks` | content hash | Snapshot pieces: one per page plus the shared model tables (chunked; see §6). Stored **once** and shared by every version and branch that has the same content. | only when content is new |
| `assets` | content hash | Images and PDFs, stored once and uncompressed. | on insert |

**What is not in the file:** derived state (nets are recomputed; wire numbers are the stored
exception, as decided in F4) and per-user view state.

**Where project fields live:** title, customer, drawing number, status, the revision table, sheet
and tag settings are **transactions in the branch log** (`SetProperty` on the project,
`SetProjectSettings`). So they're undoable, branchable and captured by versions. Their current
values also live in the snapshot's model chunk, so opening never replays history to find them.

**On open:** read `meta` and `branches`, then the active branch's latest version chunks (the shown
page first, other pages on demand), then only the `log:<branch>` rows after that snapshot. Other
branches, older versions, the full history and assets load only when used.

### How each engine holds those tables

| Logical table | Own container | redb | SQLite |
|---|---|---|---|
| `meta`, `branches`, `versions` | Typed frames; the latest copy is found through the **index frame written at the end of each save** (a committed generation) | Key-value tables in one write transaction | Ordinary tables in one transaction |
| `log:<branch>` | Typed log frames tagged with the branch ID, appended. The per-branch chain is today's v1 framing (CRC, hash chain) | A table keyed by `(branch, seq)` | A table keyed by `(branch_id, seq)` |
| `chunks`, `assets` | Typed frames keyed by hash, Zstandard-compressed (assets stored), located through the index | Tables keyed by hash | Tables keyed by hash (BLOBs) |
| Commit point | The index frame, plus its hash and generation | The redb commit | The SQLite commit (WAL), folded into the file on save |
| Compaction | Our "Compact" rewrites live frames | `compact()` | `VACUUM` |

**So the logical model is engine-independent.** That's what makes redb a real fallback behind the
same container trait: swap the physical layer, keep the tables.

---

## 2. How the arguments moved, in order

| Stage | Position argued | Main reasons given | What changed it |
|---|---|---|---|
| 1 | **A ZIP container**, Zstandard per entry, with our own "committed generations" save | A standard format readable by any tool; holds assets; per-entry compression; unzipped, it is a git folder | Owner: readability by other tools doesn't matter; git is not the model; "I'm not Autodesk" |
| 2 | **Our own container**: today's framing extended with typed frames plus an index written at the end | We already wrote and tested durable append and recovery (stage 2a); zero dependencies; full control at every major version | Owner: databases weren't excluded, show the options |
| 3 | **An embedded database**, with **redb** "my favourite on paper" and SQLite as the benchmark | Crash safety, recovery and compaction from a well-tested engine; less code of ours; a database's tables map directly onto logs, branches, versions, chunks and assets | The owner chose to measure rather than argue |
| 4 | Stated expectation before the numbers: **"redb or SQLite wins on effort and safety; our own container wins on raw speed and control. The spike will tell by how much."** | Not stated, it was a prediction | The bake-off |

The owner leaned towards redb at stage 3 (it fits, it's pure Rust, and it's a single file).

---

## 3. What was measured

The large bench project (500 pages), lean and churned, loaded as `main`. Then: 1,000 single-edit
saves, 50 saves of 100 edits each, 5 branches of 200 edits, 10 versions, and 20 assets. Medians of
3 full runs. Crash tests: the process killed mid-save 500 times (own) and 100 times each (redb,
SQLite); then torn-write tests.

### All three, compared with today's v1 file

| | Today (v1) | All three engines |
|---|---|---|
| Save one edit | 68–224 ms | **0.7–3.3 ms** |
| Open the large project | 2.4–8.5 s | **about 15–85 ms** |
| Open time as history grows 1×→10× and branches 1→5 | grows | **flat** |
| Deduplication (an unchanged version or duplicate asset writes nothing) | n/a | **works** |
| Crash tests | n/a | **0 failures** |

**So the model itself is the win,** whichever engine is chosen: per-branch logs, snapshot chunks,
and loading only what's needed.

### Between the engines

| | Own container | redb | SQLite (folded every save) |
|---|---|---|---|
| Save one edit (median) | **0.71 ms** | 1.05–1.39 ms (2.2 with quick-repair) | 2.6–3.3 ms, 38–43 ms spikes |
| Size after compaction, lean / churned | **49 / 63 MB** | 66 / 78 MB (+23–34 %; up to +65 % before compaction) | 49 / 63 MB |
| Full open of `main` | 48 ms | 78–85 ms | 38–48 ms |
| Reopen after a crash (median) | 33 ms | 149 ms (28 ms with quick-repair) | 30 ms |
| Language | Rust (ours) | Rust | C (`rusqlite`, bundled) |
| Code we write (estimate) | ~1.5–2k lines of engine plus ~1–1.5k lines of crash tests | least | small, plus WAL handling |
| Format control | **ours** | redb's | SQLite's (very stable) |

---

## 4. Each earlier argument against the findings

| Earlier argument | Finding | Verdict |
|---|---|---|
| **"A database gives crash safety for free"** (for redb and SQLite) | All three had **0 failures**. Our own container was **not** free: the spike's first recovery code had a bug that could roll back past earlier good saves and lose committed work. A torn-write test caught it; after the fix, 100 torn-write rounds were clean. | **Partly true.** A database gives safety with less of our code. Ours is equally safe **only if** its crash and torn-write tests are kept as mandatory and permanent. The bug shows the risk is real, and also that it can be tested for. |
| **"redb: less code, pure Rust, single file, my favourite on paper"** | True on code, Rust and single file. **New, against it:** redb's allocator rounds every allocation up to a power of two, so files are 23–34 % larger after compaction (up to 65 % before). **redb dropped support for its own previous file format in version 3.0 (2025)**, so `.ecad` files would depend on migrations we don't control. Reopen after a crash is slow without quick-repair. | **Weaker than argued.** The size cost is real, and the format-control finding goes directly against the owner's "never stuck behind someone else's decisions". |
| **"SQLite is the benchmark everyone measures against"** | It's fast to open and as compact as ours, but its saves are the slowest, because folding the WAL after every save costs as much as the commit. Spikes reach 38–43 ms. After a crash, committed saves may live only in the `-wal` side file, so copying just the `.ecad` loses them. It also needs a C toolchain on every target. | **Ruled out.** It fails "one self-contained file" at exactly the wrong moment, and it's not Rust. |
| **"Our own container wins on raw speed and control"** (stage 2 and the stage 4 prediction) | Confirmed: **fastest save, smallest file, full format control.** The cost was underestimated: about 3k lines including tests, with recovery the subtle part. | **Confirmed, with a realistic cost.** |
| **"We already wrote durable append and recovery in stage 2a"** | Helpful, but v2 adds typed frames, an index, chunks and compaction. The new recovery path is where the bug was. | **Partly true.** The experience transfers; the new code still needs its own tests. |
| **ZIP (stage 1)** | Not measured; dropped before the bake-off. Its one remaining advantage, readability by standard tools, has no value here. | **Rightly dropped.** |

---

## 5. Against the owner's priorities

| Priority | Own container | redb | SQLite |
|---|---|---|---|
| 1. Fast | **Best** save; good open | Good save; slower open and crash reopen | Slowest save, with spikes |
| 2. Rust | **Yes** | Yes | No (C) |
| 3. Easy management (one file, no install) | Yes | Yes | **No:** the `-wal` sidecar risk |
| Never stuck behind others' decisions | **We own the format** | Tied to redb's migrations | Very stable, but not ours |
| Size | **Smallest** | +23–34 % | Smallest (equal) |
| Effort and risk | Most code, and recovery must be proven | **Least code** | Small code; WAL handling |

---

## 6. Recommendation

**Our own container, with redb as the fallback; SQLite ruled out.**

**What changed from the earlier position:** "redb is my favourite on paper" rested on safety for
free and less code. The measurements kept "less code" but added a size cost and, more importantly,
**a format we would not control**. That runs against the owner's strongest principle. "Our own
container wins on speed and control" was confirmed outright. The deciding cost, recovery
correctness, turned out to be testable: the spike's own crash tests found the one bug and proved
the fix.

**Conditions that make "our own" the right call:**
1. **Crash and torn-write tests are permanent:** part of the product's slow test set at every QA
   gate, not a spike.
2. **Recovery code is reviewed as risky code:** the tester's risky-area list (`ecad/CLAUDE.md`)
   already includes the store.
3. **Split the shared project data into fixed-size chunks.** It's the biggest lever for any engine:
   15 of the ~19 ms open, and about 1.65 MB of each version.
4. **Fallback:** if our container's recovery proves hard to get right, redb with quick-repair is a
   measured, working alternative behind the same container trait.

---

## 7. Not covered by the bake-off (to test in the real implementation)

- Power-loss write reordering. A process kill isn't a power cut, so fsync ordering needs its own
  test.
- Cold-cache opens: the first open after a reboot.
- Linux.
- Simulated merges between branches.
- Real images and PDFs (the spike used random bytes, so compression figures for assets are
  pessimistic).

---

## 8. Decision needed

**Yes or no: our own container** (then EEP-0005 is updated and brief 021, container v2, starts), or
**redb** (less code, larger files, a format we don't control).

---

## 9. Revised decision: redb (owner, 2026-11-08)

**The custom-container recommendation in §6 is withdrawn.** It put more weight on owning the file
format than the evidence supports.

### The absolute differences are small

| Operation | Own | redb | Extra cost with redb |
|---|---|---|---|
| Save one edit | 0.71 ms | 1.05–1.39 ms | **+0.34–0.68 ms** (+48–96 %) |
| Open `main` | 48 ms | 78–85 ms | **+30–37 ms** (+63–77 %) |
| Reopen after a crash | 33 ms | 149 ms (28 ms with quick-repair) | +116 ms, or **−5 ms** with quick-repair |
| Size after compaction, lean / churned | 49 / 63 MB | 66 / 78 MB | **+15–17 MB** (+23–34 %) |

None of these is a user-facing problem we can name. The improvement over v1 (seconds down to
milliseconds) comes from **the model**: per-branch logs, snapshot chunks, and loading only what's
needed. redb delivers all of it. Saving half a millisecond and 15 MB doesn't justify owning a new
recovery engine.

### Corrections to the earlier arguments

- **Safety:** passing the spike's crash tests **does not** show that our container is as safe as
  redb, nor that its recovery is correct. It shows that the one regression found is covered.
  Power-loss write reordering and Linux weren't tested. redb has bugs too; its advantage is
  maintained transaction machinery that we reuse rather than own.
- **redb's format migrations:** 3.0 dropped the v2 file format, **but** redb 2.6 provides a
  documented `Database::upgrade()` route. We'd own ECAD's logical schema and serialisation, the
  history, branch and revision semantics, the compatibility policy, when to upgrade the dependency,
  and our converters. We would **not** own redb's physical format. That's an acceptable trade-off,
  given that majors may change with a converter and the logical model is engine-independent.
- **SQLite:** ruled out **in the WAL configuration tested** (fold cost on every save, the sidecar
  risk, C). Rollback journaling wasn't tested. That doesn't make SQLite preferable here, but the
  earlier claim was broader than the experiment.

### The analogy that settled it

Writing our own storage engine now would be like replacing CADCraft with our own drawing engine,
built up from a basic DWG viewer. It may make sense one day, but not for a long time. The custom
container spike and its benchmarks are **kept as evidence**, not maintained as a second production
engine.

### The history model, unchanged and stated precisely

- **Each branch owns a separate, logical, append-only transaction log**, in today's log style. Only
  the original project branch starts from the project root.
- A branch **starts from an immutable reference to a specific transaction in its parent branch**.
  Branches may fork from other branches. Example: Main M1→M2→M3→M4; Alternative forks at M3 and
  records A1→A2. Alternative's state is Main up to M3, then A1 and A2; M4 doesn't affect it.
- **Ancestral history is referenced, never copied.** A branch is never a snapshot-only project.
- **Snapshots are loading accelerators** tied to exact points in history. They never replace the
  log.
- **Physically in redb:**
  - one `transactions` table keyed by `(branch_id, sequence)`, plus `branches` (name, status,
    parent, fork point, head, release-line flag), `versions`, `chunks`, `assets` and `meta`;
  - sharing one physical table keeps the schema fixed as branches are created and archived; each
    branch's logical identity, ordering and ancestry stay explicit;
  - a save writes the new transactions, any new chunks and the updated branch head in **one redb
    write transaction**, so they commit together;
  - **append-only is enforced by ECAD's API**; redb doesn't prevent edits.

### Next

Update EEP-0005 to match. Then brief 021 (container v2 on redb). Neither production path is
implemented until EEP-0005 is reviewed.
