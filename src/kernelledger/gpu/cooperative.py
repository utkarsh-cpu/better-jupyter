"""Opt-in cooperative training checkpoints; importing this module loads no framework.

All registered-object mutations must use training_step(). Background data loaders
must be drained by the supplied callback. Restore belongs in a fresh process
because it restores process-global RNG and framework settings.
"""

import hashlib
import json
import threading
from collections.abc import Callable, Generator, Mapping
from concurrent.futures import Future
from contextlib import contextmanager
from pathlib import Path
from types import MappingProxyType
from typing import Any, cast

from ..artifacts import ArtifactStore, canonical
from ..experiments import (
    capture_training,
    inspect_experiment,
    restore_training,
    validate_experiment_path,
    validate_training_parameters,
)

# RNG and framework settings belong to the process, so separate registrations
# cannot train/capture concurrently within one interpreter.
_PROCESS_GATE = threading.Lock()


class CheckpointBusy(RuntimeError):
    """A training or capture operation already owns the registered objects."""


class CheckpointFaulted(RuntimeError):
    """A failed training step may have left partially mutated application state."""


def _cursor_copy(value: Any) -> Any:
    if value is None:
        raise ValueError("a committed next-batch cursor is required")
    ancestors: set[int] = set()
    nodes = 0

    def validate(child: Any, depth: int) -> None:
        nonlocal nodes
        identity = id(child)
        nodes += 1
        if depth > 32 or nodes > 10000:
            raise ValueError("cursor exceeds nesting/node limits")
        if type(child) in (type(None), bool, int, float, str):
            return
        if type(child) not in (dict, list):
            raise ValueError("cursor must contain only JSON builtin values")
        if identity in ancestors:
            raise ValueError("cyclic cursor")
        ancestors.add(identity)
        try:
            if type(child) is dict:
                mapping = cast(dict[Any, Any], child)
                if any(type(key) is not str for key in mapping):
                    raise ValueError("cursor keys must be strings")
                for item in mapping.values():
                    validate(item, depth + 1)
            else:
                for item in cast(list[Any], child):
                    validate(item, depth + 1)
        finally:
            ancestors.remove(identity)

    validate(value, 0)
    encoded = canonical(value)
    if len(encoded) > 1024**2:
        raise ValueError("cursor exceeds byte limit")
    return json.loads(encoded)


