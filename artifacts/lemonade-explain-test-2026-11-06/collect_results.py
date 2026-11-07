"""Read-only evidence collection while the desktop owns both notebook stores."""
import hashlib
import json
import sqlite3
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
NOTEBOOK = ROOT / "artifacts/ui-candidates-2026-11-05/candidate-a/Sales-analysis-notebook"


def rows(path, query):
    with sqlite3.connect(path.resolve().as_uri() + "?mode=ro", uri=True) as connection:
        connection.row_factory = sqlite3.Row
        return [dict(row) for row in connection.execute(query)]


before = json.loads((OUT / "before-send.json").read_text(encoding="utf-8"))
files = {
    str(path.relative_to(NOTEBOOK)): hashlib.sha256(path.read_bytes()).hexdigest()
    for path in NOTEBOOK.rglob("*")
    if path.is_file() and ".kl" not in path.relative_to(NOTEBOOK).parts
}
assistant = NOTEBOOK / ".kl/assistant/assistant.sqlite"
ledger = NOTEBOOK / ".kl/ledger.sqlite"
requests = rows(
    assistant,
    "SELECT id,task,provider,model,destination,receipt_hash,status,error,usage,created,finished "
    "FROM requests ORDER BY created",
)
for request in requests:
    request["usage"] = json.loads(request["usage"])
    request["elapsed_seconds"] = (
        round(request["finished"] - request["created"], 3) if request["finished"] else None
    )
messages = rows(assistant, "SELECT request_id,role,text,citations FROM messages WHERE role='assistant'")
for message in messages:
    message["citations"] = json.loads(message["citations"])
proposals = rows(assistant, "SELECT id,request_id,payload,state FROM proposals")
for proposal in proposals:
    proposal["payload"] = json.loads(proposal["payload"])
execution_count = rows(ledger, "SELECT count(*) AS n FROM executions")[0]["n"]
applications = rows(assistant, "SELECT * FROM applications")
with urllib.request.urlopen("http://127.0.0.1:13305/api/v1/health", timeout=4) as response:
    health = json.load(response)
with urllib.request.urlopen("http://127.0.0.1:13305/api/v1/stats", timeout=4) as response:
    stats = json.load(response)
result = {
    "notebook": str(NOTEBOOK),
    "requests": requests,
    "messages": messages,
    "proposals": proposals,
    "applications": applications,
    "notebook_files_unchanged": files == before["files"],
    "notebook_file_hashes": files,
    "execution_count_before": before["ledger"]["executions"],
    "execution_count_after": execution_count,
    "no_new_executions": execution_count == before["ledger"]["executions"],
    "no_proposals_applied": not applications,
    "server_health": health,
    "server_last_request_stats": stats,
}
(OUT / "results.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
print(json.dumps({
    "requests": requests,
    "messages": messages,
    "proposals": proposals,
    "notebook_files_unchanged": result["notebook_files_unchanged"],
    "no_new_executions": result["no_new_executions"],
    "no_proposals_applied": result["no_proposals_applied"],
}, indent=2))
