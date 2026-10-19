import ast
import concurrent.futures
import os
import shlex
import subprocess
import sys
import time
import zipfile

import pytest

from kernelledger.blobs import BlobStore
from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.ledger import Ledger
from kernelledger.notebook import Notebook
from kernelledger.worker import notebook_tree, windows_shell_path


def command(arguments):
    return subprocess.list2cmdline(arguments) if os.name == "nt" else shlex.join(arguments)


@pytest.fixture
def notebook(tmp_path):
    notebook = Notebook(tmp_path / "notebook")
    notebook.init()
    try:
        yield notebook
    finally:
        if alive(notebook.root):
            rpc(notebook.root, {"command": "stop"})


def execute(notebook, source):
    result = rpc(notebook.root, {"command": "run", "source": source})
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
    try:
        return result, ledger.events(result["exec_id"])
    finally:
        ledger.close()


def successful(notebook, source):
    result, events = execute(notebook, source)
    assert result["status"] == "ok", events
    return events


def test_token_aware_escapes_preserve_strings_comments_and_python_operators():
    source = '''text = """literal
!not a command
%pip not a command
!"""
fstring = f"""literal
!not a command {1 + 2}
"""
# !not a command
modulo = (12
 % 5)
assert 1 != 2
'''
    assert ast.dump(notebook_tree(source, "cell")) == ast.dump(ast.parse(source))
    tree = notebook_tree(source + '!echo "shell quotes and a ( bracket"\nprint(modulo)', "cell")
    assert tree.body[-2].lineno == len(source.splitlines()) + 1
    assert isinstance(tree.body[-2].value, ast.Call)
    assert tree.body[-2].value.args[0].value == 'echo "shell quotes and a ( bracket"'
    assert ast.dump(ast.Module(body=tree.body[:-2], type_ignores=[])) == ast.dump(ast.parse(source))


def test_windows_shell_paths_preserve_drive_and_network_targets():
    assert (
        windows_shell_path(r"\\?\C:\directory with spaces\python.exe")
        == r"C:\directory with spaces\python.exe"
    )
    assert windows_shell_path(r"\\?\UNC\server\share\folder") == r"\\server\share\folder"
    assert windows_shell_path(r"C:\ordinary\folder") == r"C:\ordinary\folder"
    assert windows_shell_path(r"\\server\share\folder") == r"\\server\share\folder"


def test_mixed_shell_output_capture_and_original_source_are_recorded(notebook):
    ensure_supervisor(notebook.root)
    script = "import sys; print('shell stdout α'); print('shell stderr β', file=sys.stderr)"
    source = "!" + command(["python", "-c", script]) + "\nprint('python-after')"
    cell = notebook.add("code", source, "shell")
    result = rpc(notebook.root, {"command": "run", "cell_id": cell})
    assert result["status"] == "ok"
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
    try:
        events = ledger.events(result["exec_id"])
        assert any(e["kind"] == "stdout" and "shell stdout α" in e.get("text", "") for e in events)
        assert any(e["kind"] == "stderr" and "shell stderr β" in e.get("text", "") for e in events)
        assert any("python-after" in e.get("text", "") for e in events)
        row = ledger.one(
            "SELECT source_sha256 FROM executions WHERE exec_id=?", (result["exec_id"],)
        )
        assert (
            BlobStore(notebook.root / ".kl", readonly=True).get(row["source_sha256"]).decode()
            == source
        )
        assert notebook.source(notebook.cell(cell)) == source
    finally:
        ledger.close()
    capture = command(["python", "-c", "print('first'); print('second')"])
    successful(
        notebook, f"lines = !{capture}\nassert lines == ['first', 'second']\nassert _exit_code == 0"
    )
    events = successful(notebook, "!!echo captured")
    assert any(
        e["kind"] == "result" and "captured" in e["bundle"]["text/plain"]["value"] for e in events
    )


def test_shell_failure_is_visible_and_runtime_remains_usable(notebook):
    ensure_supervisor(notebook.root)
    source = "!" + command(
        ["python", "-c", "import sys; print('failed detail', file=sys.stderr); sys.exit(7)"]
    )
    result, events = execute(notebook, source)
    assert result["status"] == "error"
    assert any(e["kind"] == "error" and e["ename"] == "CalledProcessError" for e in events)
    assert any(e["kind"] == "stderr" and "failed detail" in e.get("text", "") for e in events)
    successful(notebook, "assert _exit_code == 7\nassert 2 + 2 == 4")


