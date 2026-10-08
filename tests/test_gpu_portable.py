import copy
import os
import random
import subprocess
import sys
from pathlib import Path

import pytest

from kernelledger.artifacts import STATE_SCHEMA, ArtifactStore
from kernelledger.experiments import capture_training, inspect_experiment, restore_training
from kernelledger.gpu.manifest import DTYPES, validate_state
from kernelledger.gpu.portable import PortableEncoder, inspect_state, restore_values


@pytest.fixture
def torch():
    return pytest.importorskip("torch")


def test_tensor_identity_views_parameter_gradient_and_empty(tmp_path, torch):
    store = ArtifactStore(tmp_path)
    tensor = torch.arange(30, dtype=torch.float64).reshape(5, 6)
    view = tensor[1:4, 1::2]
    parameter = torch.nn.Parameter(tensor)
    digest = PortableEncoder(store).capture(
        {
            "tensor": tensor,
            "alias": tensor,
            "view": view,
            "parameter": parameter,
            "empty": torch.empty(0, 3),
        }
    )
    restored = restore_values(store, digest)
    assert restored["tensor"] is restored["alias"]
    assert torch.equal(view, restored["view"])
    assert restored["view"].stride() == view.stride()
    assert restored["view"].storage_offset() == view.storage_offset()
    assert restored["parameter"].requires_grad
    assert isinstance(restored["parameter"], torch.nn.Parameter)
    assert restored["empty"].shape == (0, 3)
    restored["view"][0, 0] = -99
    assert restored["tensor"][1, 1].item() == -99
    assert restored["parameter"][1, 1].item() == -99
    assert len(inspect_state(store, digest)["storages"]) == 2


@pytest.mark.parametrize("dtype", list(DTYPES))
def test_dtype_exact_bytes(tmp_path, torch, dtype):
    store = ArtifactStore(tmp_path)
    value = torch.tensor([0, 1, 1], dtype=getattr(torch, dtype))
    digest = PortableEncoder(store).capture(value)
    restored = restore_values(store, digest)
    assert restored.dtype == value.dtype
    assert torch.equal(restored, value)


def test_refuses_unsupported_and_budget_before_writing(tmp_path, torch):
    store = ArtifactStore(tmp_path)
    with pytest.raises(ValueError, match="allocation limit"):
        PortableEncoder(store, max_bytes=8).capture(torch.ones(10))
    with pytest.raises(ValueError, match="layout"):
        PortableEncoder(store).capture(torch.eye(3).to_sparse())
    with pytest.raises(ValueError, match="portable type"):
        PortableEncoder(store).capture(torch.nn.Linear(2, 2))
    cycle = []
    cycle.append(cycle)
    with pytest.raises(ValueError, match="cyclic"):
        PortableEncoder(store).capture(cycle)
    assert not list(store.blobs.root.iterdir())


def test_malicious_metadata_bounds_dtype_cycle_and_allocation(tmp_path, torch):
    store = ArtifactStore(tmp_path)
    digest = PortableEncoder(store).capture(torch.zeros(3))
    original = store.metadata(digest)
    for field, value, match in [
        ("offset", 1000, "bounds"),
        ("dtype", "pickle", "dtype"),
        ("shape", [1 << 80], "dimension"),
        ("stride", [-1], "stride"),
    ]:
        manifest = copy.deepcopy(original)
        manifest["objects"]["o0"][field] = value
        with pytest.raises(ValueError, match=match):
            validate_state(manifest)
    with pytest.raises(ValueError, match="storage size|allocation"):
        validate_state(original, max_bytes=1)
    cycle = {
        "schema": STATE_SCHEMA,
        "kind": "portable-values",
        "endianness": sys.byteorder,
        "storages": {},
        "objects": {"o0": {"kind": "list", "items": [{"ref": "o0"}]}},
        "value": {"ref": "o0"},
    }
    with pytest.raises(ValueError, match="cyclic"):
        validate_state(cycle)


def test_reduce_never_runs(tmp_path):
    pytest.importorskip("torch")

    class Evil:
        def __reduce__(self):
            raise AssertionError("must never be called")

    with pytest.raises(ValueError, match="unsupported portable type"):
        PortableEncoder(ArtifactStore(tmp_path)).capture(Evil())


