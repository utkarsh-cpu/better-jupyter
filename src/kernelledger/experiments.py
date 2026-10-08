"""Experimental explicit PyTorch training snapshots, outside the v4 worker API.

Factories are supplied by the caller at restore, never deserialized from bytes.
The caller owns the idle training boundary and dataset cursor/drain rule.
"""

import os
import platform
import random
import re
import shutil
import tempfile
import uuid
from collections import OrderedDict
from collections.abc import Callable, Mapping
from pathlib import Path, PurePosixPath
from typing import Any, cast

from .artifacts import STATE_SCHEMA, ArtifactStore
from .blobs import check_hash
from .gpu.portable import PortableEncoder, inspect_state, restore_values


def _class(obj: Any) -> str:
    return type(obj).__module__ + "." + type(obj).__qualname__


def _relative(name: Any) -> str:
    if not isinstance(name, str):
        raise ValueError("invalid tracked experiment path")
    path = PurePosixPath(name)
    if not name or path.is_absolute() or ".." in path.parts or "\\" in name or ":" in name:
        raise ValueError("invalid tracked experiment path")
    if path.as_posix() != name:
        raise ValueError("noncanonical tracked experiment path")
    for part in path.parts:
        if (
            part in {".git", ".kl", ".kl-gpu-experimental"}
            or part.endswith((".", " "))
            or any(character in part for character in '<>"|?*')
            or part.split(".")[0].upper()
            in {
                "CON",
                "PRN",
                "AUX",
                "NUL",
                *(prefix + str(number) for prefix in ("COM", "LPT") for number in range(1, 10)),
            }
        ):
            raise ValueError("nonportable or runtime-private experiment path")
    return name


def inspect_experiment(
    store: ArtifactStore, digest: str, *, max_bytes: int = 1024**3
) -> dict[str, Any]:
    value = store.metadata(digest)
    fields = {
        "schema",
        "kind",
        "epoch",
        "step",
        "recipe",
        "files",
        "state",
        "requirements",
        "coverage",
        "source_coherence",
        "classes",
        "optimizer_parameters",
    }
    if (
        set(value) != fields
        or value.get("schema") != STATE_SCHEMA
        or value.get("kind") != "training-experiment"
    ):
        raise ValueError("unsupported training experiment manifest")
    if not isinstance(value["epoch"], str) or not value["epoch"]:
        raise ValueError("invalid experiment epoch")
    if type(value["step"]) is not int or value["step"] < 0:
        raise ValueError("invalid training step")
    recipe = value["recipe"]
    if not isinstance(recipe, dict) or set(cast(dict[str, Any], recipe)) != {"source", "factory"}:
        raise ValueError("invalid reconstruction recipe")
    recipe = cast(dict[str, Any], recipe)
    _relative(recipe["source"])
    if not isinstance(recipe["factory"], str) or not recipe["factory"].isidentifier():
        raise ValueError("invalid reconstruction factory")
    if not isinstance(value["files"], dict) or recipe["source"] not in value["files"]:
        raise ValueError("missing recipe source")
    file_map = cast(dict[str, Any], value["files"])
    for name, reference in file_map.items():
        _relative(name)
        if not isinstance(reference, dict) or set(cast(dict[str, Any], reference)) != {"blob"}:
            raise ValueError("invalid experiment file reference")
        reference = cast(dict[str, Any], reference)
        check_hash(reference["blob"])
        if store.verify(reference["blob"])["kind"] != "input":
            raise ValueError("invalid experiment input artifact")
    paths = sorted(name.casefold() for name in file_map)
    path_set = set(paths)
    if len(paths) != len(path_set) or any(
        parent.as_posix() in path_set
        for path in paths
        for parent in PurePosixPath(path).parents
        if parent.as_posix() != "."
    ):
        raise ValueError("colliding experiment paths")
    if value["source_coherence"] != "unverified":
        raise ValueError("prototype cannot attest executed source coherence")
    if value["coverage"] != {
        "required": "complete",
        "namespace": "not-captured",
        "data_loader": "caller-supplied-cursor",
        "native": "not-captured",
    }:
        raise ValueError("incomplete required training scope")
    if not isinstance(value["requirements"], dict) or set(
        cast(dict[str, Any], value["requirements"])
    ) != {
        "python",
        "torch",
        "vendor",
        "device_count",
    }:
        raise ValueError("invalid experiment requirements")
    requirements = cast(dict[str, Any], value["requirements"])
    if requirements["vendor"] not in ("cpu", "nvidia", "amd"):
        raise ValueError("invalid experiment vendor")
    for field in ("python", "torch"):
        if not isinstance(requirements[field], str):
            raise ValueError("invalid experiment version")
    if (
        type(requirements["device_count"]) is not int
        or not 0 <= requirements["device_count"] <= 256
    ):
        raise ValueError("invalid experiment device count")
    if (
        not isinstance(value["classes"], dict)
        or not all(
            isinstance(k, str) and isinstance(v, str)
            for k, v in cast(dict[Any, Any], value["classes"]).items()
        )
        or not {"model", "optimizer"} <= value["classes"].keys()
        or not value["classes"].keys() <= {"model", "optimizer", "scheduler", "scaler"}
    ):
        raise ValueError("invalid registered experiment classes")
    groups = value["optimizer_parameters"]
    if not isinstance(groups, list) or not all(
        isinstance(group, list) and all(isinstance(name, str) for name in cast(list[Any], group))
        for group in cast(list[Any], groups)
    ):
        raise ValueError("invalid optimizer parameter mapping")
    if not isinstance(value["state"], dict) or set(cast(dict[str, Any], value["state"])) != {
        "blob"
    }:
        raise ValueError("invalid training state reference")
    inspect_state(store, check_hash(value["state"]["blob"]), max_bytes=max_bytes)
    store.verify_closure(digest)
    return value


