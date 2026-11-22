"""Tracked notebook source and standard-library ipynb conversion."""

import base64
import hashlib
import json
import re
import uuid
from pathlib import Path
from typing import Any, cast

EXTENSIONS: dict[str, str] = {"code": ".py", "markdown": ".md", "raw": ".raw", "sql": ".sql"}
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


# Cell-level keys that belong beside `metadata`, never inside it.
CELL_KEYS = frozenset(
    {
        "id",
        "type",
        "source",
        "revision",
        "attachments",
        "attachment_mimes",
        "attachment_alternatives",
    }
)
DEFAULT_SQL_OUTPUT = "_sql_df"


def normalize_metadata(metadata: Any) -> dict[str, Any]:
    """Flatten accidental `{"metadata": {...}}` nesting and drop copied cell-level keys.

    Outer keys win over nested ones, because clients set the edited key at the outer level.
    """
    if not isinstance(metadata, dict):
        return {}
    outer = cast(dict[str, Any], metadata)
    flat: dict[str, Any] = {}
    nested = outer.get("metadata")
    if isinstance(nested, dict):
        flat.update(normalize_metadata(nested))
    for key, value in outer.items():
        if key != "metadata" and key not in CELL_KEYS:
            flat[key] = value
    return flat


def cell_metadata(cell: Any) -> dict[str, Any]:
    """A cell's metadata, tolerant of the nested shape some clients wrote."""
    if not isinstance(cell, dict):
        return {}
    return normalize_metadata(cast(dict[str, Any], cell).get("metadata"))


def sql_output_var(cell: Any) -> str:
    """Output variable of a SQL cell: `metadata.output_var`, default `_sql_df`."""
    value = cell_metadata(cell).get("output_var")
    if isinstance(value, str) and value.isidentifier():
        return value
    return DEFAULT_SQL_OUTPUT


def write_json(path: Any, value: Any) -> Any:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    tmp.replace(path)


