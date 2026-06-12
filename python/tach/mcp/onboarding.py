from __future__ import annotations

import json
from typing import Any, Literal

from tach import __version__
from tach.mcp.payloads import DEFAULT_LIMIT, digest
from tach.mcp.project import (
    compact_config_summary,
    config_summary,
    project_root_from_ref,
    resolve_project_root,
    resource_uris,
)
from tach.mcp.server import MCP_PROTOCOL_VERSION, mcp
from tach.parsing import parse_project_config


@mcp.tool()
def tach_onboard(
    project_root: str | None = None,
    intent: Literal["discover", "bootstrap", "operate"] = "discover",
    view: Literal["summary", "full"] = "summary",
    limit: int = DEFAULT_LIMIT,
    offset: int = 0,
) -> dict[str, Any]:
    """Start here: versions, install/use hints, resources, and optional config summary."""
    root = resolve_project_root(project_root)
    config = parse_project_config(root)
    result: dict[str, Any] = {
        "tach": __version__,
        "mcp_protocol": MCP_PROTOCOL_VERSION,
        "project_root": str(root),
        "intent": intent,
        "resources": resource_uris(root),
        "tools": {
            "tach_onboard": "discover project state and next actions",
            "tach_configure": "create/edit/sync Tach config or install pre-commit",
            "tach_lint": "run boundary/interface/external/unused dependency lint",
            "tach_imports": "inspect imports for one file",
            "tach_report": "generate dependency/usage/external reports",
            "tach_map": "dependency map, closure, changed files, or delta",
            "tach_graph": "module graph as Mermaid or DOT",
            "tach_modularity": "local Gauge modularity report summary/full/export",
            "tach_test": "run affected tests",
        },
        "client_command": "tach mcp",
    }
    if config is None:
        result["configured"] = False
        result["next_actions"] = [
            "Call tach_configure action='create_config' with source_roots and modules.",
            "Then call tach_configure action='sync_dependencies'.",
            "Then call tach_lint.",
        ]
        return result

    result["configured"] = True
    if view == "full":
        full = config_summary(config)
        result["project"] = {"view": "full", "digest": digest(full), **full}
    else:
        result["project"] = {
            "view": "summary",
            **compact_config_summary(root, config, limit=limit, offset=offset),
        }
    if intent == "bootstrap":
        result["next_actions"] = [
            "Call tach_lint to verify boundaries, external imports, and stale dependency rules.",
            "Call tach_map mode='delta' for changed files before large reviews.",
            "Call tach_test after boundary checks pass.",
        ]
    return result


@mcp.resource("tach://version")
def version_resource() -> str:
    """Expose Tach and MCP protocol version as a resource."""
    return json.dumps(tach_onboard(), indent=2)


@mcp.resource("tach://project-config/{project_ref}")
def config_resource(project_ref: str) -> str:
    """Expose full Tach project config as a JSON resource."""
    root = project_root_from_ref(project_ref)
    return json.dumps(tach_onboard(str(root), view="full")["project"], indent=2)


@mcp.resource("tach://project-summary/{project_ref}")
def project_summary_resource(project_ref: str) -> str:
    """Expose compact Tach project summary as a JSON resource."""
    root = project_root_from_ref(project_ref)
    return json.dumps(tach_onboard(str(root))["project"], indent=2)
