"""Build a real notebook and verify capture/restore through supervisor commands."""

import argparse
import hashlib
import json
import sys
import time
from pathlib import Path

from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.notebook import Notebook


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--device", default="cpu", choices=("cpu", "cuda:0"))
    args = parser.parse_args()
    root = args.out.resolve()
    if root.exists():
        raise ValueError("use a new notebook directory")
    notebook = Notebook(root)
    notebook.init()
    recipe = Path(__file__).with_name("notebook_recipe.py").read_bytes()
    (root / "recipe.py").write_bytes(recipe)
    setup = notebook.add(
        "code",
        f"""import torch
from pathlib import Path
from recipe import build, advance
from kernelledger.gpu.notebook import register
torch.manual_seed(1309)
objects = build()
objects['model'].to({args.device!r})
training = register(objects=objects, source_root=Path.cwd(), files=['recipe.py'], recipe_source='recipe.py', factory='build', cursor={{'batch':0}})
advance(training)
""",
        "register-training",
    )
    notebook.add("code", "from recipe import advance\nadvance(training)", "next-step")
    # Remember the interpreter for desktop runs of this generated notebook.
    (root / ".kl/config.json").write_text(json.dumps({"python": sys.executable}))
    ensure_supervisor(root, sys.executable)

    def execute(source):
        result = rpc(root, {"command": "run", "source": source})
        if result["status"] != "ok":
            raise ValueError(f"notebook execution failed: {result}; inspect its recorded events")
        return result

    try:
        first = rpc(root, {"command": "run", "cell_id": setup})
        if first["status"] != "ok":
            raise ValueError("registration failed; inspect the notebook execution history")
        started = time.perf_counter()
        capture = rpc(root, {"command": "training-checkpoint"})
        capture_seconds = time.perf_counter() - started
        execute("import json\nPath('expected.json').write_text(json.dumps(advance(training)))")
        started = time.perf_counter()
        restore = rpc(
            root,
            {
                "command": "training-restore",
                "snapshot": capture["snapshot"],
                "trust": True,
                "device": args.device,
            },
        )
        restore_seconds = time.perf_counter() - started
        execute("""import json
from pathlib import Path
from importlib.util import spec_from_file_location, module_from_spec
spec = spec_from_file_location('restored_recipe', training.source_root / 'recipe.py')
recipe = module_from_spec(spec)
spec.loader.exec_module(recipe)
actual = recipe.advance(training)
assert actual == json.loads(Path('expected.json').read_text())
""")
        result = {
            "schema": "kernelledger.notebook-training-qualification/1",
            "device": args.device,
            "snapshot": capture["snapshot"],
            "capture_seconds": capture_seconds,
            "restore_seconds": restore_seconds,
            "fresh_worker": first["session_id"] != restore["session_id"],
            "exact_next_step": True,
            "coverage": capture["coverage"],
            "native_qualified": False,
            "recipe_sha256": hashlib.sha256(recipe).hexdigest(),
            "python": sys.version,
            "stored_bytes": sum(
                p.stat().st_size for p in (root / ".kl/training-artifacts/blobs").iterdir()
            ),
        }
        (root / "qualification.json").write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result, indent=2))
    finally:
        if alive(root):
            rpc(root, {"command": "stop"})


if __name__ == "__main__":
    main()
