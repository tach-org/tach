from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path
from typing import Any, Generator, Protocol

import pytest
from pytest import Collector

from tach import filesystem as fs
from tach.extension import TachPytestPluginHandler
from tach.filesystem.git_ops import get_changed_files
from tach.parsing import parse_project_config

TACH_DURATIONS_CACHE_KEY = "tach/durations"

# ANSI color codes
_COLORS_ENABLED: bool | None = None


def _colors_enabled() -> bool:
    """Check if colors should be enabled."""
    global _COLORS_ENABLED
    if _COLORS_ENABLED is None:
        # Disable colors if NO_COLOR is set or not a TTY
        _COLORS_ENABLED = (
            os.environ.get("NO_COLOR") is None
            and hasattr(sys.stdout, "isatty")
            and sys.stdout.isatty()
        )
    return _COLORS_ENABLED


def _green(text: str) -> str:
    return f"\033[32m{text}\033[0m" if _colors_enabled() else text


def _yellow(text: str) -> str:
    return f"\033[33m{text}\033[0m" if _colors_enabled() else text


def _cyan(text: str) -> str:
    return f"\033[36m{text}\033[0m" if _colors_enabled() else text


def _bold(text: str) -> str:
    return f"\033[1m{text}\033[0m" if _colors_enabled() else text


def _dim(text: str) -> str:
    return f"\033[2m{text}\033[0m" if _colors_enabled() else text


def _get_default_branch(project_root: Path) -> str:
    """Detect the default branch (main/master) for the repository."""
    # Try to get default branch from remote HEAD
    try:
        result = subprocess.run(
            ["git", "symbolic-ref", "refs/remotes/origin/HEAD"],
            cwd=project_root,
            capture_output=True,
            text=True,
        )
        if result.returncode == 0:
            # Returns something like "refs/remotes/origin/main"
            return result.stdout.strip().split("/")[-1]
    except Exception:
        pass

    # Fallback: check if common branch names exist
    for branch in ["main", "master"]:
        try:
            result = subprocess.run(
                ["git", "rev-parse", "--verify", branch],
                cwd=project_root,
                capture_output=True,
            )
            if result.returncode == 0:
                return branch
        except Exception:
            pass

    # Ultimate fallback
    return "main"


class TachConfig(Protocol):
    tach_handler: TachPytestPluginHandler
    tach_skip_enabled: bool
    """`True` if `--tach-base` was explicitly provided"""
    tach_verbose: bool
    tach_base: str
    tach_head: str
    tach_would_skip_paths: set[Path]

    @property
    def cache(self) -> pytest.Cache: ...

    def getoption(self, name: str) -> Any: ...


class HasTachConfig(Protocol):
    config: TachConfig


