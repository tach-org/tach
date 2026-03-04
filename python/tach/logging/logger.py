from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Any

logger = logging.getLogger("tach")
logger.setLevel(logging.INFO)


@dataclass
class CallInfo:
    function: str
    parameters: dict[str, Any] = field(default_factory=dict)
