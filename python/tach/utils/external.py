from __future__ import annotations

import re
import sys
from functools import cache
from typing import Any

KNOWN_MODULE_SPECIAL_CASES = {
    "__future__",
    "typing_extensions",
}


def is_stdlib_module(module: str) -> bool:
    if module in KNOWN_MODULE_SPECIAL_CASES:
        return True

    if sys.version_info >= (3, 10):
        if module in sys.builtin_module_names:
            return True
        if module in sys.stdlib_module_names:
            return True
        return False
    else:
        from stdlib_list import in_stdlib  # type: ignore

        return in_stdlib(module)  # type: ignore


def get_stdlib_modules() -> list[str]:
    if sys.version_info >= (3, 10):
        modules = set(sys.builtin_module_names)
        modules.update(sys.stdlib_module_names)
        modules.update(KNOWN_MODULE_SPECIAL_CASES)
        return sorted(modules)
    else:
        from stdlib_list import stdlib_list  # type: ignore

        modules: set[str] = set(stdlib_list())  # type: ignore
        modules.update(KNOWN_MODULE_SPECIAL_CASES)
        return list(sorted(modules))


def _get_installed_modules(dist: Any) -> list[str]:
    # This method is best-effort, and is only used for Python < 3.10
    module_names: set[str] = set()

    # Method 1: Check top_level.txt
    try:
        top_level = dist.read_text("top_level.txt")
        if top_level:
            module_names.update(
                module_name.strip()
                for module_name in top_level.splitlines()
                if module_name.strip()
            )
            return list(module_names)
    except Exception:
        pass

    # Method 2: Parse RECORD file
    try:
        record = dist.read_text("RECORD")
        if record:
            for line in record.splitlines():
                base_name = line.split(",")[0].split("/")[0]
                if (
                    "/" in line
                    and not base_name.startswith("_")
                    and not base_name.endswith("dist-info")
                    and "." not in base_name
                ):
                    module_names.add(base_name)
            if module_names:
                return list(module_names)
    except Exception:
        pass

    # Method 3: Check entry points
    for ep in dist.entry_points:
        if ":" in ep.value:
            entry_point = ep.value.split(":")[0]
        else:
            entry_point = ep.value
        module_names.add(entry_point.split(".")[0])
    return list(module_names)


@cache
def get_module_mappings() -> dict[str, list[str]]:
    if sys.version_info >= (3, 10):
        from importlib.metadata import packages_distributions

        return packages_distributions()  # type: ignore
    else:
        if sys.version_info >= (3, 8):  # noqa: UP036
            from importlib.metadata import distributions
        else:
            from importlib_metadata import distributions  # type: ignore

        result: dict[str, list[str]] = {}
        for dist in distributions():
            modules = _get_installed_modules(dist)
            name = dist.metadata["Name"]
            for module in modules:
                if module not in result:
                    result[module] = []
                result[module].append(name)
        return result


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
    "is_stdlib_module",
    "get_module_mappings",
    "get_package_name",
    "normalize_package_name",
]
