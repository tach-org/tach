from __future__ import annotations

from typing import Any, Literal

from tach import extension
from tach.filesystem import build_project_config_path
from tach.filesystem import install_pre_commit as install_pre_commit_hook
from tach.mcp.payloads import digest
from tach.mcp.project import (
    DependencyRule,
    config_summary,
    find_module_entry,
    load_config_toml,
    project_config,
    resolve_project_root,
    save_config,
    save_config_toml,
)
from tach.mcp.server import mcp
from tach.parsing import dump_project_config_to_toml


@mcp.tool()
def tach_configure(
    action: Literal[
        "create_config",
        "edit_module",
        "edit_dependency",
        "sync_dependencies",
        "install_pre_commit",
        "set_layers",
        "set_module_layer",
        "set_module_visibility",
        "deprecate_dependency",
        "undeprecate_dependency",
        "add_interface",
        "remove_interface",
    ],
    project_root: str,
    source_roots: list[str] | None = None,
    modules: list[str] | None = None,
    utilities: list[str] | None = None,
    dependencies: list[DependencyRule] | None = None,
    path: str | None = None,
    dependency: str | None = None,
    module_action: Literal["create", "delete", "mark_utility", "unmark_utility"]
    | None = None,
    dependency_action: Literal["add", "remove"] | None = None,
    forbid_circular_dependencies: bool = False,
    layers: list[str] | None = None,
    layers_explicit_depends_on: bool | None = None,
    layer: str | None = None,
    visibility: list[str] | None = None,
    interface_expose: list[str] | None = None,
    interface_from: list[str] | None = None,
    interface_visibility: list[str] | None = None,
    interface_data_types: Literal["all", "primitive"] | None = None,
    interface_exclusive: bool = False,
    force: bool = False,
    add_only: bool = False,
    include_config: bool = False,
) -> dict[str, Any]:
    """Create or edit Tach config: modules, dependencies, layers, visibility,
    interfaces, deprecations, sync, or pre-commit install."""
    root = resolve_project_root(project_root)
    result: dict[str, Any]

    if action == "create_config":
        if source_roots is None or modules is None:
            raise ValueError("source_roots and modules are required for create_config.")
        config_path = build_project_config_path(root)
        if config_path.exists() and not force:
            raise ValueError(
                f"Config already exists at '{config_path}'. Pass force=true."
            )
        config = save_config(
            root,
            source_roots,
            modules,
            utilities,
            dependencies,
            forbid_circular_dependencies=forbid_circular_dependencies,
        )
        result = {
            "ok": True,
            "action": action,
            "project_root": str(root),
            "config_path": str(config_path),
            "module_count": len(config.module_paths()),
            "dependency_count": len(dependencies or []),
        }
    elif action == "edit_module":
        if path is None or module_action is None:
            raise ValueError("path and module_action are required for edit_module.")
        config = project_config(root)
        if module_action == "create":
            config.create_module(path)
        elif module_action == "delete":
            config.delete_module(path)
        elif module_action == "mark_utility":
            config.mark_module_as_utility(path)
        else:
            config.unmark_module_as_utility(path)
        config.save_edits()
        result = {"ok": True, "action": action, "project_root": str(root), "path": path}
    elif action == "edit_dependency":
        if path is None or dependency is None or dependency_action is None:
            raise ValueError(
                "path, dependency, and dependency_action are required for edit_dependency."
            )
        config = project_config(root)
        if dependency_action == "add":
            config.add_dependency(path, dependency)
        else:
            config.remove_dependency(path, dependency)
        config.save_edits()
        result = {
            "ok": True,
            "action": action,
            "project_root": str(root),
            "path": path,
            "dependency": dependency,
        }
    elif action == "sync_dependencies":
        config = project_config(root)
        before = dump_project_config_to_toml(config)
        extension.sync_project(project_root=root, project_config=config, add=add_only)
        after_config = project_config(root)
        result = {
            "ok": True,
            "action": action,
            "changed": before != dump_project_config_to_toml(after_config),
            "project_root": str(root),
            "digest": digest(config_summary(after_config)),
        }
    elif action == "set_layers":
        if layers is None:
            raise ValueError("layers is required for set_layers.")
        config_data = load_config_toml(root)
        config_data["layers"] = layers
        if layers_explicit_depends_on is not None:
            config_data["layers_explicit_depends_on"] = layers_explicit_depends_on
        save_config_toml(root, config_data)
        result = {
            "ok": True,
            "action": action,
            "project_root": str(root),
            "layers": layers,
        }
    elif action == "set_module_layer":
        if path is None or layer is None:
            raise ValueError("path and layer are required for set_module_layer.")
        config_data = load_config_toml(root)
        entry = find_module_entry(config_data, path)
        entry["layer"] = layer
        save_config_toml(root, config_data)
        result = {
            "ok": True,
            "action": action,
            "project_root": str(root),
            "path": path,
            "layer": layer,
        }
    elif action == "set_module_visibility":
        if path is None or visibility is None:
            raise ValueError(
                "path and visibility are required for set_module_visibility."
            )
        config_data = load_config_toml(root)
        entry = find_module_entry(config_data, path)
        entry["visibility"] = visibility
        save_config_toml(root, config_data)
        result = {
            "ok": True,
            "action": action,
            "project_root": str(root),
            "path": path,
            "visibility": visibility,
        }
    elif action in {"deprecate_dependency", "undeprecate_dependency"}:
        if path is None or dependency is None:
            raise ValueError(f"path and dependency are required for {action}.")
        config_data = load_config_toml(root)
        entry = find_module_entry(config_data, path)
        depends_on = entry.get("depends_on", [])
        rewritten: list[Any] = []
        found = False
        for item in depends_on:
            item_path = item if isinstance(item, str) else item.get("path")
            if item_path != dependency:
                rewritten.append(item)
                continue
            found = True
            if action == "deprecate_dependency":
                rewritten.append({"path": dependency, "deprecated": True})
            else:
                rewritten.append(dependency)
        if not found:
            raise ValueError(
                f"Module '{path}' has no declared dependency on '{dependency}'."
            )
        entry["depends_on"] = rewritten
        save_config_toml(root, config_data)
        result = {
            "ok": True,
            "action": action,
            "project_root": str(root),
            "path": path,
            "dependency": dependency,
            "deprecated": action == "deprecate_dependency",
        }
    elif action == "add_interface":
        if interface_expose is None or interface_from is None:
            raise ValueError(
                "interface_expose and interface_from are required for add_interface."
            )
        config_data = load_config_toml(root)
        interface_data: dict[str, Any] = {
            "expose": interface_expose,
            "from": interface_from,
        }
        if interface_visibility is not None:
            interface_data["visibility"] = interface_visibility
        if interface_data_types is not None:
            interface_data["data_types"] = interface_data_types
        if interface_exclusive:
            interface_data["exclusive"] = True
        config_data.setdefault("interfaces", []).append(interface_data)
        save_config_toml(root, config_data)
        result = {
            "ok": True,
            "action": action,
            "project_root": str(root),
            "interface": interface_data,
        }
    elif action == "remove_interface":
        if interface_expose is None or interface_from is None:
            raise ValueError(
                "interface_expose and interface_from are required for remove_interface."
            )
        config_data = load_config_toml(root)
        interfaces = config_data.get("interfaces", [])
        remaining = [
            item
            for item in interfaces
            if not (
                set(item.get("expose", [])) == set(interface_expose)
                and set(item.get("from", [])) == set(interface_from)
            )
        ]
        if len(remaining) == len(interfaces):
            raise ValueError("No interface matches the given expose/from lists.")
        config_data["interfaces"] = remaining
        save_config_toml(root, config_data)
        result = {
            "ok": True,
            "action": action,
            "project_root": str(root),
            "removed": len(interfaces) - len(remaining),
        }
    else:
        installed, warning = install_pre_commit_hook(root)
        result = {
            "ok": installed,
            "action": action,
            "project_root": str(root),
            "installed": installed,
            "warning": warning,
        }

    if include_config and action != "install_pre_commit":
        result["config"] = config_summary(project_config(root))
    return result
