# Python and native Rust backend comparison

This page records the initial debug-build comparison. The newer
[release optimization report](runtime-native-optimization.md) contains the current
interleaved Python/native-before/native-after measurements and verification.

At this initial snapshot, the conclusion was to keep Python as the default. The native owner had a substantial
idle supervisor memory advantage in this local measurement, but its debug build
has significant checkpoint/restore and median status-latency regressions. These
measurements do not qualify a default switch or establish release-build performance.

Both backends use a Python worker for cell execution and Python object encoding
and reconstruction. The Rust GPUI desktop can use either owner. Moving the owner
to Rust does not compile notebook code to Rust or remove the Python dependency.

## Capability comparison

| Area | Python owner | Native Rust owner |
| --- | --- | --- |
| Selection | Default | Explicit `KERNELLEDGER_RUNTIME=native`; installed/bundled daemon required |
| Execution, restart, stop, interrupt | Implemented | Implemented and integration-tested |
| Durable operations, frozen batches, retries | Implemented reference | Implemented core subset; expanded fault/load qualification remains |
| Input ownership and interactive CLI | Implemented | Implemented; disconnect handoff and secret redaction tested |
| Committed feeds and blob ranges | Implemented | Implemented with paging, backlog/frame limits, resync and write deadlines |
| Namespace checkpoint/restore | Implemented reference | Implemented with retained Python encoder/validator; cross-backend tests pass |
| Continue/replay parity | Broader implementation | Full behavior parity remains open; replay is not advertised |
| Pins, GC, bundle/import mutations | Broader implementation | Native ownership is incomplete; not silently routed to a second writer |
| Portable training operations | Implemented APIs, environment/hardware qualification applies | Native training mutations are unavailable and not advertised |
| Offline helper/source | Python implementation | Native supported subset, with explicit Python utility fallback for other helper methods |
| UI integration | Same Rust frontend | Real Windows Run/checkpoint/restore, plus Windows/Linux service lifecycle checks pass |
| Distribution | Existing Python install path | Debug evaluation archives; clean-machine/signing/license-text release gates remain |

## Measured on this Windows machine

Five paired runs, alternating owner order. Each used a fresh disposable notebook,
the same CPython 3.12 interpreter, a 32 MiB bytes object with an alias, and verified
restore identity/content. Status and short-cell timings contain 150 samples per
backend. Other rows contain five samples and show medians, not p95 estimates.

| Median metric | Python | Native Rust debug |
| --- | ---: | ---: |
| Owner startup | 772 ms | 718 ms |
| Status round trip | 1.32 ms | 10.86 ms |
| Empty cell round trip | 42.32 ms | 36.76 ms |
| First 32 MiB checkpoint | 175 ms | 1,600 ms |
| Unchanged checkpoint | 180 ms | 2,004 ms |
| Restore | 1,330 ms | 2,759 ms |
| Idle owner RSS | 32.08 MiB | 9.27 MiB |
| Idle Python worker RSS | 60.89 MiB | 60.83 MiB |

Status p95 was 17.91 ms for Python and 21.93 ms for native; empty-cell p95 was
51.91 ms and 50.46 ms respectively. Startup ranges were 770–1,756 ms and
613–1,733 ms, so five startup samples do not establish a reliable startup win.

The native owner used about 71% less idle owner RSS. That is not a 71% reduction
in whole-application memory: worker, UI, transient utilities, peak/private memory,
and shared-page accounting are outside that number. Native checkpoint latency
was about 9.1 times Python's and restore latency about 2.1 times Python's in this
debug evaluation. No release-build comparison or cause attribution was performed.

OS cache and storage conditions were uncontrolled. This is a small legacy-CLI
workload, not a UI responsiveness, loaded-control, feed, GPU, Linux-performance,
256 MiB, or 1 GiB qualification run. Full source/build/environment identity and
raw samples are in `artifacts/runtime-v6/backend-comparison-windows312.json`.

Reproduce with the interpreter being compared:

```text
python scripts/runtime_compare.py --daemon /absolute/path/kernelledger-daemon --repetitions 5 --controls 30 --mib 32 --out artifacts/runtime-v6/backend-comparison.json
```

## Interpretation

Python is the practical choice for the full feature set and checkpoint-heavy work
today. Native is useful for opt-in evaluation of a smaller supervisor footprint
and the implemented core workflow. Prioritize status latency, checkpoint/restore
profiling, an equivalent release build, and the remaining durability/load gates
before reconsidering the default. Passing tests of different languages/components
must not be interpreted as a comparative reliability score.