class CooperativeCheckpointPlugin:
    """Register one model/optimizer experiment and capture only at owned safe points.

    UI/control threads may enqueue bounded requests. The training thread services
    them after a successful step, or by explicitly calling service_checkpoints().
    Future callbacks run after the training/capture lock is released.
    """

    def __init__(
        self,
        store: ArtifactStore,
        *,
        objects: Mapping[str, Any],
        source_root: Path,
        files: list[str],
        recipe_source: str,
        factory: str,
        cursor: Any,
        step: int = 0,
        drain: Callable[[], None] | None = None,
        max_bytes: int = 1024**3,
        max_requests: int = 32,
    ):
        registered = dict(objects)
        if not {"model", "optimizer"} <= registered.keys() or not registered.keys() <= {
            "model",
            "optimizer",
            "scheduler",
            "scaler",
        }:
            raise ValueError("register model, optimizer and optional scheduler/scaler only")
        if type(step) is not int or step < 0 or type(max_requests) is not int or max_requests < 1:
            raise ValueError("invalid step/request limit")
        if type(max_bytes) is not int or max_bytes < 1:
            raise ValueError("invalid capture byte limit")
        if (
            not factory.isidentifier()
            or recipe_source not in files
            or len(set(files)) != len(files)
        ):
            raise ValueError("a unique tracked recipe and factory identifier are required")
        for name in files:
            validate_experiment_path(name)
        if drain is not None and not callable(drain):
            raise ValueError("drain must be a callable")
        # Registration is explicit; inspection and entry-point discovery remain stdlib-only.
        import torch

        torch_module: Any = torch
        if not isinstance(registered["model"], torch_module.nn.Module) or not isinstance(
            registered["optimizer"], torch_module.optim.Optimizer
        ):
            raise ValueError("registered model/optimizer must be PyTorch objects")
        validate_training_parameters(registered["model"], registered["optimizer"])
        if (
            "scheduler" in registered
            and getattr(registered["scheduler"], "optimizer", None) is not registered["optimizer"]
        ):
            raise ValueError("scheduler must refer to the registered optimizer")
        if any(obj is None for obj in registered.values()):
            raise ValueError("omit unregistered optional objects instead of passing None")
        self.store = store
        self.objects = MappingProxyType(registered)
        self.source_root = Path(source_root).resolve()
        self.files = tuple(files)
        self.recipe_source = recipe_source
        self.factory = factory
        self.drain = drain
        self.max_bytes = max_bytes
        self.max_requests = max_requests
        self._cursor = _cursor_copy(cursor)
        self._step = step
        self._gate = _PROCESS_GATE
        self._state_lock = threading.Lock()
        self._state = "idle"
        self._pending: list[Future[str]] = []
        self._last_snapshot: str | None = None
        self.restore_report: dict[str, Any] | None = None

    @property
    def status(self) -> dict[str, Any]:
        with self._state_lock:
            return {
                "state": self._state,
                "step": self._step,
                "cursor": _cursor_copy(self._cursor),
                "pending_requests": len(self._pending),
                "snapshot": self._last_snapshot,
                "mode": "portable",
                "namespace": "not-captured",
                "native": "not-captured",
            }

    def _acquire(self, *, allow_faulted: bool = False) -> None:
        if not self._gate.acquire(blocking=False):
            raise CheckpointBusy(
                "training/capture is active; request a checkpoint at the next safe point"
            )
        with self._state_lock:
            state = self._state
        if state == "closed" or (state == "faulted" and not allow_faulted):
            self._gate.release()
            if state == "faulted":
                raise CheckpointFaulted(
                    "failed training step; restore a durable checkpoint in a fresh process"
                )
            raise RuntimeError("checkpoint plugin is closed")

    def _set_state(self, state: str) -> None:
        with self._state_lock:
            self._state = state

    def _take_requests(self) -> list[Future[str]]:
        with self._state_lock:
            pending, self._pending = self._pending, []
        return [future for future in pending if future.set_running_or_notify_cancel()]

    @staticmethod
    def _complete(
        pending: list[Future[str]], digest: str | None, error: BaseException | None
    ) -> None:
        for future in pending:
            if error is not None:
                future.set_exception(error)
            else:
                assert digest is not None
                future.set_result(digest)

    def _capture(self) -> str:
        self._set_state("capturing")
        try:
            if self.drain is not None:
                self.drain()
            digest = capture_training(
                self.store,
                **self.objects,
                step=self._step,
                cursor=self._cursor,
                source_root=self.source_root,
                files=list(self.files),
                recipe_source=self.recipe_source,
                factory=self.factory,
                boundary="idle",
                max_bytes=self.max_bytes,
            )
            with self._state_lock:
                self._last_snapshot = digest
            return digest
        finally:
            self._set_state("idle")

    def checkpoint(self) -> str:
        """Capture now; refuse rather than wait if training/capture is active."""
        self._acquire()
        try:
            return self._capture()
        finally:
            self._gate.release()

    def request_checkpoint(self) -> Future[str]:
        """Queue for the next safe point; this call never runs framework operations."""
        with self._state_lock:
            if self._state == "faulted":
                raise CheckpointFaulted("failed training step")
            if self._state == "closed":
                raise RuntimeError("checkpoint plugin is closed")
            self._pending = [future for future in self._pending if not future.cancelled()]
            if len(self._pending) >= self.max_requests:
                raise RuntimeError("checkpoint request queue is full")
            future: Future[str] = Future()
            self._pending.append(future)
            return future

    def service_checkpoints(self) -> None:
        """Drain queued requests at an idle safe point, coalescing into one snapshot."""
        self._acquire()
        pending: list[Future[str]] = []
        digest = None
        error = None
        try:
            pending = self._take_requests()
            if pending:
                try:
                    digest = self._capture()
                except BaseException as failure:
                    error = failure
                    if not isinstance(failure, Exception):
                        raise
        finally:
            self._gate.release()
            self._complete(pending, digest, error)

    @contextmanager
    def training_step(self, *, cursor: Any) -> Generator[Mapping[str, Any]]:
        """Commit one successful step and its next cursor, then service requests.

        Any exception during training faults the plugin: partially modified model,
        optimizer, RNG or loader state must not be saved as a committed boundary.
        A queued capture failure fails its Future, while the successful training
        step remains committed and can retry capture later.
        """
        next_cursor = _cursor_copy(cursor)
        self._acquire()
        self._set_state("training")
        pending: list[Future[str]] = []
        digest = None
        error = None
        try:
            try:
                yield self.objects
            except BaseException:
                self._set_state("faulted")
                pending = self._take_requests()
                error = CheckpointFaulted("training failed before a committed safe point")
                raise
            with self._state_lock:
                self._step += 1
                self._cursor = next_cursor
                self._state = "idle"
            pending = self._take_requests()
            if pending:
                try:
                    digest = self._capture()
                except BaseException as failure:
                    error = failure
                    if not isinstance(failure, Exception):
                        raise
        finally:
            self._gate.release()
            self._complete(pending, digest, error)

    def close(self) -> None:
        with self._state_lock:
            if self._state == "closed":
                return
        self._acquire(allow_faulted=True)
        try:
            self._set_state("closed")
            pending = self._take_requests()
        finally:
            self._gate.release()
        self._complete(pending, None, RuntimeError("checkpoint plugin closed before safe point"))

    @classmethod
    def from_snapshot(
        cls,
        store: ArtifactStore,
        digest: str,
        factory: Callable[[], Mapping[str, Any]],
        *,
        source_root: Path,
        trust: bool = False,
        device: str = "cpu",
        rng_policy: str = "strict",
        seed: int | None = None,
        allow_environment_drift: bool = False,
        max_bytes: int = 1024**3,
        drain: Callable[[], None] | None = None,
    ) -> "CooperativeCheckpointPlugin":
        """Reconstruct in a fresh process using an explicitly trusted factory.

        Materialize captured files first. Source hash checks precede the factory;
        matching bytes do not attest that the supplied factory executed those bytes.
        """
        manifest = inspect_experiment(store, digest, max_bytes=max_bytes)
        root = Path(source_root).resolve()
        for name, reference in manifest["files"].items():
            path = root / name
            if path.is_symlink() or not path.resolve().is_relative_to(root):
                raise ValueError("unsafe reconstruction source path")
            hasher = hashlib.sha256()
            with path.open("rb") as stream:
                while block := stream.read(1024**2):
                    hasher.update(block)
            if hasher.hexdigest() != store.manifest(reference["blob"])["sha256"]:
                raise ValueError("reconstruction source differs from snapshot")
        if not _PROCESS_GATE.acquire(blocking=False):
            raise CheckpointBusy(
                "cannot restore while another registered training/capture is active"
            )
        try:
            with store.lease(digest):
                report = restore_training(
                    store,
                    digest,
                    factory,
                    trust=trust,
                    device=device,
                    rng_policy=rng_policy,
                    seed=seed,
                    allow_environment_drift=allow_environment_drift,
                    max_bytes=max_bytes,
                )
                plugin = cls(
                    store,
                    objects=report["objects"],
                    source_root=root,
                    files=list(manifest["files"]),
                    recipe_source=manifest["recipe"]["source"],
                    factory=manifest["recipe"]["factory"],
                    cursor=report["cursor"],
                    step=report["step"],
                    drain=drain,
                    max_bytes=max_bytes,
                )
        finally:
            _PROCESS_GATE.release()
        plugin._last_snapshot = digest
        plugin.restore_report = report
        return plugin
