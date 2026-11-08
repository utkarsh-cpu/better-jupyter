# Platform qualification

Windows x64 builds with Rust 1.99.0, Visual Studio 2022 C++ Build Tools and a
Windows SDK. The native executable was compiled and linked on Windows. Native
rendering in development builds requires more than the default Windows main-thread
stack. The application linker reserves 16 MiB (committed on demand); startup was
checked with a responding native window attached to an empty notebook.
GPUI requires functioning native graphics; software rendering is not promised. If
graphics startup fails, use the CLI to inspect history and check GPU drivers.

Linux initially targets Ubuntu 24.04 LTS x64 with the GPUI X11/Wayland and font
dependencies. Linux packaging and graphics are unqualified in this Windows
environment. macOS Apple Silicon builds are checked with Rust 1.99.0. macOS target inventory
reports native NVIDIA/AMD process checkpointing as unsupported. Assistant secrets
use Apple Keychain; PDF export uses macOS Arial and Courier New when available.
The title bar reserves right-hand native caption space on Windows and Linux.

The macOS CI jobs run Python and Rust tests, build the native desktop and daemon,
and exercise disposable notebook smoke and lifecycle tests. These checks do not
qualify interactive rendering, Command-key shortcuts, clipboard/IME, Retina or
multi-monitor behavior. Intel macOS remains unqualified.

For local macOS development, install Python and the pinned Rust toolchain, then run:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -e ".[dev]"
cd desktop
cargo +1.99.0 test --locked --workspace
cargo +1.99.0 run --locked -p kernelledger-desktop
```

This is a development executable, not a distributable `.app`. Release packaging
still needs an application bundle with its runtime dependencies, a Developer ID
certificate, signing, notarization, and clean-machine verification.

Before release, record GPU/driver and mixed-DPI results, clipboard/IME, NVDA/Orca,
focus and keyboard journeys, signing certificate, installer/update provenance and
section 13 performance baselines. App version: 0.1.0; runtime plan: 4; frontend
plan: 2; desktop protocol/helper: 1. This executable is not a signed installer.
