"""Explicit installed plugin bridge for the standalone notebook worker.

Only register() imports PyTorch. This module never writes a notebook ledger.
Captured sources are retained in an owned temporary directory on reconstruction.
"""

import atexit
import importlib.util
import os
import sys
import tempfile
from pathlib import Path
from typing import Any

from ..artifacts import ArtifactStore
from ..experiments import inspect_experiment, materialize_source
from .cooperative import CooperativeCheckpointPlugin

_plugin: CooperativeCheckpointPlugin | None = None
_temporary: tempfile.TemporaryDirectory[str] | None = None
_source: tempfile.TemporaryDirectory[str] | None = None


def register(**kwargs: Any) -> CooperativeCheckpointPlugin:
    """Register exactly one experiment in this worker; pass cooperative constructor fields.

    Use the returned training_step() context for every model/optimizer mutation.
    No notebook bindings or loader state are inferred automatically.
    """
    global _plugin, _temporary
    if _plugin is not None:
        raise ValueError("one training experiment is already registered in this worker")
    temporary = tempfile.TemporaryDirectory(
        prefix="kl-training-worker-", dir=os.environ.get("KL_TRAINING_TEMP_ROOT")
    )
    try:
        plugin = CooperativeCheckpointPlugin(ArtifactStore(Path(temporary.name)), **kwargs)
    except BaseException:
        temporary.cleanup()
        raise
    _temporary, _plugin = temporary, plugin
    return plugin


def status() -> dict[str, Any]:
    return {"registered": _plugin is not None, **(_plugin.status if _plugin else {})}


def capture(destination: str) -> dict[str, Any]:
    if _plugin is None:
        raise ValueError(
            "register a training experiment with kernelledger.gpu.notebook.register first"
        )
    digest = _plugin.checkpoint()
    _plugin.store.export(digest, Path(destination))
    manifest = inspect_experiment(_plugin.store, digest)
    return {"snapshot": digest, "coverage": manifest["coverage"], **status()}


def reconstruct(
    source: str, digest: str, *, trust: bool, device: str
) -> CooperativeCheckpointPlugin:
    """Execute the verified factory only after explicit trust, in a fresh worker."""
    global _plugin, _temporary, _source
    if not trust:
        raise ValueError("training reconstruction requires --trust for captured factory execution")
    if _plugin is not None:
        raise ValueError("training reconstruction requires a fresh worker")
    temporary = tempfile.TemporaryDirectory(
        prefix="kl-training-restore-", dir=os.environ.get("KL_TRAINING_TEMP_ROOT")
    )
    sources = tempfile.TemporaryDirectory(
        prefix="kl-training-source-", dir=os.environ.get("KL_TRAINING_TEMP_ROOT")
    )
    try:
        store = ArtifactStore(Path(temporary.name))
        if store.import_directory(Path(source)) != digest:
            raise ValueError("training selection mismatch")
        manifest = inspect_experiment(store, digest)
        # materialize_source requires a new destination.
        root = Path(sources.name) / "captured"
        materialize_source(store, digest, root)
        recipe = manifest["recipe"]

        def factory() -> Any:
            # Preserve qualified custom Module class names recorded at capture.
            module_name = ".".join(Path(recipe["source"]).with_suffix("").parts)
            if module_name in sys.modules:
                raise ValueError("captured recipe module conflicts with a loaded runtime module")
            spec = importlib.util.spec_from_file_location(module_name, root / recipe["source"])
            if spec is None or spec.loader is None:
                raise ValueError("recipe must be a Python source file")
            module = importlib.util.module_from_spec(spec)
            previous_path = sys.path.copy()
            previous_cwd = Path.cwd()
            sys.path.insert(0, str(root))
            sys.modules[module_name] = module
            try:
                # Relative inputs during reconstruction refer to captured files.
                os.chdir(root)
                spec.loader.exec_module(module)
                return getattr(module, recipe["factory"])()
            except BaseException:
                sys.modules.pop(module_name, None)
                raise
            finally:
                sys.path[:] = previous_path
                os.chdir(previous_cwd)

        plugin = CooperativeCheckpointPlugin.from_snapshot(
            store, digest, factory, source_root=root, trust=True, device=device
        )
    except BaseException:
        sources.cleanup()
        temporary.cleanup()
        raise
    _temporary, _source, _plugin = temporary, sources, plugin
    return plugin


def _cleanup() -> None:
    for temporary in (_source, _temporary):
        if temporary is not None:
            temporary.cleanup()


atexit.register(_cleanup)
