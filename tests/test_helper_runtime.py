"""Desktop helper and worker runtime fixes: interpreter, RSS, formatter, forms, symlinks."""

from __future__ import annotations

import json
import mmap
import os
import shutil
import stat
import sys
from pathlib import Path
from typing import Any

import pytest
from _worker_helpers import new_worker, run

from kernelledger.desktop import helper as helper_mod
from kernelledger.desktop.helper import Helper, format_code, list_workspace_files
from kernelledger.notebook import Notebook
from kernelledger.supervisor import interpreter, project_python
from kernelledger.worker import NativeWidget, _mach_rss_bytes, _process_rss_bytes, _ps_rss_bytes


def _fake_python(path: Path) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("#!/bin/sh\n")
    path.chmod(path.stat().st_mode | stat.S_IEXEC)
    return path


def test_package_tools_use_the_kernel_interpreter(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    Notebook(tmp_path).init()
    venv = tmp_path / "elsewhere-venv"
    venv_python = _fake_python(venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python"))
    monkeypatch.setenv("VIRTUAL_ENV", str(venv))
    # Same resolution as the supervisor: VIRTUAL_ENV is honoured (it was ignored before).
    expected = interpreter(tmp_path.resolve(), None, {})[0]
    assert expected == str(venv_python)
    assert helper_mod._selected_python(tmp_path) == expected

    # A running supervisor's resolved interpreter (e.g. from --python) wins.
    explicit = _fake_python(tmp_path / "explicit" / "python")
    (tmp_path / ".kl/supervisor.json").write_text(json.dumps({"worker_python": str(explicit)}))
    assert project_python(tmp_path) == (str(explicit), "running_supervisor")
    assert helper_mod._selected_python(tmp_path) == str(explicit)

    seen: list[list[str]] = []

    class Completed:
        returncode, stdout, stderr = 0, "[]", ""

    def fake_run(cmd: list[str], **_kwargs: Any) -> Any:
        seen.append(cmd)
        return Completed()

    monkeypatch.setattr(helper_mod.subprocess, "run", fake_run)
    monkeypatch.setattr(helper_mod.shutil, "which", lambda name: None)
    assert Helper(tmp_path).dispatch({"method": "packages.list"})["python"] == str(explicit)
    helper_mod.install_package(tmp_path, "requests")
    assert seen[-1][:4] == [str(explicit), "-m", "pip", "install"]


def test_rss_reports_current_not_peak_memory() -> None:
    before = _process_rss_bytes()
    assert isinstance(before, int) and before > 0
    if sys.platform != "darwin":
        pytest.skip("mach_task_basic_info and ps fallbacks are macOS-specific")
    # Fill the region with incompressible bytes: nearly-empty pages are cheap for the
    # macOS memory compressor, so touching one byte per page under-reports on busy
    # CI runners. Thresholds are half the region so pressure-related noise passes.
    size = 128 * 1024 * 1024
    margin = size // 2
    region = mmap.mmap(-1, size)
    chunk = os.urandom(4 * 1024 * 1024)
    for offset in range(0, size, len(chunk)):
        region[offset : offset + len(chunk)] = chunk[::-1] if offset % 2 else chunk
    peak = _mach_rss_bytes()
    assert peak is not None and peak > before + margin
    region.close()
    after = _mach_rss_bytes()
    assert after is not None and after < peak - margin
    current = _process_rss_bytes()
    assert current is not None and current < peak - margin
    ps_value = _ps_rss_bytes(min_interval=0)
    assert ps_value is not None and abs(ps_value - after) < 64 * 1024 * 1024
    assert _ps_rss_bytes(min_interval=60) == ps_value  # rate-limited: cached value


def test_formatter_chain_reports_formatter_and_keeps_strings(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    source = 'x=1   \ns = """keep   \ntrailing  \n"""   \ny = f"""{x}  \n"""\n\n\n'
    if shutil.which("ruff"):
        formatted = format_code(source)
        assert formatted["formatter"] == "ruff"
        assert "keep   \ntrailing  \n" in formatted["source"]

    real_which = shutil.which
    black = tmp_path / "black"
    black.write_text("#!/bin/sh\necho 'formatted = True'\n")
    black.chmod(0o755)
    monkeypatch.setattr(
        helper_mod.shutil,
        "which",
        lambda name: (
            str(black) if name == "black" else None if name == "ruff" else real_which(name)
        ),
    )
    if os.name != "nt":
        assert format_code(source) == {
            "source": "formatted = True\n",
            "changed": True,
            "formatter": "black",
        }

    monkeypatch.setattr(helper_mod.shutil, "which", lambda name: None)
    fallback = format_code(source)
    assert fallback["formatter"] == "whitespace"
    assert fallback["source"] == 'x=1\ns = """keep   \ntrailing  \n"""\ny = f"""{x}  \n"""\n'
    broken = format_code("def f(:\n    pass   \n")
    assert broken["changed"] is False and "error" in broken


def test_form_values_are_validated_per_field() -> None:
    worker, channel = new_worker(advanced=True)
    idle, events = run(
        worker,
        channel,
        "import kernelledger as kl\n"
        "fm = kl.ui.form({\n"
        "    'n': kl.ui.slider(0, 10, value=5),\n"
        "    'lr': kl.ui.slider(0.0, 1.0, value=0.5),\n"
        "    'mode': kl.ui.dropdown(['a', 'b']),\n"
        "    'tags': kl.ui.multiselect(['x', 'y']),\n"
        "    'ok': kl.ui.checkbox(),\n"
        "    'name': kl.ui.text('bob'),\n"
        "})\n",
    )
    assert idle["outcome"] == "ok", [e for e in events if e["kind"] == "error"]
    form = worker.namespace["fm"]
    good = {"n": 7, "lr": 0.25, "mode": "b", "tags": ["y"], "ok": True, "name": "al"}
    worker.widget_event(worker.generation, form.handle, good)
    assert form.value == good
    worker.widget_event(worker.generation, form.handle, {"n": 3})  # partial: others kept
    assert form.value == {**good, "n": 3}
    for bad in (
        {"n": 11},
        {"n": 2.5},
        {"n": True},
        {"lr": 1.5},
        {"lr": float("nan")},
        {"mode": "c"},
        {"tags": ["z"]},
        {"tags": "x"},
        {"ok": 1},
        {"name": "x" * 20_000},
        {"name": 5},
        {"unknown": 1},
    ):
        with pytest.raises(ValueError):
            worker.widget_event(worker.generation, form.handle, bad)
    assert form.value == {**good, "n": 3}

    limited = NativeWidget(
        worker,
        "form",
        {"code": "ab"},
        fields=[{"name": "code", "kind": "text", "value": "ab", "max_length": 3}],
    )
    with pytest.raises(ValueError, match="at most 3"):
        limited.validate({"code": "abcd"})
    with pytest.raises(ValueError):
        NativeWidget(worker, "form", {}, fields=[{"name": "b", "kind": "button"}])


@pytest.mark.skipif(os.name == "nt", reason="symlink creation needs privileges on Windows")
def test_file_listing_does_not_follow_symlinks_outside_the_workspace(tmp_path: Path) -> None:
    root = tmp_path / "root"
    outside = tmp_path / "outside"
    (outside / "deep").mkdir(parents=True)
    (outside / "secret.csv").write_text("a\n1\n")
    (outside / "deep" / "more.csv").write_text("b\n2\n")
    root.mkdir()
    (root / "data.csv").write_text("a\n1\n")
    (root / "inner").mkdir()
    (root / "inner" / "x.json").write_text("{}")
    (root / "out_dir").symlink_to(outside, target_is_directory=True)
    (root / "out_file.csv").symlink_to(outside / "secret.csv")
    (root / "in_link.csv").symlink_to(root / "data.csv")
    (root / "loop").symlink_to(root, target_is_directory=True)
    (root / "dangling").symlink_to(tmp_path / "missing")
    items = {i["path"]: i for i in list_workspace_files(root.resolve())["items"]}
    assert not any("secret" in p or "more.csv" in p or p.startswith("out_dir/") for p in items)
    for name in ("out_dir", "out_file.csv", "dangling"):
        assert items[name]["kind"] == "symlink" and items[name]["outside_root"] is True
        assert "loader_snippet" not in items[name]
    assert items["in_link.csv"]["symlink"] is True and items["in_link.csv"]["kind"] == "file"
    assert items["loop"]["symlink"] is True and not any(p.startswith("loop/") for p in items)
    assert "symlink" not in items["data.csv"] and "inner/x.json" in items
