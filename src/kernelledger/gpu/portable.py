"""Explicit typed PyTorch values; no pickle or framework-native loaders.

Call capture only at an application-owned idle boundary. This prototype does not
change the standalone v4 worker's checkpoint or busy-refusal contract.
"""

import io
import math
import sys
import uuid
from collections import OrderedDict
from typing import Any, cast

from ..artifacts import STATE_SCHEMA, ArtifactStore
from .manifest import DTYPES, MAX_ITEMS, validate_state


class PortableEncoder:
    def __init__(
        self,
        store: ArtifactStore,
        *,
        max_bytes: int = 1024**3,
        owner: str | None = None,
        gradient_ids: set[int] | None = None,
    ) -> None:
        import torch

        self.torch: Any = torch
        self.store, self.max_bytes = store, max_bytes
        self.owner, self.storage_owner = owner, uuid.uuid4().hex
        self.gradient_ids = gradient_ids or set()
        self.objects: dict[str, Any] = {}
        self.storages: dict[str, Any] = {}
        self.identities: dict[int, str] = {}
        self.storage_ids: dict[tuple[str, int], str] = {}

    def _check(
        self,
        obj: Any,
        active: set[int],
        seen: set[int],
        devices: set[int],
        storages: dict[tuple[str, int], int],
        depth: int = 0,
    ) -> None:
        if depth > 64 or len(seen) > MAX_ITEMS:
            raise ValueError("portable object/nesting limit")
        if obj is None or type(obj) in (bool, int, str):
            return
        if type(obj) is float:
            if not math.isfinite(obj):
                raise ValueError("nonfinite portable scalar")
            return
        if id(obj) in active:
            raise ValueError("cyclic portable container")
        if id(obj) in seen:
            return
        seen.add(id(obj))
        torch = self.torch
        if type(obj) in (torch.Tensor, torch.nn.Parameter):
            if obj.grad_fn is not None:
                raise ValueError("autograd graph is not portable state")
            if obj.grad is not None and id(obj.grad) not in self.gradient_ids:
                raise ValueError("attached gradients must be explicitly registered")
            if obj.layout != torch.strided or obj.is_quantized:
                raise ValueError("unsupported tensor layout")
            if obj.device.type not in ("cpu", "cuda"):
                raise ValueError("unsupported tensor device")
            if obj.is_conj() or obj.is_neg():
                raise ValueError("unsupported tensor conjugate/negative view")
            if str(obj.dtype).removeprefix("torch.") not in DTYPES:
                raise ValueError("unsupported tensor dtype")
            storage = obj.untyped_storage()
            key = (str(obj.device), storage._cdata)
            storages[key] = storage.nbytes()
            if sum(storages.values()) > self.max_bytes:
                raise ValueError("portable allocation limit")
            if obj.device.type == "cuda":
                devices.add(obj.device.index)
            return
        if type(obj) not in (dict, OrderedDict, tuple, list):
            raise ValueError("unsupported portable type: " + type(obj).__qualname__)
        if len(obj) > MAX_ITEMS:
            raise ValueError("portable container limit")
        active.add(id(obj))
        try:
            if isinstance(obj, dict):
                for key, value in cast(dict[Any, Any], obj).items():
                    if type(key) not in (str, int, bool, float, tuple) and key is not None:
                        raise ValueError("unsupported portable dictionary key")
                    self._check(key, active, seen, devices, storages, depth + 1)
                    self._check(value, active, seen, devices, storages, depth + 1)
            else:
                for child in obj:
                    self._check(child, active, seen, devices, storages, depth + 1)
        finally:
            active.remove(id(cast(Any, obj)))

    def _encode(self, obj: Any) -> Any:
        if obj is None or type(obj) in (bool, int, float, str):
            return obj
        if id(obj) in self.identities:
            return {"ref": self.identities[id(obj)]}
        identity = "o" + str(len(self.objects))
        self.identities[id(obj)] = identity
        self.objects[identity] = None
        torch = self.torch
        if type(obj) in (torch.Tensor, torch.nn.Parameter):
            storage = obj.untyped_storage()
            key = (str(obj.device), storage._cdata)
            if key not in self.storage_ids:
                storage_id = "s" + str(len(self.storages))
                byte_size = storage.nbytes()
                # Reinterpret exact storage bytes, including gaps, rather than flattening a view.
                raw = (
                    torch.empty(0, dtype=torch.uint8, device=obj.device)
                    .set_(storage, 0, (byte_size,), (1,))
                    .cpu()
                )
                # NumPy is used only as a byte buffer bridge, never as a serializer.
                stream = _BufferReader(memoryview(raw.numpy()))
                blob = self.store.put_stream(
                    stream, kind="tensor-storage", owner=self.storage_owner
                )
                self.storage_ids[key] = storage_id
                self.storages[storage_id] = {"blob": blob, "byte_size": byte_size}
            self.objects[identity] = {
                "kind": "tensor",
                "storage": self.storage_ids[key],
                "dtype": str(obj.dtype).removeprefix("torch."),
                "shape": list(obj.shape),
                "stride": list(obj.stride()),
                "offset": obj.storage_offset(),
                "device": str(obj.device),
                "requires_grad": obj.requires_grad,
                "parameter": type(obj) is torch.nn.Parameter,
                "gradient": id(obj) in self.gradient_ids,
            }
        elif isinstance(obj, dict):
            self.objects[identity] = {
                "kind": "dict",
                "items": [
                    [self._encode(key), self._encode(value)]
                    for key, value in cast(dict[Any, Any], obj).items()
                ],
            }
        else:
            self.objects[identity] = {
                "kind": "tuple" if type(obj) is tuple else "list",
                "items": [self._encode(child) for child in cast(Any, obj)],
            }
        return {"ref": identity}

    def capture(self, value: Any) -> str:
        if self.objects:
            raise ValueError("encoder already used")
        devices: set[int] = set()
        self._check(value, set(), set(), devices, {})
        for device in sorted(devices):
            self.torch.cuda.synchronize(device)
        try:
            root = self._encode(value)
            manifest = {
                "schema": STATE_SCHEMA,
                "kind": "portable-values",
                "endianness": sys.byteorder,
                "storages": self.storages,
                "objects": self.objects,
                "value": root,
            }
            validate_state(manifest, max_bytes=self.max_bytes)
            return self.store.put_state(manifest, owner=self.owner)
        finally:
            for storage in self.storages.values():
                self.store.release(storage["blob"], owner=self.storage_owner)


