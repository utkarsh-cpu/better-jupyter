# KernelLedger runtime plan, version 5 draft: GPU experiments and Git snapshots

**Plan version:** 5, runtime draft 1.  
**Date:** 2026-10-07.  
**Status:** Draft; no GPU implementation or hardware qualification is claimed.  
**Baseline:** [Current runtime plan, version 4](../PLAN.md). This draft proposes its successor; version 4 remains authoritative.  
**Versioning:** [Plan rules](README.md).  
**Scope:** Portable GPU state, live caches, native NVIDIA/AMD process snapshots, Git commits/worktrees, native Windows, WSL2, and Windows control of Linux execution targets.  
**Frontend:** Contract proposals only. A later frontend plan must adopt them before desktop implementation.

## 1. Outcome and implementation boundary

An experiment can be saved with its source, inputs, Python values, GPU training state, environment requirements, and supported native execution state. A commit selects an immutable snapshot. A worktree restores its own independent runtime from that selection. Users can inspect coverage before choosing portable restore, native process resume, or temporary reuse of a live worker.

Cover every execution route without promising that every GPU or driver can preserve a raw context. An unsupported native route produces a precise capability report and an explicit portable alternative. Native Windows execution, WSL2 execution, and Linux execution controlled from Windows are different targets and remain visibly different in provenance.

This is a planning deliverable. Do not install drivers, enable WSL/Hyper-V, modify Git hooks, migrate ledgers, provision remote machines, or rewrite current runtime guarantees as part of publishing this draft. Proposed commands below do not exist yet.

Version 4 CPU checkpoints, source revisions, sole-writer ledgers, trust checks, and durable operation ids remain the baseline. Portable GPU snapshots extend that baseline. Native process snapshots are a separate artifact kind with a different restoration and security contract.

## 2. Evidence and feasibility decisions

Research checked on 2026-10-07. Implementation must pin tested releases and record the actual installed stack; these links are evidence, not permanent compatibility promises.

