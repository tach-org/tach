from __future__ import annotations

from typing import Any

from tach import extension
from tach.mcp.project import (
    default_python_file,
    missing_config_result,
    resolve_project_path,
    resolve_project_root,
)
from tach.mcp.server import mcp
from tach.parsing import parse_project_config


@mcp.tool()
def tach_imports(
    path: str | None = None,
    project_root: str | None = None,
    external: bool = False,
) -> dict[str, Any]:
    """Inspect first-party or external imports Tach sees in one Python file."""
    root = resolve_project_root(project_root)
    config = parse_project_config(root)
    if config is None:
        return missing_config_result("imports", root)
    if path is None:
        path = default_python_file(root, config)
    if path is None:
        return {
            "ok": False,
            "mode": "imports",
            "error": "path is required, and no Python file was found under source_roots.",
        }
    target = resolve_project_path(root, path)
    source_roots = [root / source_root for source_root in config.source_roots]
    get_imports = (
        extension.get_external_imports if external else extension.get_project_imports
    )
    found_imports = get_imports(
        project_root=root,
        source_roots=source_roots,
        file_path=target,
        project_config=config,
    )
    return {
        "mode": "imports",
        "file": str(target),
        "external": external,
        "imports": [
            {"module_path": item.module_path, "line_number": item.line_number}
            for item in found_imports
        ],
    }
