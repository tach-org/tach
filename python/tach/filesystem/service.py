from __future__ import annotations

import os
import stat
from functools import cache
from pathlib import Path
from typing import TYPE_CHECKING

from tach.utils.display import BCOLORS, colorize
from tach.utils.exclude import is_path_excluded

if TYPE_CHECKING:
    from collections.abc import Generator


def write_file(path: Path, content: str, root: Path | None = None):
    path.write_text(content)
    try:
        display_path = path.relative_to(root or Path.cwd())
    except AssertionError:
        display_path = path.resolve()
    print(colorize(f"Wrote '{display_path}'", BCOLORS.WARNING))


def mark_executable(path: Path):
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def walk(
    root: Path,
    project_root: Path | None = None,
    exclude_paths: list[str] | None = None,
    depth: int | None = None,
) -> Generator[tuple[Path, list[Path]], None, None]:
    if depth is not None and depth <= 0:
        return
    root = root.resolve()
    for dirpath, dirnames, filenames in os.walk(root):
        rel_dirpath = Path(dirpath).relative_to(root)

        if rel_dirpath.name.startswith("."):
            # This prevents recursing into child directories of hidden paths
            del dirnames[:]
            continue

        if exclude_paths:
            project_dirpath = (
                Path(dirpath).relative_to(project_root) if project_root else rel_dirpath
            )
            if is_path_excluded(exclude_paths, project_dirpath):
                del dirnames[:]
                continue

        if depth:
            # Ignore anything past requested depth
            current_depth = len(rel_dirpath.parts) - 1
            if current_depth > depth:
                continue

        def filter_filename(filename: str) -> bool:
            if exclude_paths and is_path_excluded(exclude_paths, Path(filename)):
                return False
            return not filename.startswith(".")

        yield rel_dirpath, list(map(Path, filter(filter_filename, filenames)))


def walk_pyfiles(
    root: Path,
    project_root: Path | None = None,
    exclude_paths: list[str] | None = None,
    depth: int | None = None,
) -> Generator[Path, None, None]:
    for dirpath, filepaths in walk(
        root,
        project_root=project_root,
        exclude_paths=exclude_paths,
        depth=depth,
    ):
        for filepath in filepaths:
            if filepath.name.endswith(".py"):
                yield dirpath / filepath


@cache
def file_to_module_path(source_roots: tuple[Path, ...], file_path: Path) -> str:
    # NOTE: source_roots are assumed to be absolute here
    matching_root: Path | None = None
    for root in source_roots:
        if root in file_path.parents:
            matching_root = root
            break

    if matching_root is None:
        raise ValueError(f"File path: {file_path} not found in any source root.")

    relative_path = file_path.relative_to(matching_root)
    components = list(relative_path.parent.parts)

    if relative_path.name != "__init__.py":
        components.append(relative_path.stem)

    module_path = ".".join(components)
    return "." if not module_path else module_path


@cache
def module_to_pyfile_or_dir_path(
    source_roots: tuple[Path, ...], module_path: str
) -> Path | None:
    """
    This resolves a dotted Python module path ('a.b.c')
    into a Python file or a Python package directory,
    used in cases where __init__.py is not relevant such as the
    interactive module tree.

    The module path is assumed NOT to refer to a member within a Python module.

    'source_roots' is assumed to be a list of absolute paths.
    """
    if not module_path:
        # Path("") turns into PosixPath("."), but we don't want to
        # treat an empty module path as the root directory
        return None

    base_path = module_path.replace(".", os.sep)
    for source_root in source_roots:
        dir_path = source_root / base_path
        pyinterface_path = source_root / f"{base_path}.pyi"
        pyfile_path = source_root / f"{base_path}.py"
        if dir_path.is_dir():
            return dir_path
        elif pyinterface_path.exists():
            return pyinterface_path
        elif pyfile_path.exists():
            return pyfile_path

    return None