def test_autograd_and_unregistered_attached_gradients_refuse(tmp_path, torch):
    store = ArtifactStore(tmp_path)
    leaf = torch.tensor([2.0], requires_grad=True)
    with pytest.raises(ValueError, match="autograd graph"):
        PortableEncoder(store).capture(leaf.square())
    leaf.square().sum().backward()
    with pytest.raises(ValueError, match="gradients"):
        PortableEncoder(store).capture(leaf)
    assert not list(store.blobs.root.iterdir())


def test_inspection_in_process_that_denies_framework_imports(tmp_path, torch):
    store = ArtifactStore(tmp_path)
    digest = PortableEncoder(store).capture(torch.arange(4))
    script = """
import sys
class DenyFramework:
    def find_spec(self, fullname, *args):
        if fullname.split('.')[0] in ('torch', 'numpy'):
            raise AssertionError('inspection imported a framework')
sys.meta_path.insert(0, DenyFramework())
from pathlib import Path
from kernelledger.artifacts import ArtifactStore
from kernelledger.gpu.portable import inspect_state
from kernelledger.gpu.backends.capabilities import inventory
inspect_state(ArtifactStore(Path(sys.argv[1]), readonly=True), sys.argv[2])
assert inventory()['inspection_only']
"""
    env = dict(os.environ, PYTHONPATH=str(Path(__file__).parents[1] / "src"))
    result = subprocess.run(
        [sys.executable, "-c", script, str(tmp_path), digest],
        capture_output=True,
        text=True,
        env=env,
        timeout=15,
    )
    assert result.returncode == 0, result.stderr


def test_training_continuation_new_optimizer_rng_scheduler_cursor_and_clean_import(tmp_path, torch):
    np = pytest.importorskip("numpy")
    store = ArtifactStore(tmp_path / "donor")
    source = tmp_path / "source"
    source.mkdir()
    (source / "train.py").write_text(
        "# A tracked recipe; inspection never executes it\ndef build(): pass\n"
    )

    def factory():
        model = torch.nn.Sequential(
            torch.nn.Linear(3, 4), torch.nn.Dropout(0.2), torch.nn.Linear(4, 1)
        )
        optimizer = torch.optim.Adam(model.parameters(), lr=0.01)
        scheduler = torch.optim.lr_scheduler.StepLR(optimizer, step_size=1, gamma=0.9)
        return {"model": model, "optimizer": optimizer, "scheduler": scheduler}

    def train(objects):
        batch = torch.randn(5, 3) + random.random() + float(np.random.rand())
        objects["optimizer"].zero_grad()
        loss = objects["model"](batch).square().mean()
        loss.backward()
        objects["optimizer"].step()
        objects["scheduler"].step()
        return loss.detach().clone()

    original = factory()
    train(original)
    digest = capture_training(
        store,
        **original,
        step=1,
        cursor={"batch": 1},
        source_root=source,
        files=["train.py"],
        recipe_source="train.py",
        factory="build",
        boundary="idle",
    )
    report = inspect_experiment(store, digest)
    assert report["coverage"]["required"] == "complete"
    assert report["coverage"]["namespace"] == "not-captured"
    expected_loss = train(original)
    expected_weights = {
        name: value.clone() for name, value in original["model"].state_dict().items()
    }
    store.export(digest, tmp_path / "export")
    recipient = ArtifactStore(tmp_path / "recipient")
    assert recipient.import_directory(tmp_path / "export") == digest
    restored = restore_training(recipient, digest, factory, trust=True)
    assert restored["step"] == 1 and restored["cursor"] == {"batch": 1}
    assert restored["objects"]["optimizer"] is not original["optimizer"]
    assert restored["objects"]["model"] is not original["model"]
    actual_loss = train(restored["objects"])
    assert torch.equal(actual_loss, expected_loss)
    for name, value in restored["objects"]["model"].state_dict().items():
        assert torch.equal(value, expected_weights[name])
    another = restore_training(recipient, digest, factory, trust=True)
    assert another["objects"]["model"] is not restored["objects"]["model"]
    next(another["objects"]["model"].parameters()).data.zero_()
    assert not torch.equal(
        next(another["objects"]["model"].parameters()),
        next(restored["objects"]["model"].parameters()),
    )


