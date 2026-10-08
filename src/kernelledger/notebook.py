"""Tracked notebook source and standard-library ipynb conversion."""

import base64
import hashlib
import json
import re
import uuid
from pathlib import Path
from typing import Any, cast

EXTENSIONS: dict[str, str] = {"code": ".py", "markdown": ".md", "raw": ".raw"}
DEVICES = {"CON", "PRN", "AUX", "NUL"} | {
    f"{prefix}{i}" for prefix in ("COM", "LPT") for i in range(1, 10)
}


def valid_id(value: Any) -> Any:
    return (
        isinstance(value, str)
        and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]{0,63}", value) is not None
        and value.upper() not in DEVICES
    )


def filename(value: Any) -> Any:
    if (
        not isinstance(value, str)
        or not value
        or value in (".", "..")
        or any(c in value for c in "/\\\x00:")
    ):
        raise ValueError("invalid attachment filename")
    if value.rstrip(". ") != value or value.split(".")[0].upper() in DEVICES:
        raise ValueError("invalid attachment filename")
    return value


def canonical(text: Any) -> Any:
    return text.removeprefix("\ufeff").replace("\r\n", "\n").replace("\r", "\n")


def source_hash(text: Any) -> Any:
    return hashlib.sha256(canonical(text).encode()).hexdigest()


def write_json(path: Any, value: Any) -> Any:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    tmp.replace(path)


