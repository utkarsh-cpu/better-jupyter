"""Credential-free effective inference identity, shared by receipts and probes."""

import hashlib
import json
from typing import Any
from urllib.parse import urlsplit, urlunsplit


def inference_profile(config: Any, task: str = "ask") -> dict[str, Any]:
    provider = config.get("provider")
    options = config.get("providers", {}).get(provider, {})
    return {
        "provider": provider,
        "model": config.get("task_models", {}).get(task) or config.get("model", ""),
        "base_url": options.get("base_url", "").rstrip("/"),
        "schema_enforced": provider == "openai" or options.get("structured_output", False),
        "enable_thinking": options.get("enable_thinking"),
        "context_window": options.get("context_window"),
        "max_output_tokens": config["max_output_tokens"],
        "context_bytes": config["context_bytes"],
        "timeout": config["timeout"],
        "probe_version": 2,
    }


def profile_key(profile: Any) -> str:
    return hashlib.sha256(json.dumps(profile, sort_keys=True).encode()).hexdigest()


def normalized_endpoint(value: str) -> str:
    url = urlsplit(value)
    host = (url.hostname or "").lower()
    if ":" in host:
        host = f"[{host}]"
    port = url.port
    authority = host + (
        f":{port}" if port and (url.scheme, port) not in (("http", 80), ("https", 443)) else ""
    )
    return urlunsplit((url.scheme.lower(), authority, url.path.rstrip("/"), "", ""))
