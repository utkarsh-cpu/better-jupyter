"""Generate the migration capability inventory from live catalogs, CLI and source AST.

Test references are candidate regression suites, not a claim that each behavior is
qualified. Explicit migration acceptance scenarios live in docs/runtime-v6.md.
"""

import argparse
import ast
import json
from pathlib import Path

from kernelledger.assistant.contracts import METHODS
from kernelledger.cli import parser
from kernelledger.desktop.contracts import HELPER, SUPERVISOR

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs/runtime-capabilities.json"


def commands(p, prefix=""):
    for action in p._actions:
        if isinstance(action, argparse._SubParsersAction):
            for name, child in action.choices.items():
                command = f"{prefix} {name}".strip()
                yield command
                yield from commands(child, command)


def inventory():
    rows = []

    def add(surface, name, owner, tests, **extra):
        rows.append(
            {
                "surface": surface,
                "name": name,
                "implementation": owner,
                "regression_suites": tests,
                "native": "not_implemented",
                **extra,
            }
        )

    for name in commands(parser()):
        tests = ["tests/test_runtime.py", "tests/test_boundaries.py", "tests/test_source.py"]
        if "assistant" in name:
            tests = ["tests/test_assistant.py"]
        elif "training" in name or "gpu" in name or "native" in name:
            tests = [
                "tests/test_gpu_desktop.py",
                "tests/test_gpu_capabilities.py",
                "tests/test_gpu_portable.py",
            ]
        elif "export" in name:
            tests += ["tests/test_documents.py"]
        add("cli", name, "src/kernelledger/cli.py", tests)
    for surface, catalog, owner in (
        ("desktop", SUPERVISOR, "src/kernelledger/desktop/server.py"),
        ("helper", HELPER, "src/kernelledger/desktop/helper.py"),
        ("assistant", METHODS, "src/kernelledger/assistant/service.py"),
    ):
        for name, fields in sorted(catalog.items()):
            add(
                surface,
                name,
                owner,
                ["tests/test_assistant.py"]
                if surface == "assistant"
                else ["tests/test_desktop.py", "tests/test_desktop_contracts.py"],
                required_fields=fields,
                stopped_runtime=surface != "desktop",
            )
    for path in ("src/kernelledger/worker.py", "src/kernelledger/supervisor/__init__.py"):
        tree = ast.parse((ROOT / path).read_text(encoding="utf-8"))
        names = set()
        for node in ast.walk(tree):
            if isinstance(node, ast.Call):
                called = (
                    node.func.attr
                    if isinstance(node.func, ast.Attribute)
                    else getattr(node.func, "id", "")
                )
                index = 1 if called in ("send_message",) else 0
                if (
                    called in ("send", "emit", "send_worker", "send_message")
                    and len(node.args) > index
                ):
                    kind = node.args[index]
                    if isinstance(kind, ast.Constant) and isinstance(kind.value, str):
                        names.add(kind.value)
            if (
                isinstance(node, ast.Compare)
                and isinstance(node.left, ast.Name)
                and node.left.id == "kind"
            ):
                for value in node.comparators:
                    elements = value.elts if isinstance(value, (ast.Tuple, ast.List)) else [value]
                    names.update(
                        v.value
                        for v in elements
                        if isinstance(v, ast.Constant) and isinstance(v.value, str)
                    )
        for name in sorted(names):
            add(
                "worker",
                name,
                path,
                ["tests/test_runtime.py", "tests/test_boundaries.py", "tests/test_checkpoint.py"],
            )
    for path in sorted((ROOT / "schemas").glob("*.json")):
        add(
            "persisted_or_wire_schema",
            path.stem,
            path.relative_to(ROOT).as_posix(),
            ["tests/test_boundaries.py", "tests/test_desktop_contracts.py"],
        )
    for name, owner, tests in (
        (
            "ledger_schema_2_wal_full",
            "src/kernelledger/ledger.py",
            ["tests/test_desktop.py", "tests/test_runtime.py"],
        ),
        (
            "runtime_lock_job_group_token_acl",
            "src/kernelledger/supervisor/platform.py",
            ["tests/test_boundaries.py", "tests/test_runtime.py"],
        ),
        (
            "source_lock_recovery_canonical_bytes",
            "src/kernelledger/source.py",
            ["tests/test_source.py", "tests/test_desktop.py"],
        ),
        ("registry_detached_lifetime", "src/kernelledger/registry.py", ["tests/test_desktop.py"]),
        ("blob_publication_and_gc", "src/kernelledger/blobs.py", ["tests/test_boundaries.py"]),
        (
            "portable_training_and_native_refusal",
            "src/kernelledger/gpu/backends/capabilities.py",
            ["tests/test_gpu_capabilities.py", "tests/test_gpu_portable.py"],
        ),
    ):
        add("platform_or_storage", name, owner, tests)
    return {
        "schema": 1,
        "authority": "Python catalogs remain authoritative",
        "coverage_note": "Suite references are regression entry points, not per-method qualification evidence.",
        "capabilities": rows,
    }


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--check", action="store_true")
    args = p.parse_args()
    text = json.dumps(inventory(), indent=2, ensure_ascii=False) + "\n"
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text(encoding="utf-8") != text:
            raise SystemExit("runtime inventory drift: run scripts/runtime_inventory.py")
    else:
        OUTPUT.write_text(text, encoding="utf-8")
