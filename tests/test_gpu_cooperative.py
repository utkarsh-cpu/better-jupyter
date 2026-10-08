import os
import subprocess
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import pytest

from kernelledger.artifacts import ArtifactStore
from kernelledger.experiments import inspect_experiment, materialize_source
from kernelledger.gpu.cooperative import (
    CheckpointBusy,
    CheckpointFaulted,
    CooperativeCheckpointPlugin,
)


@pytest.fixture
def setup(tmp_path):
    torch = pytest.importorskip("torch")
    (tmp_path / "train.py").write_text("def build(): pass\n")
    count = 0

    def build():
        model = torch.nn.Linear(2, 2)
        return {"model": model, "optimizer": torch.optim.SGD(model.parameters(), lr=0.1)}

    def make(**options):
        nonlocal count
        count += 1
        kwargs = dict(
            store=ArtifactStore(tmp_path / str(count)),
            objects=build(),
            source_root=tmp_path,
            files=["train.py"],
            recipe_source="train.py",
            factory="build",
            cursor={"batch": 0},
        )
        kwargs.update(options)
        return CooperativeCheckpointPlugin(**kwargs)

    return make, build


def test_cross_thread_requests_commit_coalesce_and_callbacks_run_unlocked(setup):
    make, _ = setup
    drained = []
    plugin = make(drain=lambda: drained.append(plugin.status["state"]))
    callbacks = []
    with plugin.training_step(cursor={"batch": 1}):
        with ThreadPoolExecutor(max_workers=1) as control:
            first = control.submit(plugin.request_checkpoint).result(timeout=5)
            second = control.submit(plugin.request_checkpoint).result(timeout=5)
        first.add_done_callback(lambda _: (plugin.service_checkpoints(), callbacks.append("done")))
        assert not first.done()
        with pytest.raises(CheckpointBusy):
            plugin.checkpoint()
        with pytest.raises(CheckpointBusy):
            plugin.close()
    digest = first.result(timeout=1)
    assert second.result(timeout=1) == digest
    assert drained == ["capturing"] and callbacks == ["done"]
    assert plugin.status["state"] == "idle" and plugin.status["step"] == 1
    manifest = inspect_experiment(plugin.store, digest)
    assert manifest["step"] == 1


def test_process_global_gate_covers_multiple_registrations(setup):
    make, _ = setup
    first, second = make(), make()
    with first.training_step(cursor={"batch": 1}):
        with pytest.raises(CheckpointBusy):
            second.checkpoint()
        with pytest.raises(CheckpointBusy):
            with second.training_step(cursor={"batch": 1}):
                pass
    assert second.checkpoint()


def test_capture_gate_refuses_mutation_and_late_requests_wait_for_next_boundary(setup):
    make, _ = setup
    entered, release = threading.Event(), threading.Event()

    def drain():
        entered.set()
        assert release.wait(5)

    plugin = make(drain=drain)
    with ThreadPoolExecutor(max_workers=1) as pool:
        capture = pool.submit(plugin.checkpoint)
        try:
            assert entered.wait(5)
            assert plugin.status["state"] == "capturing"
            with pytest.raises(CheckpointBusy):
                with plugin.training_step(cursor={"batch": 1}):
                    pass
            pending = plugin.request_checkpoint()
            assert not pending.done()
        finally:
            release.set()
        assert capture.result(timeout=5)
    assert not pending.done()
    plugin.service_checkpoints()
    assert pending.result(timeout=1)


def test_failed_training_is_faulted_and_never_published(setup):
    make, _ = setup
    plugin = make()
    queued = plugin.request_checkpoint()
    with pytest.raises(ValueError, match="training failure"):
        with plugin.training_step(cursor={"batch": 1}) as objects:
            objects["optimizer"].param_groups[0]["lr"] = 4
            raise ValueError("training failure")
    assert plugin.status["state"] == "faulted"
    assert plugin.status["step"] == 0 and plugin.status["snapshot"] is None
    with pytest.raises(CheckpointFaulted):
        queued.result(timeout=1)
    with pytest.raises(CheckpointFaulted):
        plugin.checkpoint()
    with pytest.raises(CheckpointFaulted):
        plugin.request_checkpoint()
    plugin.close()
    plugin.close()
    assert plugin.status["state"] == "closed"


def test_failed_queued_capture_preserves_successful_training_and_retries(setup):
    make, _ = setup

    def refuse():
        raise OSError("loader did not drain")

    plugin = make(drain=refuse)
    pending = plugin.request_checkpoint()
    with plugin.training_step(cursor={"batch": 1}):
        pass
    with pytest.raises(OSError, match="drain"):
        pending.result(timeout=1)
    assert plugin.status["state"] == "idle" and plugin.status["step"] == 1
    assert plugin.status["snapshot"] is None
    plugin.drain = None
    assert plugin.checkpoint()


