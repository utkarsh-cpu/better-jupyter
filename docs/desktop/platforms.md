# Platform qualification

Windows x64 builds with Rust 1.99.0, Visual Studio 2022 C++ Build Tools and a
Windows SDK. The native executable was compiled and linked on Windows. Native
rendering in development builds requires more than the default Windows main-thread
stack. The application linker reserves 16 MiB (committed on demand); startup was
checked with a responding native window attached to an empty notebook.
GPUI requires functioning native graphics; software rendering is not promised. If
graphics startup fails, use the CLI to inspect history and check GPU drivers.

## Linux (Ubuntu 24.04 LTS x64) — preview

Linux packaging and hardware-accelerated graphics are **not release-qualified**.
A development window was opened on 2026-11-07 under XFCE with Mesa **llvmpipe**
(software GL; no `/dev/dri`). That path is evidence of buildability, not a ship
matrix. Windows remains the primary release target until the table below is green
on hardware GL/Vulkan (live UX workstream G).

### Build and run packages

```sh
sudo apt install \
  build-essential pkg-config clang cmake \
  libfontconfig1-dev libfreetype-dev libdbus-1-dev \
  libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev \
  libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libssl-dev libvulkan-dev mesa-vulkan-drivers mesa-utils
```

Fonts: UI uses **Ubuntu** / **Ubuntu Mono** on non-Windows hosts (`fonts-ubuntu`).
Fontconfig supplies a fallback when those faces are missing.

### Environment

| Variable | Role |
| --- | --- |
| `DISPLAY` or Wayland session | Required for a native window |
| `XDG_STATE_HOME` | Desktop profile root (`…/kernelledger/desktop`) |
| `KL_RUNTIME_PYTHON` | Interpreter for the notebook worker/supervisor |
| `VK_ICD_FILENAMES` | Optional; set to lavapipe ICD for software Vulkan probes |
| `LIBGL_ALWAYS_SOFTWARE` | Optional; force Mesa software GL |

Isolated fixture launcher: [artifacts/frontend-live-ux-2026-11-07/launch_linux.py](../../artifacts/frontend-live-ux-2026-11-07/launch_linux.py).

```sh
cd desktop && cargo build -p kernelledger-desktop
. ../.venv/bin/activate   # or another env with kernelledger installed
python ../artifacts/frontend-live-ux-2026-11-07/launch_linux.py
```

### Qualification matrix (fill as evidence lands)

| Check | Software GL (llvmpipe) | Hardware GL/Vulkan | Notes |
| --- | --- | --- | --- |
| Debug window opens | Pass (2026-11-07) | Untested | |
| Notebook edit/run/save | Pass (fixture) | Untested | |
| Clipboard | Untested | Untested | |
| IME | Untested | Untested | |
| Mixed DPI / multi-monitor | Untested | Untested | |
| Orca keyboard journey | Untested | Untested | |
| Packaging / signing | N/A | N/A | Unsigned debug only |

macOS and ARM64 remain out of scope.

Before release, record GPU/driver and mixed-DPI results, clipboard/IME, NVDA/Orca,
focus and keyboard journeys, signing certificate, installer/update provenance and
section 13 performance baselines. App version: 0.1.0; runtime plan: 5; frontend
plan: 4 (live UX draft in progress); desktop protocol/helper: 1. This executable
is not a signed installer.
