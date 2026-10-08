# Runtime v5 implementation status

These implement a bounded subset of [the v5 draft](../../plans/v5-draft.md).
Runtime plan 5 section 24 adopts opt-in notebook training integration; frontend
plan 4 adds its desktop controls. The full draft and remaining qualification
gates are unfinished. CPU checkpoint schema 1 and ledger schema 2 are retained.
The experimental APIs do not migrate existing ledgers, install Git hooks, or
automatically provision drivers, WSL distributions, privileged helpers or remote
targets. Explicitly requested local Windows/WSL test environments are documented
separately below.

Implemented and exercised:

- An opt-in [cooperative checkpoint plugin](cooperative-plugin.md) with explicit
  object registration, owned training boundaries, queued/coalesced requests,
  failure handling and fresh-process reconstruction. It passed donor-exit
  training continuation on the Windows Radeon 8060S. This application-level
  plugin now has [notebook, CLI and desktop integration](notebook-training.md),
  separate rooted artifacts and fresh candidate worker admission. Ordinary
  CPU startup still imports neither the plugin nor its framework.
- Read-only local target identity and per-feature capability reports. Inventory
  reads installed distribution metadata and executable presence without importing
  PyTorch, starting a worker, or creating a GPU context.
- Explicit disposable interpreter probes. A successful portable tensor/view probe
  does not qualify native process capture. Windows, WSL DXG, missing Linux tools,
  and configurations without hardware evidence have distinct reasons.
- Bounded, streamed content-addressed artifacts. Defaults are 4 MiB chunks,
  16 MiB maximum chunk, 8 MiB metadata, 64 GiB per artifact, and 128 GiB store
  quota. The portable adapter separately defaults to a 1 GiB allocation budget.
  Exact bytes, ordered chunk lengths, and aggregate hashes are verified.
- Explicit durable roots, temporary capture ownership, leases, transitive closure,
  deduplication, GC, and self-contained directory export/import. Failed capture
  cleans up temporary roots; surviving chunks are collected by GC. Interrupted
  process leases deliberately do not expire automatically. A corrupt root aborts
  GC. Independent pins survive component ownership handoff.
- Dense PyTorch tensor bytes with an explicit dtype allowlist, shape, strides,
  offset, shared storage identity, device provenance, parameter and gradient flags.
  Shared storage, noncontiguous views, empty tensors, and parameter identity are
  reconstructed. Autograd graphs and unregistered attached gradients refuse.
  Unsupported layouts, subclasses, devices, cycles, and arbitrary
  Python objects refuse. There is no pickle or `torch.load` path.
- Explicit idle-boundary training capture: model state and metadata, optimizer
  slots/groups and parameter mapping, optional scheduler/scaler, gradients,
  per-module training mode, requires-grad flags, step/cursor, Python/NumPy/torch
  CPU RNG, participating-device RNG, and algorithm settings. CPU capture does not
  initialize an unrelated GPU. Multi-device training refuses pending a mapping adapter.
- Training restore checks all artifact bytes before invoking a caller-supplied
  trusted factory, reconstructs independent objects, checks optimizer ownership
  and model storage layout, and applies RNG last. Cross-device/vendor RNG changes
  require explicit reseeding and environment drift acceptance. The result claims
  application-state reconstruction, not deterministic GPU continuation.
- Captured source/input bytes, safe fresh-directory materialization, and source
  staleness inspection. Recipe source is retained as data and never executed while
  browsing. Source coherence is explicitly `unverified`: this prototype cannot
  bind a live worker to executed definitions or create a coherent experiment commit.

The prototype store defaults to `.kl-gpu-experimental`, separate from `.kl`. It is
an explicit store with its own root/lease/GC operations, not a hidden payload store
for the v4 ledger. Existing v4 bundles cannot contain or restore these snapshots;
use the prototype directory export/import. The namespaced JSON formats and their
schemas are experimental and do not claim to be the draft's final schema 2.

## Try it

Install the selected compatible PyTorch build and NumPy in the project interpreter.
The optional `gpu` extra pins PyTorch 2.12.0 and, on Python 3.12+, NumPy 2.5.2
(compatible local build suffixes are accepted). Python 3.11 resolves an older
NumPy 2.x release; that adapter environment remains unqualified. The extra does
not choose a vendor wheel or qualify a device. Core inspection needs neither package.

```sh
kl experimental-gpu inventory
kl experimental-gpu probe --device cpu
kl experimental-gpu probe --device cuda:0
python examples/gpu/portable_training.py --out artifacts/my-portable-training
```

The example starts a separate donor process, captures a small CPU training
experiment, exports its complete closure, waits for the donor to exit, imports
into a fresh store, materializes tracked recipe bytes, and checks the next
training step against the donor's expected result. It reports timings and bytes.
Choose a new output directory on each run.

