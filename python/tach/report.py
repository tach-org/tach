from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from tach import errors
from tach.extension import (
    create_dependency_report,
    get_external_imports,
)
from tach.filesystem import walk_pyfiles
from tach.utils.display import BCOLORS, colorize, create_clickable_link
from tach.utils.exclude import is_path_excluded
from tach.utils.external import (
    get_package_name,
    is_stdlib_module,
    normalize_package_name,
)

if TYPE_CHECKING:
    from tach.extension import ProjectConfig


def report(
    project_root: Path,
    path: Path,
    project_config: ProjectConfig,
    include_dependency_modules: list[str] | None = None,
    include_usage_modules: list[str] | None = None,
    skip_dependencies: bool = False,
    skip_usages: bool = False,
    raw: bool = False,
) -> str:
    if not project_root.is_dir():
        raise errors.TachSetupError(
            f"The path '{project_root}' is not a valid directory."
        )

    if not path.exists():
        raise errors.TachError(f"The path '{path}' does not exist.")

    # We prefer resolving symlinks and relative paths in Python
    # because Rust's canonicalize adds an 'extended length path' prefix on Windows
    # which breaks downstream code that compares to Python-resolved paths
    path = path.resolve().relative_to(project_root)
    try:
        return create_dependency_report(
            project_root=project_root,
            project_config=project_config,
            path=path,
            include_dependency_modules=include_dependency_modules,
            include_usage_modules=include_usage_modules,
            skip_dependencies=skip_dependencies,
            skip_usages=skip_usages,
            raw=raw,
        )
    except ValueError as e:
        raise errors.TachError(str(e))


@dataclass
class ExternalDependency:
    absolute_file_path: Path
    import_module_path: str
    import_line_number: int
    package_name: str


def render_external_dependency(
    dependency: ExternalDependency,
    project_root: Path,
) -> str:
    clickable_link = create_clickable_link(
        file_path=dependency.absolute_file_path.relative_to(project_root),
        line=dependency.import_line_number,
    )
    import_info = f"Import '{dependency.import_module_path}' from package '{dependency.package_name}'"
    return (
        f"{colorize(clickable_link, BCOLORS.OKGREEN)}: "
        f"{colorize(import_info, BCOLORS.OKCYAN)}"
    )


def render_external_dependency_report(
    project_root: Path,
    path: Path,
    dependencies: list[ExternalDependency],
    raw: bool = False,
) -> str:
    if not dependencies:
        if raw:
            return ""
        no_deps_msg = "No external dependencies found in "
        path_str = f"'{path}'"
        return f"{colorize(no_deps_msg, BCOLORS.OKCYAN)}{colorize(path_str, BCOLORS.OKGREEN)}."

    if raw:
        return "# External Dependencies\n" + "\n".join(
            sorted({dependency.package_name for dependency in dependencies})
        )

    title = f"[ External Dependencies in '{path}' ]"
    divider = "-" * len(title)
    lines = [title, divider]

    if not dependencies:
        lines.append(colorize("No external dependencies found.", BCOLORS.OKGREEN))
        return "\n".join(lines)

    for dependency in dependencies:
        lines.append(
            render_external_dependency(dependency=dependency, project_root=project_root)
        )

    return "\n".join(lines)


def get_external_dependencies(
    project_root: Path,
    source_roots: list[Path],
    file_path: Path,
    project_config: ProjectConfig,
    excluded_modules: set[str] | None = None,
) -> list[ExternalDependency]:
    external_imports = get_external_imports(
        project_root=project_root,
        source_roots=source_roots,
        file_path=file_path,
        project_config=project_config,
    )

    excluded_modules = excluded_modules or set()
    external_dependencies: list[ExternalDependency] = []
    for external_import in external_imports:
        external_package = get_package_name(external_import.module_path)
        if external_package in excluded_modules:
            continue

        if is_stdlib_module(external_package):
            continue

        external_dependencies.append(
            ExternalDependency(
                absolute_file_path=Path(file_path),
                import_module_path=external_import.module_path,
                import_line_number=external_import.line_number,
                package_name=normalize_package_name(external_import.module_path),
            )
        )
    return external_dependencies


def external_dependency_report(
    project_root: Path,
    path: Path,
    project_config: ProjectConfig,
    raw: bool = False,
) -> str:
    if not project_root.is_dir():
        raise errors.TachSetupError(
            f"The path '{project_root}' is not a valid directory."
        )

    if not path.exists():
        raise errors.TachError(f"The path '{path}' does not exist.")

    if project_config.exclude and is_path_excluded(project_config.exclude, path):
        raise errors.TachError(f"The path '{path}' is excluded.")

    source_roots = [
        project_root / source_root for source_root in project_config.source_roots
    ]

    if path.is_file():
        external_dependencies = get_external_dependencies(
            project_root=project_root,
            source_roots=source_roots,
            file_path=path.resolve(),
            excluded_modules=set(project_config.external.exclude),
            project_config=project_config,
        )
        return render_external_dependency_report(
            project_root=project_root,
            path=path,
            dependencies=external_dependencies,
            raw=raw,
        )

    all_external_dependencies: list[ExternalDependency] = []
    for pyfile in walk_pyfiles(
        path, project_root=project_root, exclude_paths=project_config.exclude
    ):
        all_external_dependencies.extend(
            get_external_dependencies(
                project_root=project_root,
                source_roots=source_roots,
                file_path=path.resolve() / pyfile,
                excluded_modules=set(project_config.external.exclude),
                project_config=project_config,
            )
        )

    return render_external_dependency_report(
        project_root=project_root,
        path=path,
        dependencies=all_external_dependencies,
        raw=raw,
    )


__all__ = ["external_dependency_report", "report"]
