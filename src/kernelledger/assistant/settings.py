"""Versioned machine settings and notebook policy; no credentials on disk."""

import json
import os
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

from ..source import SourceError, atomic


def settings_path() -> Path:
    if os.name == "nt":
        return (
            Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData/Local"))
            / "KernelLedger/assistant.json"
        )
    return (
        Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
        / "kernelledger/assistant.json"
    )


DEFAULTS: dict[str, Any] = {
    "version": 1,
    "provider": None,
    "model": "",
    "task_models": {},
    "providers": {},
    "max_output_tokens": 2048,
    "daily_token_cap": 100000,
    "context_bytes": 131072,
    "timeout": 60,
    "retention_days": 90,
    "prices": {},
    "approvals": {},
}


def load(path: Path | None = None) -> dict[str, Any]:
    target = path or settings_path()
    data: Any = json.loads(target.read_bytes()) if target.exists() else {}
    if not isinstance(data, dict):
        raise SourceError("invalid_request", "assistant settings must be an object")
    result: dict[str, Any] = {**json.loads(json.dumps(DEFAULTS)), **data}
    validate(result)
    return result


def validate(data: dict[str, Any]) -> None:
    if data.get("version") != 1 or data.get("provider") not in (
        None,
        "openai",
        "openai-compatible",
        "fake",
    ):
        raise SourceError("invalid_request", "unsupported assistant settings or provider")
    if not isinstance(data.get("model"), str) or len(data["model"]) > 256:
        raise SourceError("invalid_request", "invalid model")
    for key, low, high in (
        ("max_output_tokens", 16, 32768),
        ("daily_token_cap", 1, 100000000),
        ("context_bytes", 1024, 262144),
        ("timeout", 1, 300),
        ("retention_days", 1, 3650),
    ):
        if type(data.get(key)) is not int or not low <= data[key] <= high:
            raise SourceError("invalid_request", f"invalid {key}")
    if set(data) - set(DEFAULTS):
        raise SourceError(
            "invalid_request", "unknown assistant setting; credentials belong in the OS store"
        )
    for field in ("providers", "approvals", "prices", "task_models"):
        if not isinstance(data.get(field), dict):
            raise SourceError("invalid_request", f"{field} must be an object")
    for task, model_name in data["task_models"].items():
        if not isinstance(task, str) or not isinstance(model_name, str) or len(model_name) > 256:
            raise SourceError("invalid_request", "invalid task model mapping")
    for name, config in data["providers"].items():
        if name not in ("openai", "openai-compatible", "fake") or set(config) - {
            "base_url",
            "allow_http_host",
            "structured_output",
        }:
            raise SourceError("invalid_request", "invalid provider settings")
        base_url = config.get("base_url", "https://api.openai.com/v1")
        if not isinstance(base_url, str) or len(base_url) > 2048:
            raise SourceError("invalid_request", "invalid endpoint")
        url = urlsplit(base_url)
        if (
            url.scheme not in ("https", "http")
            or not url.hostname
            or url.username
            or url.password
            or url.query
            or url.fragment
        ):
            raise SourceError(
                "invalid_request", "endpoint must not contain credentials, query, or fragment"
            )
    for key, entry in data["approvals"].items():
        if not isinstance(key, str) or not isinstance(entry, (dict, bool, str)):
            raise SourceError("invalid_request", "invalid receipt auto-approval entry")


def save(data: dict[str, Any], path: Path | None = None) -> None:
    validate(data)
    atomic(path or settings_path(), (json.dumps(data, indent=2) + "\n").encode())


def policy(root: Path) -> tuple[str, dict[str, Any]]:
    path = root / ".kl/config.json"
    config: dict[str, Any] = json.loads(path.read_bytes()) if path.exists() else {}
    value = config.get("assistant", "allowed")
    if value not in ("off", "local_only", "allowed"):
        raise SourceError("invalid_request", "assistant policy must be off, local_only, or allowed")
    return value, config
