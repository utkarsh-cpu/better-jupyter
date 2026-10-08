import base64
import json
import shutil

import pytest

from kernelledger.notebook import Notebook, valid_id


@pytest.mark.parametrize(
    "value", ["CON", "con", "NUL", "COM1", "a/b", "../a", "", "_start", "x" * 65]
)
def test_invalid_ids(value):
    assert not valid_id(value)


def test_ipynb_round_trip_and_clone(tmp_path):
    n = Notebook(tmp_path / "source")
    n.init()
    doc = {
        "nbformat": 4,
        "metadata": {"custom": "value"},
        "cells": [
            {
                "id": "code",
                "cell_type": "code",
                "source": ["print(1)\r\n"],
                "metadata": {},
                "outputs": [],
                "execution_count": 12,
            },
            {
                "id": "intro",
                "cell_type": "markdown",
                "source": "![img](attachment:image.png)",
                "metadata": {"tag": True},
                "attachments": {"image.png": {"image/png": base64.b64encode(b"image").decode()}},
            },
            {"id": "raw", "cell_type": "raw", "source": "raw", "metadata": {}},
        ],
    }
    path = tmp_path / "input.ipynb"
    path.write_text(json.dumps(doc))
    n.import_ipynb(path)
    assert not n.doctor()["issues"]
    assert n.source(n.cell("code")) == "print(1)\n"
    shutil.copytree(n.root, tmp_path / "clone", ignore=shutil.ignore_patterns(".kl"))
    clone = Notebook(tmp_path / "clone")
    out = tmp_path / "output.ipynb"
    clone.export_ipynb(out)
    result = json.loads(out.read_text())
    assert result["metadata"] == doc["metadata"]
    assert result["cells"][1] == doc["cells"][1]
    assert result["cells"][0]["execution_count"] is None
    assert clone.read()["id"] == n.read()["id"]


def test_doctor_and_explicit_repair(tmp_path):
    n = Notebook(tmp_path)
    n.init()
    n.add("code", "x=1", "A")
    n.cell_path(n.cell("A")).write_bytes(b"\xef\xbb\xbfx=1\r\n")
    assert "noncanonical" in n.doctor()["issues"][0]
    doc = n.read()
    doc["cells"].append({"id": "A", "type": "code"})
    n.write(doc)
    with pytest.raises(ValueError):
        n.read()
    repair = n.doctor(repair=True)
    assert repair["repairs"][0]["old"] == "A"
    assert len(n.read()["cells"]) == 2


def test_attachment_with_multiple_mimes(tmp_path):
    n = Notebook(tmp_path / "notebook")
    n.init()
    attachment = {
        "image/png": base64.b64encode(b"png").decode(),
        "image/jpeg": base64.b64encode(b"jpeg").decode(),
    }
    doc = {
        "nbformat": 4,
        "metadata": {},
        "cells": [
            {
                "id": "image",
                "cell_type": "markdown",
                "metadata": {},
                "source": "attachment",
                "attachments": {"image.png": attachment},
            }
        ],
    }
    path = tmp_path / "input.ipynb"
    path.write_text(json.dumps(doc))
    n.import_ipynb(path)
    assert not n.doctor()["issues"]
    output = tmp_path / "output.ipynb"
    n.export_ipynb(output)
    assert (
        json.loads(output.read_text())["cells"][0]["attachments"] == doc["cells"][0]["attachments"]
    )