def test_quota_failure_does_not_replace_durable_snapshot(setup):
    make, _ = setup
    plugin = make()
    digest = plugin.checkpoint()
    plugin.store.quota_bytes = plugin.store._usage()
    with plugin.training_step(cursor={"batch": 1}) as objects:
        objects["optimizer"].param_groups[0]["lr"] = 0.5
    with pytest.raises(ValueError, match="quota"):
        plugin.checkpoint()
    assert plugin.status["snapshot"] == digest and plugin.status["state"] == "idle"
    inspect_experiment(plugin.store, digest)


def test_cancel_close_and_bounded_request_queue(setup):
    make, _ = setup
    plugin = make(max_requests=1)
    first = plugin.request_checkpoint()
    with pytest.raises(RuntimeError, match="full"):
        plugin.request_checkpoint()
    assert first.cancel()
    second = plugin.request_checkpoint()
    plugin.close()
    with pytest.raises(RuntimeError, match="closed"):
        second.result(timeout=1)
    with pytest.raises(RuntimeError, match="closed"):
        plugin.request_checkpoint()
    assert plugin.status["snapshot"] is None


def test_cancelled_only_request_performs_no_capture(setup):
    make, _ = setup
    plugin = make(drain=lambda: pytest.fail("cancelled requests must not capture"))
    plugin.request_checkpoint().cancel()
    plugin.service_checkpoints()
    assert plugin.status["snapshot"] is None


@pytest.mark.parametrize("boundary", ["service", "training"])
def test_capture_interrupt_propagates_and_releases_gate(setup, boundary):
    make, _ = setup

    def interrupt():
        raise KeyboardInterrupt()

    plugin = make(drain=interrupt)
    pending = plugin.request_checkpoint()
    with pytest.raises(KeyboardInterrupt):
        if boundary == "service":
            plugin.service_checkpoints()
        else:
            with plugin.training_step(cursor={"batch": 1}):
                pass
    with pytest.raises(KeyboardInterrupt):
        pending.result(timeout=1)
    assert plugin.status["state"] == "idle"
    plugin.drain = None
    assert plugin.checkpoint()


def test_cursors_are_frozen_and_invalid_inputs_refuse_before_mutation(setup):
    make, _ = setup
    cursor = {"batch": 0, "shards": [1]}
    plugin = make(cursor=cursor)
    cursor["shards"].append(2)
    status = plugin.status
    status["cursor"]["shards"].append(3)
    assert plugin.status["cursor"] == {"batch": 0, "shards": [1]}
    for bad in (None, {"batch": float("nan")}, {"bad": object()}):
        with pytest.raises(ValueError):
            with plugin.training_step(cursor=bad):
                pytest.fail("invalid cursor entered training")
    cycle = []
    cycle.append(cycle)
    with pytest.raises(ValueError, match="cyclic"):
        with plugin.training_step(cursor=cycle):
            pass
    assert plugin.status["state"] == "idle" and plugin.status["step"] == 0


def test_registration_refuses_stale_optimizer_and_scheduler(setup):
    make, build = setup
    original, other = build(), build()
    with pytest.raises(ValueError, match="outside"):
        make(objects={"model": original["model"], "optimizer": other["optimizer"]})
    with pytest.raises(ValueError, match="scheduler"):
        make(objects={**original, "scheduler": object()})


def test_restore_checks_source_trust_and_active_training_before_factory(setup, tmp_path):
    make, build = setup
    original = make()
    digest = original.checkpoint()
    materialize_source(original.store, digest, tmp_path / "restored")

    def forbidden():
        pytest.fail("preflight invoked the factory")

    kwargs = dict(source_root=tmp_path / "restored")
    with pytest.raises(ValueError, match="trust"):
        CooperativeCheckpointPlugin.from_snapshot(original.store, digest, forbidden, **kwargs)
    with original.training_step(cursor={"batch": 1}):
        with pytest.raises(CheckpointBusy):
            CooperativeCheckpointPlugin.from_snapshot(
                original.store, digest, forbidden, trust=True, **kwargs
            )
    restored = CooperativeCheckpointPlugin.from_snapshot(
        original.store, digest, build, trust=True, **kwargs
    )
    assert restored.status["step"] == 0
    assert restored.objects["model"] is not original.objects["model"]
    (tmp_path / "restored" / "train.py").write_text("changed")
    with pytest.raises(ValueError, match="source differs"):
        CooperativeCheckpointPlugin.from_snapshot(
            original.store, digest, forbidden, trust=True, **kwargs
        )


def test_plugin_module_import_is_framework_free():
    script = """
import sys
class Deny:
    def find_spec(self, fullname, *args):
        if fullname.split('.')[0] in ('torch', 'numpy'):
            raise AssertionError('plugin discovery imported a framework')
sys.meta_path.insert(0, Deny())
import kernelledger.gpu.cooperative
assert 'torch' not in sys.modules and 'numpy' not in sys.modules
"""
    environment = dict(os.environ, PYTHONPATH=str(Path(__file__).parents[1] / "src"))
    result = subprocess.run(
        [sys.executable, "-c", script], env=environment, capture_output=True, text=True, timeout=10
    )
    assert result.returncode == 0, result.stderr
