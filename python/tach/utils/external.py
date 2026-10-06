from __future__ import annotations

import ast
import re
import sys
from functools import cache
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from importlib.metadata import Distribution, PackagePath

KNOWN_MODULE_SPECIAL_CASES = {
    "__future__",
    "typing_extensions",
}


def is_stdlib_module(module: str) -> bool:
    if module in KNOWN_MODULE_SPECIAL_CASES:
        return True

    if module in sys.builtin_module_names:
        return True
    return module in sys.stdlib_module_names


def get_stdlib_modules() -> list[str]:
    modules = set(sys.builtin_module_names)
    modules.update(sys.stdlib_module_names)
    modules.update(KNOWN_MODULE_SPECIAL_CASES)
    return sorted(modules)


_FINDER_IMPORT_REGEX = re.compile(
    r"import\s+(?P<name>[A-Za-z_]\w*)\s*(?:;\s*(?P=name)\.install\(\s*\))?\s*$"
)
_FINDER_ASSIGNMENTS = frozenset({"MAPPING", "NAMESPACES"})
_SKIPPED_DIRECTORY_NAMES = frozenset(
    {
        "__pycache__",
        ".git",
        "node_modules",
        ".venv",
        "venv",
        "site-packages",
    }
)
_SOURCE_SEARCH_DEPTH = 5


@cache
def get_module_mappings() -> dict[str, list[str]]:
    from importlib.metadata import distributions, packages_distributions

    module_mappings = {
        module_name: list(distribution_names)
        for module_name, distribution_names in packages_distributions().items()
    }
    seen_distribution_names = {
        distribution_name
        for distribution_names in module_mappings.values()
        for distribution_name in distribution_names
    }
    for distribution in distributions():
        distribution_name = _distribution_name(distribution)
        if distribution_name is None or distribution_name in seen_distribution_names:
            continue
        file_entries = distribution.files
        if file_entries is None:
            continue
        file_entries = list(file_entries)
        top_level_names: set[str] = set()
        for file_entry in file_entries:
            if file_entry.suffix != ".pth":
                continue
            top_level_names.update(
                _top_level_names_from_pth(distribution, file_entry, file_entries)
            )
        if not top_level_names:
            continue
        for top_level_name in sorted(top_level_names):
            # Append so get_package_name keeps the first already-mapped distribution.
            module_mappings.setdefault(top_level_name, []).append(distribution_name)
        seen_distribution_names.add(distribution_name)
    return module_mappings


def _distribution_name(distribution: Distribution) -> str | None:
    try:
        name = distribution.metadata["Name"]
    except KeyError:
        return None
    if not name:
        return None
    return name


def _top_level_names_from_pth(
    distribution: Distribution,
    file_entry: PackagePath,
    file_entries: list[PackagePath],
) -> set[str]:
    pth_path = _locate_path(distribution, file_entry)
    if pth_path is None:
        return set()
    text = _read_text(pth_path)
    if text is None:
        return set()

    names: set[str] = set()
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        finder_name = _finder_import_name(line)
        if finder_name is not None:
            names.update(
                _top_level_names_from_finder(
                    distribution, file_entries, pth_path.parent, finder_name
                )
            )
            continue
        directory = _existing_directory(line)
        if directory is None:
            continue
        names.update(_top_level_names_from_source_directory(directory))
    return names


def _finder_import_name(line: str) -> str | None:
    match = _FINDER_IMPORT_REGEX.fullmatch(line)
    if match is None:
        return None
    return match.group("name")


def _existing_directory(line: str) -> Path | None:
    try:
        candidate = Path(line)
        if candidate.is_dir():
            return candidate
    except (OSError, ValueError):
        return None
    return None


def _top_level_names_from_finder(
    distribution: Distribution,
    file_entries: list[PackagePath],
    pth_directory: Path,
    finder_name: str,
) -> set[str]:
    finder_path = _resolve_finder_path(
        distribution, file_entries, pth_directory, finder_name
    )
    if finder_path is None:
        return set()
    source = _read_text(finder_path)
    if source is None:
        return set()
    return _top_level_names_from_finder_source(source)


