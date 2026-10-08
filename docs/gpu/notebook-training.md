# Notebook and desktop training checkpoints

The notebook worker now supports experimental portable training capture and fresh
worker reconstruction. Runtime plan 5 section 24 and frontend plan 4 define the
bounded integration. Native process snapshots remain unavailable on this Windows
and AMD WSL target. No CRIU dump, thread/handle restoration or GPU driver-context
reconnection is performed.

Use a project interpreter with KernelLedger and a working PyTorch GPU stack
installed. The qualified interpreter here is `.venv-gpu-qualification/Scripts/python.exe`.
Choose it in desktop Settings, or pass its absolute path to `kl serve --python`.
Ordinary CPU notebook startup remains independent of the optional package/framework.

## Register in a notebook cell

Keep a reconstruction factory in a regular Python file, such as `recipe.py`:

```python
import torch

def build():
    model = torch.nn.Linear(4, 1)
    return {"model": model, "optimizer": torch.optim.Adam(model.parameters())}
```

Then register one experiment in the live worker:

```python
from pathlib import Path
from recipe import build
from kernelledger.gpu.notebook import register

objects = build()
objects["model"].to("cuda:0")  # use "cpu" for CPU training
training = register(
    objects=objects, source_root=Path.cwd(), files=["recipe.py"],
    recipe_source="recipe.py", factory="build", cursor={"batch": 0},
)
```

Perform every registered mutation inside `training.training_step(cursor=next_cursor)`.
The context commits the next-batch cursor only after success. Pass `drain=` during
registration when a loader needs a barrier; supply a deterministic reconstruction
factory and any required input files. The library cannot discover mutations made
through retained references, external threads or undisclosed loader state.

## Save and restore

After the notebook cell finishes, select **Training checkpoint** in the notebook
toolbar or command palette. **Checkpoints → Training snapshots** lists saved roots;
inspect the coverage before choosing **Restore training in fresh worker**. The
confirmation requires trust in the captured factory. CLI equivalents:

```powershell
kl -C path/to/notebook training-checkpoint
kl -C path/to/notebook training-list
kl -C path/to/notebook training-inspect SNAPSHOT_SHA256
kl -C path/to/notebook training-restore SNAPSHOT_SHA256 --trust --device cuda:0
kl -C path/to/notebook native-checkpoint
```

Inspection and native capability reports work offline and execute no factory.
Capture refuses a busy notebook worker; the cooperative plugin still supports
queued requests within training code, but desktop saves require an idle cell.
No automatic checkpoint mode fallback occurs.

Restore validates the stored closure, materializes captured source in owned
temporary storage, then executes the trusted factory in a candidate worker.
During factory execution, relative file reads use the captured source/input
directory. The notebook project working directory is restored afterward.
Failure or a 120-second reconstruction timeout stops the candidate and preserves
the healthy live worker. Success replaces the previous worker. The restored plugin
is named `training`; registered values are available through `training.objects`.
Notebook variables and imports are absent. The reconstructed recipe module retains
its captured bytes and qualified class names, so `from recipe import advance`
can load its saved step helper after restoration. Captured source coherence
remains **unverified**: matching stored bytes do not prove what code originally ran.

Training chunks have separate durable roots under `.kl/training-artifacts`.
The supervisor alone records session events and desktop operation outcomes.
The worker receives only OS temporary transfer paths, never `.kl` storage paths.
Session temporary directories are removed after worker exit, including failed
candidates. CPU GC and CPU bundles do not include training snapshots. Explicit
transfer uses the experimental artifact store export/import API with this store.
Independent simultaneous RNG streams in one interpreter are unsupported.
Supervisor power loss can leave OS temporary directories; automatic orphan
recovery and the complete crash/large-model matrix remain unqualified.

## Runnable local GPU verification

```powershell
.venv-gpu-qualification/Scripts/python.exe examples/gpu/notebook_training.py `
  --device cuda:0 --out artifacts/my-notebook-gpu-test
```

Choose a new output directory. This creates a notebook, registers training, saves
through the supervisor, computes an expected next step, restores into a fresh
worker and checks exact loss/weights. It records `qualification.json`, leaves the
saved notebook and artifact roots available for offline inspection, and stops its
runtime. On 2026-10-08 the Windows Radeon 8060S run passed: capture 1.33 seconds,
fresh worker restore 4.91 seconds, 34,771 stored bytes for the toy model.
The WSL Ubuntu-24.04 GPU run also passed: capture 1.32 seconds, restore
7.25 seconds, 34,805 stored bytes. These measurements include notebook transfer
and fresh interpreter startup and do not characterize large models.

The tests additionally check edited project source, failed-factory preservation,
trust refusal, busy refusal, CPU import independence, desktop operation retries,
and continuation after admission. Portable success does not qualify native snapshots.

Validation: the WSL suite passed 129 tests; final captured-input/CPU/GPU notebook
checks passed on Windows and WSL; the Rust workspace passed 19 tests and builds.
Windows skipped the symlink-creation check because of OS privileges. See the
[Windows qualification record](qualification/windows-8060s-notebook-2026-10-08.json)
and [WSL qualification record](qualification/wsl-8060s-notebook-2026-10-08.json).
Manual desktop interaction, accessibility and large-model release qualification
remain pending.