def test_training_refuses_missing_scope_trust_and_stale_optimizer(tmp_path, torch):
    store = ArtifactStore(tmp_path / "store")
    (tmp_path / "train.py").write_text("def build(): pass\n")
    model = torch.nn.Linear(2, 2)
    optimizer = torch.optim.SGD(model.parameters(), lr=0.1)
    kwargs = dict(
        model=model,
        optimizer=optimizer,
        step=0,
        cursor={"batch": 0},
        source_root=tmp_path,
        files=["train.py"],
        recipe_source="train.py",
        factory="build",
    )
    with pytest.raises(ValueError, match="idle"):
        capture_training(store, **kwargs, boundary="busy")
    with pytest.raises(ValueError, match="cursor"):
        capture_training(store, **{**kwargs, "cursor": None}, boundary="idle")
    digest = capture_training(store, **kwargs, boundary="idle")

    def forbidden():
        raise AssertionError("factory must not run during preflight")

    with pytest.raises(ValueError, match="trust"):
        restore_training(store, digest, forbidden)
    with pytest.raises(ValueError, match="outside the registered model"):
        restore_training(
            store,
            digest,
            lambda: {"model": torch.nn.Linear(2, 2), "optimizer": optimizer},
            trust=True,
        )
    corrupted = store.metadata(digest)
    corrupted["requirements"]["torch"] = "changed"
    modified = store.put_state(corrupted)
    with pytest.raises(ValueError, match="drift"):
        restore_training(store, modified, forbidden, trust=True)


def test_source_staleness(tmp_path, torch):
    from kernelledger.git_state import experiment_status

    store = ArtifactStore(tmp_path / "store")
    (tmp_path / "train.py").write_text("def build(): pass\n")
    model = torch.nn.Linear(2, 2)
    digest = capture_training(
        store,
        model=model,
        optimizer=torch.optim.SGD(model.parameters(), lr=0.1),
        step=0,
        cursor=0,
        source_root=tmp_path,
        files=["train.py"],
        recipe_source="train.py",
        factory="build",
        boundary="idle",
    )
    assert experiment_status(store, digest, tmp_path)["source"] == "matching-stored-bytes"
    assert not experiment_status(store, digest, tmp_path)["coherent_commit_eligible"]
    (tmp_path / "train.py").write_text("def build(): return 1\n")
    assert experiment_status(store, digest, tmp_path)["source"] == "stale"


def test_failed_training_scope_has_no_roots_and_invalid_device_does_not_run_factory(
    tmp_path, torch
):
    store = ArtifactStore(tmp_path / "store")
    (tmp_path / "train.py").write_text("def build(): pass\n")
    model = torch.nn.Linear(2, 2)
    kwargs = dict(
        model=model,
        optimizer=torch.optim.SGD(model.parameters(), lr=0.1),
        step=0,
        source_root=tmp_path,
        files=["train.py"],
        recipe_source="train.py",
        factory="build",
        boundary="idle",
    )
    with pytest.raises(ValueError, match="portable type"):
        capture_training(store, **kwargs, cursor=object())
    assert not list((store.root / "roots").iterdir())
    digest = capture_training(store, **kwargs, cursor=0)

    def forbidden():
        raise AssertionError("invalid device must not execute a factory")

    with pytest.raises(ValueError, match="device"):
        restore_training(store, digest, forbidden, trust=True, device="unsupported")
    from kernelledger.experiments import materialize_source

    materialize_source(store, digest, tmp_path / "restored-source")
    assert (tmp_path / "restored-source/train.py").read_bytes() == (
        tmp_path / "train.py"
    ).read_bytes()
    with pytest.raises(ValueError, match="destination exists"):
        materialize_source(store, digest, tmp_path / "restored-source")


def test_json_schema_fixtures(tmp_path, torch):
    import json

    jsonschema = pytest.importorskip("jsonschema")
    store = ArtifactStore(tmp_path)
    artifact = store.put_bytes(b"bytes")
    state = PortableEncoder(store).capture({"weights": torch.ones(2)})
    schemas = Path(__file__).parents[1] / "schemas"
    for filename, value in [
        ("experimental-gpu-artifact-v1.schema.json", store.metadata(artifact)),
        ("experimental-gpu-portable-v1.schema.json", store.metadata(state)),
    ]:
        schema = json.loads((schemas / filename).read_text())
        jsonschema.Draft202012Validator.check_schema(schema)
        jsonschema.validate(value, schema)
