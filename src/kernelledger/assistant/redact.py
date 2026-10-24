"""Best-effort redaction, applied before receipts, network requests, and persistence."""

import re
from typing import Any, cast

PATTERNS = (
    r"-----BEGIN [^-]*PRIVATE KEY-----[\s\S]*?-----END [^-]*PRIVATE KEY-----",
    r"\b(?:sk-[A-Za-z0-9_-]{12,}|gh[pousr]_[A-Za-z0-9_]{12,}|AKIA[A-Z0-9]{16})\b",
    r"(?i)\bBearer\s+[^\s\"',;]+",
    r"(?i)(?:[a-z][a-z0-9+.-]*://)[^\s/:@]+:[^\s/@]+@",
    r"(?im)(?:api[_-]?key|password|secret|access_token|refresh_token)\s*[:=]\s*[\"']?[^\s\"',;]+",
)


def redact(text: str, denylist: list[str] | None = None, secrets: tuple[str, ...] = ()) -> str:
    for secret in secrets:
        if secret:
            text = text.replace(secret, "[REDACTED]")
    for pattern in PATTERNS:
        text = re.sub(pattern, "[REDACTED]", text)
    for name in denylist or []:
        # A named binding may have multiline representations: withhold the whole item.
        if re.search(r"\b" + re.escape(name) + r"\b", text):
            return "[REDACTED: checkpoint denylist binding]"
    return text


def clean(value: Any, denylist: list[str] | None = None, secrets: tuple[str, ...] = ()) -> Any:
    if isinstance(value, str):
        return redact(value, denylist, secrets)
    if isinstance(value, list):
        return [clean(v, denylist, secrets) for v in cast(list[Any], value)]
    if isinstance(value, dict):
        return {
            k: "[REDACTED: checkpoint denylist binding]"
            if k in (denylist or [])
            else clean(v, denylist, secrets)
            for k, v in cast(dict[str, Any], value).items()
        }
    return value