@pytest.mark.parametrize(
    "extended_paths",
    [
        False,
        pytest.param(
            True, marks=pytest.mark.skipif(os.name != "nt", reason="Windows path namespace")
        ),
    ],
)
def test_pip_installs_into_selected_standalone_environment(notebook, tmp_path, extended_paths):
    environment = tmp_path / "selected environment"
    options = ["--without-pip"] if extended_paths else []
    subprocess.run(
        [sys.executable, "-m", "venv", *options, str(environment)], check=True, timeout=90
    )
    python = environment / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    wheel = tmp_path / "kl_shell_probe-1.0-py3-none-any.whl"
    info = "kl_shell_probe-1.0.dist-info"
    files = {
        "kl_shell_probe.py": "VALUE = 'installed in notebook environment'\n",
        f"{info}/METADATA": "Metadata-Version: 2.1\nName: kl-shell-probe\nVersion: 1.0\n",
        f"{info}/WHEEL": "Wheel-Version: 1.0\nGenerator: kernelledger-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    }
    files[f"{info}/RECORD"] = "".join(f"{name},,\n" for name in [*files, f"{info}/RECORD"])
    with zipfile.ZipFile(wheel, "w") as archive:
        for name, content in files.items():
            archive.writestr(name, content)
    root = type(notebook.root)("\\\\?\\" + str(notebook.root)) if extended_paths else notebook.root
    interpreter = "\\\\?\\" + str(python) if extended_paths else str(python)
    ensure_supervisor(root, interpreter)
    successful(
        notebook,
        "import os\ndirectories = !cd\nassert os.path.samefile(directories[0], os.getcwd())"
        if os.name == "nt"
        else "!pwd",
    )
    successful(notebook, "!pip install --no-index --no-deps " + command([str(wheel)]))
    successful(
        notebook,
        "import os, sys, kl_shell_probe\nassert kl_shell_probe.VALUE == 'installed in notebook environment'\nassert 'kernelledger' not in sys.modules\nassert os.path.samefile(sys.prefix, "
        + repr(str(environment))
        + ")",
    )
    events = successful(notebook, "%pip show kl-shell-probe")
    assert any("kl-shell-probe" in e.get("text", "") for e in events)
    child = command(["python", "-c", "import sys; print(sys.executable)"])
    successful(
        notebook,
        f"child_python = !{child}\nassert os.path.samefile(child_python[0], sys.executable)",
    )


def test_directory_magics_and_shell_functions_survive_checkpoint_restore(notebook):
    ensure_supervisor(notebook.root)
    (notebook.root / "folder with spaces").mkdir()
    successful(
        notebook,
        '%cd "folder with spaces"\nlocation = %pwd\nassert location.endswith("folder with spaces")\n%cd ..',
    )
    successful(notebook, "def shell_function():\n    !echo restored-shell\nshell_function()")
    checkpoint = rpc(notebook.root, {"command": "checkpoint"})
    assert checkpoint["status"] == "complete", checkpoint
    rpc(notebook.root, {"command": "restore", "checkpoint_id": checkpoint["checkpoint_id"]})
    events = successful(notebook, "shell_function()\n!echo after-restore")
    assert any("restored-shell" in e.get("text", "") for e in events)
    result, events = execute(notebook, "%unknown_magic")
    assert result["status"] == "error"
    assert any("Supported: %pip, %cd, %pwd" in e.get("evalue", "") for e in events)


def test_interrupt_stops_shell_child_and_preserves_worker(notebook):
    ensure_supervisor(notebook.root)
    script = notebook.root / "long_command.py"
    script.write_text(
        "import os, time\nfrom pathlib import Path\nPath('shell-child.pid').write_text(str(os.getpid()))\nprint('shell started', flush=True)\ntime.sleep(60)\n",
        encoding="utf-8",
    )
    with concurrent.futures.ThreadPoolExecutor() as pool:
        future = pool.submit(execute, notebook, "!" + command(["python", str(script)]))
        marker = notebook.root / "shell-child.pid"
        deadline = time.monotonic() + 15
        while not marker.exists():
            assert time.monotonic() < deadline
            time.sleep(0.02)
        rpc(notebook.root, {"command": "interrupt"})
        result, events = future.result(timeout=10)
        assert result["status"] == "interrupted", events
    child_pid = int(marker.read_text())
    if os.name == "nt":
        processes = subprocess.run(
            ["tasklist", "/FI", f"PID eq {child_pid}", "/FO", "CSV", "/NH"],
            capture_output=True,
            text=True,
            check=True,
        )
        assert f'","{child_pid}",' not in processes.stdout
    else:
        processes = subprocess.run(
            ["ps", "-p", str(child_pid), "-o", "stat="], capture_output=True, text=True
        )
        assert not processes.stdout.strip() or processes.stdout.strip().startswith("Z")
    successful(notebook, "assert 6 * 7 == 42")