- NVIDIA documents GPU checkpoint APIs as Linux-only. `cuda-checkpoint` supports driver 550 onward; later drivers add features. GPU migration requires supported hardware, and current API documentation requires the same chip type and enough destination memory. Treat UVM, IPC, and other driver-specific features as separately probed capabilities. [NVIDIA utility](https://github.com/NVIDIA/cuda-checkpoint), [CUDA driver API](https://docs.nvidia.com/cuda/cuda-driver-api/cuda_driver_api/group__CUDA__CHECKPOINT.html).
- AMD provides a CRIU plugin requiring checkpoint-capable amdgpu/KFD support. Its documented support includes Gfx9, and its checks include device capability, firmware, and VRAM constraints. Do not extrapolate this to every Radeon or Instinct GPU. [AMD plugin manual](https://github.com/checkpoint-restore/criu/blob/criu-dev/Documentation/criu-amdgpu-plugin.txt).
- CUDA computation in WSL2 uses the Windows host driver exposed to Linux. NVIDIA specifically instructs users not to install a Linux GPU driver inside WSL. This establishes compute support, not native checkpoint support. [CUDA on WSL](https://docs.nvidia.com/cuda/wsl-user-guide/).
- AMD publishes a WSL hardware/framework matrix. Its WSL tooling uses the Windows GPU path through `/dev/dxg`; that differs from the native Linux KFD route required by the CRIU plugin. Our conclusion is to leave native AMD snapshots in WSL unqualified until a dedicated probe demonstrates support. [AMD WSL matrix](https://rocm.docs.amd.com/projects/radeon-ryzen/en/docs-7.2/docs/compatibility/compatibilityrad/wsl/wsl_compatibility.html), [ROCm WSL tooling](https://github.com/ROCm/rocm-cli/blob/main/docs/wsl.md).
- Hyper-V GPU device assignment has explicit host/hardware restrictions. It is not a general Windows desktop fallback, and ordinary VM checkpoints must not be assumed to capture an assigned GPU. Qualify the guest driver and process-checkpoint path independently. [Microsoft GPU assignment requirements](https://learn.microsoft.com/en-us/troubleshoot/windows-server/virtualization/troubleshoot-hyper-v-gpu-assignment-partitioning-passthrough-issues).
- PyTorch training restoration includes model and optimizer state; ROCm builds intentionally retain the `torch.cuda` interfaces. Shared APIs support one adapter design, not identical RNG algorithms or numerics across vendors. [PyTorch training checkpoints](https://docs.pytorch.org/tutorials/beginner/saving_loading_models.html), [PyTorch HIP semantics](https://docs.pytorch.org/docs/2.14/notes/hip.html).

Decisions:

1. Portable application state is the durable, cross-platform baseline.
2. A warm worker is a performance cache. It is never the sole durable copy associated with a commit.
3. Native NVIDIA and AMD snapshots are optional Linux backends, enabled only for qualified configurations.
4. Windows first receives native portable state and live-worker caching. WSL2 receives a Windows-to-Linux execution bridge with portable state; native resume is a separate qualification gate.
5. A Windows client can control a qualified Linux worker and retain its native snapshot on that Linux target. This does not translate a Linux process image into a Windows process.
6. Raw CUDA and ROCm contexts are not interchangeable. Cross-vendor restoration reconstructs application state on a fresh device and reports drift.
7. Neither process snapshots nor portable training checkpoints claim to restore external services or merge two running kernels.

## 3. Execution profiles and support status

Keep a capability record per target, not one global `gpu_supported` boolean. Each feature is `qualified`, `experimental`, `unsupported`, or `unavailable`, with a reason and evidence id. All proposed features start unqualified.

- **Native Windows, NVIDIA:** portable tensors/training state and warm caches on a supported CUDA framework installation. Native CUDA process images remain unavailable while vendor APIs are Linux-only.
- **Native Windows, AMD:** portable state and warm caches where the selected Windows framework/driver supports the actual GPU. ROCm support is checked against the exact Windows release and GPU; otherwise select WSL2 or a Linux target explicitly. DirectML/other framework adapters are separate extensions.
- **WSL2, NVIDIA:** portable state and warm Linux workers on qualified CUDA installations. Native process resume remains experimental until both the checkpoint driver functions and CRIU resource restoration pass.
- **WSL2, AMD:** portable state and warm workers for qualified combinations from the AMD matrix. KFD-based native resume is unavailable by default on the DXG route; a future supported path needs its own implementation and evidence.
- **Bare-metal Linux, NVIDIA:** portable state, warm workers, and qualified CUDA/CRIU process snapshots.
- **Bare-metal Linux, AMD:** portable state, warm workers, and qualified amdgpu/CRIU snapshots on proven GPU/driver combinations.
- **Linux VM with an assigned GPU:** portable state by default. Native process snapshots require proven guest-driver/device support. VM memory/disk snapshots alone are not a GPU checkpoint backend.
- **Remote Linux, controlled from Windows or Linux:** the qualified capabilities of that host. Remote execution is opt-in; source/artifact transfer and authentication are explicit.
- **CPU-only target:** inspect any snapshot without GPU initialization; restore portable values onto CPU when the adapter supports their operations. Native GPU images cannot resume on CPU.

Capability discovery records OS/build, architecture, Linux kernel, WSL distribution/build if applicable, framework/runtime versions, driver, GPU UUID and architecture, VRAM, topology, checkpoint tools/plugins, privileges, and tested features. Missing tools and insufficient privilege differ from unsupported hardware. Read-only inspection must not import user code, create a CUDA context, or start a worker; an active capability probe is a named operation using a disposable process.

## 4. Snapshot meaning, coverage, and identity

Three snapshot/cache kinds exist:

- `portable`: typed Python state plus registered GPU tensor/framework state and content-addressed inputs.
- `native-process`: a frozen Linux worker/process group, vendor GPU state, dependencies required for process restoration, and optional portable fallback captured at the same boundary.
- `warm-cache`: a live worker identity and generation. A cache entry refers to an existing durable snapshot and cannot appear as a durable payload in an exported snapshot.

Keep version 4 checkpoint outcomes intact. Composite experiments report outcome per component and overall required scope. `complete` on an experiment means every required declared component was captured; it does not redefine a partial Python namespace checkpoint as complete. Report namespace coverage separately, including every excluded binding. Missing required model/optimizer/RNG/data state refuses strict capture. Optional omissions make coverage partial and remain visible.

A snapshot records its immutable id, content manifest hash, parent snapshot ids, execution/session boundary, source/input hashes, selected modes, backend/version, environment fingerprint, rank/device mapping, captured features, omissions, compatibility requirements, artifact locations, and fallback availability. Record RNG coverage, sampler cursor, gradients, model training/eval mode, deterministic settings, and registered adapter versions. Do not infer full training continuity from a weights-only file.

Distinguish four claims: exact stored bytes, application-state reconstruction, qualified native process continuation, and reproduction of future computation. Only measured configurations may claim deterministic continuation; cross-vendor or changed environments report a downgraded reproducibility level. Recreating a CUDA context is different from restoring a saved native context.

## 5. Runtime architecture and ownership

Preserve one supervisor as the ledger writer. Introduce a target abstraction with `local-windows`, `local-linux`, `wsl`, and `ssh-linux` implementations. Each execution target has a local agent/supervisor and worker. Windows client/proxy code forwards durable operations to that authority; it never concurrently writes the Linux ledger.

Proposed boundaries:

- `gpu/manifest.py`: stdlib validation, coverage, requirements, and compatibility reports; inspection does not load a framework.
- `gpu/backends/`: host-side capability probes, native capture/resume orchestration, and driver/tool adapters.
- `execution/targets.py`, `execution/wsl.py`, `execution/ssh.py`: target identity, transport, cancellation, liveness, and source staging.
- `experiments.py`, `git_state.py`: immutable experiment records, source/input capture, commit association, worktree lifecycle.
- `artifacts.py`: streamed immutable payloads, chunk manifests, resolvers, verification, export/import, leases, and GC.
- Standalone `worker.py`: optional PyTorch tensor/adapter support and cooperative capture/reconnect messages, loaded only when requested. Preserve its ability to run by absolute path without importing the KernelLedger package. If it must be split, publish a separately deployable worker bundle and change that contract explicitly.
- Existing `blobs.py`, `sharing.py`, `ledger.py`, CLI, desktop helper/contracts: extend through versioned formats; no hidden side store that export or GC cannot see.

Framework packages live in the project interpreter. NVIDIA/AMD/CRIU tools are optional host dependencies. The stdlib supervisor and offline history reader remain usable without them. Target identifiers, credentials, interpreter paths, leases, and broker endpoints are machine-local configuration; tracked files contain requirements and logical identities rather than credentials.

## 6. Portable tensors and training adapters

First implement explicit registered PyTorch experiments, then broaden type support. Do not blindly serialize a `torch.nn.Module`, optimizer, framework internals, arbitrary serializer callback, or an autograd graph.

Portable tensor encoding uses a versioned JSON descriptor plus raw immutable CPU bytes. Store dtype, endianness, shape, stride, storage offset, storage identity, device requirement, parameter/gradient flags, and chunk hashes. Validate size arithmetic, bounds, alignment, and dtype before allocating. No pickle, dill, cloudpickle, `__reduce__`, or unrestricted `torch.load` in the built-in checkpoint format. Framework-native files may be attached as explicitly typed opaque artifacts, but a trusted adapter controls their loading separately from the graph decoder.

Qualification stages:

1. Dense contiguous CPU/CUDA/ROCm tensors with a documented dtype allowlist.
2. Aliases, noncontiguous views, tied weights, empty tensors, and shared storage. Until supported, refuse required alias-dependent state or report an explicit copy; never claim identity preservation after flattening it.
3. Sparse, quantized, unusual dtype, tensor subclasses, custom devices, and extension storage, each behind its own adapter capability. Unsupported kinds remain excluded or refused.

Capture waits at a safe application boundary and synchronizes every participating device/stream through the framework's supported API. Memory allocation, pinned staging, host RAM, free disk, and VRAM budgets are preflighted. No checkpoint of half-applied optimizer updates. A long-running cell uses a registered cooperative training-step hook in a later phase; without that hook the existing busy refusal remains.

An adapter registers a logical object id and a reconstruction recipe from tracked source, and captures model/state dictionaries, optimizer slots and param groups, scheduler/scaler, gradients when relevant, Python/NumPy/framework CPU RNG, per-device RNG, training mode, step, sampler/shuffle state, and relevant algorithm settings. Dataset files are content-hashed; loader prefetch and worker processes require an adapter-defined drain/cursor rule. Reconstruct factories and bind registered objects before dependent globals/functions; restore RNG after initialization that consumes randomness. Restored state must refer to the new model parameters, not stale optimizer objects.

All required parts share one capture epoch. User factories can execute code, so restore requires the existing provenance/trust policy. Inspecting tensor metadata does not instantiate a model. Cross-vendor weights can be reconstructed where supported; raw RNG states are restored only to compatible generators. A changed generator requires an explicit reseed/replay decision and records the loss of exact RNG continuity.

After PyTorch, the adapter interface covers NumPy/CuPy/JAX/TensorFlow, custom CUDA/HIP kernels, and supported DirectML applications. Each extension supplies state coverage, safe-point behavior, reconstruction, and tests. Compiler caches, JIT binaries, and tuning caches are optional environment-keyed artifacts; invalidate on incompatible runtime/architecture and never treat them as model state or native context images.

## 7. Native NVIDIA process snapshots

Use the Linux CUDA checkpoint APIs or a pinned `cuda-checkpoint` utility with CRIU. Prefer a qualified CRIU CUDA integration where it owns GPU lock/checkpoint/restore transitions. A manual adapter must own that sequence exclusively; never run both mechanisms over one capture.

The vendor operation drains submitted GPU work and can stage GPU memory in host RAM. Durable capture also needs CPU process images. Supported migration and custom-storage APIs are capability-specific, not a guarantee for all driver releases. [NVIDIA checkpoint implementation](https://github.com/NVIDIA/cuda-checkpoint), [storage and migration APIs](https://docs.nvidia.com/cuda/cuda-driver-api/cuda_driver_api/group__CUDA__CHECKPOINT.html).

Capture only the worker and explicitly registered child processes, not the notebook supervisor, ledger, desktop, registry, or credential broker. Preflight GPU stack, device/topology, process namespace, CRIU support, external files, shared/managed allocations, graphics interoperability, CUDA graphs, native extensions, IPC, multi-process relationships, and collectives. Features not qualified are refused for native capture and listed in the report.

At the controlled checkpoint boundary, close/rebind supervisor transport and diagnostic pipes under a checkpoint handshake. Worker reconnect logic reads a fresh rendezvous from a broker outside the process image. Restoring an image must not reuse the saved authentication token, port, session id, stale socket, or subprocess monitor. The restored worker authenticates with a new credential and opens a new ledger session linked to its origin snapshot.

Publish a native image only after hash verification and confirmation of the captured process/device state. Capture-and-continue must resume the original worker before reporting that whole operation as successful. A driver failure can leave the worker unusable: recover if verified, otherwise stop it and record failed/unknown rather than presenting it as runnable. Privileged native restore runs through a narrowly scoped helper, not a root desktop or root notebook supervisor.

Prove same-target resume first, then compatible-device/host migration, image clones in distinct process namespaces, and custom-storage/delta optimizations. Never silently substitute portable restore for an explicitly requested native resume.

## 8. Native AMD process snapshots

Use a pinned CRIU build with its amdgpu plugin and a qualified Linux amdgpu/KFD driver. Preserve kernel-driver memory/queue/event state through that integration; copying tensor data or saving a HIP handle alone is insufficient.

The backend records plugin/driver/kernel/firmware versions, GPU capabilities, VRAM, and topology. It keeps compatibility checks enabled and refuses a mismatched target; disabling a vendor check is not a product compatibility strategy. [AMD CRIU plugin requirements](https://github.com/checkpoint-restore/criu/blob/criu-dev/Documentation/criu-amdgpu-plugin.txt).

Apply the same worker-only scope, capture epoch, source mount, reconnect, privilege boundary, crash publication, and portable fallback rules as NVIDIA. Qualify actual devices separately, beginning with a supported Instinct/Gfx9 configuration, then expanding only with evidence. HIP graphs, IPC, custom native extensions, multi-GPU queues, and RCCL/distributed communication are distinct qualification features.

WSL DXG-backed AMD execution must not be sent to the KFD plugin merely because ROCm imports successfully. A future vendor-supported DXG snapshot interface would be a new backend, not an assumption that the native plugin works unchanged.

## 9. Bringing Linux execution to Windows

### 9.1 Native Windows

Keep the existing Windows supervisor process tree/Job Object behavior. Add portable tensors, registered training state, artifact attachment, and warm worker selection without requiring Linux. Device contexts are created anew during portable restore. A native Linux process image is never executed by Windows Python.

Detect framework/device support rather than assuming all AMD Windows GPUs run ROCm. Provide separate recipes for supported CUDA/ROCm/DirectML stacks and record their capabilities. Installer/driver setup is an explicit user-controlled step, not a consequence of opening a snapshot.

### 9.2 WSL2 bridge

The Windows desktop/CLI starts an agent using `wsl.exe` with an explicit distribution, user, Linux project root, and interpreter. Pass structured arguments; avoid shell-built command strings. Bootstrap over stdio, then use a bounded framed channel or authenticated local forwarding. Negotiation covers API versions and target identity. Windows process launch success is not Linux-worker liveness.

Run the supervisor, worker, ledger, locks, and large artifacts in the distribution's Linux filesystem. Windows edits are synchronized as revision-checked source transactions into a Linux worktree/mirror; choose one source-writing authority per project mode. Never have Windows and Linux supervisors write one SQLite ledger on a shared path. Track Windows path, Linux path, filesystem semantics, and case collisions in the local mapping. No automatic conversion of arbitrary saved path variables.

Source synchronization stages an immutable tree plus explicit input files and verifies hashes at capture. Do not copy `.kl`, Git runtime credentials, virtual environments, or host-specific caches between OSes. Reject traversal, symlink escapes, conflicting revisions, mixed EOL conversions, and ambiguous drive/UNC mapping. Python/platform drift remains visible.

Deliver portable snapshots and warm kernels in WSL first. Run disposable native checkpoint probes for each vendor/WSL/driver stack; a successful compute test or `criu check` alone is insufficient. If native GPU restore fails, return a reason and offer portable restore or a qualified external Linux target. Do not label WSL native resume qualified by inference. Do not install a Linux NVIDIA GPU driver inside WSL. [NVIDIA WSL driver guidance](https://docs.nvidia.com/cuda/wsl-user-guide/).

WSL shutdown, Windows reboot, GPU reset, sleep/resume, driver update, distribution unregister, and lost forwarding invalidate warm caches. A durable snapshot remains an artifact, but compatibility is rechecked before restore. Stopping a Windows proxy must either stop its Linux worker or explicitly detach; an accidental orphan is not an accepted outcome.

### 9.3 Windows client controlling a Linux host

Add an opt-in SSH target with host-key verification, a user-scoped agent, framed transport, durable request ids, source/input staging, artifact streaming, and resumable transfers. Privileged checkpoint operations are delegated to the target's scoped helper. Secrets remain in local/remote credential stores and never enter Git metadata.

A Windows commit references the Linux snapshot manifest and content hashes. Native restore occurs on the compatible Linux target. The client streams output and edits source through the normal runtime contracts. Transfer portable state to Windows for local execution when explicitly requested; report platform/device/RNG/recipe drift. No raw process translation is offered.

A disconnected client can reconnect to its durable operation outcome. Cancellation targets the remote process group and rank group, not merely the local SSH process. Target-side leases, heartbeats, and restart recovery decide whether a worker is running, stopped, or unknown. Offline inspection of downloaded metadata remains available.

### 9.4 Qualified Linux guest on Windows hardware

A separately configured VM with usable GPU assignment is an optional execution target. Verify the host edition, hardware, GPU assignment mode, guest driver, and process-checkpoint support. Do not promise supported desktop Hyper-V passthrough, assume a WSL GPU is native PCI passthrough, or rely on a VM checkpoint to capture GPU state. If qualification fails, portable WSL/local state or a remote Linux host remains the delivered route.

Research native Windows vendor APIs again at release time. If vendors introduce Windows checkpoint APIs, implement a separate backend with the same capture, transport, worktree, security, and qualification contract. Until then `native-process` on native Windows returns an explicit unsupported capability.

## 10. Git commits, worktrees, and experiment history

Tracked additions are `experiments/<snapshot-id>.json`, optional `experiment.json` selecting one experiment, environment lockfiles/recipes, content-addressed inputs, and exported payloads or immutable external artifact references. `.kl` stays ignored. Git metadata and runtime metadata remain separate.

### 10.1 Save and commit

Freeze an explicit source/input file list and relevant Git index revision. Record hashes of exact bytes and the Git object format; never assume every object id is a 40-character SHA-1. The source/input tree fingerprint excludes generated snapshot records, artifact payloads, and unrelated files so it has no self-reference. It includes every reconstruction recipe and declared dataset/config/lockfile.

Record both checkout source hashes and executed-source/definition provenance. Files edited before capture may never have executed; matching capture-time file hashes alone do not prove the live kernel came from that code. Strict experiment commits require adapter-validated input/config state and coherent required definition provenance, or refuse with a list of cells/recipes to rerun. Historical mixed-source state may still be saved explicitly, with the mismatch attached, but never labeled a coherent experiment result by inference.

Capture the experiment at that revision, publish immutable artifacts, then stage those exact source/input files, the descriptor, and selected artifact references. Refuse accidental unrelated staged files or changes after capture; allow deliberate selection only with a recorded scope. Serialize KernelLedger Git operations per repository, preserve external Git/index locks, and compare-and-swap the intended ref/HEAD. Validate the staged bytes again before committing.

Git and SQLite cannot form one transaction. Journal states `snapshot_published`, `commit_prepared`, `commit_created`, and `binding_recorded`; recovery locates an existing commit before retrying. Never create a duplicate commit on a lost reply. Pre-commit hooks/signing may mutate/fail: revalidate snapshot files and source hashes before commit creation, then verify the committed tree before binding. If a hook changed included bytes, refuse coherent binding and leave a reviewable recovery result.

The snapshot contains parent/source hashes, not the hash of the commit containing itself. That would be circular. Associate the final commit through its tracked descriptor and a local ledger binding after creation. Git notes or `refs/kernelledger/*` are optional discovery indexes, never the only association; normal clones must work from tracked files alone.

### 10.2 Worktree creation and restore

`experiment worktree add` wraps normal `git worktree add`: choose a commit/branch/tag, locate its selected descriptor, preflight the requested target, materialize exact source/inputs, then restore explicitly. A failed restore leaves a valid Git worktree and a durable failure report. Detached checkouts work; commit requires a branch or an explicit detached-commit policy.

Each worktree gets a unique runtime identity based on repository identity, canonical worktree root, target id, and generation. The logical notebook id can remain shared across experiment branches. Never share mutable kernels or local ledgers between worktrees. Repository history and immutable blob caches can be shared; mutation state cannot.

Portable restore can move to another directory subject to recipe/path mapping. Native process images initially resume only into the qualified captured layout. Add raw-image worktree cloning only after an isolated Linux mount/PID namespace can expose the new worktree at the captured stable path, remap registered dependencies, and reconnect the worker. Without that isolation, changing a CRIU image's working directory does not make it a safe branch clone: refuse native restore into another path or use an explicitly selected portable fallback.

### 10.3 Other Git operations

- `status`, `diff`, `log`, `show`, `blame`, branches, and tags inspect normal files. Add an experiment comparison showing source/input changes, tensor shapes/dtypes/hashes, metrics, state coverage, and compatibility. Tensor values are inspected lazily and only on request.
- Merge, rebase, cherry-pick, and patch application can move snapshots as immutable artifacts, but cannot merge model weights or raw contexts. Resolve the selected experiment record explicitly. Changed source/inputs mark a snapshot stale; recompute a result unless the user intentionally restores the historic state.
- Branch switching, reset, revert, and stash never mutate a live namespace automatically. Mark source/state divergence and require a selected restore before claiming coherence. Offer export-before-switch without installing Git hooks by default. Stashing source alone does not capture unsaved GPU memory.
- Bisect has an explicit runner: restore the commit's selected compatible snapshot or replay its recipe, run a declared probe, and report unsupported/missing-state commits as skipped. No implicit execution while browsing history.
- Push/pull/clone transfer tracked descriptors and any Git-managed payloads. Validate external artifact presence separately; Git success is not proof that an external snapshot is retrievable.
- Moving/removing worktrees consults runtime leases and unpublished state. Stop/detach as explicitly selected, update mappings on a verified move, and refuse destructive removal while required artifacts exist only locally. Ref rewrites do not automatically delete immutable snapshots.

No custom merge driver that combines native images or tensors. Any optional hooks are versioned, opt-in, removable, and do not claim atomicity across arbitrary external Git commands.

## 11. Artifacts, datasets, and storage

New artifact manifest kinds include portable tensor storage, adapter state, dataset/input blobs, native process images, environment/rootfs requirements, and optional compiled caches. Store a SHA-256 over exact bytes plus length, media/kind, format version, and ordered chunk hashes. Payloads are immutable; incremental snapshots reference parents and chunks explicitly.

Add bounded streaming and chunked upload/read/write before capturing multi-GiB VRAM. Existing full-buffer blob/frame paths and 2 GiB defaults cannot be assumed sufficient. Use configurable quotas, admission control, deadlines, and disk/RAM estimates. Chunk manifests are validated transitively; exported bundles can be materialized independently of the donor ledger.

Artifact resolvers support in-repository payloads for small experiments, Git LFS with explicit retrieval verification, user-managed directories, and opt-in remote object stores. Normal Git stores a descriptor when payload transport is external. Credentials, local absolute locations, and signed URLs are not committed. A descriptor lists immutable content ids and logical resolver names. External dataset references need hashes and a materialization recipe; recording a URL alone does not make a snapshot reproducible.

Extend `references()`/blob closure, ledger references, bundle selection/import/export, pinning, and GC together. New GPU/chunk/native manifest references must never be missed because the current scanner recognizes only a small set of hash field names. Active captures/restores/transfers use leases. GC honors local snapshot roots, declared parents, pins, imported manifests, and active leases; report when Git or an external store independently retains data.

Deduplicate repeated bytes by content hash. Randomized encrypted payloads do not promise global deduplication. Test storage growth for unchanged and slightly changed models rather than promising efficient tensor deltas before measurement. Deleting a local artifact never rewrites Git history or deletes remote data implicitly.

## 12. Capture transaction and consistency

One capture operation has a durable id and request digest. Stages are `preflight`, `source_frozen`, `worker_quiesced`, `device_synchronized`, `components_captured`, `artifacts_verified`, `published`, and `worker_resumed` where applicable. Stage outcomes and ownership survive client disconnects.

At an idle boundary, freeze source/inputs, establish one capture epoch, capture all required CPU/GPU/adapter/native components, verify every artifact, and publish through a sole-writer ledger transaction. Native plus portable fallback requires both from the same epoch; never combine a prior model checkpoint with a later process image and call them equivalent.

Do not hold the source mutation lock while waiting on GPU work, tool subprocesses, remote agents, or privilege helpers. Snapshot immutable source copies under the short lock and validate expected revisions before publication. The existing lock order remains acyclic. Edits during capture can continue, but a commit operation refuses mismatched included source rather than attributing the old kernel to new code.

Failure before publication yields no snapshot id, except explicitly recoverable staged artifact ids that are not advertised as snapshots. A published artifact with a lost response is found by operation id. Publication and original-worker recovery are separate durable outcomes: if resume fails after publication, retain the valid snapshot id and report `artifact: published`, `worker: failed|unknown`, and an unsuccessful capture-and-continue operation. Do not redefine the existing CPU checkpoint `failed` outcome or delete the only saved state. A crash after native capture requires checking actual driver/process state; no blind unlock or restart. Incomplete files and unreachable chunks are swept only after leases and recovery journals are reconciled.

Strict capture refuses missing required components, unsupported context resources, unsafe active native work, invalid environment, insufficient capacity, or incompatible multi-rank epoch. Deadlines and cancellation return reasoned outcomes. Never silently reduce native scope or claim a successful snapshot after a required worker-resume step failed.

## 13. Restore transaction and compatibility

Preflight does not execute definitions. It verifies artifact hashes/closure, schema, required source/input tree, backend capabilities, GPU mapping/topology, framework/Python/OS/ABI requirements, rootfs/native dependencies, resources, privilege, and trust. It returns `eligible`, `eligible_with_drift`, `missing_artifacts`, `unsupported`, or `incompatible`, with requested mode and any available fallback.

Choose a mode explicitly: portable, native, or a documented `auto` policy that prioritizes qualified native only when its requirements match. Every actual mode is recorded. A strict native request never falls back silently. A weights-only restore cannot be relabeled full training continuation. `--allow-python-mismatch` never overrides native kernel/driver incompatibility.

Stage into a new candidate worker/process namespace with fresh credentials and a new session id. Portable restore reconstructs registered objects, restores supported CPU values, materializes GPU storage, applies state, then restores RNG/cursor and validates coverage. Native restore rebuilds process/device resources on the Linux target and reconnects through the external broker. No reuse of donor ledger writers or endpoints.

Validate the candidate with registered non-mutating probes before switching the runtime binding. Keep the existing usable worker until admission succeeds where resources permit; otherwise report required destructive replacement explicitly before starting that operation. Successful restore creates an immutable provenance link and runtime generation. Partial admission is visible and requires the selected policy; failures do not replace a healthy runtime silently.

External sockets, open dataset files, shared memory, other processes, mount namespaces, and remote services need registered handling. Native images capture some OS resources, but support is determined by CRIU plus adapter validation; the product never promises arbitrary external systems can be rewound.

## 14. Warm workers, host-RAM suspension, and resource management

Maintain an optional cache of live workers keyed by durable snapshot id, source/input fingerprint, environment/target/device, and runtime generation. On resumed mutation, advance generation and invalidate the entry as an exact representation of its parent snapshot. Reusing the worker must prove liveness through authentication and an epoch challenge; pid/path existence is insufficient.

One mutable worker belongs to one worktree/runtime. A second worktree receives an independent restore even if it starts from the same snapshot. Warm reuse never shares model objects or memory allocations between independent experiments without an explicit immutable-sharing design.

Use per-target VRAM/RAM quotas, idle expiry, LRU eviction, concurrency limits, and device leases. Eviction preserves the selected durable artifact; unsaved state needs capture or an explicit discard. WSL termination, GPU reset, process crash, driver updates, and host restart invalidate the cache. Keep cache entries out of Git, exported manifests, and snapshot completeness claims.

For qualified NVIDIA targets, host-RAM suspended CUDA workers can free GPU resources while keeping CPU state alive. This is a separate non-durable cache state until a full process image is written. AMD suspend/free-resource behavior requires its own measured backend support; do not infer symmetry. Record suspend/resume latency and memory footprint before exposing automatic swapping.

Compiled kernels, autotuning results, CUDA/HIP graph captures, and memory-pool caches have separate compatibility keys. Invalidate/rebuild them for portable restore unless an adapter proves restoration. A memory-allocation diagnostic snapshot is not serialized tensor contents or an executable context checkpoint.

## 15. Multi-GPU, distributed, and unusual workloads

Support local single-GPU first. Local multi-GPU requires a manifest mapping logical devices to actual UUIDs, synchronization across all devices, and admission based on destination topology/capacity. A checkpoint taken from only one participating device is partial unless explicitly declared sufficient.

Distributed training uses a coordinator, a barrier at a supported training boundary, one capture epoch for all ranks, collective quiescence, per-rank artifacts, and all-or-nothing manifest publication. Record world size, rank mapping, sharding, optimizer placement, and data cursor. Native NCCL/RCCL sockets, IPC, and collectives need independent qualification; rank snapshots are not interchangeable with portable distributed checkpoint files.

Multi-node process snapshots remain experimental until reconnection of the full rank group and topology is proven. A failed rank capture aborts the group and resumes/stops participants consistently. Elastic world-size restore uses a framework-supported reshaping adapter and reports drift; raw images do not provide it automatically.

For long-running custom kernels, active autograd tapes, custom allocator/extensions, graphics/compute interop, video contexts, DMA/RDMA resources, UVM/shared memory, external inference KV caches, or live data streams: define adapter capture boundaries and coverage. Native capture may drain work rather than freeze an arbitrary GPU instruction. Each unsupported resource has a reason and a selected fallback, rather than being omitted under a generic success label.

## 16. Proposed CLI and protocol surface

Command names are draft design, not existing functionality:

```text
kl -C <notebook> gpu inspect
kl -C <notebook> gpu probe --target <target>
kl -C <notebook> experiment capture --mode portable|native|both --require <component>
kl -C <notebook> experiment show <snapshot-id>
kl -C <notebook> experiment compare <left> <right>
kl -C <notebook> experiment preflight <snapshot-id> --target <target> --mode <mode>
kl -C <notebook> experiment restore <snapshot-id> --target <target> --mode <mode>
kl -C <notebook> experiment commit <snapshot-id> --message <message>
kl -C <notebook> experiment worktree add <path> --ref <ref> --target <target> --mode <mode>
kl -C <notebook> artifact verify <snapshot-id>
kl -C <notebook> artifact fetch <snapshot-id> --resolver <resolver>
kl -C <notebook> cache list
kl -C <notebook> target add <name> --kind local|wsl|ssh-linux
```

Provide matching supervisor operations and helper inspection methods. Every mutation uses durable operation ids, stable retry keys, request digests, progress/cancellation, and lookup after disconnect. Read-only methods remain worker-free. Progress and diagnostics expose target, mode, captured scope, drift, stale-source state, and fallback availability without exposing credentials.

Use capability negotiation for GPU and target features; keep version 4 clients functional for CPU-only operations. A client unable to render native/portable coverage must refuse that operation rather than showing its old generic Complete label. Desktop behavior requires a new frontend plan covering target selection, state coverage, restore mode, source/runtime mismatch, cache lifetime, and progress/errors.

## 17. Format versions and migration proposal

Reserve the following draft schema changes for specification and fixtures before implementation:

- `notebook.json` remains schema 1; it does not carry machine-local GPU state.
- Existing typed CPU graph remains readable as schema 1. A portable graph with tensor/adapter reference kinds uses checkpoint schema 2; older loaders refuse it.
- New experiment and GPU artifact manifests begin at schema 1, with explicit kind/backend/coverage and references to typed graphs or native images.
- Bundles and published descriptors use schema 2 when they include composite experiments/artifacts. Keep reading schema 1 and exporting CPU-only schema 1 where sufficient.
- Ledger schema 3 adds experiment snapshots/components, artifact/chunk references, target/runtime generations, leases, Git bindings, and staged-operation recovery. Final DDL and foreign keys are a G1 deliverable, not invented dynamically by backends.
- Worker protocol 2 adds safe-point/reconnect/adapter operations. A new supervisor may serve protocol 1 workers only for their advertised CPU scope. Legacy protocol 2, desktop API 2, and helper API 2 carry composite-operation semantics, with version 1 compatibility for existing operations.
- Registry schema 2 adds target/runtime identity hints without credentials. Unknown newer entries remain ignorable hints.

Plan numbers, artifact schemas, protocol versions, backend versions, and package releases remain independent. Publish hand-validated runtime contracts and JSON Schemas together; update contract generators and Python/Rust fixtures. A schema 2 ledger migrates explicitly to schema 3 with a verified backup. Schema 1 uses the existing migration first. No backend edits SQLite directly and no downgrade discards GPU history silently.

When adopted, this draft replaces runtime version 4 sections 2 (scope), 3/6 (targets and worker), 4 (files), 8/9 (protocol/ledger), 11/12 (GPU checkpoint/restore), 13/14 (artifacts/Git), 15 (native security), 17/18 (compatibility/dependencies), and 19/20 (phases/tests) only as specified here. Version 4 guarantees remain in force for their original operations. The adopted successor must integrate these changes into the full runtime document and archive version 4 under the plan rules.

## 18. Security and native-image handling

Portable data decoding preserves the existing no-pickle rule. Reconstruction recipes and external framework loaders execute only under explicit provenance/trust policy. Imported artifacts are hash-verified before any restore operation. Hashes prove content identity, not that executing that content is safe.

Native images can contain credentials, environment secrets, non-user globals, cached input, and external-resource state beyond the graph denylist. Do not claim named-binding exclusions scrub raw process memory. Native export is a separate sensitive operation with scope disclosure, encryption support, local-only policy by default, and destination-specific authorization. Git commit integration rejects staging raw native images by default; use an encrypted payload or an approved external artifact reference, with explicit policy for any plaintext export. Encryption keys and transport credentials never go into Git; offline readers can still inspect a non-secret envelope. Define whether plaintext identity hashes may be disclosed before publishing encrypted metadata.

Constrain privileged helpers to owned registered processes and validated operations, namespaces, image paths, size limits, and plugin/tool allowlists. Recheck target process identity/start time and authenticated runtime generation to avoid PID reuse. Enforce the original unprivileged worker identity, permitted capabilities, and namespace boundaries on restore; an image cannot request a host-root notebook process. Deny arbitrary `sudo` commands or restore destinations. Never run notebook code as root to avoid implementing the helper boundary.

Do not checkpoint the privileged helper or broker. Revoke donor transport credentials at the capture handshake and obtain fresh credentials through the external broker on restore/resume. Clear active transport references where possible, but assume raw memory may retain revoked strings; never claim heap redaction or removal of user secrets. Validate paths, symlinks, archive entries, filesystem mounts, and device mapping before privileged actions. Enforce allocation and decompression limits on imported metadata/chunks.

Artifact stores and remote targets are opt-in. Access policies remain local configuration. Imported native images can execute arbitrary saved process code with powerful driver access; a portable trust choice cannot silently authorize privileged native resume.

## 19. Delivery phases and exit gates

Do not assign release dates before feasibility and storage measurements. Native/WSL/VM qualification can proceed independently after the shared contracts exist, but no phase bypasses its prerequisites.

**G0 — Feasibility and hardware inventory.** Identify available Windows NVIDIA, supported AMD Windows/WSL, bare-metal Linux NVIDIA, and AMD native test targets. Record existing runtime coverage, exact OS/driver/framework/tool revisions, and disposable portable/native probes. Timebox each native/WSL feasibility spike to five engineer-days per candidate configuration; end with qualified candidate, experimental evidence, or explicit unsupported result. Evidence includes restored GPU computation, not just tool detection. No physical GPU access means that native feature remains unqualified.

**G1 — Experiment contracts and artifact streaming.** Freeze manifests, schema/DDL migrations, coverage labels, target identity, compatibility rules, capture epochs, tensor byte formats, limits, operation recovery, and native security boundaries. Adopt the full runtime successor before merging changed public runtime contracts; G0 prototypes may remain isolated experiments while this draft is reviewed. Implement content-addressed chunks/leases and bundle/GC closure first. Exit: schema fixtures, tamper/limit/crash tests, backward CPU compatibility, and large synthetic payload streaming pass without GPUs.

**G2 — Portable PyTorch baseline.** Dense tensors plus registered model/optimizer/step state, then scheduler/scaler/RNG/data cursor and alias/view support. Deliver native Windows and native Linux capture/restore. Exit: small real training continues from a restored boundary; required omissions refuse, optional ones report partial; a clean worktree/clone restores without donor `.kl`; CPU inspection works without PyTorch. Add cooperative in-cell hooks only after idle-boundary capture is qualified.

**G3 — Git and worktree association.** Snapshot source/input capture, selected descriptors, coherent staging/commit, repository locks, ref association journals, stale-state detection, comparison, clone/push artifact verification, rollback, and worktree lifecycle. Exit: commit/source/snapshot stay coherent under concurrent edits, Git hooks/ref changes, lost replies, merge/cherry-pick/rebase, and failure during worktree restore. This phase depends on G1/G2.

**G4 — Warm caches and device resource policy.** Authenticated generations, per-worktree ownership, VRAM/RAM budgets, leases, eviction, and explicit discard/capture handling. NVIDIA host-RAM suspend is a separately qualified enhancement. Exit: two experiments never mutate one shared worker; reboot/reset/stale pid cannot resurrect a false cache hit. Durable restore remains available after eviction.

**G5 — Windows/WSL2 portable bridge.** Distribution/interpreter discovery, agent bootstrap, protocol forwarding, source synchronization, Linux-owned ledgers, output/input/cancellation, artifact retrieval, and crash recovery. Exit: a Windows client runs and restores GPU training in qualified WSL NVIDIA/AMD configurations and survives proxy/distro disconnects. Native WSL probes are recorded as independent capability results; a failed probe does not block portable WSL delivery.

**G6 — NVIDIA Linux native backend.** Pinned driver/CRIU adapter, worker-only native images, broker reconnect, stable source layout, privilege helper, same-epoch fallback, and capture-and-continue recovery. Exit: donor worker is stopped, a new process resumes saved GPU allocations and supported context objects, authenticates into a new session, and continues computation. Incompatible stacks refuse before replacing a worker. Depends on G0/G1/G2.

**G7 — AMD Linux native backend.** Qualified KFD/plugin path, topology checks, worker transport/bootstrap, security, and failure recovery. Exit: the same native continuation evidence on named real AMD configurations; unsupported Radeon/WSL devices remain explicitly unsupported. Depends on G0/G1/G2 and shared native machinery from G6, not on assuming NVIDIA behavior.

**G8 — Native snapshots in new worktrees.** Per-restore PID/mount isolation, stable captured path, dependency/rootfs materialization, file/socket policies, and concurrent native clones. Exit: two worktrees restored from one image have independent mutable files/processes/GPU allocations and cannot reconnect to each other's supervisor. Without this gate native resumes remain restricted to qualified same-layout restoration.

**G9 — Windows-to-Linux targets and qualified guests.** SSH agent, verified source/artifact transfer, durable remote operations, host-key/credential handling, rank-group cancellation, and optional VM target. Exit: Windows commits/restores a Linux-native experiment on that Linux target, and separately imports its portable fallback into a supported Windows worker. VM qualification remains conditional on available supported hardware and host edition. Depends on G3 plus the relevant qualified backend.

**G10 — Multi-GPU/distributed and adapter expansion.** All-device/rank safe points, world-size mappings, sharded state, collectives, and selected additional frameworks/resources. Exit: all-or-nothing multi-rank publication, failure recovery, and proven continuation for each advertised adapter/topology. Unsupported configurations retain explicit exclusions. Single-GPU release does not claim this phase completed.

**G11 — Product integration and release qualification.** Update implementation status against the adopted runtime plan, publish the dependent frontend version before implementing its changed requirements, complete UI/CLI documentation and clean-machine setup, refresh hardware evidence, and measure performance. Exit: every advertised platform/mode has real hardware evidence, packaged optional dependencies, failure recovery, user-visible coverage, and the acceptance scenarios below. Portable support can ship first; native, distributed, and VM profiles receive independent release statuses.

## 20. Acceptance scenarios and evidence

Store qualification results under `benchmarks/gpu/` and `docs/gpu/qualification/`, with exact stack, hardware, commands, artifact ids, measurements, and known limitations. Proposed ids are stable within this draft.

- **GS-01:** Existing CPU tests, schema 1 bundles, legacy clients, trust behavior, source transactions, and new schema migrations pass on Windows and Linux without any GPU packages.
- **GS-02:** GPU snapshots can be listed, compared by metadata, and hash-verified on a machine without the framework or a GPU; inspection never initializes a worker/device.
- **GS-03:** Model, optimizer slots, scheduler/scaler, gradients when required, step, sampler cursor, and RNG coverage round-trip on actual NVIDIA and actual AMD targets. A weights-only fixture cannot pass the full-training gate.
- **GS-04:** Dense dtype fixtures, empty tensors, noncontiguous views, tied weights, shared storage, and alias-dependent mutation match their advertised semantics; unsupported kinds have exact reasons.
- **GS-05:** Checkpointing during uncoordinated work refuses. A cooperative hook captures between complete training steps; no half-updated CPU/GPU combination is published.
- **GS-06:** A clean clone imports committed/exported artifacts and resumes training after the donors stop. No source, credentials, or runtime database are fetched implicitly from donor `.kl`.
- **GS-07:** Baseline, hyperparameter, and dataset worktrees have independent kernels, device allocations, local ledgers, and reproducible selected snapshot associations.
- **GS-08:** Pre-capture edits that never ran, concurrent source edits, unrelated index changes, hook mutations, signing failure, HEAD/ref races, and dropped commit replies leave a recoverable coherent result or a clear refusal; never a falsely bound commit.
- **GS-09:** Merge/rebase/cherry-pick/reset/revert/stash mark changed source/state associations stale. Explicit historic restore remains possible; no native/tensor merge is inferred.
- **GS-10:** Offline/missing external artifacts, corrupt chunks, incomplete parent chains, disk full, allocation overflow, and quota exhaustion fail before executable restore/admission.
- **GS-11:** GC/export/import retain every GPU/native/chunk reference and active lease; an unchanged snapshot deduplicates measurable bytes without depending on donor runtime history.
- **GS-12:** Qualified NVIDIA native capture restores supported allocations/context resources after the donor exits, reconnects with fresh credentials, and records a new session. UVM/IPC/graph/interop fixtures either qualify individually or refuse precisely.
- **GS-13:** Qualified AMD native capture provides the same evidence on named hardware, with capability/firmware/VRAM checks enforced. An unqualified GPU never receives a generic supported label.
- **GS-14:** Incompatible native driver/kernel/GPU/topology/source layouts refuse before healthy-runtime replacement; portable fallback requires an explicit selected policy and records the actual mode.
- **GS-15:** Native-image worktree clones cannot access another worktree's writable source/input/runtime files or reconnect to its supervisor. Same-path restriction remains enforced before namespace isolation ships.
- **GS-16:** Kill the supervisor, worker, broker, driver tool, resolver, and client at each capture/restore/commit journal boundary. Published artifacts are complete or identified failures; unknown worker state is never shown as healthy.
- **GS-17:** Warm-cache reuse authenticates generation and exact source/state identity. Mutation, eviction, reboot, GPU reset, and WSL shutdown cannot create false hits or discard uncaptured state silently.
- **GS-18:** Windows-native portable capture/restore runs on qualified framework/GPU combinations. A native Linux image is inspectable but not presented as locally executable Windows state.
- **GS-19:** Windows/WSL bridge handles spaces, Unicode, long paths, case collisions, symlinks, EOLs, drive/UNC mappings, distribution/user identity, and source revision races. Windows and Linux never write the same ledger concurrently.
- **GS-20:** Windows proxy crash, Linux agent restart, WSL termination, sleep/reboot, or a dropped channel has an explicit runtime outcome; cancellation reaches the Linux process tree, and reconnect does not duplicate operations.
- **GS-21:** WSL GPU compute success does not qualify native GPU snapshots. Tests record actual native checkpoint/restore success or a reasoned unsupported result for both vendors.
- **GS-22:** A Windows client restores a Linux-native image on a qualified remote target with verified host identity and artifact closure; no local root process or raw Linux-to-Windows translation is involved.
- **GS-23:** Portable Linux-to-Windows, Windows-to-Linux, CPU-to-GPU, and qualified cross-vendor restoration preserve supported application bytes and report environment/RNG/numeric drift. Device incompatibility cannot be bypassed by a Python-version flag.
- **GS-24:** Multi-device/rank capture has one epoch and complete rank closure. Rank/tool/link failure aborts the whole required scope. Elastic restore occurs only through a qualified reshaping adapter.
- **GS-25:** Native images obey sensitive export policy, fresh credentials, trust boundaries, scoped helper authorization, process identity, and import limits. Binding denylist tests never imply raw-memory redaction.
- **GS-26:** Malicious recipes/artifacts do not execute during inspection/preflight; arbitrary pickle/reduce payloads never enter the typed decoder. Imported native resume has its own explicit trust/privilege boundary.
- **GS-27:** Measured capture pause, GPU synchronization, CPU staging, snapshot bytes, dedup growth, write/upload time, cold/native/warm restore time, peak RAM/VRAM, and recovery time are recorded for small and representative multi-GiB models. Set latency/storage targets after G0/G2 measurements.
- **GS-28:** CLI and desktop negotiated capabilities show the same target, mode, coverage, drift, progress, and operation outcome. Older clients safely refuse composite operations they cannot represent.

Ordinary CI tests schemas, synthetic large streams, fake backend state machines, Git/worktree transactions, bridge framing, and failure injection. Dedicated hardware jobs qualify NVIDIA Windows/Linux/WSL and AMD Linux plus supported Windows/WSL profiles. Simulation validates orchestration only; it never substitutes for real GPU qualification. Device inventory and access are explicit G0 dependencies.

## 21. Decisions to close during implementation

- **GD-01:** Exact hardware, driver, kernel, framework, and CRIU/plugin revisions for the first qualified profiles. Owner: backend maintainers; gate: G0.
- **GD-02:** Tensor storage/alias encoding, dtype allowlist, adapter registration/factory contract, and RNG compatibility classes. Owner: worker/codec implementation; gate: G1/G2.
- **GD-03:** Artifact chunk size, memory limits, resolver interface, encryption envelope, key handling, and external GC policy. Owner: artifact implementation; gate: G1, measured in G2.
- **GD-04:** Native worker safe-point/reconnect design, stable mount layout, credential removal, and scoped privileged-helper boundary. Owner: native runtime implementation; gate: G0/G1, proven in G6/G8.
- **GD-05:** WSL transport, Windows-to-Linux source authority, path mapping, and whether each vendor's native WSL route is qualified. Owner: platform bridge implementation; gate: G0/G5.
- **GD-06:** Linux target packaging/bootstrap, SSH credential policy, target liveness, and qualified VM availability. Owner: target implementation; gate: G9.
- **GD-07:** Capture-and-commit handling of hooks/signing and external Git races. Owner: Git integration; gate: G3.
- **GD-08:** Distributed barrier/sharding/collective support, world-size policy, and initial additional adapters. Owner: framework integrations; gate: G10.
- **GD-09:** Frontend requirement ids, capability/coverage presentation, and optional-mode release statuses. Owner: frontend plan author; gate: G11.

Do not leave critical consistency/security contracts implicit while coding. A failed feasibility gate narrows the qualified profile, records evidence, and ships the covered portable/control route; it does not produce a promise of raw context capture on unavailable hardware.

## 22. Frontend impact and adoption

The current frontend version 3 remains a client of runtime version 4. Adoption requires a frontend successor that explicitly revises/adds requirements around runtime discovery/management, interpreter/environment choice, Git source/runtime mismatch, checkpoint reports, trust, restore/Continue, bundles, and settings. Existing `F-GOAL-01` inspection safety and `F-GOAL-03` editing responsiveness remain mandatory. `F-GOAL-04` currently excludes remote execution; the successor must change that scope deliberately for G9 rather than exposing an undocumented remote feature.

Publish full runtime version 5 and archive version 4 only when the draft is adopted under the plan rules. Update compatibility mappings, coverage reports, schemas, fixture generators, CLI/protocol docs, worktree examples, and frontend dependencies together. This draft adds a plan; it does not report the proposed GPU guarantees as shipped.
