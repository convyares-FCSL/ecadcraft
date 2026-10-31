# Brief 002: Windows persistence equivalents

Status: Draft (overseer, 2026-10-31)
Branch: `ecad/windows-persistence`
Designer: no · Size: M
Depends on: 001 (to prove it on a Windows runner)

## Goal

The stage 2a guarantees (proposal §20.5, E8, E13) hold on native Windows as they do on Linux, and
where the operating system cannot give a guarantee, the code says so rather than reporting success.

## Scope

Crates `ecad-store` and `ecad-file` only. Finding ids refer to `ecad/docs/review-2026-10-31.md`.

1. **R-S1 File identity on Windows.**
   - `FileIdentity` becomes the volume serial number plus the file index, through a safe crate
     (proposed: `same-file`; owner question 3), since `unsafe` is forbidden.
   - Rename-over and delete-and-recreate behind an open writer give `DiskError::Replaced`.
2. **R-S2 Durable replace.**
   - Flush the folder on Windows (open it with backup semantics, then `sync_all`), or use a
     replace-with-write-through, whichever can be done without `unsafe`.
   - If neither can be verified, report `Durability::Unconfirmed` with the reason. Never report
     `Confirmed` without a real flush.
   - Update proposal §20.5 if the mechanism differs from `MoveFileEx` (owner gate, outside `ecad/`).
3. **R-M1 Read-only files** (both platforms): a project whose file cannot be opened for writing opens
   read-only with a new reason, for example `ReadOnly::NotWritable`.
4. **R-M4 Lock reporting.**
   - Separate "no lock could be taken" (`ReadOnly::NoLock`) from "another session holds it".
   - The holder on Windows follows owner question 4: an unlocked sidecar record, or a tested `None`.
5. **R-M2 Sharing violations:** a bounded retry for `replace` and `remove_file` on Windows only.
   Document FAT, exFAT and network shares as unverified.
6. **R-M3** The `HeldByOther` text mentions that a session that has just ended may take a moment to
   release its lock.
7. Remove the `ignore` markers that brief 001 added for these gaps.

**Out of scope:**
- any change to the file format, the lock file's name or E8 semantics;
- sweeping stale `.tmp-*` files (review minor 4: a later brief if wanted);
- the trial's path handling (brief 003).

## Acceptance criteria

1. On the Windows runner, every test in `crates/ecad-store/tests/disk_contracts.rs` and
   `crates/ecad-file/tests/disk_contracts.rs` runs and passes, with no `cfg(unix)` gating of an
   assertion and no `ignore`.
2. A new contract test: a project file renamed over, or deleted and recreated, while open for writing
   makes the next Save fail with `Replaced`. It runs on both platforms.
3. A new contract test: Save As reports `Confirmed` only when the folder flush ran and succeeded. With
   the flush injected to fail, or unavailable, it reports `Unconfirmed`.
4. A new contract test: opening a file with the read-only attribute (Windows) or mode 444 (Unix) opens
   it read-only and gives the reason.
5. A new contract test: a lock that cannot be taken (for example a read-only folder) is reported as
   `NoLock`, not as another session editing.
6. The crash-reclaim test passes 20 runs in a row on the Windows runner.
7. `cargo xtask ci` is green on both platforms. Never-crash rules hold, and there is no `unsafe`.
8. `ecad/docs/quick-start.md` lists anything about saving that behaves differently on Windows.

## Needs owner check (native Windows)

- Kill the trial with Task Manager while editing, then reopen.
- A second window opens read-only.
- Save to a OneDrive-synced folder.