class Notebook:
    def __init__(self, root: Any) -> None:
        self.direct = False
        self.root = Path(root).resolve()
        self.path = self.root / "notebook.json"

    def init(self) -> Any:
        if self.path.exists():
            raise ValueError("notebook already exists")
        self.root.mkdir(parents=True, exist_ok=True)
        for name in ("cells", "attachments", "checkpoints", ".kl"):
            (self.root / name).mkdir(exist_ok=True)
        self.write({"schema": 1, "id": str(uuid.uuid4()), "metadata": {}, "cells": []})
        ignore = self.root / ".gitignore"
        content = ignore.read_text() if ignore.exists() else ""
        if ".kl/" not in content.splitlines():
            ignore.write_text(
                content + ("\n" if content and not content.endswith("\n") else "") + ".kl/\n",
                encoding="utf-8",
            )

    def read(self, validate: Any = True) -> Any:
        data = json.loads(self.path.read_text(encoding="utf-8"))
        if data.get("schema") != 1:
            raise ValueError("unsupported notebook schema")
        try:
            if str(uuid.UUID(data["id"])) != data["id"]:
                raise ValueError()
        except (ValueError, KeyError, TypeError):
            raise ValueError("invalid notebook UUID") from None
        if not isinstance(data.get("metadata"), dict) or not isinstance(data.get("cells"), list):
            raise ValueError("invalid notebook manifest")
        if validate:
            seen: set[Any]
            seen = set()
            for cell in data["cells"]:
                cid = cell.get("id")
                if not valid_id(cid) or cid.lower() in seen:
                    raise ValueError(
                        f"invalid or duplicate cell id: {cid!r}; use doctor --repair-ids"
                    )
                seen.add(cid.lower())
                if cell.get("type") not in EXTENSIONS:
                    raise ValueError("invalid cell type")
                names = cell.get("attachments", [])
                if not isinstance(names, list):
                    raise ValueError("attachments must be a list")
                names = cast(list[str], names)
                if len({n.lower() for n in names}) != len(names):
                    raise ValueError("duplicate attachment names")
                for name in names:
                    filename(name)
        return data

    def write(self, data: Any) -> Any:
        if not self.direct:
            from .source import SourceService

            return SourceService(self.root).legacy("write", data)
        # Cell objects stay on one line, so reordering has a small, readable diff.
        header: dict[Any, Any]
        header = {k: v for k, v in data.items() if k != "cells"}
        text = json.dumps(header, ensure_ascii=False, indent=2)[:-2] + ',\n  "cells": [\n'
        text += ",\n".join(
            "    " + json.dumps(cell, ensure_ascii=False, sort_keys=True) for cell in data["cells"]
        )
        text += "\n  ]\n}\n"
        self.root.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(text, encoding="utf-8", newline="\n")
        tmp.replace(self.path)

    def cell(self, cid: Any) -> Any:
        for cell in self.read()["cells"]:
            if cell["id"] == cid:
                return cell
        raise ValueError(f"unknown cell: {cid}")

    def cell_path(self, cell: Any) -> Any:
        if not valid_id(cell["id"]):
            raise ValueError("invalid cell id")
        return self.root / "cells" / (cell["id"] + EXTENSIONS[cell["type"]])

    def attachment_names(self, cell: dict[str, Any]) -> list[str]:
        names = list(cell.get("attachments", []))
        for alternatives in cell.get("attachment_alternatives", {}).values():
            names.extend(alternatives.values())
        for name in names:
            filename(name)
        return names

    def source(self, cell: Any) -> Any:
        return canonical(self.cell_path(cell).read_bytes().decode("utf-8"))

    def add(self, kind: Any, source: Any = "", cid: Any = None, metadata: Any = None) -> Any:
        if not self.direct:
            from .source import SourceService

            return SourceService(self.root).legacy("add", kind, source, cid, metadata)
        data = self.read()
        cid = cid or uuid.uuid4().hex[:12]
        if (
            kind not in EXTENSIONS
            or not valid_id(cid)
            or cid.lower() in {c["id"].lower() for c in data["cells"]}
        ):
            raise ValueError("invalid cell type or id")
        cell: dict[Any, Any]
        cell = {"id": cid, "type": kind}
        if metadata:
            cell["metadata"] = metadata
        self.cell_path(cell).write_text(canonical(source), encoding="utf-8", newline="\n")
        data["cells"].append(cell)
        self.write(data)
        return cid

    def remove(self, cid: Any) -> Any:
        if not self.direct:
            from .source import SourceService

            return SourceService(self.root).legacy("remove", cid)
        cell, data = self.cell(cid), self.read()
        data["cells"] = [c for c in data["cells"] if c["id"] != cid]
        self.write(data)
        self.cell_path(cell).unlink(missing_ok=True)

    def doctor(self, repair: Any = False) -> Any:
        if repair and not self.direct:
            from .source import SourceService

            return SourceService(self.root).legacy("doctor", repair)
        data = self.read(validate=False)
        issues: list[Any]
        mapping: list[Any]
        seen: set[Any]
        issues, mapping, seen = [], [], set()
        for cell in data["cells"]:
            cid = cell.get("id")
            if not valid_id(cid) or cid.lower() in seen:
                issues.append(f"invalid or duplicate id: {cid!r}")
                if repair:
                    replacement = uuid.uuid4().hex[:12]
                    if valid_id(cid):
                        old = self.root / "cells" / (cid + EXTENSIONS.get(cell.get("type"), ".raw"))
                        if old.exists():
                            (old.parent / (replacement + old.suffix)).write_bytes(old.read_bytes())
                        for name in self.attachment_names(cell):
                            old_attachment = self.root / "attachments" / cid / name
                            if old_attachment.exists():
                                destination = self.root / "attachments" / replacement / name
                                destination.parent.mkdir(parents=True, exist_ok=True)
                                destination.write_bytes(old_attachment.read_bytes())
                    mapping.append({"old": cid, "new": replacement})
                    cell["id"] = replacement
                    cid = replacement
            if not valid_id(cid):
                continue
            seen.add(cid.lower())
            try:
                path = self.cell_path(cell)
                raw = path.read_bytes()
                raw.decode("utf-8")
                if raw.startswith(b"\xef\xbb\xbf") or b"\r" in raw:
                    issues.append(f"noncanonical source: {path.name}")
                for name in self.attachment_names(cell):
                    filename(name)
                    if not (self.root / "attachments" / cid / name).is_file():
                        issues.append(f"missing attachment: {cid}/{name}")
            except (OSError, ValueError, KeyError) as error:
                issues.append(str(error))
        if repair and mapping:
            self.write(data)
        return {"id": data["id"], "issues": issues, "repairs": mapping}

    def import_ipynb(self, path: Any) -> Any:
        if not self.direct:
            from .source import SourceService

            return SourceService(self.root).legacy("import_ipynb", path)
        original = json.loads(Path(path).read_text(encoding="utf-8"))
        if original.get("nbformat") != 4:
            raise ValueError("only ipynb format 4 is supported")
        if self.read()["cells"]:
            raise ValueError("import needs an empty notebook")
        data = self.read()
        data["metadata"] = original.get("metadata", {})
        imported: dict[Any, Any]
        seen: set[Any]
        imported, seen = {}, set()
        self.write(data)
        output_rows: list[Any]
        output_rows = []
        for item in original["cells"]:
            old = item.get("id")
            cid = old if valid_id(old) and old.lower() not in seen else uuid.uuid4().hex[:12]
            if cid != old:
                imported[cid] = old
            seen.add(cid.lower())
            source = item.get("source", "")
            source = "".join(cast(list[str], source)) if isinstance(source, list) else source
            metadata: dict[Any, Any]
            metadata = {
                k: v
                for k, v in item.get("metadata", {}).items()
                if k not in ("execution", "execution_count")
            }
            self.add(item["cell_type"], source, cid, metadata)
            cell = self.cell(cid)
            attachments = item.get("attachments", {})
            if attachments:
                cell["attachments"] = []
                cell["attachment_mimes"] = {}
                for name, bundle in attachments.items():
                    filename(name)
                    if not isinstance(bundle, dict) or not bundle:
                        raise ValueError("attachment MIME bundle must be nonempty")
                    for index, (mime, encoded) in enumerate(cast(dict[str, Any], bundle).items()):
                        stored_name = name if index == 0 else "alt-" + uuid.uuid4().hex
                        dest = self.root / "attachments" / cid / stored_name
                        dest.parent.mkdir(parents=True, exist_ok=True)
                        dest.write_bytes(
                            base64.b64decode(
                                "".join(cast(list[str], encoded))
                                if isinstance(encoded, list)
                                else encoded,
                                validate=True,
                            )
                        )
                        if index == 0:
                            cell["attachments"].append(name)
                            cell["attachment_mimes"][name] = mime
                        else:
                            cell.setdefault("attachment_alternatives", {}).setdefault(name, {})[
                                mime
                            ] = stored_name
                data = self.read()
                data["cells"][-1] = cell
                self.write(data)
            if item["cell_type"] == "code":
                output_rows.append(
                    {
                        "cell_id": cid,
                        "source": canonical(source),
                        "outputs": item.get("outputs", []),
                    }
                )
        if imported:
            data = self.read()
            data["metadata"].setdefault("kernelledger", {})["imported_ids"] = imported
            self.write(data)
        return output_rows

    def export_ipynb(self, path: Any, outputs: Any = None) -> Any:
        outputs = outputs or {}
        cells: list[Any]
        data, cells = self.read(), []
        for cell in data["cells"]:
            source = self.source(cell)
            item: dict[Any, Any]
            item = {
                "id": cell["id"],
                "cell_type": cell["type"],
                "metadata": dict(cell.get("metadata", {})),
                "source": source,
            }
            if cell.get("attachments"):
                item["attachments"] = {
                    name: {
                        cell.get("attachment_mimes", {}).get(
                            name, "application/octet-stream"
                        ): base64.b64encode(
                            (self.root / "attachments" / cell["id"] / name).read_bytes()
                        ).decode()
                    }
                    for name in cell["attachments"]
                }
                for name, alternatives in cell.get("attachment_alternatives", {}).items():
                    for mime, stored_name in alternatives.items():
                        filename(stored_name)
                        item["attachments"][name][mime] = base64.b64encode(
                            (self.root / "attachments" / cell["id"] / stored_name).read_bytes()
                        ).decode()
            if cell["type"] == "code":
                record = outputs.get(cell["id"], {})
                item.update(
                    outputs=record.get("outputs", []), execution_count=record.get("ordinal")
                )
                if record:
                    item["metadata"]["kernelledger"] = {
                        "output_source_sha256": record["source_sha256"],
                        "source_sha256": source_hash(source),
                        "output_mismatch": record["source_sha256"] != source_hash(source),
                    }
            cells.append(item)
        write_json(
            Path(path),
            {"nbformat": 4, "nbformat_minor": 5, "metadata": data["metadata"], "cells": cells},
        )