def _format_duration(seconds: float) -> str:
    """Format duration in human-readable format."""
    if seconds < 60:
        return f"{seconds:.1f}s"
    minutes = int(seconds // 60)
    secs = seconds % 60
    if minutes < 60:
        return f"{minutes}m {secs:.0f}s"
    hours = minutes // 60
    mins = minutes % 60
    return f"{hours}h {mins}m"


def _get_cached_durations(config: TachConfig) -> dict[str, float]:
    """Get cached test durations from pytest cache."""
    try:
        cached = config.cache.get(TACH_DURATIONS_CACHE_KEY, None)
        if cached is not None:
            return cached
    except Exception:
        pass
    return {}


def _save_durations(config: TachConfig, durations: dict[str, float]) -> None:
    """Save test durations to pytest cache."""
    try:
        config.cache.set(TACH_DURATIONS_CACHE_KEY, durations)
    except Exception:
        pass


def _estimate_skipped_duration(
    config: TachConfig, skipped_paths: set[Path]
) -> float | None:
    """Estimate total duration of skipped tests based on cached durations."""
    if not skipped_paths:
        return None

    cached_durations = _get_cached_durations(config)
    if not cached_durations:
        return None

    total_duration = 0.0
    resolved_skipped = {str(p.resolve()) for p in skipped_paths}

    for nodeid, duration in cached_durations.items():
        # nodeids are like "test_file.py::test_name" or "test_file.py::TestClass::test_name"
        # Extract the file path (before ::)
        if "::" in nodeid:
            file_part = nodeid.split("::")[0]
            # Resolve to absolute path for comparison
            try:
                abs_path = str(Path(file_part).resolve())
                if abs_path in resolved_skipped:
                    total_duration += duration
            except Exception:
                pass

    return total_duration if total_duration > 0 else None


def pytest_addoption(parser: pytest.Parser):
    group = parser.getgroup("tach")
    group.addoption(
        "--tach-base",
        default=None,
        help="Base commit to compare against. When provided, unaffected tests are skipped. [default: main]",
    )
    group.addoption(
        "--tach-head",
        default="",
        help="Head commit to compare against when determining affected tests [default: current filesystem]",
    )
    group.addoption(
        "--tach-verbose",
        action="store_true",
        default=False,
        help="Show detailed tach output including changed files and skipped test paths.",
    )
    group.addoption(
        "--no-tach",
        action="store_true",
        default=False,
        help="Disable the tach pytest plugin entirely.",
    )


@pytest.hookimpl(tryfirst=True)
def pytest_configure(config: TachConfig):
    # Check if plugin is disabled
    if config.getoption("--no-tach"):
        return

    project_root = fs.find_project_config_root() or Path.cwd()
    project_config = parse_project_config(root=project_root)
    if project_config is None:
        # No tach config found, silently disable
        return

    tach_base_option = config.getoption("--tach-base")
    head = config.getoption("--tach-head")

    # Track if skipping is enabled (--tach-base explicitly provided)
    config.tach_skip_enabled = tach_base_option is not None
    config.tach_verbose = config.getoption("--tach-verbose")

    # Use auto-detected default branch for comparison
    base = (
        tach_base_option
        if tach_base_option is not None
        else _get_default_branch(project_root)
    )
    config.tach_base = base
    config.tach_head = head

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
    # Check if plugin is active
    if not hasattr(parent.config, "tach_handler"):
        result = yield
        return result

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
        config.tach_would_skip_paths.add(file_path)

        # Only skip if --tach-base was explicitly provided
        if config.tach_skip_enabled:
            return []

        # Otherwise, run the tests but we've recorded them as would-skip
        return result

    return result


def pytest_report_collectionfinish(
    config: TachConfig,
    start_path: Path,
    startdir: Any,
    items: list[pytest.Item],
) -> str | list[str]:
    # Check if plugin is active
    if not hasattr(config, "tach_handler"):
        return []

    handler = config.tach_handler
    lines: list[str] = []

    num_files = len(handler.removed_test_paths)
    num_tests = handler.num_removed_items

    if num_tests == 0:
        # No tests would be skipped
        return []

    # Build the skip command suggestion
    skip_cmd = f"--tach-base {config.tach_base}"
    if config.tach_head:
        skip_cmd += f" --tach-head {config.tach_head}"

    # Estimate time saved based on cached durations
    estimated_duration = _estimate_skipped_duration(
        config, config.tach_would_skip_paths
    )

    prefix = _cyan("[Tach]")

    if config.tach_skip_enabled:
        # Skipping is active - show what was skipped
        if config.tach_verbose:
            # Verbose: show changed files
            if handler.all_affected_modules:
                lines.append(
                    f"{prefix} {len(handler.all_affected_modules)} file{'s' if len(handler.all_affected_modules) > 1 else ''} changed:"
                )
                for changed_path in sorted(handler.all_affected_modules):
                    lines.append(f"{prefix}   {_green('+')} {_dim(str(changed_path))}")

        # Normal + Verbose: show skipped files
        duration_str = (
            f" ({_green('~' + _format_duration(estimated_duration) + ' saved')})"
            if estimated_duration
            else ""
        )
        lines.append(
            f"{prefix} {_green('Skipped')} {num_tests} test{'s' if num_tests != 1 else ''}"
            f" ({num_files} file{'s' if num_files != 1 else ''}){duration_str}"
            " - unaffected by current changes."
        )

        if config.tach_verbose or num_files <= 5:
            for test_path in handler.removed_test_paths:
                lines.append(f"{prefix}   {_green('-')} {_dim(str(test_path))}")
        elif num_files > 5:
            # Show first 3 and indicate more
            for test_path in list(handler.removed_test_paths)[:3]:
                lines.append(f"{prefix}   {_green('-')} {_dim(str(test_path))}")
            lines.append(f"{prefix}   {_dim(f'... and {num_files - 3} more')}")
    else:
        # Skipping not active - show summary with suggestion
        duration_str = (
            f" ({_yellow('~' + _format_duration(estimated_duration) + ' could be saved')})"
            if estimated_duration
            else ""
        )
        lines.append(
            f"{prefix} {num_tests} test{'s' if num_tests != 1 else ''} in "
            f"{num_files} file{'s' if num_files != 1 else ''} unaffected by changes{duration_str}. "
            f"Skip with: {_bold('pytest ' + skip_cmd)}"
        )

        if config.tach_verbose:
            # Verbose: show details even in info mode
            if handler.all_affected_modules:
                lines.append(
                    f"{prefix} {len(handler.all_affected_modules)} file{'s' if len(handler.all_affected_modules) > 1 else ''} changed:"
                )
                for changed_path in sorted(handler.all_affected_modules):
                    lines.append(f"{prefix}   {_green('+')} {_dim(str(changed_path))}")

            lines.append(f"{prefix} Would skip:")
            for test_path in handler.removed_test_paths:
                lines.append(f"{prefix}   {_yellow('?')} {_dim(str(test_path))}")

    return lines


def pytest_terminal_summary(terminalreporter: Any, exitstatus: int, config: TachConfig):
    # Check if plugin is active
    if not hasattr(config, "tach_handler"):
        return

    config.tach_handler.tests_ran_to_completion = True

    # Only show validation results when skipping is NOT enabled
    # (i.e., when we ran all tests including would-be-skipped ones)
    if not config.tach_skip_enabled and config.tach_would_skip_paths:
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

        if failed_would_skip:
            terminalreporter.write_sep("=", "Tach Impact Analysis Warning")
            terminalreporter.write_line(
                f"[Tach] WARNING: {len(failed_would_skip)} test(s) failed that would be skipped by impact analysis!",
                yellow=True,
                bold=True,
            )
            terminalreporter.write_line(
                "[Tach] These failures would be missed when using --tach-base:",
                yellow=True,
            )
            for nodeid in failed_would_skip:
                terminalreporter.write_line(f"[Tach]   - {nodeid}", yellow=True)

    # Record test durations for future estimation
    _record_test_durations(terminalreporter, config)


def _record_test_durations(terminalreporter: Any, config: TachConfig) -> None:
    """Record test durations to cache for future time estimation."""
    # Get all test reports (passed, failed, etc.)
    all_reports: list[Any] = []
    for category in ["passed", "failed", "error"]:
        all_reports.extend(terminalreporter.stats.get(category, []))

    if not all_reports:
        return

    # Get existing durations and update with new ones
    durations = _get_cached_durations(config)

    for report in all_reports:
        # Only record "call" phase duration (not setup/teardown)
        if hasattr(report, "when") and report.when == "call":
            if hasattr(report, "nodeid") and hasattr(report, "duration"):
                durations[report.nodeid] = report.duration

    # Save updated durations
    _save_durations(config, durations)
