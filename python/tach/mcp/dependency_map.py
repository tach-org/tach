from __future__ import annotations

import json
import tempfile
from pathlib import Path
from typing import Any, Literal

from tach import extension
from tach.extension import Direction, ProjectConfig
from tach.filesystem.git_ops import get_changed_files
from tach.mcp.payloads import DEFAULT_LIMIT, digest, truncate_items
from tach.mcp.project import (
    missing_config_result,
    project_ref,
    project_root_from_ref,
    rel_project_path,
    resolve_project_path,
    resolve_project_root,
)
from tach.mcp.server import mcp
from tach.parsing import parse_project_config


def dependency_map_data(
    root: Path,
    config: ProjectConfig,
    direction: Literal["dependencies", "dependents"],
) -> dict[str, list[str]]:
    dep_map = extension.DependentMap(
        root,
        config,
        Direction.Dependencies if direction == "dependencies" else Direction.Dependents,
    )
    with tempfile.NamedTemporaryFile("r+", suffix=".json") as temp_file:
        dep_map.write_to_file(Path(temp_file.name))
        temp_file.seek(0)
        return json.load(temp_file)


def dependency_map_summary(
    data: dict[str, list[str]],
    *,
    limit: int = DEFAULT_LIMIT,
    offset: int = 0,
) -> dict[str, Any]:
    rows = [
        {"path": path, "edge_count": len(edges)} for path, edges in sorted(data.items())
    ]
    page = truncate_items(rows, limit=limit, offset=offset)
    return {
        "file_count": len(data),
        "edge_count": sum(len(edges) for edges in data.values()),
        "digest": digest(data),
        "entries": page,
        "truncated": page["truncated"],
        "next_offset": page["next_offset"],
    }


@mcp.tool()
def tach_map(
    project_root: str | None = None,
    mode: Literal["summary", "full", "closure", "delta", "changed_files"] = "summary",
    direction: Literal["dependencies", "dependents"] = "dependencies",
    path: str | None = None,
    changed: list[str] | None = None,
    base: str = "main",
    head: str | None = None,
    limit: int = DEFAULT_LIMIT,
    offset: int = 0,
) -> dict[str, Any]:
    """Inspect dependency map, closure, changed files, or affected-file delta."""
    root = resolve_project_root(project_root)
    config = parse_project_config(root)
    if config is None:
        return missing_config_result(mode, root)

    if mode == "changed_files":
        files = [
            str(file_path.relative_to(root))
            for file_path in get_changed_files(root, head=head, base=base)
        ]
        files.sort()
        page = truncate_items(files, limit=limit, offset=offset)
        return {
            "mode": mode,
            "project_root": str(root),
            "base": base,
            "head": head,
            "digest": digest(files),
            "files": page,
            "truncated": page["truncated"],
            "next_offset": page["next_offset"],
        }

    ref = project_ref(root)
    if mode == "closure":
        if path is None:
            return {
                "ok": False,
                "mode": mode,
                "error": "path is required for closure mode.",
            }
        dep_map = extension.DependentMap(
            root,
            config,
            Direction.Dependencies
            if direction == "dependencies"
            else Direction.Dependents,
        )
        rel_path = rel_project_path(root, path)
        return {
            "mode": mode,
            "direction": direction,
            "closure": {str(rel_path): sorted(dep_map.get_closure([rel_path]))},
        }

    if mode == "delta":
        if changed is None:
            changed_paths = get_changed_files(root, head=head, base=base)
        else:
            changed_paths = [resolve_project_path(root, item) for item in changed]
        rel_changed = sorted({item.relative_to(root) for item in changed_paths})
        dependents_data = dependency_map_data(root, config, "dependents")
        seed_paths = [item for item in rel_changed if str(item) in dependents_data]
        affected = {str(item) for item in rel_changed}
        if seed_paths:
            dependents_map = extension.DependentMap(root, config, Direction.Dependents)
            affected.update(dependents_map.get_closure(seed_paths))
        closure = sorted(affected)
        page = truncate_items(closure, limit=limit, offset=offset)
        snapshot = digest(
            {
                "base": base,
                "head": head,
                "changed": [str(item) for item in rel_changed],
            }
        )
        return {
            "mode": mode,
            "project_root": str(root),
            "base": base,
            "head": head,
            "changed_files": [str(item) for item in rel_changed],
            "affected_count": len(closure),
            "affected": page,
            "digest": digest(closure),
            "resource_uri": f"tach://delta/{ref}/{snapshot}",
            "truncated": page["truncated"],
            "next_offset": page["next_offset"],
        }

    data = dependency_map_data(root, config, direction)
    if mode == "full":
        return {
            "mode": mode,
            "direction": direction,
            "digest": digest(data),
            "resource_uri": f"tach://dependency-map/{ref}?view=full",
            "map": data,
        }
    return {
        "mode": mode,
        "direction": direction,
        "resource_uri": f"tach://dependency-map/{ref}?view=full",
        **dependency_map_summary(data, limit=limit, offset=offset),
    }


@mcp.resource("tach://dependency-map/{project_ref}")
def dependency_map_resource(project_ref: str) -> str:
    """Expose Tach dependency map summary as a JSON resource."""
    root = project_root_from_ref(project_ref)
    return json.dumps(tach_map(project_root=str(root), mode="summary"), indent=2)


@mcp.resource("tach://dependency-map/{project_ref}?view={view}")
def dependency_map_view_resource(project_ref: str, view: str) -> str:
    """Expose Tach dependency map as summary or full JSON resource."""
    root = project_root_from_ref(project_ref)
    selected = "full" if view == "full" else "summary"
    return json.dumps(tach_map(project_root=str(root), mode=selected), indent=2)


@mcp.resource("tach://delta/{project_ref}/{snapshot_or_ref}")
def delta_resource(project_ref: str, snapshot_or_ref: str) -> str:
    """Expose a compact dependency delta resource for a git base ref."""
    root = project_root_from_ref(project_ref)
    try:
        payload = tach_map(project_root=str(root), mode="delta", base=snapshot_or_ref)
    except Exception as exc:
        payload = {"error": str(exc), "snapshot_or_ref": snapshot_or_ref}
    return json.dumps(payload, indent=2)
