from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

from tach import errors
from tach.filesystem import build_project_config_path, file_to_module_path
from tach.interactive import (
    InteractiveModuleConfiguration,
    get_selected_modules_interactive,
)
from tach.parsing import dump_project_config_to_toml

if TYPE_CHECKING:
    from tach.extension import ProjectConfig


def handle_module_edits(
    project_config: ProjectConfig,
    selected_modules: list[str],
    selected_utilities: list[str],
) -> None:
    existing_modules = set(project_config.module_paths())
    existing_utilities = set(project_config.utility_paths())
    selected_modules_set = set(selected_modules)
    selected_utilities_set = set(selected_utilities)
    all_selected_paths = selected_modules_set | selected_utilities_set

    modules_to_add = all_selected_paths - existing_modules
    modules_to_remove = existing_modules - all_selected_paths

    for module in modules_to_add:
        project_config.create_module(module)

    for module in modules_to_remove:
        project_config.delete_module(module)

    utilities_to_add = selected_utilities_set - existing_utilities
    utilities_to_remove = existing_utilities - selected_utilities_set

    for utility in utilities_to_add:
        project_config.mark_module_as_utility(utility)

    for utility in utilities_to_remove:
        project_config.unmark_module_as_utility(utility)


def handle_utility_edits(
    project_config: ProjectConfig, selected_utilities: list[str]
) -> None:
    existing_utilities = set(project_config.utility_paths())
    selected_utilities_set = set(selected_utilities)

    utilities_to_add = selected_utilities_set - existing_utilities
    utilities_to_remove = existing_utilities - selected_utilities_set

    for utility in utilities_to_add:
        project_config.mark_module_as_utility(utility)

    for utility in utilities_to_remove:
        project_config.unmark_module_as_utility(utility)


def handle_source_root_edits(
    project_config: ProjectConfig, selected_source_roots: list[Path]
) -> None:
    existing_source_roots = set(project_config.source_roots)
    selected_source_roots_set = set(selected_source_roots)

    source_roots_to_add = selected_source_roots_set - existing_source_roots
    source_roots_to_remove = existing_source_roots - selected_source_roots_set

    for source_root in source_roots_to_add:
        project_config.add_source_root(Path(source_root))

    for source_root in source_roots_to_remove:
        project_config.remove_source_root(Path(source_root))


def apply_selected_configuration(
    project_config: ProjectConfig,
    project_root: Path,
    selected_source_roots: list[Path],
    selected_modules: list[Path],
    selected_utilities: list[Path],
):
    # Write initial config file (tach.toml) if it doesn't exist
    if not project_config.exists():
        project_config_path = build_project_config_path(project_root)
        config_toml_content = dump_project_config_to_toml(project_config)
        project_config_path.write_text(config_toml_content)
        project_config.set_location(project_config_path)

    relative_selected_source_roots = [
        source_root.relative_to(project_root) for source_root in selected_source_roots
    ]
    handle_source_root_edits(project_config, relative_selected_source_roots)

    selected_module_paths = [
        file_to_module_path(tuple(selected_source_roots), module_filepath)
        for module_filepath in selected_modules
    ]
    selected_utility_paths = [
        file_to_module_path(tuple(selected_source_roots), utility_filepath)
        for utility_filepath in selected_utilities
    ]
    handle_module_edits(project_config, selected_module_paths, selected_utility_paths)

    project_config.save_edits()


@dataclass
class ValidationResult:
    ok: bool
    errors: list[str] = field(default_factory=list)


def validate_configuration(
    configuration: InteractiveModuleConfiguration,
) -> ValidationResult:
    errors: list[str] = []
    for module_path in configuration.module_paths:
        module_path = Path(module_path).resolve()

        if not any(
            source_root in module_path.parents
            for source_root in configuration.source_roots
        ):
            # This module exists outside of the source root
            # This is not allowed and should be reported as a configuration error
            errors.append(
                f"Module '{module_path}' is not contained within any source root: {[str(root) for root in configuration.source_roots]}"
            )
    return ValidationResult(ok=not errors, errors=errors)


def mod_edit_interactive(
    project_root: Path,
    project_config: ProjectConfig,
    exclude_paths: list[str],
    depth: int | None = 1,
) -> tuple[bool, list[str]]:
    if not Path(project_root).is_dir():
        raise errors.TachSetupError(f"The path {project_root} is not a directory.")

    interactive_module_configuration = get_selected_modules_interactive(
        path=project_root,
        project_config=project_config,
        depth=depth,
        exclude_paths=exclude_paths,
    )
    if interactive_module_configuration is not None:
        validation_result = validate_configuration(interactive_module_configuration)
        if validation_result.errors:
            return False, [
                f"[red]Validation error: [yellow]{error}[/][/]"
                for error in validation_result.errors
            ]
        apply_selected_configuration(
            project_config=project_config,
            project_root=project_root,
            selected_source_roots=interactive_module_configuration.source_roots,
            selected_modules=interactive_module_configuration.module_paths,
            selected_utilities=interactive_module_configuration.utility_paths,
        )
        return True, []
    else:
        return False, ["[cyan]No changes saved.[/]"]


__all__ = ["mod_edit_interactive"]
