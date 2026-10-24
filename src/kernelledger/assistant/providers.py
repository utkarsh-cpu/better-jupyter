"""Bounded stdlib HTTP/SSE adapters. No redirects, proxies, or notebook effects."""

import http.client
import ipaddress
import json
import socket
import threading
import time
from collections.abc import Callable
from typing import Any
from urllib.parse import urlsplit

from ..source import SourceError
from .tools import RESPONSE_SCHEMA


def destination(base_url: str, allow_http_host: str | None = None) -> dict[str, Any]:
    url = urlsplit(base_url)
    if (
        url.scheme not in ("http", "https")
        or not url.hostname
        or url.username
        or url.password
        or url.query
        or url.fragment
    ):
        raise SourceError("invalid_request", "invalid provider URL")
    port = url.port or (443 if url.scheme == "https" else 80)
    addresses = sorted(
        {v[4][0] for v in socket.getaddrinfo(url.hostname, port, type=socket.SOCK_STREAM)}
    )
    local = bool(addresses) and all(ipaddress.ip_address(v).is_loopback for v in addresses)
    if not local and url.scheme != "https" and allow_http_host != url.hostname:
        raise SourceError(
            "policy", "remote providers require HTTPS or an explicit named HTTP host exception"
        )
    return {
        "host": url.hostname,
        "port": port,
        "scheme": url.scheme,
        "addresses": addresses,
        "base_url": base_url.rstrip("/"),
        "local": local,
        "label": "Local" if local else "Remote",
    }


