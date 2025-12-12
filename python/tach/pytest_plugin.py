from __future__ import annotations

from pathlib import Path
from typing import Any, Protocol

import pytest

from tach import filesystem as fs
from tach.errors import TachSetupError
from tach.extension import TachPytestPluginHandler
from tach.filesystem.git_ops import get_changed_files
from tach.parsing import parse_project_config


class TachConfig(Protocol):
    tach_handler: TachPytestPluginHandler

    def getoption(self, name: str) -> Any: ...


def pytest_addoption(parser: pytest.Parser):
    group = parser.getgroup("tach")
    group.addoption(
        "--tach-base",
        default="main",
        help="Base commit to compare against when determining affected tests [default: main]",
    )
    group.addoption(
        "--tach-head",
        default="",
        help="Head commit to compare against when determining affected tests [default: current filesystem]",
    )


@pytest.hookimpl(tryfirst=True)
def pytest_configure(config: TachConfig):
    project_root = fs.find_project_config_root() or Path.cwd()
    project_config = parse_project_config(root=project_root)
    if project_config is None:
        raise TachSetupError("In Tach pytest plugin: No project config found")

    base = config.getoption("--tach-base")
    head = config.getoption("--tach-head")

    kwargs: dict[str, Any] = {"project_root": project_root}
    if head:
        kwargs["head"] = head
    if base:
        kwargs["base"] = base
    changed_files = get_changed_files(**kwargs)

    # Store the handler instance on the config object so other hooks can access it
    config.tach_handler = TachPytestPluginHandler(
        project_root=project_root,
        project_config=project_config,
        changed_files=changed_files,
        all_affected_modules={changed_file.resolve() for changed_file in changed_files},
    )


def pytest_collection_modifyitems(
    session: pytest.Session,
    config: TachConfig,
    items: list[pytest.Item],
):
    handler = config.tach_handler
    seen: set[Path] = set()
    paths_to_remove: set[Path] = set()

    # First pass: determine which paths should be removed
    for item in items:
        if not item.path or item.path in seen:
            continue
        seen.add(item.path)

        if str(item.path) in handler.removed_test_paths:
            paths_to_remove.add(item.path)
        elif str(item.path) not in handler.all_affected_modules:
            # If this test file was not changed, check if it should be removed
            if handler.should_remove_items(file_path=item.path.resolve()):
                paths_to_remove.add(item.path)
                handler.remove_test_path(item.path)

    # Second pass: remove all items from paths marked for removal
    original_count = len(items)
    items[:] = [
        item for item in items
        if not item.path or item.path not in paths_to_remove
    ]
    handler.num_removed_items = original_count - len(items)


def pytest_report_collectionfinish(
    config: TachConfig,
    start_path: Path,
    startdir: Any,
    items: list[pytest.Item],
) -> str | list[str]:
    handler = config.tach_handler
    return [
        f"[Tach] Skipped {len(handler.removed_test_paths)} test file{'s' if len(handler.removed_test_paths) > 1 else ''}"
        f" ({handler.num_removed_items} tests)"
        " since they were unaffected by current changes.",
        *(
            f"[Tach] > Skipped '{test_path}'"
            for test_path in handler.removed_test_paths
        ),
    ]


def pytest_terminal_summary(terminalreporter: Any, exitstatus: int, config: TachConfig):
    config.tach_handler.tests_ran_to_completion = True
