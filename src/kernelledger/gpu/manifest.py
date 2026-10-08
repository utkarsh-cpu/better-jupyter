"""Framework-free validation of the experimental portable tensor format."""

import re
from typing import Any, cast

from ..artifacts import STATE_SCHEMA, integer
from ..blobs import check_hash

DTYPES = {
    "bool": 1,
    "uint8": 1,
    "int8": 1,
    "int16": 2,
    "int32": 4,
    "int64": 8,
    "float16": 2,
    "bfloat16": 2,
    "float32": 4,
    "float64": 8,
    "complex64": 8,
    "complex128": 16,
}
MAX_STORAGE = 64 * 1024**3
MAX_ITEMS = 100000


def validate_tensor(node: Any, storages: dict[str, Any]) -> None:
    fields = {
        "kind",
        "storage",
        "dtype",
        "shape",
        "stride",
        "offset",
        "device",
        "requires_grad",
        "parameter",
        "gradient",
    }
    if not isinstance(node, dict):
        raise ValueError("invalid tensor descriptor")
    node = cast(dict[str, Any], node)
    if set(node) != fields or node["kind"] != "tensor":
        raise ValueError("invalid tensor descriptor")
    storage_id = node["storage"]
    if not isinstance(storage_id, str) or storage_id not in storages:
        raise ValueError("missing tensor storage")
    dtype = node["dtype"]
    if not isinstance(dtype, str) or dtype not in DTYPES:
        raise ValueError("unsupported tensor dtype")
    item_size = DTYPES[dtype]
    size = storages[storage_id]["byte_size"]
    if size % item_size:
        raise ValueError("unaligned tensor storage")
    shape, stride = node["shape"], node["stride"]
    if not isinstance(shape, list) or not isinstance(stride, list):
        raise ValueError("invalid tensor dimensions")
    shape, stride = cast(list[Any], shape), cast(list[Any], stride)
    if len(shape) > 32 or len(shape) != len(stride):
        raise ValueError("invalid tensor rank")
    for dim in shape:
        integer(dim, MAX_STORAGE, "tensor dimension")
    for step in stride:
        integer(step, MAX_STORAGE, "tensor stride")
    offset = integer(node["offset"], MAX_STORAGE, "tensor offset")
    elements = 1
    for dim in shape:
        elements *= dim
        if elements > MAX_STORAGE // item_size:
            raise ValueError("tensor allocation limit")
    end = (
        offset
        if elements == 0
        else offset + 1 + sum((dim - 1) * step for dim, step in zip(shape, stride))
    )
    if end * item_size > size:
        raise ValueError("tensor exceeds storage bounds")
    if not isinstance(node["device"], str) or not re.fullmatch(r"cpu|cuda:\d+", node["device"]):
        raise ValueError("unsupported tensor device")
    for field in ("requires_grad", "parameter", "gradient"):
        if type(node[field]) is not bool:
            raise ValueError("invalid tensor flags")
    if node["requires_grad"] and not dtype.startswith(("float", "bfloat", "complex")):
        raise ValueError("invalid tensor gradient dtype")


def validate_state(value: Any, *, max_bytes: int = MAX_STORAGE) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError("unsupported portable state schema")
    value = cast(dict[str, Any], value)
    if value.get("schema") != STATE_SCHEMA:
        raise ValueError("unsupported portable state schema")
    if set(value) != {"schema", "kind", "endianness", "storages", "objects", "value"}:
        raise ValueError("invalid portable state fields")
    if value["kind"] != "portable-values" or value["endianness"] not in ("little", "big"):
        raise ValueError("unsupported portable state format")
    storages, objects = value["storages"], value["objects"]
    if not isinstance(storages, dict) or not isinstance(objects, dict):
        raise ValueError("invalid portable state graph")
    storages, objects = cast(dict[str, Any], storages), cast(dict[str, Any], objects)
    if len(storages) + len(objects) > MAX_ITEMS:
        raise ValueError("portable object limit")
    total = 0
    for identity, storage in storages.items():
        if not isinstance(cast(Any, identity), str) or not re.fullmatch(r"s\d+", identity):
            raise ValueError("invalid storage identity")
        if not isinstance(storage, dict) or set(cast(dict[str, Any], storage)) != {
            "blob",
            "byte_size",
        }:
            raise ValueError("invalid storage descriptor")
        check_hash(storage["blob"])
        total += integer(storage["byte_size"], max_bytes, "storage size")
        if total > max_bytes:
            raise ValueError("portable allocation limit")

    def node(item: Any, depth: int = 0) -> None:
        if depth > 64:
            raise ValueError("portable nesting limit")
        if item is None or type(item) in (bool, int, str):
            return
        if type(item) is float:
            # JSON canonicalization rejects nan/inf; validate imported JSON as well.
            import math

            if not math.isfinite(item):
                raise ValueError("nonfinite portable scalar")
            return
        if not isinstance(item, dict) or set(cast(dict[str, Any], item)) != {"ref"}:
            raise ValueError("invalid portable value")
        if not isinstance(item["ref"], str) or item["ref"] not in objects:
            raise ValueError("dangling portable reference")

    node(value["value"])
    for identity, item in objects.items():
        if not isinstance(cast(Any, identity), str) or not re.fullmatch(r"o\d+", identity):
            raise ValueError("invalid object identity")
        if not isinstance(item, dict):
            raise ValueError("invalid portable object")
        item = cast(dict[str, Any], item)
        if not isinstance(item.get("kind"), str):
            raise ValueError("invalid portable object")
        kind = item["kind"]
        if kind == "tensor":
            validate_tensor(item, storages)
        elif kind in ("list", "tuple", "dict"):
            if set(item) != {"kind", "items"} or not isinstance(item["items"], list):
                raise ValueError("invalid portable container")
            children = cast(list[Any], item["items"])
            if len(children) > MAX_ITEMS:
                raise ValueError("portable container limit")
            for child in children:
                if kind == "dict":
                    if not isinstance(child, list) or len(cast(list[Any], child)) != 2:
                        raise ValueError("invalid portable dictionary")
                    for part in cast(list[Any], child):
                        node(part)
                else:
                    node(child)
        else:
            raise ValueError("unsupported portable object kind")
    # No cyclic containers in this first adapter. Bound traversal before any framework import.
    visited: set[str] = set()
    active: set[str] = set()

    def walk(item: Any, depth: int = 0) -> None:
        if depth > 64:
            raise ValueError("portable nesting limit")
        if not isinstance(item, dict):
            return
        item = cast(dict[str, Any], item)
        identity = item["ref"]
        if identity in active:
            raise ValueError("cyclic portable container")
        if identity in visited:
            return
        active.add(identity)
        obj = objects[identity]
        for child in obj.get("items", []):
            if obj["kind"] == "dict":
                for part in child:
                    walk(part, depth + 1)
            else:
                walk(child, depth + 1)
        active.remove(identity)
        visited.add(identity)

    for identity in objects:
        walk({"ref": identity})
    return value
