from __future__ import annotations

from typing import Any

from tach import extension
from tach.check_external import check_external
from tach.errors import TachCircularDependencyError, TachVisibilityError
from tach.mcp.payloads import DEFAULT_LIMIT, truncate_items
from tach.mcp.project import project_config, resolve_project_root
from tach.mcp.server import mcp


def diagnostics_to_data(diagnostics: list[Any]) -> list[dict[str, Any]]:
    data: list[dict[str, Any]] = []
    for diagnostic in diagnostics:
        usage_module = (
            diagnostic.usage_module() if hasattr(diagnostic, "usage_module") else None
        )
        definition_module = (
            diagnostic.definition_module()
            if hasattr(diagnostic, "definition_module")
            else None
        )
        data.append(
            {
                "message": diagnostic.to_string(),
                "severity": "error" if diagnostic.is_error() else "warning",
                "kind": (
                    "dependency"
                    if diagnostic.is_dependency_error()
                    else "interface"
                    if diagnostic.is_interface_error()
                    else "configuration"
                    if diagnostic.is_configuration()
                    else "code"
                    if diagnostic.is_code()
                    else "unknown"
                ),
                "deprecated": diagnostic.is_deprecated(),
                "usage_module": usage_module,
                "definition_module": definition_module,
                "file": diagnostic.pyfile_path(),
                "line": diagnostic.pyline_number(),
            }
        )
    return data


@mcp.tool()
def tach_lint(
    project_root: str | None = None,
    checks: list[str] | str | None = None,
    exact: bool = False,
    dependencies: bool = True,
    interfaces: bool = True,
    exclude: list[str] | None = None,
    limit: int = DEFAULT_LIMIT,
    offset: int = 0,
) -> dict[str, Any]:
    """Strong Tach lint: boundaries, public interfaces, external deps, and stale rules."""
    root = resolve_project_root(project_root)
    allowed_checks = {"boundaries", "external", "unused"}
    if checks is None:
        requested_checks = ["boundaries", "external", "unused"]
    elif isinstance(checks, str):
        requested_checks = [checks]
    else:
        requested_checks = checks
    invalid_checks = [
        check for check in requested_checks if check not in allowed_checks
    ]
    selected = [check for check in requested_checks if check in allowed_checks]
    if not selected:
        selected = ["boundaries", "external", "unused"]
    config = project_config(root, exclude)
    result: dict[str, Any] = {
        "project_root": str(root),
        "checks": selected,
        "ignored_checks": invalid_checks,
        "diagnostic_count": 0,
        "error_count": 0,
        "warning_count": 0,
    }
    ok = True

    if "boundaries" in selected:
        try:
            diagnostics = extension.check(
                project_root=root,
                project_config=config,
                dependencies=dependencies,
                interfaces=interfaces,
            )
        except TachCircularDependencyError as exc:
            result["boundaries"] = {
                "ok": False,
                "circular_dependencies": exc.dependencies,
            }
            result["diagnostic_count"] += 1
            result["error_count"] += 1
            result["ok"] = False
            result["next_actions"] = [
                "Break the cycle by removing one dependency edge listed in "
                "boundaries.circular_dependencies (tach_configure "
                "action='edit_dependency', dependency_action='remove').",
                "Use tach_report on the modules in the cycle to see which "
                "imports create each edge.",
            ]
            return result
        except TachVisibilityError as exc:
            result["boundaries"] = {
                "ok": False,
                "visibility_errors": [
                    {
                        "dependent_module": dependent,
                        "dependency_module": dependency,
                        "visibility": visibility,
                    }
                    for dependent, dependency, visibility in exc.visibility_errors
                ],
            }
            result["diagnostic_count"] += len(exc.visibility_errors)
            result["error_count"] += len(exc.visibility_errors)
            result["ok"] = False
            result["next_actions"] = [
                "Adjust 'visibility' on the listed modules in tach.toml, or "
                "remove the dependency edges that violate it.",
            ]
            return result
        diagnostics_data = diagnostics_to_data(diagnostics)
        page = truncate_items(diagnostics_data, limit=limit, offset=offset)
        boundaries_ok = not any(diagnostic.is_error() for diagnostic in diagnostics)
        result["boundaries"] = {
            "ok": boundaries_ok,
            "diagnostics": page,
        }
        result["diagnostic_count"] += len(diagnostics_data)
        result["error_count"] += sum(
            1 for diagnostic in diagnostics_data if diagnostic["severity"] == "error"
        )
        result["warning_count"] += sum(
            1 for diagnostic in diagnostics_data if diagnostic["severity"] == "warning"
        )
        ok &= boundaries_ok

    if "external" in selected:
        diagnostics = check_external(project_root=root, project_config=config)
        diagnostics_data = diagnostics_to_data(diagnostics)
        page = truncate_items(diagnostics_data, limit=limit, offset=offset)
        external_ok = not any(diagnostic.is_error() for diagnostic in diagnostics)
        result["external"] = {
            "ok": external_ok,
            "diagnostics": page,
        }
        result["diagnostic_count"] += len(diagnostics_data)
        result["error_count"] += sum(
            1 for diagnostic in diagnostics_data if diagnostic["severity"] == "error"
        )
        result["warning_count"] += sum(
            1 for diagnostic in diagnostics_data if diagnostic["severity"] == "warning"
        )
        ok &= external_ok

    if "unused" in selected:
        unused = (
            extension.detect_unused_dependencies(root, config)
            if exact or config.exact
            else []
        )
        unused_rows = [
            {
                "path": item.path,
                "dependencies": [dependency.path for dependency in item.dependencies],
            }
            for item in unused
        ]
        unused_ok = not unused
        result["unused"] = {
            "ok": unused_ok,
            "exact_checked": bool(exact or config.exact),
            "unused_dependencies": truncate_items(
                unused_rows, limit=limit, offset=offset
            ),
        }
        result["diagnostic_count"] += len(unused_rows)
        result["error_count"] += len(unused_rows)
        ok &= unused_ok

    result["ok"] = ok
    if not ok:
        result["next_actions"] = [
            "Use tach_report for failing files.",
            "Use tach_configure action='sync_dependencies' if unused or missing dependencies are stale.",
            "Use tach_map mode='delta' to scope changed-file impact before editing.",
            "For violations that must stay temporarily, add a "
            "'# tach-ignore(reason)' comment on the import line instead of "
            "loosening rules.",
        ]
    return result


@mcp.prompt()
def diagnose_tach_boundaries(project_root: str = ".") -> str:
    """Prompt for investigating Tach boundary violations."""
    return (
        f"Inspect Tach project at {project_root}. Run tach_onboard, "
        "tach_lint, tach_report for failing files, and tach_graph. "
        "Explain root cause and propose minimal tach.toml or import changes."
    )
