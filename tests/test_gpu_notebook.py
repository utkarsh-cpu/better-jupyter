"""Real supervisor/worker portable training admission, including GPU continuation."""

import concurrent.futures
import sys
import time

import pytest

from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.desktop.helper import Helper
from kernelledger.ledger import Ledger
from kernelledger.notebook import Notebook


@pytest.fixture
def notebook(tmp_path):
    project = Notebook(tmp_path)
    project.init()
    ensure_supervisor(tmp_path, sys.executable)
    try:
        yield project
    finally:
        if alive(tmp_path):
            rpc(tmp_path, {"command": "stop"})


def run(notebook, source):
    result = rpc(notebook.root, {"command": "run", "source": source})
    if result["status"] != "ok":
        ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
        try:
            pytest.fail(str(ledger.events(result["exec_id"])))
        finally:
            ledger.close()
    return result


@pytest.mark.parametrize("device", ["cpu", "cuda:0"])
def test_training_capture_candidate_failure_and_exact_next_step(notebook, device):
    torch = pytest.importorskip("torch")
    if device != "cpu" and not torch.cuda.is_available():
        pytest.skip("requires an operational GPU test environment")
    recipe = """import torch
from pathlib import Path
class Net(torch.nn.Sequential):
    pass
def build():
    if Path(FAIL_PATH).exists():
        raise RuntimeError("candidate factory rejected")
    assert Path('input.txt').read_text() == 'captured input'
    model = Net(torch.nn.Linear(4, 8), torch.nn.Dropout(.2), torch.nn.Linear(8, 1))
    optimizer = torch.optim.Adam(model.parameters(), lr=.01)
    return {"model": model, "optimizer": optimizer}
def advance(training):
    with training.training_step(cursor={"batch": training.status["step"] + 1}) as objects:
        model, optimizer = objects["model"], objects["optimizer"]
        device = next(model.parameters()).device
        x = torch.arange(16, device=device, dtype=torch.float32).reshape(4, 4) / 16
        optimizer.zero_grad()
        loss = model(x).square().mean()
        loss.backward()
        optimizer.step()
    return {"loss": loss.item(), "weights": [p.detach().cpu().tolist() for p in model.parameters()]}
""".replace("FAIL_PATH", repr(str(notebook.root / "fail-candidate")))
    (notebook.root / "recipe.py").write_text(recipe, encoding="utf-8")
    (notebook.root / "input.txt").write_text("captured input")
    run(
        notebook,
        f"""import torch, json
from pathlib import Path
from recipe import build, advance
from kernelledger.gpu.notebook import register
torch.manual_seed(1309)
objects = build()
objects['model'].to({device!r})
training = register(objects=objects, source_root=Path.cwd(), files=['recipe.py', 'input.txt'], recipe_source='recipe.py', factory='build', cursor={{'batch':0}})
advance(training)
sentinel = 'healthy worker'
""",
    )
    old = alive(notebook.root)["session_id"]
    capture = rpc(notebook.root, {"command": "training-checkpoint"})
    digest = capture["snapshot"]
    assert capture["coverage"]["native"] == "not-captured"
    assert capture["step"] == 1 and not capture["native_qualified"]
    run(
        notebook,
        "expected = advance(training)\nPath('expected.json').write_text(json.dumps(expected))",
    )
    inspected = Helper(notebook.root).dispatch({"method": "training.inspect", "snapshot": digest})
    assert inspected["step"] == 1 and inspected["source_coherence"] == "unverified"
    with pytest.raises(ValueError, match="trust"):
        rpc(notebook.root, {"command": "training-restore", "snapshot": digest, "device": device})
    assert alive(notebook.root)["session_id"] == old
    (notebook.root / "fail-candidate").touch()
    with pytest.raises(ValueError, match="candidate factory rejected"):
        rpc(
            notebook.root,
            {"command": "training-restore", "snapshot": digest, "trust": True, "device": device},
        )
    assert alive(notebook.root)["session_id"] == old
    run(notebook, "assert sentinel == 'healthy worker'")
    (notebook.root / "fail-candidate").unlink()
    (notebook.root / "input.txt").write_text("edited input must not be used")
    # Source edits do not alter the reconstruction recipe in captured artifacts.
    (notebook.root / "recipe.py").write_text("raise RuntimeError('edited source must not run')")
    restored = rpc(
        notebook.root,
        {"command": "training-restore", "snapshot": digest, "trust": True, "device": device},
    )
    assert restored["session_id"] != old
    assert restored["step"] == 1 and restored["cursor"] == {"batch": 1}
    run(
        notebook,
        """assert 'sentinel' not in globals()
import json
from pathlib import Path
# Fresh reconstruction retains the captured recipe module, despite edited project bytes.
from recipe import advance
actual = advance(training)
assert actual == json.loads(Path('expected.json').read_text()), (actual, Path('expected.json').read_text())
""",
    )
    again = rpc(notebook.root, {"command": "training-checkpoint"})
    assert again["step"] == 2
    ledger = Ledger(notebook.root / ".kl/ledger.sqlite", readonly=True)
    try:
        assert (
            ledger.one("SELECT state FROM sessions WHERE session_id=?", (old,))["state"] == "closed"
        )
        assert any(
            s["state"] == "restore_failed" for s in ledger.rows("SELECT state FROM sessions")
        )
    finally:
        ledger.close()


def test_busy_missing_plugin_and_native_refusal_preserve_worker(notebook):
    run(
        notebook,
        "sentinel = 42\nimport sys\nassert 'torch' not in sys.modules\nassert 'kernelledger' not in sys.modules",
    )
    with pytest.raises(ValueError, match="registered"):
        rpc(notebook.root, {"command": "training-checkpoint"})
    with concurrent.futures.ThreadPoolExecutor() as pool:
        future = pool.submit(run, notebook, "import time\ntime.sleep(.8)")
        for _ in range(100):
            if alive(notebook.root)["in_flight"]:
                break
            time.sleep(0.02)
        assert rpc(notebook.root, {"command": "training-checkpoint"}) == {"refused": "busy"}
        assert (
            rpc(notebook.root, {"command": "native-checkpoint"})["refused"]
            == "native_backend_unavailable"
        )
        future.result(timeout=10)
    run(notebook, "assert sentinel == 42\nassert 'torch' not in sys.modules")
    assert Helper(notebook.root).dispatch({"method": "training.list"})["items"] == []


def test_explicit_venv_interpreter_keeps_symlink_path(tmp_path):
    from kernelledger.supervisor import interpreter

    target = tmp_path / "system-python"
    target.touch()
    link = tmp_path / "venv-python"
    try:
        link.symlink_to(target)
    except OSError:
        pytest.skip("symlink creation requires OS support or developer mode")
    assert interpreter(tmp_path, str(link), {}) == (str(link), "command")
    assert interpreter(tmp_path, None, {"python": str(link)}) == (str(link), "config")
