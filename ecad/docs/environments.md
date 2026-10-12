# Environments for the integration experiment

> **Status:** developer notes for the bounded integration experiment (proposal §17). Nothing in
> this file has been run on WSL or on the NVIDIA Thor yet. It lists what to check before building,
> what the build needs, and how to run the experiment and the inspector. Report the check output
> back so the notes can be corrected.
>
> **There is no ECAD desktop application yet.** On any environment the experiment runs as tests
> and a headless example. The inspector is a static page that loads exported snapshots, with no
> live connection. Persistence is test-only: nothing is written to disk.

## Where it has run so far

| Environment | Checked | Result |
| --- | --- | --- |
| Claude Code cloud container | `uname -m` → `x86_64`; Ubuntu 24.04.5 LTS; 4 cores; 15 GiB RAM; `cc` 13.3; Rust 1.97.0 | CADCraft workspace and ECAD workspace build and test headless. No display; no GUI was run. |
| Windows laptop with WSL | not yet | first local trial route |
| NVIDIA Thor (JetPack) | not yet | later ARM64 target |

Notes from the container:

- The full build needs no `-dev` system packages. The windowing libraries (`libxkbcommon`,
  Wayland, X11, Vulkan/EGL) are loaded at run time by the desktop app, not linked at build time.
- CI still installs `libxkbcommon-dev libwayland-dev libxcb-render0-dev libxcb-shape0-dev
  libxcb-xfixes0-dev` for the CADCraft clippy job. Install them too if a build reports them
  missing.

## 1. Checks to run first (any Linux, including WSL and Thor)

Copy the output of this block into the issue or PR:

```sh
uname -m                      # x86_64 or aarch64: decides which binaries can run
cat /etc/os-release | head -4 # distribution and version
cat /proc/version             # on WSL this names Microsoft
nproc; free -h; df -h ~       # cores, memory, free disk in your home directory
cc --version | head -1        # a C compiler and linker is required
git --version
command -v rustup && rustc -V && cargo -V
echo "DISPLAY=$DISPLAY WAYLAND_DISPLAY=$WAYLAND_DISPLAY XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"
ls /mnt/wslg 2>/dev/null && echo "WSLg present"
command -v vulkaninfo && vulkaninfo --summary 2>/dev/null | head -20
command -v glxinfo && glxinfo -B 2>/dev/null | head -10
```

On Thor, add:

```sh
cat /etc/nv_tegra_release 2>/dev/null       # L4T release
dpkg -l | grep -i -E 'nvidia-jetpack|nvidia-l4t-core' | head
nvidia-smi 2>/dev/null | head -5             # may not exist on Jetson-class devices
```

Do not assume:

- the architecture: an `x86_64` binary will not run on an `aarch64` Thor;
- that a display exists;
- that Vulkan is available;
- the distribution or its version.

## 2. Build requirements

| Need | Why | Notes |
| --- | --- | --- |
| Rust stable ≥ 1.90 via `rustup` (tested with 1.97.0) | both workspaces declare `rust-version = "1.90"` | install from <https://rustup.rs>; the distribution's packaged Rust may be too old |
| C compiler and linker (`build-essential` on Debian/Ubuntu) | some crates compile C | |
| `git` and network access to github.com and crates.io | `craft-ui` is a git dependency on `storytold/craft-libs` | first build only |
| About 6-8 GB free disk | build output | build with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`; with debug info a full build passed 25 GB |
| 8 GB RAM or more | linking the engine and UI | 4 cores take about 10 minutes for the full CADCraft test suite |
| Optional: `rustup target add wasm32-unknown-unknown` | `cargo xtask wasm` gates | |

Share one target directory between the two workspaces to save disk and time:
`export CARGO_TARGET_DIR="$PWD/target"` from the repository root.

## 3. WSL (first local trial)

1. Clone **inside the WSL filesystem** (for example `~/src/ecadcraft`), not under `/mnt/c`: builds
   on the Windows mount are much slower.
2. Run the checks in section 1. Install `build-essential` and `rustup` if they are missing.
3. Headless experiment (no GUI needed):

   ```sh
   cd ecad
   export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
   cargo test -p ecad-cadcraft --all-features             # scenario S1-S16, refusal cases, census
   cargo run -p ecad-cadcraft --example experiment_snapshots -- ../target/ecad-snapshots
   ```

4. Inspector: open `ecad/tools/inspector/index.html` in a Windows browser. From WSL,
   `explorer.exe "$(wslpath -w ecad/tools/inspector)"` opens the folder. Load the files from
   `target/ecad-snapshots` (reachable from Windows as `\\wsl$\<distro>\…`) with **Open
   snapshots…**. No server is needed.
5. GUI (optional): **upstream CADCraft only**. Its desktop app does not load ECAD commands or
   projects, so this checks only that the display stack works under WSL. Needs WSLg, which
   Windows 11 provides: the section 1 check shows `WAYLAND_DISPLAY` and `/mnt/wslg`.

   ```sh
   cargo run --release -p cadcraft -- --sample   # from the repository root
   ```

   The renderer is `wgpu`. Under WSLg it may use Vulkan or GL through Mesa's D3D12 path,
   depending on the Mesa version. If the window does not appear, report the error together with
   the `vulkaninfo` and `glxinfo` output. If runtime libraries are missing, the Linux package lists
   `libxkbcommon0`, `libvulkan1` or `libegl1`, and `mesa-vulkan-drivers` (from
   `packaging/linux/nfpm.yaml`).

## 4. NVIDIA Thor (later ARM64 target)

- Expected but unverified: `uname -m` prints `aarch64`, and JetPack is Ubuntu-based (L4T). Confirm
  with section 1 before relying on either.
- **Build on the Thor itself** with `rustup` (`aarch64-unknown-linux-gnu`). Binaries built on the
  x86-64 laptop or in WSL will not run there.
- Cross-compiling from x86-64 (`--target aarch64-unknown-linux-gnu` with an aarch64 linker and
  sysroot) is possible later. It is not set up, and it is not needed for the experiment.
- The headless experiment (`cargo test -p ecad-cadcraft --all-features`) needs no display or GPU.
- Viewing the inspector: copy the snapshot files to the laptop, or serve the viewer folder on the
  Thor's loopback address and forward the port over SSH, for example
  `ssh -L 8000:127.0.0.1:8000 thor`. Bind any server to `127.0.0.1` only.
- GUI on the Thor depends on its display, desktop session and Vulkan driver (`vulkaninfo`). It is
  not part of the experiment.

## 5. What the experiment does not need

- No GPU, display, or network at run time (only for the first build).
- No AutoCAD, DWG files or ECAD production file format. The store is test-only and in memory:
  Save records a path but writes no file.
