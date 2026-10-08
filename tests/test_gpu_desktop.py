"""Desktop training effects retain operation identity and safe restore admission."""

import uuid

import pytest
from test_desktop import Client, project  # noqa: F401 -- shared runtime fixture

from kernelledger.cli import alive, rpc


def test_training_desktop_durable_capture_and_restore(project):  # noqa: F811 -- imported fixture
    pytest.importorskip("torch")
    (project.root / "recipe.py").write_text("""import torch
def build():
    model = torch.nn.Linear(2, 1)
    return {'model': model, 'optimizer': torch.optim.Adam(model.parameters())}
""")
    result = rpc(
        project.root,
        {
            "command": "run",
            "source": """from pathlib import Path
from recipe import build
from kernelledger.gpu.notebook import register
training = register(objects=build(), source_root=Path.cwd(), files=['recipe.py'], recipe_source='recipe.py', factory='build', cursor={'batch':0})
sentinel = 73
""",
        },
    )
    assert result["status"] == "ok"
    client = Client(project.root)
    try:
        assert "portable_training" in client.welcome["capabilities"]
        key = str(uuid.uuid4())
        accepted = client.call("training-checkpoint", client_request_id=key)
        repeated = client.call("training-checkpoint", client_request_id=key)
        assert accepted["operation_id"] == repeated["operation_id"]
        outcome = client.wait(accepted["operation_id"])
        assert outcome["state"] == "completed", outcome
        snapshot = outcome["result"]["snapshot"]
        assert (
            client.call("training.inspect", snapshot=snapshot)["coverage"]["native"]
            == "not-captured"
        )
        assert len(client.call("training.list")["items"]) == 1
        assert not client.call("native.status")["native_qualified"]
        old = alive(project.root)["session_id"]
        refused = client.wait(client.submit("training-restore", snapshot=snapshot)["operation_id"])
        assert refused["state"] == "failed" and alive(project.root)["session_id"] == old
        restored = client.wait(
            client.submit("training-restore", snapshot=snapshot, trust=True, device="cpu")[
                "operation_id"
            ]
        )
        assert restored["state"] == "completed", restored
        assert restored["result"]["session_id"] != old
        assert (
            rpc(
                project.root,
                {
                    "command": "run",
                    "source": "assert 'sentinel' not in globals()\nassert training.status['cursor'] == {'batch':0}",
                },
            )["status"]
            == "ok"
        )
    finally:
        client.close()
