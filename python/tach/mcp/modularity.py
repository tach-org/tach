from __future__ import annotations

import json
from contextlib import redirect_stdout
from dataclasses import asdict
from io import StringIO
from pathlib import Path
from typing import TYPE_CHECKING, Any, Literal

from tach.mcp.payloads import (
    DEFAULT_LIMIT,
    digest,
    payload_size,
    truncate_items,
)
from tach.mcp.project import (
    missing_config_result,
    project_ref,
    project_root_from_ref,
    resolve_project_root,
)
from tach.mcp.server import mcp
from tach.modularity import export_report, generate_modularity_report
from tach.parsing import parse_project_config

if TYPE_CHECKING:
    from tach.extension import ProjectConfig


def modularity_report_payload(
    root: Path, config: ProjectConfig, force: bool
) -> dict[str, Any]:
    with redirect_stdout(StringIO()):
        return asdict(generate_modularity_report(root, config, force=force))


def modularity_report_summary(
    payload: dict[str, Any],
    *,
    limit: int = DEFAULT_LIMIT,
    offset: int = 0,
) -> dict[str, Any]:
    modules = payload.get("modules", [])
    module_rows = [
        {
            "path": module.get("path"),
            "dependency_count": len(module.get("depends_on", [])),
            "has_interface": module.get("has_interface"),
            "layer": module.get("layer"),
        }
        for module in modules
    ]
    return {
        "repo": payload.get("repo"),
        "module_count": len(modules),
        "diagnostic_count": len(payload.get("diagnostics", [])),
        "usage_count": len(payload.get("usages", [])),
        "digest": digest(payload),
        "bytes": payload_size(payload),
        "modules": truncate_items(module_rows, limit=limit, offset=offset),
    }


@mcp.tool()
def tach_modularity(
    project_root: str | None = None,
    mode: Literal["summary", "full", "export"] = "summary",
    output_path: str | None = None,
    force: bool = True,
    limit: int = DEFAULT_LIMIT,
    offset: int = 0,
) -> dict[str, Any]:
    """Generate or export local Gauge modularity report."""
    try:
        root = resolve_project_root(project_root)
        config = parse_project_config(root)
        if config is None:
            return missing_config_result(mode, root)

        if mode == "export":
            output = Path(output_path).expanduser().resolve() if output_path else None
            with redirect_stdout(StringIO()):
                export_report(root, config, output, force=force)
            written_path = output or root / "modularity_report.json"
            return {
                "ok": True,
                "mode": mode,
                "output_path": str(written_path),
                "bytes": written_path.stat().st_size,
            }

        payload = modularity_report_payload(root, config, force=force)
        ref = project_ref(root)
        if mode == "full":
            return {
                "mode": mode,
                "digest": digest(payload),
                "resource_uri": f"tach://modularity-report/{ref}?view=full",
                **payload,
            }
        return {
            "mode": mode,
            "resource_uri": f"tach://modularity-report/{ref}?view=full",
            **modularity_report_summary(payload, limit=limit, offset=offset),
        }
    except Exception as exc:
        return {
            "ok": False,
            "mode": mode,
            "error": str(exc),
            "next_action": "Retry with force=true or use tach_lint/tach_map for local-only analysis.",
        }


@mcp.resource("tach://modularity-report/{project_ref}")
def modularity_report_resource(project_ref: str) -> str:
    """Expose Tach modularity report summary as a JSON resource."""
    root = project_root_from_ref(project_ref)
    return json.dumps(tach_modularity(project_root=str(root), mode="summary"), indent=2)


@mcp.resource("tach://modularity-report/{project_ref}?view={view}")
def modularity_report_view_resource(project_ref: str, view: str) -> str:
    """Expose Tach modularity report as summary or full JSON resource."""
    root = project_root_from_ref(project_ref)
    selected = "full" if view == "full" else "summary"
    return json.dumps(tach_modularity(project_root=str(root), mode=selected), indent=2)


@mcp.prompt()
def plan_tach_modularization(project_root: str = ".") -> str:
    """Prompt for planning module-boundary adoption or cleanup."""
    return (
        f"Analyze Tach config at {project_root}. Use tach_onboard, "
        "tach_map mode='summary' or mode='delta', and tach_lint. "
        "Recommend source roots, modules, dependencies, utilities, and interface "
        "boundaries."
    )
