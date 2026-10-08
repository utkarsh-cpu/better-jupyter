# Versioning experiments and kernel state with Git

KernelLedger can associate a Git commit with runnable code, experiment inputs,
and a portable snapshot of supported Python values. This is viable for exploring
hyperparameters and datasets: save a baseline, restore it into separate worktrees,
then commit each experiment's source and new checkpoint together.

Git worktrees have separate working files and indexes while sharing repository
history and refs. KernelLedger locates runtimes by the notebook's absolute directory,
so each worktree also gets an independent worker and `.kl` directory. Creating a
worktree checks out files; restoring its kernel is an explicit second action.
See the [Git worktree reference](https://git-scm.com/docs/git-worktree).

## Run the complete example

```sh
python -m pip install -e .
python examples/worktrees/demo.py --output ../kernelledger-worktrees
```

The [example](../examples/worktrees/README.md) creates a baseline, a learning-rate
experiment, and a dataset experiment, then proves restoration from a clean clone.
Its `experiment.json` ties a checkpoint id to hyperparameters, input/source hashes,
Python/platform information, parent checkpoint, saved weights, and metrics. Bundles
use directory format so JSON and content-addressed files can be inspected in Git.

The experiment uses plain Python model weights. It does not depend on a framework
model object being serializable. The dataset experiment intentionally fine-tunes
the baseline model on different data. To compare training from scratch instead,
run the `load` cell to reset the model before running `train`.

## Save a kernel with its code and inputs

The following commands use PowerShell and the generated baseline notebook. The
same `kl` and Git commands work in other shells; adapt variable assignments and
JSON parsing to your shell.

```powershell
$project = "../kernelledger-worktrees/baseline"
$saved = Get-Content "$project/experiment.json" -Raw | ConvertFrom-Json
kl -C $project bundle import "$project/$($saved.bundle)"
kl -C $project restore $saved.checkpoint_id --trust

# Continue an experiment before saving; run code after any input/source edits.
kl -C $project run --source "train(5)"
$checkpoint = kl -C $project checkpoint | ConvertFrom-Json
if ($checkpoint.status -ne "complete") { throw "Review checkpoint exclusions before committing" }
$cid = $checkpoint.checkpoint_id
kl -C $project publish $cid
kl -C $project bundle export "$project/artifacts/$cid" --checkpoint $cid
kl bundle show "$project/artifacts/$cid"
```

Review `excluded`, `notes`, and the bundle's `hash_mismatches`. `publish` writes a
descriptor with payload hashes, not the payloads themselves. Commit the exported
bundle as well as the descriptor: a descriptor alone cannot restore a fresh clone.
`kl fetch` only verifies local payload availability; it does not download blobs.

Update `experiment.json` to select this new checkpoint before committing. Record
the relative bundle path, parent checkpoint, actual parameters and metrics, and
the SHA-256 hashes of the committed source and data. The demo's `snapshot` function
implements this for the example and refuses partial checkpoints. A snapshot
describes the state at a particular execution boundary; edited files may not have
run yet, so include execution history and explicitly rerun the cells affected by
changes before presenting it as an experiment result.

```powershell
# After updating experiment.json and inspecting the artifacts:
git -C $project add notebook.json cells parameters.json data requirements.txt .python-version .gitattributes checkpoints artifacts experiment.json
git -C $project diff --cached --stat
git -C $project commit -m "Save experiment source, inputs, and trained kernel"
git -C $project tag experiment-saved
```

The example preserves raw bytes using `.gitattributes` (`* -text`), which disables
automatic line-ending conversion while retaining Git's normal diff detection.
This lets file and bundle hashes survive checkouts on machines with different
`core.autocrlf` settings. Keep `.kl/` ignored: copy snapshots through bundles rather
than copying a live SQLite database, token, or supervisor state.

## Fork a baseline into a worktree

```powershell
$base = "../kernelledger-worktrees/baseline"
$trial = "../kernelledger-worktrees/epochs"
git -C $base worktree add -b experiment/epochs $trial baseline
$saved = Get-Content "$trial/experiment.json" -Raw | ConvertFrom-Json
kl -C $trial bundle import "$trial/$($saved.bundle)"
kl -C $trial restore $saved.checkpoint_id --trust
kl -C $trial run --source "assert training_steps == 10; assert model_alias is model"
```

The `baseline` tag selects both the tracked source and saved checkpoint. The
import copies history and blobs into the new local ledger; it leaves the Git
checkout's source files intact. `--trust` acknowledges that restored definitions
can execute code. Inspect artifacts before trusting snapshots from elsewhere.

Edit the trial's `parameters.json`, for example setting `epochs` to `20`, then
apply that configuration and continue from the restored weights:

```powershell
kl -C $trial run --source "config = json.loads(pathlib.Path('parameters.json').read_text())"
kl -C $trial run train
```

For a dataset change, also reload `samples` from `config['dataset']`. Running
`load` resets the model and step counter; skip that cell when continuing a model.
Save a new checkpoint and bundle, update `experiment.json`, and commit as above.
Each worktree can have its own `.kl/config.json` interpreter selection and local
virtual environment. Dependencies and the Python version are recorded files;
restore does not install them. Cloned branches retain the same logical notebook
id; their absolute paths isolate the runtimes. `kl retarget` is for creating a
different logical notebook, rather than an experiment branch of this notebook.

## Git operations and their kernel meaning

Status, diff, log, tags, branches, commits, push, and pull operate on experiment
files and serialized snapshots. Their usual Git behavior is available in each
worktree. They never automatically update a running Python namespace.

**Compare experiments.** Diff `parameters.json` and `experiment.json` for input,
weight, and metric changes, and diff `cells/` for source changes. Blob hashes and
bundle manifests expose payload changes; Git does not provide a semantic tensor
diff.

```sh
git -C ../kernelledger-worktrees/baseline diff baseline experiment/learning-rate -- parameters.json experiment.json cells
```

**Combine experiments.** Create an integration worktree, merge source and input
changes, resolve conflicts, and compute a new checkpoint under the resulting
configuration. A file merge cannot combine model weights or two live namespaces.
Retain the existing snapshots for comparison; select a coherent parent checkpoint
and record a new result. Resolve `experiment.json` explicitly, then update it to
point at the integration checkpoint rather than implying either parent result
was computed from the merged inputs.

```sh
git -C ../kernelledger-worktrees/baseline worktree add -b experiment/combined ../kernelledger-worktrees/combined experiment/learning-rate
git -C ../kernelledger-worktrees/combined merge experiment/dataset
```

Expect conflicts in the experiment metadata; resolve the parameters to select
the intended combined configuration. Then import and restore the chosen parent bundle,
load the merged parameters and dataset, run the affected cells, and save a new
experiment. Rebase and cherry-pick require the same check: an inherited snapshot
can become stale relative to the resulting source and inputs.

**Cherry-pick a complete experiment.** The generated learning-rate branch has one
experiment commit after the baseline. You can transfer that commit, including its
coherent source and snapshot, to another branch:

```sh
git -C ../kernelledger-worktrees/baseline worktree add -b experiment/picked ../kernelledger-worktrees/picked baseline
git -C ../kernelledger-worktrees/picked cherry-pick experiment/learning-rate
```

Import and restore the checkpoint selected by the new checkout's
`experiment.json`. If the destination already has different source or inputs,
resolve conflicts and calculate a new experiment result before saving its kernel.

**Rollback or inspect an older commit.** A detached worktree provides the old
source, parameters, and selected snapshot together. Import and restore using that
checkout's `experiment.json`. Prefer a new branch if you want to commit changes.

```sh
git -C ../kernelledger-worktrees/baseline worktree add --detach ../kernelledger-worktrees/inspect baseline
```

Switching branches, reverting, resetting, or applying a stash in an existing
worktree changes files while leaving its live kernel untouched. Stop it before
switching experiments, then explicitly restore the selected checkpoint. A stash
also needs a previously exported bundle to preserve unsaved runtime values.

**Share.** Push the branch containing the source, datasets, environment files,
descriptor, bundle, and experiment record. A collaborator clones it, installs the
recorded dependencies, imports the bundle, and explicitly restores it. The demo
does this with a local clone and stops the donors before the restore check.

**Remove a worktree.** Save and commit the experiment first, stop its runtime,
then use `git worktree remove <path>`. Avoid forcing removal to save a step:
local `.kl` may contain executions or checkpoints that were never exported. After
confirming exported snapshots are sufficient, remove those local runtime files
if Git refuses removal because of ignored files. The branch and committed bundles
remain available for a later worktree.

## Viability and boundaries

This is a serialized Python namespace checkpoint, not a process-memory image.
Supported values include built-in containers and scalars, supported Python
definitions, and optional NumPy/pandas codecs. Object identity and supported
mutable cycles are preserved. Every unsupported binding has a path and reason;
check those exclusions before treating a saved state as complete.

Open files, sockets, child processes, remote service state, and GPU contexts are
outside the checkpoint contract. Native framework model/optimizer objects are
not guaranteed to work. For those experiments, save framework-native weights and
optimizer artifacts separately, commit their hashes and locations, and explicitly
load them during restoration. Record RNG state where relevant; reimporting a
module does not preserve its mutable global state. Restored definitions may run
decorators again, and shared closure cells have the limitations documented in
the [runtime README](../README.md).

The example commits small datasets and complete bundles directly. Large datasets
or model checkpoints need a separate artifact transport, with immutable content
hashes and an explicit way to retrieve them. `bundle export --external` records
external references; it does not include or fetch those resources. Git history
keeps committed payloads even if local `kl gc` removes runtime blobs, so storage
growth must be budgeted separately from local runtime cleanup.
