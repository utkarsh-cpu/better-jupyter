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
environment. macOS and ARM64 are out of scope.

Before release, record GPU/driver and mixed-DPI results, clipboard/IME, NVDA/Orca,
focus and keyboard journeys, signing certificate, installer/update provenance and
section 13 performance baselines. App version: 0.1.0; runtime plan: 4; frontend
plan: 2; desktop protocol/helper: 1. This executable is not a signed installer.
