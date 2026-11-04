"""Artifact-integrity and release-policy tests that need no compiled daemon."""

import copy
import hashlib
import json
import runpy
import zipfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
GATES = runpy.run_path(str(ROOT / "scripts/native_gates.py"))
PACKAGE = runpy.run_path(str(ROOT / "scripts/native_package.py"))


def test_default_gate_requires_full_evidence_and_two_distinct_candidates():
    record = json.loads((ROOT / "docs/runtime-v6-gates.json").read_text())
    assert GATES["blockers"](record, 8)
    assert record["default_backend"] == "python"
    complete = copy.deepcopy(record)
    for phase in complete["phases"]:
        phase.update(status="passed", remaining=[], evidence=["qualification-record"])
    assert GATES["blockers"](complete, 8)
    candidate = {
        "id": "rc1",
        "status": "passed",
        "platforms": ["linux", "windows"],
        "evidence": ["qualification-record"],
    }
    complete["release_candidates"] = [candidate, copy.deepcopy(candidate)]
    assert GATES["blockers"](complete, 8)
    complete["release_candidates"][1]["id"] = "rc2"
    assert not GATES["blockers"](complete, 8)
    complete["phases"].pop()
    with pytest.raises(ValueError, match="exactly once"):
        GATES["blockers"](complete, 8)


@pytest.mark.parametrize("tamper", ["none", "bytes", "unexpected", "path"])
def test_native_archive_checks_content_and_paths(tmp_path, tamper):
    name = "../kernelledger-daemon" if tamper == "path" else "kernelledger-daemon"
    manifest = {
        "schema": "kernelledger.native-package/1",
        "files": {name: {"sha256": hashlib.sha256(b"binary").hexdigest(), "byte_size": 6}},
    }
    archive = tmp_path / "package.zip"
    with zipfile.ZipFile(archive, "w") as output:
        output.writestr("manifest.json", json.dumps(manifest))
        output.writestr(name, b"broken" if tamper == "bytes" else b"binary")
        if tamper == "unexpected":
            output.writestr("unexpected", b"x")
    if tamper == "none":
        assert PACKAGE["verify"](archive) == manifest
    else:
        with pytest.raises(ValueError):
            PACKAGE["verify"](archive)
