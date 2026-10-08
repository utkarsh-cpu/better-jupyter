# Git worktrees with saved kernels

Run from the KernelLedger project directory, with Git and Python 3.11+ installed:

```sh
python -m pip install -e .
python examples/worktrees/demo.py --output ../kernelledger-worktrees
```

Choose a destination that does not exist. The demo creates a separate repository,
sets its own local Git identity, and retains all generated artifacts. It stops every
runtime before exiting. No NumPy, data downloads, or external services are required.

The experiment fits a two-weight linear model using gradient descent. It saves ten
training steps, then restores that same model in two real Git worktrees:

- `baseline/`, branch `main`: original code, dataset, parameters, and kernel.
- `learning-rate/`, branch `experiment/learning-rate`: changes the learning rate
  from `0.05` to `0.01` and trains for ten more steps.
- `dataset/`, branch `experiment/dataset`: switches to the shifted dataset and
  trains the original model for ten more steps.
- `restored-clone/`: a separate clone of the learning-rate branch. The demo
  restores its committed checkpoint and resumes training for one more step.

The demo checks that changing either experiment leaves the baseline kernel intact.
It checks restored weights, metrics, training step count, shared object identity,
and callable training code after the original runtimes have stopped. Clone verification
also enables Git line-ending conversion to check that the committed attributes
preserve hashed bytes. `summary.json` records the generated paths and checkpoint ids.

Each experiment commit includes:

```text
notebook.json                   cell identity and order
cells/load.py, cells/train.py    runnable source
parameters.json                learning rate, epochs, dataset selection
data/*.json                    both small, complete datasets
requirements.txt               dependencies (standard library in this example)
.python-version                exact Python version used
.gitattributes                 preserve snapshot file bytes across checkouts
checkpoints/<id>.json           published checkpoint descriptor
artifacts/<id>/                 verified runtime bundle: manifest, history, blobs, source
experiment.json                selected checkpoint, ancestry, hashes, weights, metrics
```

`.kl/` remains ignored. It holds the local SQLite ledger, supervisor endpoint,
authentication token, configuration, and working blob store. A new worktree gets
its own `.kl` when restored; Git transfers the exported bundle.

Inspect the results using normal Git commands:

```sh
git -C ../kernelledger-worktrees/baseline worktree list
git -C ../kernelledger-worktrees/baseline log --all --oneline --decorate
git -C ../kernelledger-worktrees/baseline diff baseline experiment/learning-rate -- parameters.json experiment.json
git -C ../kernelledger-worktrees/baseline diff baseline experiment/dataset -- parameters.json experiment.json
```

See the [worktree guide](../../docs/git-worktrees.md) for manual save/restore,
merging, cherry-picking, rollback, sharing, and the supported-state boundary.
