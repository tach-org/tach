from __future__ import annotations

from pathlib import Path
from typing import Any, Generator, Protocol

import pytest
from pytest import Collector

from tach import filesystem as fs
from tach.errors import TachSetupError
from tach.extension import TachPytestPluginHandler
from tach.filesystem.git_ops import get_changed_files
from tach.parsing import parse_project_config


class TachConfig(Protocol):
    tach_handler: TachPytestPluginHandler
    tach_validate_mode: bool
    tach_would_skip_paths: set[Path]

    def getoption(self, name: str) -> Any: ...


class HasTachConfig(Protocol):
    config: TachConfig


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
    group.addoption(
        "--tach-validate",
        action="store_true",
        default=False,
        help="Validation mode: run all tests but report what would have been skipped. "
        "Fails if any 'would-be-skipped' test fails.",
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

    # Validation mode: run all tests but track what would be skipped
    config.tach_validate_mode = config.getoption("--tach-validate")
    config.tach_would_skip_paths = set()


def _count_items(collector: Collector) -> int:
    """Recursively count test items from a collector."""
    count = 0
    for item in collector.collect():
        if isinstance(item, Collector):
            # It's a collector (e.g., Class), recurse
            count += _count_items(item)
        else:
            # It's a test item
            count += 1
    return count


@pytest.hookimpl(wrapper=True)
def pytest_collect_file(
    file_path: Path, parent: HasTachConfig
) -> Generator[None, list[Collector], list[Collector]]:
    handler = parent.config.tach_handler
    config = parent.config
    # Skip any paths that already get filtered out by other hook impls
    result = yield
    if not result:
        return result

    resolved_path = file_path.resolve()

    # If this test file was changed, keep it
    if str(resolved_path) in handler.all_affected_modules:
        return result

    # Check if file should be removed based on its imports
    if handler.should_remove_items(file_path=resolved_path):
        # Recursively count all test items before discarding
        for collector in result:
            handler.num_removed_items += _count_items(collector)
        handler.remove_test_path(file_path)

        # In validation mode, track but don't skip
        if config.tach_validate_mode:
            config.tach_would_skip_paths.add(file_path)
            return result  # Run the tests anyway

        return []

    return result


def pytest_report_collectionfinish(
    config: TachConfig,
    start_path: Path,
    startdir: Any,
    items: list[pytest.Item],
) -> str | list[str]:
    handler = config.tach_handler
    lines: list[str] = []

    # Show validation mode header
    if config.tach_validate_mode:
        lines.append(
            "[Tach] VALIDATION MODE - running all tests to verify impact analysis"
        )

    # Show changed files if any
    if handler.all_affected_modules:
        lines.append(
            f"[Tach] {len(handler.all_affected_modules)} file{'s' if len(handler.all_affected_modules) > 1 else ''} changed:"
        )
        for changed_path in sorted(handler.all_affected_modules):
            lines.append(f"[Tach] + {changed_path}")

    # Show skipped/would-skip files
    if config.tach_validate_mode:
        lines.append(
            f"[Tach] Would skip {len(handler.removed_test_paths)} test file{'s' if len(handler.removed_test_paths) > 1 else ''}"
            f" ({handler.num_removed_items} tests) - running anyway to validate."
        )
        for test_path in handler.removed_test_paths:
            lines.append(f"[Tach] ? Would skip '{test_path}'")
    else:
        lines.append(
            f"[Tach] Skipped {len(handler.removed_test_paths)} test file{'s' if len(handler.removed_test_paths) > 1 else ''}"
            f" ({handler.num_removed_items} tests)"
            " since they were unaffected by current changes."
        )
        for test_path in handler.removed_test_paths:
            lines.append(f"[Tach] - Skipped '{test_path}'")

    return lines


def pytest_terminal_summary(terminalreporter: Any, exitstatus: int, config: TachConfig):
    config.tach_handler.tests_ran_to_completion = True

    # In validation mode, check if any would-be-skipped tests failed
    if config.tach_validate_mode and config.tach_would_skip_paths:
        failed_reports = terminalreporter.stats.get("failed", [])
        failed_would_skip: list[str] = []

        # Resolve would-skip paths for comparison
        resolved_would_skip = {p.resolve() for p in config.tach_would_skip_paths}

        for report in failed_reports:
            # TestReport uses fspath for the file path
            if hasattr(report, "fspath") and report.fspath:
                report_path = Path(report.fspath).resolve()
                if report_path in resolved_would_skip:
                    failed_would_skip.append(report.nodeid)

        terminalreporter.write_sep("=", "Tach Validation Results")
        if failed_would_skip:
            terminalreporter.write_line(
                f"[Tach] VALIDATION FAILED: {len(failed_would_skip)} 'would-be-skipped' test(s) failed!",
                red=True,
                bold=True,
            )
            terminalreporter.write_line(
                "[Tach] Impact analysis would have missed these failures:",
                red=True,
            )
            for nodeid in failed_would_skip:
                terminalreporter.write_line(f"[Tach]   - {nodeid}", red=True)
        elif failed_reports:
            # Some tests failed, but none were would-be-skipped
            terminalreporter.write_line(
                f"[Tach] {len(failed_reports)} test(s) failed, but none were 'would-be-skipped'.",
                yellow=True,
            )
            terminalreporter.write_line(
                "[Tach] Impact analysis would have caught these failures.",
                yellow=True,
            )
        else:
            terminalreporter.write_line(
                f"[Tach] VALIDATION PASSED: All {config.tach_handler.num_removed_items} 'would-be-skipped' tests passed.",
                green=True,
                bold=True,
            )
            terminalreporter.write_line(
                "[Tach] Impact analysis is safe to use for these changes.",
                green=True,
            )