class _BufferReader(io.RawIOBase):
    def __init__(self, buffer: memoryview) -> None:
        super().__init__()
        self.buffer, self.position = buffer.cast("B"), 0

    def read(self, size: int = -1) -> bytes:
        end = len(self.buffer) if size < 0 else min(len(self.buffer), self.position + size)
        data = self.buffer[self.position : end].tobytes()
        self.position = end
        return data


def inspect_state(store: ArtifactStore, digest: str, *, max_bytes: int = 1024**3) -> dict[str, Any]:
    """Validate all metadata/bytes without importing torch or constructing values."""
    manifest = validate_state(store.metadata(digest), max_bytes=max_bytes)
    store.verify_closure(digest)
    for storage in manifest["storages"].values():
        artifact = store.verify(storage["blob"])
        if artifact["kind"] != "tensor-storage" or artifact["byte_size"] != storage["byte_size"]:
            raise ValueError("tensor storage artifact mismatch")
    return manifest


def restore_values(
    store: ArtifactStore, digest: str, *, device: str = "cpu", max_bytes: int = 1024**3
) -> Any:
    manifest = inspect_state(store, digest, max_bytes=max_bytes)
    if manifest["endianness"] != sys.byteorder:
        raise ValueError("incompatible tensor endianness")
    import re

    if not re.fullmatch(r"cpu|cuda:\d+", device):
        raise ValueError("unsupported restore device")
    import torch

    torch_module: Any = torch
    if device != "cpu" and (
        not torch_module.cuda.is_available()
        or int(device.split(":")[1]) >= torch_module.cuda.device_count()
    ):
        raise ValueError("unavailable restore device")
    storages: dict[str, Any] = {}
    for identity, storage in manifest["storages"].items():
        size = storage["byte_size"]
        raw = torch_module.empty(size, dtype=torch_module.uint8)
        target = memoryview(raw.numpy()).cast("B")
        position = 0
        for block in store.read_chunks(storage["blob"]):
            target[position : position + len(block)] = block
            position += len(block)
        # One allocation per captured storage on the selected destination device.
        storages[identity] = raw.to(device).untyped_storage()
    restored: dict[str, Any] = {}

    def decode(value: Any) -> Any:
        if not isinstance(value, dict):
            return value
        value = cast(dict[str, Any], value)
        identity = value["ref"]
        if identity in restored:
            return restored[identity]
        node = manifest["objects"][identity]
        kind = node["kind"]
        result: Any
        if kind == "tensor":
            dtype = getattr(torch_module, node["dtype"])
            result = torch_module.empty(0, dtype=dtype, device=device).set_(
                storages[node["storage"]], node["offset"], node["shape"], node["stride"]
            )
            result.requires_grad_(node["requires_grad"])
            if node["parameter"]:
                result = torch_module.nn.Parameter(result, requires_grad=node["requires_grad"])
        elif kind == "dict":
            result = {decode(key): decode(child) for key, child in node["items"]}
        else:
            items = [decode(child) for child in node["items"]]
            result = tuple(items) if kind == "tuple" else items
        restored[identity] = result
        return result

    return decode(manifest["value"])