def materialize_source(store: ArtifactStore, digest: str, destination: Path) -> None:
    """Restore captured file bytes to a fresh directory without executing recipes."""
    manifest = inspect_experiment(store, digest)
    destination = Path(destination)
    if destination.exists():
        raise ValueError("source destination exists")
    with store.lease(digest):
        destination.parent.mkdir(parents=True, exist_ok=True)
        stage = Path(tempfile.mkdtemp(dir=destination.parent))
        try:
            for name, reference in manifest["files"].items():
                path = stage / name
                path.parent.mkdir(parents=True, exist_ok=True)
                with path.open("xb") as stream:
                    for chunk in store.read_chunks(reference["blob"]):
                        stream.write(chunk)
                    stream.flush()
                    os.fsync(stream.fileno())
            os.rename(stage, destination)
        finally:
            if stage.exists():
                shutil.rmtree(stage)


def _parameters(model: Any, optimizer: Any) -> list[list[str]]:
    names = {id(parameter): name for name, parameter in model.named_parameters()}
    try:
        return [
            [names[id(parameter)] for parameter in group["params"]]
            for group in optimizer.param_groups
        ]
    except KeyError:
        raise ValueError("optimizer refers to parameters outside the registered model") from None


def validate_experiment_path(name: Any) -> str:
    """Validate a portable tracked path for explicit experiment registration."""
    return _relative(name)


def validate_training_parameters(model: Any, optimizer: Any) -> list[list[str]]:
    """Validate optimizer ownership and return its registered parameter groups."""
    return _parameters(model, optimizer)


def _model_layout(state: Any, torch: Any) -> dict[str, Any]:
    storages: dict[tuple[str, int], int] = {}
    result: dict[str, Any] = {}
    for name, tensor in state.items():
        if isinstance(tensor, torch.Tensor):
            key = (str(tensor.device), tensor.untyped_storage()._cdata)
            identity = storages.setdefault(key, len(storages))
            result[name] = (
                identity,
                str(tensor.dtype),
                tuple(tensor.shape),
                tuple(tensor.stride()),
                tensor.storage_offset(),
            )
    return result


