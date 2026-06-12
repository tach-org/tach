from __future__ import annotations

from pathlib import Path
from typing import Any, Literal

from tach.mcp.payloads import DEFAULT_MAX_BYTES, digest, tail_text
from tach.mcp.project import (
    missing_config_result,
    project_ref,
    project_root_from_ref,
    resolve_project_root,
)
from tach.mcp.server import mcp
from tach.parsing import parse_project_config
from tach.show import (
    generate_module_graph_dot_string,
    generate_module_graph_mermaid_string,
)


@mcp.tool()
def tach_graph(
    project_root: str | None = None,
    format: Literal["mermaid", "dot"] = "mermaid",
    included_paths: list[str] | None = None,
    include_full: bool = False,
    max_bytes: int = DEFAULT_MAX_BYTES,
) -> dict[str, Any]:
    """Generate compact module graph summary; pass include_full=true for graph text."""
    root = resolve_project_root(project_root)
    config = parse_project_config(root)
    if config is None:
        return missing_config_result("graph", root)
    paths = [root / Path(path_item) for path_item in included_paths or []]
    if format == "dot":
        graph_text = generate_module_graph_dot_string(config, paths)
    else:
        graph_text = generate_module_graph_mermaid_string(config, paths)
    ref = project_ref(root)
    tail = tail_text(graph_text, max_bytes=max_bytes)
    result = {
        "mode": "graph",
        "format": format,
        "bytes": len(graph_text.encode()),
        "digest": digest(graph_text),
        "resource_uri": f"tach://project-graph/{ref}",
        "truncated": tail["truncated"],
    }
    if include_full:
        result["graph"] = graph_text
    else:
        result["graph_preview"] = tail["text"]
    return result


@mcp.resource("tach://project-graph/{project_ref}")
def graph_resource(project_ref: str) -> str:
    """Expose Tach project graph as a Mermaid resource."""
    root = project_root_from_ref(project_ref)
    return tach_graph(project_root=str(root), format="mermaid", include_full=True)[
        "graph"
    ]
