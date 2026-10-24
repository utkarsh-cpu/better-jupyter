"""CLI orchestration: reviewed proposals travel through the existing source service."""

import difflib
import getpass
import json
import os
from pathlib import Path
from typing import Any

from ..desktop.helper import Helper
from ..source import SourceError
from . import settings
from .credentials import credential, from_environment
from .service import Assistant


def add_parser(sub: Any) -> None:
    ai = sub.add_parser("ai", help="optional ledger assistant")
    commands = ai.add_subparsers(dest="ai_command", required=True)
    for name in ("status", "test", "purge", "summary", "logout"):
        commands.add_parser(name)
    login = commands.add_parser("login")
    login.add_argument("--provider", choices=["openai", "openai-compatible"], default="openai")
    configure = commands.add_parser("configure")
    configure.add_argument("--provider", choices=["openai", "openai-compatible"], required=True)
    configure.add_argument("--model", required=True)
    configure.add_argument("--base-url")
    configure.add_argument("--policy", choices=["off", "local_only", "allowed"])
    for name in ("ask", "explain", "compare", "summary"):
        if name == "summary":
            command = commands.choices[name]
        else:
            command = commands.add_parser(name)
        if name == "ask":
            command.add_argument("question")
        if name == "explain":
            command.add_argument("exec_id")
        if name == "compare":
            command.add_argument("first")
            command.add_argument("second")
        command.add_argument("--cell", action="append", default=[])
        command.add_argument("--exec", dest="executions", action="append", default=[])
        command.add_argument("--receipt", action="store_true")
        command.add_argument(
            "--approve",
            action="store_true",
            help="approve the printed remote context receipt for this invocation",
        )
    apply = commands.add_parser("apply")
    apply.add_argument("proposal_id")
    commands.add_parser("proposals")
    search = commands.add_parser("search")
    search.add_argument("query")
    export = commands.add_parser("export")
    export.add_argument("conversation_id")
    export.add_argument("path")
    sub.add_parser("mcp", help="read-only notebook MCP over stdio")


def execute(root: Path, args: Any) -> int:
    action = args.ai_command
    config = settings.load()
    provider = config["provider"] or "openai"
    if action in ("login", "logout"):
        provider = getattr(args, "provider", provider)
        credential(
            "set" if action == "login" else "delete",
            provider,
            getpass.getpass("API key (OS credential store): ") if action == "login" else "",
        )
        print("Credential stored" if action == "login" else "Credential removed")
        return 0
    if action == "configure":
        config.update(provider=args.provider, model=args.model)
        config["providers"] = {
            **config["providers"],
            args.provider: {
                "base_url": args.base_url
                or (
                    "https://api.openai.com/v1"
                    if args.provider == "openai"
                    else "http://127.0.0.1:11434/v1"
                )
            },
        }
        settings.save(config)
        if args.policy:
            assistant = Assistant(root)
            assistant.dispatch({"method": "policy.update", "policy": args.policy})
        print("Assistant configured")
        return 0
    assistant = Assistant(
        root,
        from_environment(provider) if config["provider"] in ("openai", "openai-compatible") else "",
    )
    try:
        if action in ("status", "test", "purge", "proposals", "search"):
            request = {
                "method": {"test": "providers.test", "proposals": "proposals.list"}.get(
                    action, action
                )
            }
            if action == "search":
                request["query"] = args.query
            result = assistant.dispatch(request)
            if action == "status" and any(
                os.environ.get(k) for k in ("KL_AI_API_KEY", "OPENAI_API_KEY")
            ):
                result["environment_notice"] = (
                    "Exported API keys can reach user code if a supervisor is started from this shell."
                )
            print(json.dumps(result, indent=2))
            return 0
        if action == "export":
            conversation = assistant.dispatch(
                {"method": "conversation.get", "conversation_id": args.conversation_id}
            )
            Path(args.path).write_text(
                "\n\n".join(
                    f"## {m['role']}\n\n{m['text']}\n\n{m['citations']}"
                    for m in conversation["messages"]
                ),
                encoding="utf-8",
            )
            return 0
        if action == "apply":
            proposals = assistant.dispatch({"method": "proposals.list"})["items"]
            item = next((p for p in proposals if p["id"] == args.proposal_id), None)
            if item is None:
                raise SourceError("not_found", "proposal not found")
            if item["payload"]["kind"] not in ("edit_cell", "insert_cells"):
                raise SourceError(
                    "unsupported",
                    "CLI apply supports source proposals; run proposals require desktop review, commands are copy-only",
                )
            result = assistant.dispatch(
                {"method": "proposal.resolve", "proposal_id": args.proposal_id, "state": "accepted"}
            )
            if not result["mutation"]:
                raise SourceError(
                    "unsupported",
                    "CLI apply supports source proposals; run proposals require desktop review, commands are copy-only",
                )
            snapshot = Helper(root).dispatch({"method": "source.mutate", **result["mutation"]})
            assistant.dispatch(
                {
                    "method": "application.record",
                    "proposal_id": args.proposal_id,
                    "result": {"manifest_revision": snapshot["manifest_revision"]},
                }
            )
            print("Proposal applied")
            return 0
        question = getattr(
            args,
            "question",
            {
                "explain": "Explain the recorded error and propose a fix.",
                "compare": "Compare these executions; distinguish observations from suggested causes.",
                "summary": "Summarize where I left off, citing recorded evidence.",
            }.get(action, ""),
        )
        executions = args.executions + (
            [args.exec_id]
            if action == "explain"
            else [args.first, args.second]
            if action == "compare"
            else []
        )
        receipt = assistant.dispatch(
            {
                "method": "context.preview",
                "task": action,
                "question": question,
                "cell_ids": args.cell,
                "exec_ids": executions,
            }
        )
        print(json.dumps(receipt, indent=2))
        if args.receipt:
            return 0
        if not receipt["destination"]["local"] and not args.approve:
            raise SourceError(
                "approval_required",
                "inspect --receipt, then use --approve to authorize remote context",
            )
        result = assistant.dispatch(
            {
                "method": "respond",
                "receipt_hash": receipt["hash"],
                "approved_receipt": receipt["hash"] if args.approve else "",
            }
        )
        print(result["answer"])
        print(result["evidence_notice"])
        for citation in result["citations"]:
            print(json.dumps(citation))
        snapshot = assistant.tools.read("source.read")
        for proposal in result["proposals"]:
            print(json.dumps(proposal, indent=2))
            payload = proposal["payload"]
            if payload["kind"] == "edit_cell":
                old = next(c["source"] for c in snapshot["cells"] if c["id"] == payload["cell_id"])
                print(
                    "".join(
                        difflib.unified_diff(
                            old.splitlines(True),
                            payload["source"].splitlines(True),
                            fromfile="current",
                            tofile="proposed",
                        )
                    )
                )
        return 0
    finally:
        assistant.close()
