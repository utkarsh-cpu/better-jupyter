# Cooperative PyTorch checkpoint plugin

`CooperativeCheckpointPlugin` adds registration, training boundaries, queued
checkpoint requests and independent object reconstruction to the experimental
portable GPU adapter. It works with the tested Windows ROCm interpreter. It is
a Python package plugin, discoverable under `kernelledger.checkpoint_plugins`
as `pytorch`; it is not a Codex connector or a CRIU process-image plugin.
Discovery/import does not load PyTorch or NumPy. Explicit registration does.

The optional [notebook and desktop integration](notebook-training.md) provides
idle-cell saves and candidate worker restoration. Its coverage remains portable
application training state; native process state and arbitrary namespace are excluded.

```python
from pathlib import Path
from kernelledger.artifacts import ArtifactStore
from kernelledger.gpu.cooperative import CooperativeCheckpointPlugin

plugin = CooperativeCheckpointPlugin(
    ArtifactStore(Path(".kl-gpu-experimental")),
    objects={"model": model, "optimizer": optimizer, "scheduler": scheduler},
    source_root=Path("."),
    files=["train.py"],
    recipe_source="train.py",
    factory="build",
    step=0,
    cursor={"batch": 0},
    drain=stop_and_drain_loader,
)

# A control/UI thread may enqueue this without importing or calling the framework.
request = plugin.request_checkpoint()
with plugin.training_step(cursor={"batch": 1}) as objects:
    run_one_training_step(objects)
# The successful step commits its next-batch cursor, drains the loader, synchronizes
# participating GPU work, and saves one snapshot for the pending requests.
snapshot = request.result()
plugin.store.export(snapshot, Path("saved-experiment"))
```

Use `plugin.checkpoint()` for immediate idle capture. It raises `CheckpointBusy`
while training or another capture is active. Queued requests are serviced after
a successful training step or by `plugin.service_checkpoints()` at an idle
boundary. The queue defaults to 32 requests; concurrent requests at the same
boundary share one snapshot. A request can be cancelled until capture begins.
Callbacks execute after the capture/training lock is released. Queued requests
are live process state, not durable operation records.

All mutations of the registered model, optimizer, scheduler/scaler, and associated
training state must occur inside `training_step`. The plugin cannot intercept
direct mutations through other references or unregistered threads. The drain
callback must stop background loaders and reconcile prefetched input with the
supplied logical next-batch cursor. Loader processes and external data services
are not serialized. Cursors are frozen copies of bounded JSON builtin values;
they cannot contain custom objects, cycles or nonfinite numbers.

If the training body raises, the plugin enters `faulted`: it refuses captures
and further training, and pending requests fail. Model/optimizer/RNG changes
already made are not rolled back. Recover from a durable snapshot in a fresh
process. If only checkpoint capture fails, the completed training step remains
committed, requests fail with the capture error, and idle capture can be retried.
The last successful snapshot remains valid. `close()` fails outstanding requests
and can close a faulted registration; it refuses while training/capture is active.

## Restore in a fresh process

```python
from kernelledger.experiments import materialize_source

store = ArtifactStore(Path("restored-store"))
snapshot = store.import_directory(Path("saved-experiment"))
materialize_source(store, snapshot, Path("restored-source"))
plugin = CooperativeCheckpointPlugin.from_snapshot(
    store,
    snapshot,
    build,  # Explicit caller-supplied factory; recorded recipe names are not executed.
    source_root=Path("restored-source"),
    trust=True,
    device="cuda:0",  # PyTorch uses this spelling for ROCm too.
)
```

Restoration validates the artifact closure, materialized source hashes, trust,
environment/device policy and registered classes before admitting new objects.
It leases the snapshot during reconstruction and refuses while a registered
training/capture operation is active. Matching source bytes do not prove which
source the supplied factory executed; source coherence stays `unverified`.
No existing model/optimizer object is replaced by this API.

RNG and framework settings belong to the process. Use a fresh process for restore
and one experiment per process for independent runtime continuation. The plugin
serializes training/capture across registrations in one interpreter, but does
not virtualize RNG between them or prevent unrelated code from consuming it.
Model objects and storage can be independently reconstructed within one process;
that is a narrower claim than independent runtimes.

Saved coverage includes model values/storage layouts, optimizer slots/groups,
optional scheduler/scaler state, gradients, training modes, RNG, framework
settings, step, cursor and explicitly selected files. Arbitrary Python namespace,
execution stacks, native GPU contexts, sockets and external resources remain
excluded. State inspection, export/import, leases and GC use the existing
experimental artifact store. The optional notebook bridge now integrates idle-cell
saves and fresh worker restoration with the supervisor and desktop. The main `kl`
bundle format remains CPU-only; full v5 adoption and qualification remain separate work.

## Run the Windows GPU test

```powershell
.venv-gpu-qualification/Scripts/python.exe examples/gpu/cooperative_training.py --device cuda:0 --out artifacts/my-cooperative-test
```

Choose a fresh output directory. The example makes a control-thread request
during training, proves busy capture refusal, exports the safe-point snapshot,
waits for the donor to exit, and restores in a fresh process. It checks gradients,
step/cursor, exact next loss and model parameters, and independent model storage.
It writes `result.json`, the exported snapshot and the recipient store.

The Radeon 8060S Windows test passed on 2026-10-08 with PyTorch
2.12.0+rocm7.14.0. The donor exited before restoration, gradients matched, and
the next loss and model parameters were exactly equal. Capture took 0.61 seconds,
restore 1.94 seconds, and the small-model store used 44,757 bytes. The targeted
Windows suite passed 54 tests (including 14 cooperative boundary/failure tests);
the complete final WSL regression suite passed 124 tests. Installed plugin
discovery and wheel contents were verified without importing a framework.

See [the exact qualification record](qualification/windows-8060s-cooperative-2026-10-08.json)
and [the benchmark](../../benchmarks/gpu/windows-8060s-cooperative-2026-10-08.json).
