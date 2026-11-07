# Frontend live UX evidence (2026-11-07)

Implements Phase 0 tooling and records the Linux software-render walkthrough
that seeded [plans/frontend-live-ux-draft.md](../../plans/frontend-live-ux-draft.md).

## Launchers

| Script | Platform |
| --- | --- |
| [launch_linux.py](launch_linux.py) | Ubuntu 24.04+ X11/Wayland; uses `XDG_STATE_HOME` and `KL_RUNTIME_PYTHON` |
| [../frontend-v6/native/launch.py](../frontend-v6/native/launch.py) | Windows (existing) |

## Graphics backends observed

| Backend | Result | Notes |
| --- | --- | --- |
| Mesa llvmpipe (software GL) | Window opened | No `/dev/dri`; GPUI surface via GL path |
| Hardware Vulkan/GL | Not tested here | Required before Linux leaves “preview” |

See [platforms.md](../../docs/desktop/platforms.md) for package and env requirements.
