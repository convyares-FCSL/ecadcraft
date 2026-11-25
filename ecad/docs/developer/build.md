# Building ECAD

How to set up a machine, where builds go, the build profiles, and the notes for Windows, Linux and WSL. Running
and trying the app as a user is in [`../user/quick-start.md`](../user/quick-start.md).

## Two workspaces

The repository holds two Cargo workspaces. ECAD is its own workspace in `ecad/`; CADCraft, the upstream drawing
platform, is the root workspace, and ECAD uses its crates by path. Build ECAD from `ecad/`. The ECAD workspace
copies the root's `[patch.crates-io]` entries (the egui and winit forks, and `krilla`), because patches apply
only at a workspace root; they are re-copied on every upstream sync.

ECAD is desktop-only: it builds, tests and runs natively on **Windows and Linux**, and there is no web or wasm
build. CADCraft's own wasm build is not affected.

## What a build needs

| Need | Notes |
| --- | --- |
| Rust through `rustup` | `ecad/rust-toolchain.toml` pins the toolchain (with `rustfmt` and `clippy`), so local builds and CI format and lint with the same Rust; `rustup` installs it on first use. The workspace's minimum is its `rust-version` in `ecad/Cargo.toml`. A distribution's packaged Rust may be too old |
| A C compiler and linker | Some dependencies compile C. Linux: `build-essential` (Debian, Ubuntu) and `pkg-config`. Windows: the Visual Studio Build Tools with the "Desktop development with C++" workload (MSVC and a Windows SDK), and the default `x86_64-pc-windows-msvc` toolchain |
| `git`, and network access to github.com and crates.io | First build only: some dependencies are git dependencies on `storytold` repositories |
| Disk | A first build takes several minutes and 6–10 GB with the debug settings below; with full debug info a build folder grew past 25 GB |
| Memory | 8 GB or more: linking the engine and the UI is the heavy step |

On Windows, set `git config core.autocrlf false`: ECAD files are LF.

## Where builds go

Each checkout builds into **its own** folder, so builds running at the same time never mix. Leave
`CARGO_TARGET_DIR` unset: cargo then uses that checkout's own `ecad/target` (git-ignored), and the CADCraft root
workspace its own `target`. Do not share one target folder between checkouts or worktrees: a shared folder has
mixed a stale `ecad-model` into another worktree's build.

Two people or agents working in the same checkout share its build, so they do not build at the same time.

