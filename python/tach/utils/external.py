from __future__ import annotations

import re
import sys
from functools import cache

KNOWN_MODULE_SPECIAL_CASES = {
    "__future__",
    "typing_extensions",
}


def is_stdlib_module(module: str) -> bool:
    if module in KNOWN_MODULE_SPECIAL_CASES:
        return True

    if module in sys.builtin_module_names:
        return True
    if module in sys.stdlib_module_names:
        return True
    return False


def get_stdlib_modules() -> list[str]:
    modules = set(sys.builtin_module_names)
    modules.update(sys.stdlib_module_names)
    modules.update(KNOWN_MODULE_SPECIAL_CASES)
    return sorted(modules)


@cache
def get_module_mappings():
    from importlib.metadata import packages_distributions

    return packages_distributions()


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
