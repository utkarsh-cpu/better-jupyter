"""Read-only, bounded replay of session-scoped display updates.

The raw ledger and execution-local History views remain immutable. A current
view may replace a visible display only with a later update in the same session.
"""

import json
import math
from typing import Any, cast

SCAN_LIMIT = 128
UPDATE_LIMIT = 16
PAYLOAD_LIMIT = 65536
PAGE_BUDGET = 262144
ROWID_LIMIT = 2**63 - 1


def valid_id(value: Any) -> bool:
    return (
        type(value) is str
        and 0 < len(value) <= 256
        and all(ord(c) >= 32 and not 0xD800 <= ord(c) <= 0xDFFF for c in value)
        and len(value.encode("utf-8")) <= 256
    )


def _nonfinite(value: str) -> Any:
    raise ValueError(f"Non-finite JSON: {value}")


def _integer(value: str) -> int:
    number = int(value)
    if not -(2**63) <= number <= 2**64 - 1:
        raise ValueError("Display JSON integer exceeds native precision")
    return number


def _float(value: str) -> float:
    number = float(value)
    if not math.isfinite(number):
        raise ValueError("Non-finite display JSON")
    return number


def _bounded_json(raw: str) -> Any:
    depth, quoted, escaped = 0, False, False
    for char in raw:
        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
        elif char == '"':
            quoted = True
        elif char in "[{":
            depth += 1
            if depth > 64:
                raise ValueError("Display JSON exceeds 64 nesting levels")
        elif char in "]}":
            depth -= 1
    return json.loads(raw, parse_constant=_nonfinite, parse_int=_integer, parse_float=_float)


def update_page(ledger: Any, request: Any) -> Any:
    after, through = request["after_rowid"], request["through_rowid"]
    sql = (
        "SELECT ev.rowid AS event_rowid,ev.kind='display_update' AS is_update,"
        "CASE WHEN length(CAST(ev.exec_id AS BLOB))<=128 THEN ev.exec_id END AS exec_id,"
        "CASE WHEN typeof(ev.seq)='integer' AND ev.seq>0 THEN ev.seq END AS seq,"
        "CASE WHEN length(CAST(ev.timestamp AS BLOB))<=128 THEN ev.timestamp END AS timestamp,"
        "CASE WHEN length(CAST(x.session_id AS BLOB))<=128 THEN x.session_id END AS session_id,"
        "CASE WHEN typeof(x.ordinal)='integer' AND x.ordinal>0 THEN x.ordinal END AS ordinal,"
        "CASE WHEN ev.kind='display_update' THEN length(CAST(ev.payload_json AS BLOB)) ELSE 0 END AS payload_bytes,"
        "CASE WHEN ev.kind='display_update' AND length(CAST(ev.payload_json AS BLOB))<=65536 "
        "THEN ev.payload_json END AS payload_json "
        "FROM events ev JOIN executions x ON x.exec_id=ev.exec_id "
        "WHERE ev.rowid>? AND ev.rowid<=?"
    )
    args: tuple[Any, ...] = (after, through)
    if request.get("session_id"):
        sql += " AND x.session_id=?"
        args += (request["session_id"],)
    rows = ledger.rows(sql + " ORDER BY ev.rowid LIMIT 128", args)
    items: list[Any] = []
    used = 0
    for row in rows:
        if row["is_update"] == 1:
            cost = min(row["payload_bytes"], PAYLOAD_LIMIT) + 4096
            if len(items) >= UPDATE_LIMIT or used + cost > PAGE_BUDGET:
                break
            item = {
                key: row[key]
                for key in ("event_rowid", "exec_id", "session_id", "ordinal", "seq", "timestamp")
            }
            if any(
                not isinstance(row[key], str) or not row[key]
                for key in ("exec_id", "session_id", "timestamp")
            ) or any(type(row[key]) is not int for key in ("ordinal", "seq")):
                item["omitted"] = "invalid_metadata"
            elif row["payload_json"] is None:
                item["omitted"] = "oversized_payload"
            else:
                try:
                    event = _bounded_json(row["payload_json"])
                    if not isinstance(event, dict):
                        raise ValueError("Display event must be an object")
                    event = cast(dict[str, Any], event)
                    # Python's decoder otherwise permits unpaired surrogates.
                    json.dumps(event, ensure_ascii=False).encode("utf-8")
                    if valid_id(event.get("display_id")):
                        item["event"] = {**event, "kind": "display_update"}
                    else:
                        item["omitted"] = "invalid_display_id"
                except (ValueError, UnicodeError, RecursionError):
                    item["omitted"] = "invalid_payload"
            used += cost
            items.append(item)
        after = row["event_rowid"]
    else:
        if len(rows) < SCAN_LIMIT:
            after = through
    return {"items": items, "after_rowid": after, "through_rowid": through, "more": after < through}


def apply_update(view: Any, update: Any) -> bool:
    if view["session_id"] != update["session_id"]:
        return False
    event = update["event"]
    changed = False
    stamp = (update["ordinal"], update["seq"])
    for index, previous in enumerate(view["outputs"]):
        if previous.get("kind") not in ("display", "result"):
            continue
        if previous.get("display_id") != event["display_id"]:
            continue
        origin = previous.get("display_update_origin", {})
        baseline = (origin.get("ordinal", view["ordinal"]), origin.get("seq", previous["seq"]))
        if stamp <= baseline:
            continue
        view["outputs"][index] = {
            **event,
            "kind": "display",
            "seq": previous["seq"],
            "timestamp": previous["timestamp"],
            "display_update_origin": {
                key: update[key] for key in ("exec_id", "ordinal", "seq", "timestamp")
            },
        }
        changed = True
    return changed


def session_views(ledger: Any, session_id: str) -> Any:
    """Fold final notebook cells, then replay updates without materializing the feed."""
    own_snapshot = not ledger.db.in_transaction
    if own_snapshot:
        ledger.db.execute("BEGIN DEFERRED")
    try:
        return _session_views(ledger, session_id)
    finally:
        if own_snapshot:
            ledger.db.rollback()


def _session_views(ledger: Any, session_id: str) -> Any:
    from .ledger import fold

    views = {
        row["cell_id"]: {**row, "outputs": fold(ledger.events(row["exec_id"]))}
        for row in ledger.rows(
            "SELECT * FROM executions WHERE session_id=? AND cell_id IS NOT NULL ORDER BY ordinal",
            (session_id,),
        )
    }
    through = ledger.rows("SELECT COALESCE(MAX(rowid),0) AS last FROM events")[0]["last"]
    after = 0
    while after < through:
        page = update_page(
            ledger, {"after_rowid": after, "through_rowid": through, "session_id": session_id}
        )
        for update in page["items"]:
            if update.get("omitted"):
                raise ValueError(
                    "Cannot fold this display update for export: "
                    + update["omitted"]
                    + "; raw events remain available in History and bundles"
                )
            for view in views.values():
                apply_update(view, update)
        after = page["after_rowid"]
    return views.values()