class Provider:
    def __init__(
        self,
        name: str,
        config: Any,
        model: str,
        credential: str = "",
        timeout: int = 60,
        max_output: int = 2048,
        cancel: threading.Event | None = None,
    ):
        self.name, self.config, self.model = name, config, model
        if len(credential) > 4096 or "\n" in credential or "\r" in credential:
            raise SourceError("invalid_request", "invalid credential")
        self.credential, self.timeout, self.max_output = credential, timeout, max_output
        self.cancelled = cancel or threading.Event()
        self.connection: Any = None
        self.usage: dict[str, int] = {}
        self.destination: dict[str, Any] = (
            destination(
                config.get(
                    "base_url",
                    "http://127.0.0.1:11434/v1"
                    if name == "openai-compatible"
                    else "https://api.openai.com/v1",
                ),
                config.get("allow_http_host"),
            )
            if name != "fake"
            else {"host": "fake", "local": True, "label": "Local", "base_url": "fake"}
        )

    def cancel(self) -> None:
        self.cancelled.set()
        if self.connection:
            self.connection.close()

    def _check(self) -> None:
        if self.cancelled.is_set():
            raise SourceError("cancelled", "assistant request cancelled")

    def _open(self, path: str, body: Any = None) -> Any:
        self._check()
        dest = self.destination
        url = urlsplit(str(dest["base_url"]))
        for attempt in range(2):
            cls = (
                http.client.HTTPSConnection
                if dest["scheme"] == "https"
                else http.client.HTTPConnection
            )
            conn: Any = cls(dest["host"], dest["port"], timeout=self.timeout)

            # Pin the resolution approved in the receipt while preserving TLS hostname verification.
            def connect(_address: Any, timeout: Any, source_address: Any = None) -> socket.socket:
                return socket.create_connection(
                    (dest["addresses"][0], dest["port"]), timeout, source_address
                )

            conn._create_connection = connect
            self.connection = conn
            headers = {
                "Content-Type": "application/json",
                "Accept": "text/event-stream" if body else "application/json",
            }
            if self.credential:
                headers["Authorization"] = "Bearer " + self.credential
            try:
                conn.request(
                    "POST" if body is not None else "GET",
                    url.path.rstrip("/") + path,
                    json.dumps(body).encode() if body is not None else None,
                    headers,
                )
                response = conn.getresponse()
            except (OSError, http.client.HTTPException):
                conn.close()
                raise SourceError(
                    "provider_unavailable", "provider connection failed or timed out"
                ) from None
            if response.status in (429, 500, 502, 503, 504) and attempt == 0:
                conn.close()
                if self.cancelled.wait(0.2):
                    self._check()
                continue
            if response.status != 200:
                conn.close()
                # Provider error bodies can echo credentials or request data.
                raise SourceError("provider_error", f"provider returned HTTP {response.status}")
            return response
        raise SourceError("provider_error", "provider retry exhausted")

    def models(self) -> Any:
        if self.name == "fake":
            return [{"id": "fake"}]
        try:
            response = self._open("/models")
            raw = response.read(1048577)
            if len(raw) > 1048576:
                raise SourceError("too_large", "model catalog exceeds limit")
            return json.loads(raw).get("data", [])
        finally:
            if self.connection:
                self.connection.close()

    def respond(
        self, messages: Any, emit: Callable[[str], None], schema: Any = RESPONSE_SCHEMA
    ) -> str:
        self._check()
        if self.name == "fake":
            result = json.dumps(
                {
                    "answer": "Test provider connected. No notebook effects were performed.",
                    "citations": [],
                    "proposals": [],
                }
            )
            emit(result)
            return result
        body: dict[str, Any] = {"model": self.model, "stream": True}
        if self.name == "openai":
            body.update(
                input=messages,
                max_output_tokens=self.max_output,
                store=False,
                text={
                    "format": {
                        "type": "json_schema",
                        "name": "ledger_answer",
                        "strict": True,
                        "schema": schema,
                    }
                },
            )
            path = "/responses"
        else:
            body.update(
                messages=messages,
                max_tokens=self.max_output,
                stream_options={"include_usage": True},
            )
            if self.config.get("structured_output", False):
                body["response_format"] = {
                    "type": "json_schema",
                    "json_schema": {"name": "ledger_answer", "strict": True, "schema": schema},
                }
            path = "/chat/completions"
        output: list[str] = []
        size, wire_size = 0, 0
        started = time.monotonic()
        try:
            response = self._open(path, body)
            if "text/event-stream" not in response.getheader("Content-Type", ""):
                raise SourceError("unsupported_capability", "provider must support SSE streaming")
            data: list[str] = []
            completed = False
            while True:
                self._check()
                if time.monotonic() - started > self.timeout:
                    raise SourceError("timeout", "provider request exceeded deadline")
                line = response.readline(65537)
                wire_size += len(line)
                if len(line) > 65536 or wire_size > 4194304:
                    raise SourceError("too_large", "provider stream exceeds limit")
                if not line:
                    break
                if line.strip():
                    if line.startswith(b"data:"):
                        data.append(line[5:].decode("utf-8").strip())
                    continue
                payload, data = "\n".join(data), []
                if not payload:
                    continue
                if payload == "[DONE]":
                    completed = True
                    break
                event = json.loads(payload)
                delta = ""
                if self.name == "openai":
                    if event.get("type") in ("error", "response.failed", "response.incomplete"):
                        raise SourceError(
                            "provider_error", "provider response failed, refused, or was incomplete"
                        )
                    if event.get("type") == "response.output_text.delta":
                        delta = event.get("delta", "")
                    if event.get("type") == "response.completed":
                        self.usage = event.get("response", {}).get("usage", {})
                        completed = True
                else:
                    if event.get("error"):
                        raise SourceError("provider_error", "provider stream reported an error")
                    choices = event.get("choices", [])
                    if choices:
                        if choices[0].get("finish_reason") in (
                            "length",
                            "content_filter",
                            "tool_calls",
                        ):
                            raise SourceError(
                                "provider_error", "provider response was incomplete or unsupported"
                            )
                        delta = choices[0].get("delta", {}).get("content") or ""
                    if event.get("usage"):
                        self.usage = event["usage"]
                size += len(delta.encode())
                if size > min(262144, self.max_output * 32):
                    raise SourceError("too_large", "provider output exceeds limit")
                if delta:
                    output.append(delta)
                    emit(delta)
                if self.name == "openai" and completed:
                    break
            self._check()
            if not completed or not output:
                raise SourceError("provider_error", "provider stream ended before completion")
            return "".join(output)
        except (OSError, http.client.HTTPException, UnicodeError, json.JSONDecodeError):
            self._check()
            raise SourceError("provider_error", "invalid or interrupted provider stream") from None
        finally:
            if self.connection:
                self.connection.close()