def capture_training(
    store: ArtifactStore,
    *,
    model: Any,
    optimizer: Any,
    step: int,
    cursor: Any,
    source_root: Path,
    files: list[str],
    recipe_source: str,
    factory: str,
    scheduler: Any = None,
    scaler: Any = None,
    boundary: str,
    max_bytes: int = 1024**3,
) -> str:
    with store.staging() as owner:
        return _capture_training(
            store,
            model=model,
            optimizer=optimizer,
            step=step,
            cursor=cursor,
            source_root=source_root,
            files=files,
            recipe_source=recipe_source,
            factory=factory,
            scheduler=scheduler,
            scaler=scaler,
            boundary=boundary,
            max_bytes=max_bytes,
            owner=owner,
        )


def _capture_training(
    store: ArtifactStore,
    *,
    model: Any,
    optimizer: Any,
    step: int,
    cursor: Any,
    source_root: Path,
    files: list[str],
    recipe_source: str,
    factory: str,
    scheduler: Any = None,
    scaler: Any = None,
    boundary: str,
    max_bytes: int = 1024**3,
    owner: str,
) -> str:
    if boundary != "idle":
        raise ValueError("training capture requires an application-owned idle boundary")
    if type(step) is not int or step < 0 or cursor is None:
        raise ValueError("required step and sampler cursor are missing")
    import numpy as np
    import torch

    torch_module: Any = torch
    if not isinstance(model, torch_module.nn.Module) or not isinstance(
        optimizer, torch_module.optim.Optimizer
    ):
        raise ValueError("model and optimizer must be explicitly registered PyTorch objects")
    if recipe_source not in files or not factory.isidentifier():
        raise ValueError("required tracked reconstruction recipe is missing")
    parameter_names = _parameters(model, optimizer)
    source_root = Path(source_root).resolve()
    captured_files: dict[str, Any] = {}
    # Freeze source before device work. Reject symlink escapes and ambiguous names.
    for name in files:
        _relative(name)
        path = source_root / name
        resolved = path.resolve(strict=True)
        if not resolved.is_relative_to(source_root) or path.is_symlink():
            raise ValueError("experiment source escapes its root")
        with resolved.open("rb") as stream:
            captured_files[name] = {"blob": store.put_stream(stream, kind="input", owner=owner)}
    model_state = model.state_dict()
    numpy_rng = np.random.get_state()
    state: dict[str, Any] = {
        "model": dict(model_state),
        "model_metadata": dict(getattr(model_state, "_metadata", {})),
        "optimizer": optimizer.state_dict(),
        "step": step,
        "cursor": cursor,
        "training": {name: module.training for name, module in model.named_modules()},
        "gradients": {name: parameter.grad for name, parameter in model.named_parameters()},
        "requires_grad": {
            name: parameter.requires_grad for name, parameter in model.named_parameters()
        },
        "python_rng": random.getstate(),
        "numpy_rng": (numpy_rng[0], numpy_rng[1].tolist(), *numpy_rng[2:]),
        "torch_rng": torch_module.get_rng_state(),
        "device_rng": [],
        "settings": {
            "deterministic": torch_module.are_deterministic_algorithms_enabled(),
            "warn_only": torch_module.is_deterministic_algorithms_warn_only_enabled(),
            "cudnn_benchmark": torch_module.backends.cudnn.benchmark,
            "cudnn_deterministic": torch_module.backends.cudnn.deterministic,
            "matmul_tf32": torch_module.backends.cuda.matmul.allow_tf32,
            "cudnn_tf32": torch_module.backends.cudnn.allow_tf32,
        },
    }
    registered = {"model": model, "optimizer": optimizer}
    if scheduler is not None:
        if scheduler.optimizer is not optimizer:
            raise ValueError("scheduler is not bound to the registered optimizer")
        state["scheduler"] = scheduler.state_dict()
        registered["scheduler"] = scheduler
    if scaler is not None:
        state["scaler"] = scaler.state_dict()
        registered["scaler"] = scaler
    devices: set[int] = set()
    seen: set[int] = set()

    def find_devices(value: Any) -> None:
        if id(value) in seen:
            return
        seen.add(id(value))
        if isinstance(value, torch_module.Tensor):
            if value.device.type == "cuda":
                devices.add(value.device.index)
        elif isinstance(value, dict):
            for child in cast(dict[Any, Any], value).values():
                find_devices(child)
        elif isinstance(value, (tuple, list)):
            for child in cast(Any, value):
                find_devices(child)

    find_devices(state)
    if len(devices) > 1:
        raise ValueError("multi-device training requires a qualified mapping adapter")
    for device_index in sorted(devices):
        torch_module.cuda.synchronize(device_index)
        state["device_rng"].append(torch_module.cuda.get_rng_state(device_index))
    device_count = len(devices)
    gradient_ids = {
        id(gradient) for gradient in state["gradients"].values() if gradient is not None
    }
    state_hash = PortableEncoder(
        store, max_bytes=max_bytes, owner=owner, gradient_ids=gradient_ids
    ).capture(state)

    manifest = {
        "schema": STATE_SCHEMA,
        "kind": "training-experiment",
        "epoch": uuid.uuid4().hex,
        "step": step,
        "recipe": {"source": recipe_source, "factory": factory},
        "files": captured_files,
        "state": {"blob": state_hash},
        "requirements": {
            "python": platform.python_version(),
            "torch": torch_module.__version__,
            "vendor": "amd"
            if device_count and torch_module.version.hip
            else "nvidia"
            if device_count
            else "cpu",
            "device_count": device_count,
        },
        "coverage": {
            "required": "complete",
            "namespace": "not-captured",
            "data_loader": "caller-supplied-cursor",
            "native": "not-captured",
        },
        "source_coherence": "unverified",
        "classes": {name: _class(obj) for name, obj in registered.items()},
        "optimizer_parameters": parameter_names,
    }
    digest = store.put_state(manifest, owner=owner)
    inspect_experiment(store, digest, max_bytes=max_bytes)
    store.retain(digest)
    return digest


