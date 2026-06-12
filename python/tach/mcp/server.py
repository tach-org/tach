from __future__ import annotations

from mcp.server.fastmcp import FastMCP

MCP_PROTOCOL_VERSION = "2025-11-25"
DEFAULT_LIMIT = 50
DEFAULT_MAX_BYTES = 12_000

mcp = FastMCP(
    "tach",
    instructions=(
        "Use Tach to inspect and maintain Python module boundaries, dependency "
        "rules, public interfaces, dependency maps, reports, and affected-test scope."
    ),
)


def run() -> None:
    """Run Tach MCP server over stdio."""
    mcp.run()


__all__ = ["DEFAULT_LIMIT", "DEFAULT_MAX_BYTES", "MCP_PROTOCOL_VERSION", "mcp", "run"]
