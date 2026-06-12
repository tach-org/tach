from __future__ import annotations

import json
from hashlib import sha256
from typing import Any

DEFAULT_LIMIT = 50
DEFAULT_MAX_BYTES = 12_000


def digest(data: Any) -> str:
    payload = json.dumps(data, sort_keys=True, default=str, separators=(",", ":"))
    return sha256(payload.encode()).hexdigest()[:16]


def payload_size(data: Any) -> int:
    return len(json.dumps(data, default=str, separators=(",", ":")).encode())


def truncate_items(
    items: list[Any],
    *,
    limit: int = DEFAULT_LIMIT,
    offset: int = 0,
) -> dict[str, Any]:
    limit = max(0, limit)
    offset = max(0, offset)
    end = offset + limit
    return {
        "items": items[offset:end],
        "count": len(items),
        "offset": offset,
        "limit": limit,
        "truncated": end < len(items),
        "next_offset": end if end < len(items) else None,
    }


def tail_text(text: str, max_bytes: int = DEFAULT_MAX_BYTES) -> dict[str, Any]:
    max_bytes = max(0, max_bytes)
    encoded = text.encode()
    if len(encoded) <= max_bytes:
        return {"text": text, "bytes": len(encoded), "truncated": False}
    tail = encoded[-max_bytes:].decode(errors="replace")
    return {"text": tail, "bytes": len(encoded), "truncated": True}