def _script_sql(body: str) -> tuple[str, str | None]:
    """SQL text (and bound name) of an exported SQL cell body; raw SQL is returned as-is."""
    import ast

    try:
        tree = ast.parse(body)
    except SyntaxError:
        return body, None
    for node in tree.body:
        if (
            isinstance(node, ast.Assign)
            and len(node.targets) == 1
            and isinstance(node.targets[0], ast.Name)
        ):
            value = node.value
            if (
                isinstance(value, ast.Call)
                and isinstance(value.func, ast.Name)
                and value.func.id in ("_kl_run_sql", "run_sql")
                and value.args
            ):
                value = value.args[0]
            elif node.targets[0].id != "_kl_sql":  # pre-marker exports bound `_kl_sql = '...'`
                continue
            if isinstance(value, ast.Constant) and isinstance(value.value, str):
                text = value.value
                if text.startswith("\n") and text.endswith("\n"):
                    text = text[1:-1]
                bound = node.targets[0].id
                return text, None if bound == "_kl_sql" else bound
    return body, None


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
        with Path(path).open("rb") as stream:
            raw = stream.read(64 * 1024 * 1024 + 1)
        if len(raw) > 64 * 1024 * 1024:
            raise ValueError("Notebook import exceeds the 64 MiB file limit")
        original = json.loads(raw.decode("utf-8-sig"))
        if original.get("nbformat") != 4:
            raise ValueError("only ipynb format 4 is supported")
        if self.read()["cells"]:
            raise ValueError("import needs an empty notebook")
        data = self.read()
        data["metadata"] = original.get("metadata", {})
        extras = {
            key: value
            for key, value in original.items()
            if key not in ("nbformat", "nbformat_minor", "metadata", "cells")
        }
        if extras:
            data["metadata"].setdefault("kernelledger", {})["ipynb_fields"] = extras
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
            extras = {
                key: value
                for key, value in item.items()
                if key
                not in (
                    "id",
                    "cell_type",
                    "source",
                    "metadata",
                    "outputs",
                    "execution_count",
                    "attachments",
                )
            }
            cell_type = item["cell_type"]
            marker = metadata.get("kernelledger")
            if (
                cell_type == "code"
                and isinstance(marker, dict)
                and cast(dict[str, Any], marker).get("type") == "sql"
            ):
                # KernelLedger SQL cells are exported as `%%sql` code cells plus metadata.
                kl_meta = {
                    k: v
                    for k, v in cast(dict[str, Any], marker).items()
                    if k not in ("type", "output_var")
                }
                if kl_meta:
                    metadata["kernelledger"] = kl_meta
                else:
                    metadata.pop("kernelledger")
                out_var = cast(dict[str, Any], marker).get("output_var")
                if isinstance(out_var, str) and out_var.isidentifier():
                    metadata["output_var"] = out_var
                cell_type = "sql"
                first, newline, rest = source.partition("\n")
                if first.strip().startswith("%%sql"):
                    source = rest if newline else ""
            if extras or item.get("execution_count") is not None:
                interchange = metadata.setdefault("kernelledger", {})
                if extras:
                    interchange["ipynb_fields"] = extras
                if item.get("execution_count") is not None:
                    interchange["imported_execution_count"] = item["execution_count"]
            self.add(cell_type, source, cid, metadata)
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
            if cell_type in ("code", "sql"):
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
            cell_meta = cell_metadata(cell)
            export_type = "code" if cell["type"] == "sql" else cell["type"]
            item_meta: dict[str, Any] = dict(cell_meta)
            if cell["type"] == "sql":
                # Plain `%%sql` (jupysql reads arguments as a connection), with the
                # KernelLedger cell type and output variable carried in metadata.
                out_var = sql_output_var(cell)
                item_meta.pop("output_var", None)
                item_meta["kernelledger"] = {
                    **cast(dict[str, Any], item_meta.get("kernelledger") or {}),
                    "type": "sql",
                    "output_var": out_var,
                }
                source = f"%%sql\n{source}"
            item: dict[Any, Any]
            item = {
                **cell_meta.get("kernelledger", {}).get("ipynb_fields", {}),
                "id": cell["id"],
                "cell_type": export_type,
                "metadata": item_meta,
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
            if export_type == "code":
                record = outputs.get(cell["id"], {})
                item.update(
                    outputs=record.get("outputs", []), execution_count=record.get("ordinal")
                )
                if record:
                    item["metadata"]["kernelledger"] = {
                        **item["metadata"].get("kernelledger", {}),
                        "output_source_sha256": record["source_sha256"],
                        "source_sha256": source_hash(source),
                        "output_mismatch": record["source_sha256"] != source_hash(source),
                    }
            cells.append(item)
        write_json(
            Path(path),
            {
                **data["metadata"].get("kernelledger", {}).get("ipynb_fields", {}),
                "nbformat": 4,
                "nbformat_minor": 5,
                "metadata": data["metadata"],
                "cells": cells,
            },
        )

    def import_script(self, path: Any) -> list[dict[str, Any]]:
        """Import a marimo (@app.cell) notebook or Jupytext/VS Code (# %%) Python script."""
        import ast
        import textwrap

        data = self.read()
        if data["cells"]:
            raise ValueError("import target notebook must be empty")
        raw = canonical(Path(path).read_text(encoding="utf-8"))
        parsed_cells: list[tuple[str, str, dict[str, Any]]] = []

        # 1. Try marimo @app.cell AST extraction first.
        try:
            tree = ast.parse(raw)
        except SyntaxError:
            tree = None

        if tree is not None:
            marimo_funcs: list[tuple[ast.FunctionDef | ast.AsyncFunctionDef, dict[str, Any]]] = []
            for node in tree.body:
                if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    continue
                for dec in node.decorator_list:
                    dec_attr = dec.func if isinstance(dec, ast.Call) else dec
                    if isinstance(dec_attr, ast.Attribute) and dec_attr.attr == "cell":
                        meta: dict[str, Any] = {}
                        if isinstance(dec, ast.Call):
                            for kw in dec.keywords:
                                if (
                                    kw.arg in ("hide_code", "disabled")
                                    and isinstance(kw.value, ast.Constant)
                                    and isinstance(kw.value.value, bool)
                                ):
                                    meta[kw.arg] = kw.value.value
                        marimo_funcs.append((node, meta))
                        break
            if marimo_funcs:
                lines = raw.splitlines()
                for func, meta in marimo_funcs:
                    body_stmts = list(func.body)
                    if body_stmts and isinstance(body_stmts[-1], ast.Return):
                        body_stmts = body_stmts[:-1]
                    if not body_stmts:
                        continue
                    # Detect pure `mo.md("...")` markdown cell
                    if (
                        len(body_stmts) == 1
                        and isinstance(body_stmts[0], ast.Expr)
                        and isinstance(body_stmts[0].value, ast.Call)
                        and isinstance(body_stmts[0].value.func, ast.Attribute)
                        and body_stmts[0].value.func.attr == "md"
                        and len(body_stmts[0].value.args) == 1
                        and isinstance(body_stmts[0].value.args[0], ast.Constant)
                        and isinstance(body_stmts[0].value.args[0].value, str)
                    ):
                        md_text = textwrap.dedent(body_stmts[0].value.args[0].value).strip("\n")
                        parsed_cells.append(("markdown", md_text, meta))
                        continue
                    # Detect `out = mo.sql("...")` SQL cell
                    if (
                        len(body_stmts) == 1
                        and isinstance(body_stmts[0], ast.Assign)
                        and len(body_stmts[0].targets) == 1
                        and isinstance(body_stmts[0].targets[0], ast.Name)
                        and isinstance(body_stmts[0].value, ast.Call)
                        and isinstance(body_stmts[0].value.func, ast.Attribute)
                        and body_stmts[0].value.func.attr == "sql"
                        and len(body_stmts[0].value.args) >= 1
                        and isinstance(body_stmts[0].value.args[0], ast.Constant)
                        and isinstance(body_stmts[0].value.args[0].value, str)
                    ):
                        sql_text = textwrap.dedent(body_stmts[0].value.args[0].value).strip("\n")
                        sql_meta = {**meta, "output_var": body_stmts[0].targets[0].id}
                        parsed_cells.append(("sql", sql_text, sql_meta))
                        continue
                    start_line = body_stmts[0].lineno - 1
                    end_line = body_stmts[-1].end_lineno or body_stmts[-1].lineno
                    snippet = textwrap.dedent("\n".join(lines[start_line:end_line])).strip("\n")
                    if snippet:
                        parsed_cells.append(("code", snippet, meta))

        # 2. Fallback: Jupytext / VS Code percent format (`# %%`), including the
        # KernelLedger export markers `# %% [kind] [output_var] id=.. disabled=true ..`.
        ids: list[str | None] = []
        if not parsed_cells:
            marker = re.compile(r"^#\s*%%(?:\s*\[([^\]]+)\])?(.*)$")
            lines = raw.splitlines()
            if any(marker.match(ln) for ln in lines):
                cur_type = "code"
                cur_meta: dict[str, Any] = {}
                cur_id: str | None = None
                buf: list[str] = []

                def uncomment(text: str) -> str:
                    return "\n".join(
                        ln[2:] if ln.startswith("# ") else ln[1:] if ln.startswith("#") else ln
                        for ln in text.splitlines()
                    )

                def flush() -> None:
                    nonlocal buf
                    body = list(buf)
                    meta = dict(cur_meta)
                    if body and body[0].strip() == "# [disabled]":  # pre-marker exports
                        body = body[1:]
                        meta["disabled"] = True
                    text = "\n".join(body).strip("\n")
                    if text:
                        kind = cur_type
                        if kind in ("markdown", "raw") or meta.get("disabled") is True:
                            text = uncomment(text)
                        elif kind == "sql":
                            text, bound = _script_sql(text)
                            if bound and "output_var" not in meta:
                                meta["output_var"] = bound
                        parsed_cells.append((kind, text, meta))
                        ids.append(cur_id)
                    buf = []

                seen_marker = False
                for ln in lines:
                    m = marker.match(ln)
                    if m:
                        if seen_marker or any(
                            b.strip() and not b.lstrip().startswith("#") for b in buf
                        ):
                            flush()
                        else:
                            buf = []
                        seen_marker = True
                        tag = (m.group(1) or "").strip().lower()
                        cur_meta = {}
                        cur_id = None
                        bare: list[str] = []
                        for word in (m.group(2) or "").split():
                            key, eq, value = word.partition("=")
                            if not eq:
                                bare.append(word)
                            elif key == "id" and valid_id(value):
                                cur_id = value
                            elif key in ("disabled", "hide_code") and value in ("true", "false"):
                                cur_meta[key] = value == "true"
                        if tag in ("markdown", "md"):
                            cur_type = "markdown"
                        elif tag == "sql":
                            cur_type = "sql"
                            if bare and bare[0].isidentifier():
                                cur_meta["output_var"] = bare[0]
                        elif tag == "raw":
                            cur_type = "raw"
                        else:
                            cur_type = "code"
                    else:
                        buf.append(ln)
                flush()
            elif raw.strip():
                parsed_cells.append(("code", raw.strip("\n"), {}))

        created: list[dict[str, Any]] = []
        used: set[str] = set()
        for idx, (ctype, src, meta) in enumerate(parsed_cells, 1):
            wanted = ids[idx - 1] if idx - 1 < len(ids) else None
            cid = wanted if wanted and wanted.lower() not in used else f"cell-{idx}"
            while cid.lower() in used:
                cid = uuid.uuid4().hex[:12]
            used.add(cid.lower())
            # Exports drop each cell's final newline; restore the canonical file ending.
            self.add(ctype, src + "\n" if src else src, cid, meta or None)
            created.append({"id": cid, "type": ctype, "metadata": meta})
        return created