Always set:

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
```

```powershell
$env:CARGO_PROFILE_DEV_DEBUG = "0"; $env:CARGO_PROFILE_TEST_DEBUG = "0"
```

Incremental builds stay on: each checkout's own folder makes them safe, and they are faster.

**Freeing disk.** `cargo xtask clean-builds --root <folder>` deletes `ecad/target` and `target` in every checkout
directly under `<folder>` and under `<folder>/_tmp`, skipping a build folder that a running cargo, rustc, test or
app is using. It also removes git worktrees whose branch is merged into `origin/dev` (never one with
uncommitted changes, untracked files, ignored files other than its build folders, or an open pull request), and
prints the space freed. `--dry-run` only says what it would do. If it is scheduled to run regularly, a build
folder may be gone the next morning; the next build makes it again.

## sccache

The repository does not configure `sccache` or any other compiler wrapper. Whether to use one is each
developer's choice: set `RUSTC_WRAPPER` in your own environment or your user-level cargo configuration, never in
the repository's `Cargo.toml` or `.cargo/config.toml`.

## Profiles

From `ecad/Cargo.toml`:

| Profile | ECAD's own crates | Dependencies |
| --- | --- | --- |
| dev and test | `opt-level = 0`, `debug = "line-tables-only"` (file and line in backtraces) | `opt-level = 2`, `debug = false` |
| release | Cargo's defaults | Cargo's defaults |

ECAD's crates build unoptimised so a change recompiles quickly. Dependencies are optimised once and cached, which
keeps egui and wgpu usable in tests and in the dev app. A test that is too slow at this level goes in the slow set
([`testing.md`](testing.md#the-fast-set-and-the-slow-set)); it is not optimised.

`ecad/rustfmt.toml` sets the format (`max_width = 150`); `ecad/clippy.toml` allows `unwrap`, `expect`, `panic!`
and indexing in tests only.

## Linux and WSL

```sh
sudo apt-get update && sudo apt-get install -y build-essential pkg-config git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
cd <checkout>/ecad
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo run --release -p ecad-desktop
```

- A build needs no `-dev` system packages: the window libraries (`libxkbcommon`, Wayland, X11, Vulkan or EGL)
  are loaded when the app runs. If a build does report them missing, install `libxkbcommon-dev
  libwayland-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev`.
- To run the app, the runtime libraries are `libxkbcommon0`, `libvulkan1`, `libegl1` and
  `mesa-vulkan-drivers`.
- The renderer is wgpu. If no window appears or rendering fails, try `WGPU_BACKEND=gl` (or `vulkan`), and report
  the terminal output with `vulkaninfo --summary` and `glxinfo -B`.

**WSL.** Clone inside the Linux file system (your home folder), not under `/mnt/c`: builds on the Windows mount
are much slower. A window needs WSLg, which Windows 11 provides: `echo $WAYLAND_DISPLAY $DISPLAY` prints
something and `/mnt/wslg` exists. Keep `.ecad` files in the Linux home folder; file locking and durability on
Windows drives under `/mnt` are not verified. Do not edit in a WSL clone and a Windows clone at once: keep one
for editing and pull into the other.

Before reporting a build problem on Linux, include the output of:

```sh
uname -m; head -4 /etc/os-release; cat /proc/version
nproc; free -h; df -h ~
cc --version | head -1; git --version; rustc -V; cargo -V
echo "DISPLAY=$DISPLAY WAYLAND_DISPLAY=$WAYLAND_DISPLAY XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"
ls /mnt/wslg 2>/dev/null && echo "WSLg present"
command -v vulkaninfo && vulkaninfo --summary 2>/dev/null | head -20
command -v glxinfo && glxinfo -B 2>/dev/null | head -10
```

Do not assume the architecture (an `x86_64` binary does not run on `aarch64`), a display, Vulkan, or the
distribution.

## Windows

Install Git for Windows, the Visual Studio Build Tools (above) and Rust from <https://rustup.rs>, then open a new
PowerShell window so `cargo` is on the `PATH`.

```powershell
cd <checkout>\ecad
$env:CARGO_PROFILE_DEV_DEBUG = "0"; $env:CARGO_PROFILE_TEST_DEBUG = "0"
cargo run --release -p ecad-desktop
```

A release build opens only the app's window, with no console: it shows no console output, also through
`cargo run`. For `--help` and usage errors use a debug build (`cargo run -p ecad-desktop -- --help`) or redirect
the output to a file.

## Running from a build

```sh
cargo run --release -p ecad-desktop                              # a new project
cargo run --release -p ecad-desktop -- <path>.ecad               # open a project
cargo run --release -p ecad-desktop -- --snapshots <folder>      # allow exporting inspector snapshots
cargo run --release -p ecad-desktop -- --control <port>          # agent access on for this run (127.0.0.1)
cargo xtask demo [<path>.ecad] [--force]                         # write the demo project
```

`cargo xtask demo` builds the demo project from `ecad/examples/demo/commands.jsonl` through `ecad-mcp`'s
`execute` tool, so it never goes stale; an existing file is replaced only with `--force`. Headless and agent use
go through `ecad-mcp` ([`../agents/mcp.md`](../agents/mcp.md)).

## Start-up timing

`ECAD_STARTUP_TIMING=1` makes `ecad-desktop` print a mark on stderr at each start-up step (entering `main`,
commands registered, the window and GPU set up, the project opened, the first frame built and presented), then
the GPU adapter and the total, and write the same marks to a new `ecad-startup-<time>.json`. The file goes in
the `--snapshots` folder when one is given, otherwise in the temp folder; the last stderr line says where. Closing
the window before its first frame still writes the marks reached, marked `"complete": false`. Unset, `0` or
`off`: no output and no file, and no cost.

On Linux the marks count from the process's creation; on Windows they count from `main`. A release build on
Windows has no console, so read the JSON file (or redirect stderr from Git Bash).

## Environment variables

| Variable | Effect |
| --- | --- |
| `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` | Smaller build folders; always set |
| `ECAD_STARTUP_TIMING=1` | Start-up marks (above) |
| `ECAD_VERIFY_INVARIANTS=1` | Verify mode at run time: the full invariant check after every transaction ([`testing.md`](testing.md#where-tests-go)) |
| `ECAD_TESTER_SHOTS=<folder>` | UI tests write their screenshots there |
| `ECAD_TEST_CHILD` | Set by a test for the child process it starts; never set it yourself |
| `WGPU_BACKEND=gl` or `vulkan` | Choose the GPU back end when the default fails |