```sh
kl experimental-gpu --store artifacts/my-portable-training/recipient inspect <snapshot>
kl experimental-gpu --store artifacts/my-portable-training/recipient export <snapshot> saved-experiment
kl experimental-gpu --store fresh-store import saved-experiment
kl experimental-gpu --store fresh-store materialize <snapshot> restored-source
kl -C restored-source experimental-gpu --store <absolute-store-path> source-status <snapshot>
kl experimental-gpu --store fresh-store gc
kl experimental-gpu --store fresh-store gc --apply
```

Capture and restore APIs live in `kernelledger.experiments`. Capture requires an
explicit `boundary="idle"`; the embedding application must stop training mutations
and drain its loader before calling it. Passing that argument alone does not
enforce a safe point in a running v4 notebook. Factories are supplied as Python
callables on standalone restore; serialized callbacks are unsupported. The notebook
bridge executes captured recipe source only with explicit trust in a fresh candidate
worker. The returned cursor belongs to the caller's dataset adapter.

Export excludes native-process artifacts, because a sensitive export policy and
encryption/key handling have not shipped. Imported directories are size/hash
checked and symlinks refused before publication. Exported recipe/data files may
still contain user secrets; only explicitly chosen files are captured.

## Evidence and remaining gates

On 2026-10-07 the local Windows interpreter (Python 3.12.10,
PyTorch 2.12.0+rocm7.14.0, HIP 7.14.60850) passed the disposable CPU tensor/view
probe and donor-exit CPU training example. The GPU probe failed actual computation
with `hipErrorInvalidImage` / "device kernel image is invalid". That GPU profile
was unavailable in that original environment. A subsequent local configuration
check identified missing split device packages. With matching host/device packages
colocated in `.venv-gpu-qualification`, the Radeon 8060S (`gfx1151`) passed GPU
tensor/view capture and restore, a 1024 by 1024 matrix multiplication, and exact
next-step training continuation after donor exit (Adam, StepLR, dropout, gradients
and RNG). See [the hardware record](qualification/windows-8060s-2026-10-07.json).
This qualifies those measured experimental checks only; no full v5 release,
native-context, NVIDIA, Linux, WSL, distributed or large-model qualification is
claimed. Evidence records live in [qualification/](qualification/).

### Local Radeon 8060S test environment

The tested Python 3.12 environment inherits the installed NumPy 2.5.2 and other
Python dependencies. All ROCm host libraries and PyTorch device files must be
installed together inside the environment; adding only device wheels alongside
an inherited system PyTorch does not work because their package paths differ.
The system interpreter and Windows driver were not changed.

```powershell
uv venv --system-site-packages .venv-gpu-qualification
uv pip install --python .venv-gpu-qualification/Scripts/python.exe --no-deps --index-url https://repo.amd.com/rocm/whl-multi-arch/ "torch==2.12.0+rocm7.14.0" "rocm-sdk-core==7.14.0" "rocm-sdk-libraries==7.14.0" "rocm-sdk-device-gfx1151==7.14.0" "amd-torch-device-gfx1151==2.12.0+rocm7.14.0" "amd-torch-device-gfx11==2.12.0+rocm7.14.0"
.venv-gpu-qualification/Scripts/python.exe -I src/kernelledger/gpu/backends/probe.py cuda:0
.venv-gpu-qualification/Scripts/python.exe examples/gpu/portable_training.py --device cuda:0 --out artifacts/my-8060s-test
```

AMD documents the required architecture-specific device package in its
[ROCm 7.14 installation guide](https://rocm.docs.amd.com/en/docs-7.14.0/install/rocm.html).
Use `cuda:0` for the PyTorch ROCm device. The measured small-model capture took
0.60 seconds, restore 1.27 seconds, with 38,360 stored bytes; these measurements
are not representative of a multi-GiB model. There are no configured SSH host
aliases. Ubuntu-24.04 was subsequently installed on WSL 2 and its GPU checkpoint
checks passed; see [the WSL environment and evidence](wsl-8060s.md).

No delivery phase is labeled complete:

- G0 still needs hardware/stack inventory and native feasibility evidence on the
  named NVIDIA/AMD Windows/Linux/WSL profiles.
- G1 still needs adoption of the full runtime successor, final experiment/native
  manifests, verified ledger migration, durable operation recovery, resolver and
  encryption contracts, and integration with released bundle/GC/protocol versions.
- G2 now has optional notebook registration, idle-cell capture and candidate
  admission with exact continuation on the local Windows GPU. Full resource
  preflight, device mapping/RNG compatibility classes and broader hardware
  qualification remain pending.
- G3-G11 remain: coherent Git commit/ref journals and worktree transactions,
  authenticated warm caches, WSL/SSH agents, native NVIDIA/AMD capture and reconnect,
  scoped privilege helper, mount/PID isolation, distributed capture, complete
  frontend adoption, and release qualification. Source matching alone never qualifies a
  coherent commit; browsing and checkout never mutate a live kernel.

Tests cover artifact integrity/limits/leases/ownership, typed tensor round-trips,
training continuation, independent reconstruction, preflight trust/refusal,
framework-free inspection, capability reasons, schema fixtures, and preservation
of the v4 worker's absolute-path independence. Ordinary CPU CI provides no claim
that native GPU state or cross-vendor computation has been restored.