def _resolve_finder_path(
    distribution: Distribution,
    file_entries: list[PackagePath],
    pth_directory: Path,
    finder_name: str,
) -> Path | None:
    sibling = pth_directory / f"{finder_name}.py"
    try:
        if sibling.is_file():
            return sibling
    except OSError:
        pass

    filename = f"{finder_name}.py"
    for file_entry in file_entries:
        if file_entry.name != filename:
            continue
        located = _locate_path(distribution, file_entry)
        if located is None:
            continue
        try:
            if located.is_file():
                return located
        except OSError:
            continue
    return None


def _top_level_names_from_finder_source(source: str) -> set[str]:
    try:
        tree = ast.parse(source)
    except (SyntaxError, ValueError):
        return set()

    names: set[str] = set()
    for statement in tree.body:
        mapping = _assigned_finder_mapping(statement)
        if mapping is None:
            continue
        names.update(_identifier_roots(mapping))
    return names


def _assigned_finder_mapping(statement: ast.stmt) -> ast.Dict | None:
    if isinstance(statement, ast.Assign):
        targets = statement.targets
        value = statement.value
    elif isinstance(statement, ast.AnnAssign):
        if statement.value is None:
            return None
        targets = (statement.target,)
        value = statement.value
    else:
        return None
    if not isinstance(value, ast.Dict):
        return None
    if any(
        isinstance(target, ast.Name) and target.id in _FINDER_ASSIGNMENTS
        for target in targets
    ):
        return value
    return None


def _identifier_roots(mapping: ast.Dict) -> set[str]:
    names: set[str] = set()
    for key in mapping.keys:
        if not isinstance(key, ast.Constant) or not isinstance(key.value, str):
            continue
        root = key.value.split(".", 1)[0]
        if root.isidentifier():
            names.add(root)
    return names


def _top_level_names_from_source_directory(directory: Path) -> set[str]:
    names: set[str] = set()
    try:
        children = list(directory.iterdir())
    except OSError:
        return names
    for child in children:
        try:
            if child.is_symlink():
                continue
            if child.is_file():
                stem = _importable_stem(child)
                if stem is not None:
                    names.add(stem)
                continue
            if (
                child.is_dir()
                and child.name.isidentifier()
                and not _is_skipped_directory(child.name)
                and _contains_python_file(child, _SOURCE_SEARCH_DEPTH)
            ):
                names.add(child.name)
        except OSError:
            continue
    return names


def _importable_stem(path: Path) -> str | None:
    if path.suffix != ".py":
        return None
    stem = path.stem
    if stem == "__init__" or not stem.isidentifier():
        return None
    return stem


def _contains_python_file(directory: Path, remaining_depth: int) -> bool:
    if remaining_depth <= 0:
        return False
    try:
        entries = list(directory.iterdir())
    except OSError:
        return False
    for entry in entries:
        try:
            if entry.is_symlink():
                continue
            if entry.is_file() and entry.suffix == ".py":
                return True
            if (
                entry.is_dir()
                and not _is_skipped_directory(entry.name)
                and _contains_python_file(entry, remaining_depth - 1)
            ):
                return True
        except OSError:
            continue
    return False


def _is_skipped_directory(name: str) -> bool:
    return name in _SKIPPED_DIRECTORY_NAMES or name.endswith(
        (".dist-info", ".egg-info")
    )


def _locate_path(distribution: Distribution, file_entry: PackagePath) -> Path | None:
    try:
        return Path(str(distribution.locate_file(file_entry)))
    except (OSError, ValueError):
        return None


def _read_text(path: Path) -> str | None:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError):
        return None


PYPI_PACKAGE_REGEX = re.compile(r"[-_.]+")


def get_package_name(import_module_path: str) -> str:
    top_level_name = import_module_path.split(".")[0]
    module_mappings = get_module_mappings()
    # Ignoring the case of multiple packages providing this module,
    # using the first one in the mapping
    return module_mappings.get(top_level_name, [top_level_name])[0]


def normalize_package_name(import_module_path: str) -> str:
    return PYPI_PACKAGE_REGEX.sub("-", get_package_name(import_module_path)).lower()


__all__ = [
    "get_module_mappings",
    "get_package_name",
    "is_stdlib_module",
    "normalize_package_name",
]