def restore_training(
    store: ArtifactStore,
    digest: str,
    factory: Callable[[], Mapping[str, Any]],
    *,
    trust: bool = False,
    device: str = "cpu",
    rng_policy: str = "strict",
    seed: int | None = None,
    allow_environment_drift: bool = False,
    max_bytes: int = 1024**3,
) -> dict[str, Any]:
    # All data and compatibility checks precede factory invocation.
    manifest = inspect_experiment(store, digest, max_bytes=max_bytes)
    if not trust:
        raise ValueError(
            "training reconstruction executes user factories; explicit trust is required"
        )
    if rng_policy not in ("strict", "reseed") or (rng_policy == "reseed" and type(seed) is not int):
        raise ValueError("an explicit seed is required for reseeding")
    if not isinstance(cast(Any, device), str) or not re.fullmatch(r"cpu|cuda:\d+", device):
        raise ValueError("unsupported restore device")
    import numpy as np
    import torch

    torch_module: Any = torch
    if device != "cpu" and (
        not torch_module.cuda.is_available()
        or int(device.split(":")[1]) >= torch_module.cuda.device_count()
    ):
        raise ValueError("unavailable restore device")
    source = manifest["requirements"]
    vendor = (
        "amd"
        if device.startswith("cuda:") and torch_module.version.hip
        else ("nvidia" if device.startswith("cuda:") else "cpu")
    )
    device_count = 1 if device != "cpu" else 0
    compatible_rng = vendor == source["vendor"] and device_count == source["device_count"]
    if not compatible_rng and rng_policy == "strict":
        raise ValueError("incompatible device RNG; select explicit reseeding")
    drift: list[str] = []
    if source["torch"] != torch_module.__version__ or source["python"] != platform.python_version():
        drift.append("framework/python version changed")
    if not compatible_rng:
        drift.append("device/generator changed")
    if drift and not allow_environment_drift:
        raise ValueError("environment drift requires explicit acceptance: " + "; ".join(drift))
    state = restore_values(store, manifest["state"]["blob"], device="cpu", max_bytes=max_bytes)
    expected_state = {
        "model",
        "model_metadata",
        "optimizer",
        "step",
        "cursor",
        "training",
        "gradients",
        "requires_grad",
        "python_rng",
        "numpy_rng",
        "torch_rng",
        "device_rng",
        "settings",
    } | (set(manifest["classes"]) - {"model", "optimizer"})
    if (
        not isinstance(state, dict)
        or set(cast(dict[str, Any], state)) != expected_state
        or state["step"] != manifest["step"]
    ):
        raise ValueError("incomplete training state")
    state = cast(dict[str, Any], state)
    if len(state["device_rng"]) != source["device_count"]:
        raise ValueError("incomplete device RNG state")
    # Factories create independent objects. No pre-existing healthy runtime is replaced here.
    registered = dict(factory())
    if set(registered) != set(manifest["classes"]) or any(
        _class(registered[name]) != cls for name, cls in manifest["classes"].items()
    ):
        raise ValueError("reconstruction classes differ from the registered experiment")
    model, optimizer = registered["model"], registered["optimizer"]
    model.to(device)
    if _parameters(model, optimizer) != manifest["optimizer_parameters"]:
        raise ValueError("restored optimizer is not bound to the new model parameters")
    if _model_layout(state["model"], torch_module) != _model_layout(
        model.state_dict(), torch_module
    ):
        raise ValueError("restored model storage aliases/layout differ from the captured model")
    model_state = OrderedDict(state["model"])
    model_state._metadata = state["model_metadata"]  # type: ignore[attr-defined]
    model.load_state_dict(model_state, strict=True)
    optimizer.load_state_dict(state["optimizer"])
    modules = dict(model.named_modules())
    parameters = dict(model.named_parameters())
    if (
        set(modules) != set(state["training"])
        or set(parameters) != set(state["gradients"])
        or set(parameters) != set(state["requires_grad"])
    ):
        raise ValueError("restored model scope mismatch")
    for name, module in modules.items():
        module.training = state["training"][name]
    for name, parameter in parameters.items():
        parameter.requires_grad_(state["requires_grad"][name])
        gradient = state["gradients"][name]
        parameter.grad = None if gradient is None else gradient.to(parameter.device)
    for name in ("scheduler", "scaler"):
        if name in registered:
            if name == "scheduler" and registered[name].optimizer is not optimizer:
                raise ValueError("restored scheduler refers to a stale optimizer")
            registered[name].load_state_dict(state[name])
    settings = state["settings"]
    torch_module.use_deterministic_algorithms(
        settings["deterministic"], warn_only=settings["warn_only"]
    )
    torch_module.backends.cudnn.benchmark = settings["cudnn_benchmark"]
    torch_module.backends.cudnn.deterministic = settings["cudnn_deterministic"]
    torch_module.backends.cuda.matmul.allow_tf32 = settings["matmul_tf32"]
    torch_module.backends.cudnn.allow_tf32 = settings["cudnn_tf32"]
    # Apply RNG last: model/optimizer construction may consume randomness.
    if rng_policy == "strict":
        random.setstate(state["python_rng"])
        numpy_rng = state["numpy_rng"]
        np.random.set_state((numpy_rng[0], np.array(numpy_rng[1], dtype=np.uint32), *numpy_rng[2:]))
        torch_module.set_rng_state(state["torch_rng"])
        if device_count:
            torch_module.cuda.set_rng_state(state["device_rng"][0], device=device)
    else:
        random.seed(seed)
        np.random.seed(seed % 2**32)  # type: ignore[operator]
        torch_module.manual_seed(seed)
    return {
        "objects": registered,
        "step": state["step"],
        "cursor": state["cursor"],
        "snapshot": digest,
        "mode": "portable",
        "drift": drift,
        "rng": "restored" if rng_policy == "strict" else "reseeded",
        "reproducibility": "application-state-reconstruction",
    }
