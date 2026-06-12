from __future__ import annotations

from typing import Any

from tach.mcp.payloads import DEFAULT_MAX_BYTES, tail_text
from tach.mcp.project import (
    default_python_file,
    missing_config_result,
    resolve_project_path,
    resolve_project_root,
)
from tach.mcp.server import mcp
from tach.parsing import parse_project_config
from tach.report import external_dependency_report, report


@mcp.tool()
def tach_report(
    path: str | None = None,
    project_root: str | None = None,
    external: bool = False,
    dependencies: bool = True,
    usages: bool = True,
    raw: bool = False,
    dependency_modules: list[str] | None = None,
    usage_modules: list[str] | None = None,
    max_bytes: int = DEFAULT_MAX_BYTES,
) -> dict[str, Any]:
    """Generate Tach dependency, usage, and optional external dependency report."""
    root = resolve_project_root(project_root)
    config = parse_project_config(root)
    if config is None:
        return missing_config_result("report", root)
    if path is None:
        path = default_python_file(root, config)
    if path is None:
        return {
            "ok": False,
            "mode": "report",
            "error": "path is required, and no module or Python file was found.",
        }
    target = resolve_project_path(root, path)
    reports: list[str] = []
    if dependencies or usages:
        reports.append(
            report(
                root,
                target,
                project_config=config,
                include_dependency_modules=dependency_modules,
                include_usage_modules=usage_modules,
                skip_dependencies=not dependencies,
                skip_usages=not usages,
                raw=raw,
            )
        )
    if external:
        reports.append(
            external_dependency_report(root, target, project_config=config, raw=raw)
        )
    tail = tail_text("\n".join(reports), max_bytes=max_bytes)
    return {
        "mode": "report",
        "path": str(target),
        "report": tail["text"],
        "report_bytes": tail["bytes"],
        "report_truncated": tail["truncated"],
    }
