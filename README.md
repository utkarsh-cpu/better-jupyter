# KernelLedger

A local Python notebook runtime. Source lives in ordinary files with stable cell ids.
A persistent supervisor records executions in SQLite, and typed checkpoints restore
supported values into a new worker without rerunning earlier cells.

Requires CPython 3.11 or newer on Windows or Linux. The runtime uses only the standard
library. NumPy, pandas, and pyarrow are optional, installed in the worker environment.

The [v5 GPU implementation notes](docs/gpu/v5-implementation.md) describe isolated
experimental artifact and PyTorch training APIs. Runtime plan 5 adopts the bounded notebook integration in section 24;
`kl experimental-gpu inventory` inspects capabilities without loading a framework.
The [cooperative checkpoint plugin](docs/gpu/cooperative-plugin.md) adds opt-in
training boundaries and queued saves, tested on the Windows Radeon 8060S.
[Notebook and desktop training checkpoints](docs/gpu/notebook-training.md) add safe capture and fresh worker reconstruction.

```sh
python -m pip install -e .
kl -C analysis init
kl -C analysis add code --id start --source "x = [1, 2, 3]"
kl -C analysis run start
kl -C analysis run --source "sum(x)"
kl -C analysis checkpoint
kl -C analysis log
kl -C analysis stop
kl -C analysis restore <checkpoint-id>
kl -C analysis run --source "x.append(4); sum(x)"
```

`kl --help` and `kl <command> --help` describe all commands. `python -m kernelledger`
is equivalent to `kl`. Use `-C <directory>` before the command to select a notebook.
Edit `cells/<id>.py`, `.md`, and `.raw` with any editor. Cell order lives in
`notebook.json`. Running code never rewrites those source files.

The first run starts a detached supervisor. Later commands share its worker namespace.
`kl stop` closes it, `kl restart` starts an empty worker, and `kl interrupt --kill`
terminates the worker and its child processes. The supervisor holds an OS file lock;
crash recovery marks unfinished executions `incomplete`. `kl log` and `kl show`
only read history and do not launch a worker.

```sh
kl -C analysis show <exec-id> --source
kl -C analysis show <exec-id> --events
kl -C analysis show <exec-id> --mime image/png --out figure.png
kl -C analysis replay <session-id>
kl -C analysis continue <checkpoint-id>
kl -C analysis publish <checkpoint-id>
kl -C analysis bundle export saved.zip
kl bundle show saved.zip
kl -C other bundle import saved.zip
kl -C other restore <checkpoint-id> --trust
```

Restore and replay execute saved Python definitions or cells. Imported artifacts require
`--trust`. Bundles may contain secrets from variables, output, or recorded input. HTML
is retained as data and is never rendered by the CLI. Checkpoints preserve supported
object identities and mutable cycles; every unsupported binding is reported with its
path and reason. They do not restore processes, sockets, files, or remote state.
Classes are rebuilt from their saved definition and decorators run again. Mutable
module globals are reimported. Closures retain their values but shared closure cells
split. Cycles through immutable or hashed construction positions are excluded.

Import an ipynb into an initialized, empty notebook with `kl import file.ipynb`.
`--keep-outputs` records an imported session without executing code. Export with
`kl export file.ipynb --session <session-id>` to include folded output and source
mismatch metadata. `kl doctor` checks tracked source, ids, attachments, the ledger,
interpreter, and token permissions. Id repair is explicit with `--repair-ids`.

For Git branches that carry code, datasets, hyperparameters, and saved kernel values,
see the [worktree guide](docs/git-worktrees.md) and the
[runnable experiment example](examples/worktrees/README.md). The example restores a
baseline into two independent worktrees, commits their verified bundles, and checks
that a fresh clone can restore and resume training.

The [GPU implementation plan](plans/v5-draft.md) is a runtime version 5 draft for
portable model/training state, NVIDIA/AMD native snapshots, commit/worktree
association, and Windows/WSL2/Linux execution targets. These GPU additions are
planned and do not change the currently supported checkpoint contract.

Machine settings live in `.kl/config.json`, which is gitignored and never bundled:

```json
{
  "python": "/path/to/project/.venv/bin/python",
  "env_allowlist": ["LANG"],
  "checkpoint_denylist": ["credentials"],
  "thread_allowlist": ["tqdm._monitor.TMonitor"],
  "output_limit_bytes": 268435456,
  "siblings": ["../another-working-copy"]
}
```

The worker interpreter is selected from `--python`, config, the nearest project `.venv`,
`VIRTUAL_ENV`, then the supervisor interpreter. The standalone worker runs with `-I`,
adds the notebook directory to its import path, and authenticates to the supervisor.
Windows uses a kill-on-close Job Object and a current-user token ACL; Linux uses a
process group and a `0600` token. A failed ACL setup is reported by `doctor`.

For development:

```sh
python -m pip install -e ".[dev,codecs]"
# Alternatively: uv sync --all-extras, then uv run pytest
python -m pytest -q
python -m ruff check src tests examples/worktrees
python -m ruff format --check src tests examples/worktrees
python -m pyright
python scripts/benchmark.py --mib 256
python scripts/benchmark.py --mib 256 --supervised --out benchmarks/supervised.json
```

Tests include generated graphs, real worker processes, supervisor crashes, checkpoint
publication crashes, trust checks, source conversions, output limits, and bundle
integrity. CI covers Windows and Ubuntu, Python 3.11–3.13, with and without optional
codecs. Artifact contracts live in `schemas/`. Measurements live in `benchmarks/`.
The runtime now includes revision-checked source transactions, schema 2 ledgers,
durable desktop operations, frozen batches, input ownership, and an authenticated
desktop protocol. Existing schema 1 ledgers require explicit `kl migrate`, which
preserves a verified backup. See [implementation notes](docs/implementation.md) and
the [desktop protocol](docs/desktop-protocol.md) for operational details.

The [desktop frontend plan](FRONTEND_PLAN.md) specifies the GPUI interface, editing UX,
output rendering, accessibility, packaging, and release gates, as a client of the runtime
plan. A Rust GPUI Kit development app is available in `desktop/`; see its
[build and run instructions](docs/desktop/README.md). It includes cell editing,
autosave and recovery drafts, execution controls, safe output rendering, and history
inspection. The [coverage report](docs/desktop/coverage.md) identifies unfinished
workflows and qualification gates across both plans. This is not a qualified release.
[plans/README.md](plans/README.md) explains how both plans are versioned and archived.
