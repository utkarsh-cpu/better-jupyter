"""Marimo-inspired ergonomic native UI helpers (`kl.ui.*`) backed by `kernelledger.widget/1`."""

from __future__ import annotations

import sys
from typing import Any

_ACTIVE_WORKER: Any = None


def _set_active_worker(worker: Any) -> None:
    global _ACTIVE_WORKER
    _ACTIVE_WORKER = worker
    if worker is None and hasattr(sys, "_kernelledger_active_worker"):
        delattr(sys, "_kernelledger_active_worker")


class _StaticWidget:
    """Fallback declarative snapshot when called outside a live interactive worker."""

    def __init__(self, state: dict[str, Any]) -> None:
        self._state = state
        self.value = state.get("value")
        self.kind = state.get("kind", "")
        self.label = state.get("label", "")

    def state(self) -> dict[str, Any]:
        return dict(self._state)

    def _repr_mimebundle_(self) -> dict[str, Any]:
        return {"application/vnd.kernelledger.widget+json": self.state()}

    def __repr__(self) -> str:
        return f"<{self.kind}: {self.label or self.value!r}>"


def _make_widget(kind: str, value: Any = None, **options: Any) -> Any:
    worker = (
        _ACTIVE_WORKER
        if _ACTIVE_WORKER is not None
        else getattr(sys, "_kernelledger_active_worker", None)
    )
    if worker is not None:
        return worker.widget(kind, value, **options)
    return _StaticWidget(
        {
            "schema": "kernelledger.widget/1",
            "handle": "",
            "worker_generation": "",
            "kind": kind,
            "value": value,
            **{k: v for k, v in options.items() if k != "callback"},
        }
    )


def slider(
    start: int | float,
    stop: int | float,
    *,
    value: int | float | None = None,
    step: int | float | None = None,
    label: str = "",
    callback: Any = None,
) -> Any:
    """Create a bounded integer or float slider widget."""
    use_float = any(isinstance(v, float) for v in (start, stop, value, step) if v is not None)
    initial = start if value is None else value
    if use_float:
        return _make_widget(
            "float_slider",
            float(initial),
            label=label,
            lower=float(start),
            upper=float(stop),
            step=float(step) if step is not None else max((float(stop) - float(start)) / 100.0, 1e-6),
            callback=callback,
        )
    return _make_widget(
        "integer_slider",
        int(initial),
        label=label,
        lower=int(start),
        upper=int(stop),
        callback=callback,
    )


def checkbox(value: bool = False, *, label: str = "", callback: Any = None) -> Any:
    return _make_widget("checkbox", bool(value), label=label, callback=callback)


def dropdown(
    options: list[str] | tuple[str, ...],
    *,
    value: str | None = None,
    label: str = "",
    callback: Any = None,
) -> Any:
    opts = list(options)
    if not opts:
        raise ValueError("dropdown requires at least one option")
    return _make_widget(
        "dropdown",
        opts[0] if value is None else value,
        label=label,
        options=opts,
        callback=callback,
    )


def multiselect(
    options: list[str] | tuple[str, ...],
    *,
    value: list[str] | None = None,
    label: str = "",
    callback: Any = None,
) -> Any:
    opts = list(options)
    return _make_widget(
        "multiselect",
        list(value or []),
        label=label,
        options=opts,
        callback=callback,
    )


def text(
    value: str = "",
    *,
    label: str = "",
    placeholder: str = "",
    callback: Any = None,
) -> Any:
    return _make_widget(
        "text",
        str(value),
        label=label,
        placeholder=placeholder,
        callback=callback,
    )


def button(*, label: str = "Run", callback: Any = None) -> Any:
    return _make_widget("button", None, label=label, callback=callback)


def progress(value: int = 0, *, total: int = 100, label: str = "") -> Any:
    return _make_widget("progress", int(value), label=label, lower=0, upper=int(total))


def stat(
    value: Any,
    *,
    label: str = "",
    caption: str = "",
    delta: str = "",
    direction: str = "neutral",
) -> Any:
    """Display a read-only KPI stat card (like `mo.stat`)."""
    return _make_widget(
        "stat",
        str(value),
        label=label,
        caption=caption,
        delta=delta,
        direction=direction,
    )


def callout(message: str, *, kind: str = "info", title: str = "") -> Any:
    """Display a styled callout box (`info`, `success`, `warning`, `danger`)."""
    return _make_widget(
        "callout",
        str(message),
        label=title,
        variant=kind,
    )


def form(
    fields: dict[str, Any],
    *,
    label: str = "Form",
    submit_label: str = "Submit",
    callback: Any = None,
) -> Any:
    """Bundle multiple widget controls into one explicit submit boundary (`mo.ui.form`)."""
    specs: list[dict[str, Any]] = []
    initial: dict[str, Any] = {}
    for key, ctrl in fields.items():
        if not isinstance(key, str) or not key:
            raise ValueError("Form field keys must be non-empty strings")
        st = ctrl.state() if hasattr(ctrl, "state") else dict(ctrl)
        specs.append(
            {
                "name": key,
                "kind": st.get("kind", "text"),
                "label": st.get("label") or key,
                "value": st.get("value"),
                "options": st.get("options", []),
                "lower": st.get("lower", 0),
                "upper": st.get("upper", 100),
                "step": st.get("step"),
                "placeholder": st.get("placeholder", ""),
            }
        )
        initial[key] = st.get("value")
    return _make_widget(
        "form",
        initial,
        label=label,
        submit_label=submit_label,
        fields=specs,
        callback=callback,
    )
