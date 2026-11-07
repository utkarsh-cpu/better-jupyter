"""Conservative, observed capability probes; never assume vision or automation."""

import json
import time
from typing import Any

from .profile import profile_key
from .providers import Provider
from .store import Store
from .tools import RESPONSE_SCHEMA, check_schema


def probe(provider: Provider, store: Store, profile: Any = None) -> Any:
    output = provider.respond(
        [
            {"role": "system", "content": "Return JSON matching: " + json.dumps(RESPONSE_SCHEMA)},
            {
                "role": "user",
                "content": 'Return {"answer":"connected","citations":[],"proposals":[]}. No notebook data is provided.',
            },
        ],
        lambda _text: None,
    )
    check_schema(json.loads(output), RESPONSE_SCHEMA)
    result = {
        "streaming": True,
        "structured_output": True,
        "tool_calls": False,
        "vision": False,
        "fill_in_the_middle": False,
        "context_window": None,
        "checked": time.time(),
        "schema_enforced": provider.name == "openai"
        or provider.config.get("structured_output", False),
        "task_qualified": False,
        "profile": profile,
        "reason": "Observed SSE and one valid JSON response only; task quality, tools, images and agents are not qualified.",
    }
    key = (
        profile_key(profile)
        if profile is not None
        else provider.name + ":" + provider.destination["base_url"] + ":" + provider.model
    )
    with store.db:
        store.db.execute(
            "INSERT OR REPLACE INTO capabilities VALUES(?,?,?)",
            (key, json.dumps(result), time.time()),
        )
    return result
