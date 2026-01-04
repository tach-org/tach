from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any, Generator, cast

import pytest
from pytest import Collector, Config, StashKey
from rich.console import Console

if TYPE_CHECKING:
    from _pytest.reports import TestReport
    from _pytest.terminal import TerminalReporter

from tach import filesystem as fs
from tach.extension import TachPytestPluginHandler
from tach.filesystem.git_ops import get_changed_files
from tach.parsing import parse_project_config

TACH_DURATIONS_CACHE_KEY = "tach/durations"

# Rich console for colored output. force_terminal=True ensures ANSI codes are
# always generated even when captured. This is needed because pytest_report_collectionfinish
# requires returning strings (can't print directly), so we capture styled output
# and return it for pytest to display. soft_wrap=True prevents Rich from inserting
# hard line breaks in long paths.
_console = Console(highlight=False, force_terminal=True, soft_wrap=True)


def _styled(text: str, style: str) -> str:
    """Return text with ANSI styling using rich."""
    with _console.capture() as capture:
        _console.print(text, style=style, end="")
    return capture.get()


def _green(text: str) -> str:
    return _styled(text, "green")


def _yellow(text: str) -> str:
    return _styled(text, "yellow")


def _cyan(text: str) -> str:
    return _styled(text, "cyan")


def _bold(text: str) -> str:
    return _styled(text, "bold")


def _dim(text: str) -> str:
    return _styled(text, "dim")


def _get_default_branch(project_root: Path) -> str:
    """Detect the default branch (main/master) for the repository.

    Uses multiple detection methods because:
    1. Remote HEAD is most reliable but requires remote to be configured
    2. Local branch check works offline but may be ambiguous if both exist
    3. Falls back to "main" as sensible default for new repos
    """
    # Method 1: Check remote HEAD symref (most reliable when remote exists)
    # This tells us what branch the remote considers its default
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

    # Method 2: Check which common branch names exist locally
    # Prefer "main" over "master" when both exist (modern convention)
    existing_branches: list[str] = []
    for branch in ["main", "master"]:
        try:
            result = subprocess.run(
                ["git", "rev-parse", "--verify", branch],
                cwd=project_root,
                capture_output=True,
            )
            if result.returncode == 0:
                existing_branches.append(branch)
        except Exception:
            pass

    if existing_branches:
        # If both exist, prefer "main" (it's first in our check order)
        return existing_branches[0]

    # Method 3: Ultimate fallback for repos without standard branch names
    return "main"


@dataclass
class TachPluginState:
    """State for the tach pytest plugin, stored in pytest's stash."""

    handler: TachPytestPluginHandler
    skip_enabled: bool
    """True if --tach or --tach-base was explicitly provided to enable skipping."""
    verbose: bool
    base: str
    head: str | None
    would_skip_paths: set[Path]


tach_state_key: StashKey[TachPluginState] = StashKey()
"""StashKey for storing plugin state on pytest Config."""


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


def _get_cached_durations(config: Config) -> dict[str, float]:
    """Get cached test durations from pytest cache."""
    cache = config.cache
    if cache is not None:
        cached = cast(
            "dict[str, float] | None",
            cache.get(TACH_DURATIONS_CACHE_KEY, None),  # pyright: ignore[reportUnknownMemberType]
        )
        if cached is not None:
            return cached
    return {}


def _save_durations(config: Config, durations: dict[str, float]) -> None:
    """Save test durations to pytest cache."""
    cache = config.cache
    if cache is not None:
        cache.set(TACH_DURATIONS_CACHE_KEY, durations)


def _estimate_skipped_duration(
    config: Config, skipped_paths: set[Path]
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
        "--tach",
        action="store_true",
        default=False,
        help="Enable test skipping based on impact analysis using auto-detected base branch.",
    )
    group.addoption(
        "--tach-base",
        default=None,
        help="Base commit to compare against. Enables test skipping. [default: auto-detected]",
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


@pytest.hookimpl(tryfirst=True)
def pytest_configure(config: Config):
    project_root = fs.find_project_config_root() or Path.cwd()
    project_config = parse_project_config(root=project_root)
    if project_config is None:
        # No tach config found, silently disable
        return

    tach_flag = cast("bool", config.getoption("--tach"))
    tach_base_option = cast("str | None", config.getoption("--tach-base"))
    head = cast("str | None", config.getoption("--tach-head")) or None
    verbose = cast("bool", config.getoption("--tach-verbose"))

    # Skipping is enabled if --tach, --tach-base, or --tach-head is provided
    skip_enabled = tach_flag or tach_base_option is not None or bool(head)

    # Use explicit base if provided, otherwise auto-detect
    base = (
        tach_base_option
        if tach_base_option is not None
        else _get_default_branch(project_root)
    )

    try:
        changed_files = get_changed_files(
            project_root=project_root,
            head=head,
            base=base,
        )
    except Exception:
        # If we can't determine changed files (e.g., shallow clone in CI
        # without the base branch), only silently disable if no --tach* flags
        # were explicitly provided. Otherwise the user expects the plugin to work.
        if skip_enabled:
            raise pytest.UsageError(
                f"[tach] Could not determine changed files (base='{base}'). "
                "The base branch may not exist in this checkout. "
                f"In CI, try: git fetch origin {base}:{base}"
            )
        return

    handler = TachPytestPluginHandler(
        project_root=project_root,
        project_config=project_config,
        changed_files=changed_files,
        all_affected_modules={changed_file.resolve() for changed_file in changed_files},
    )

    # Store state in pytest's stash (the proper way to store plugin state)
    config.stash[tach_state_key] = TachPluginState(
        handler=handler,
        skip_enabled=skip_enabled,
        verbose=verbose,
        base=base,
        head=head,
        would_skip_paths=set(),
    )


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
    file_path: Path, parent: Collector
) -> Generator[None, list[Collector], list[Collector]]:
    # Check if plugin is active
    if tach_state_key not in parent.config.stash:
        result = yield
        return result

    state = parent.config.stash[tach_state_key]
    handler = state.handler

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
        state.would_skip_paths.add(file_path)

        # Only skip if --tach or --tach-base was provided
        if state.skip_enabled:
            return []

        # Otherwise, run the tests but we've recorded them as would-skip
        return result

    return result


def _pluralize(word: str, count: int) -> str:
    """Return word with 's' suffix if count != 1."""
    return word if count == 1 else f"{word}s"


def pytest_report_collectionfinish(
    config: Config,
    start_path: Path,
    startdir: Any,
    items: list[pytest.Item],
) -> str | list[str]:
    """Report skipped/would-skip test files after collection.

    Output when skipping enabled (--tach or --tach-base):
        [Tach] Skipped 5 tests (2 files) (~12.3s saved) - unaffected by current changes.
        [Tach]   - path/to/test_file.py
        [Tach]   - path/to/other_test.py

    Output when skipping disabled (default):
        [Tach] 5 tests in 2 files unaffected by changes (~12.3s could be saved). Skip with: pytest --tach

    With --tach-verbose, also shows changed files and would-skip paths.
    """
    if tach_state_key not in config.stash:
        return []

    state = config.stash[tach_state_key]
    handler = state.handler

    num_files = len(handler.removed_test_paths)
    num_tests = handler.num_removed_items

    if num_tests == 0:
        return []

    prefix = _cyan("[Tach]")
    estimated_duration = _estimate_skipped_duration(config, state.would_skip_paths)

    # Format helpers
    def _format_paths(
        paths: set[str] | set[Path], marker: str, max_shown: int = 5
    ) -> str:
        path_list = list(paths)
        show_all = state.verbose or len(path_list) <= max_shown
        lines = [
            f"{prefix}   {marker} {_dim(str(p))}"
            for p in (path_list if show_all else path_list[:max_shown])
        ]
        if not show_all:
            remaining = len(path_list) - max_shown
            lines.append(f"{prefix}   {_dim(f'... and {remaining} more')}")
        return "\n".join(lines)

    def _format_changed() -> str:
        if not handler.all_affected_modules:
            return ""
        num = len(handler.all_affected_modules)
        header = f"{prefix} {num} {_pluralize('file', num)} changed:"
        lines = [
            f"{prefix}   {_green('+')} {_dim(str(p))}"
            for p in sorted(handler.all_affected_modules)
        ]
        return header + "\n" + "\n".join(lines)

    if state.skip_enabled:
        duration = (
            f" ({_green('~' + _format_duration(estimated_duration) + ' saved')})"
            if estimated_duration
            else ""
        )
        changed_section = (
            (_format_changed() + "\n")
            if state.verbose and handler.all_affected_modules
            else ""
        )
        skipped_paths = _format_paths(handler.removed_test_paths, _green("-"))

        output = f"""\
{changed_section}\
{prefix} {_green("Skipped")} {num_tests} {_pluralize("test", num_tests)} ({num_files} {_pluralize("file", num_files)}){duration} - unaffected by current changes.
{skipped_paths}"""

    else:
        duration = (
            f" ({_yellow('~' + _format_duration(estimated_duration) + ' could be saved')})"
            if estimated_duration
            else ""
        )
        disable_hint = f"{prefix} {_dim('To disable: pytest -p no:tach (https://docs.gauge.sh/usage/commands#using-the-pytest-plugin-directly)')}"

        if state.verbose:
            changed_section = (
                (_format_changed() + "\n") if handler.all_affected_modules else ""
            )
            would_skip_paths = "\n".join(
                f"{prefix}   {_yellow('?')} {_dim(str(p))}"
                for p in handler.removed_test_paths
            )
            output = f"""\
{prefix} {num_tests} {_pluralize("test", num_tests)} in {num_files} {_pluralize("file", num_files)} unaffected by changes{duration}. Skip with: {_bold("pytest --tach")}
{disable_hint}
{changed_section}\
{prefix} Would skip:
{would_skip_paths}"""
        else:
            output = f"""\
{prefix} {num_tests} {_pluralize("test", num_tests)} in {num_files} {_pluralize("file", num_files)} unaffected by changes{duration}. Skip with: {_bold("pytest --tach")}
{disable_hint}"""

    return output.strip().split("\n")


def pytest_terminal_summary(
    terminalreporter: TerminalReporter,
    config: Config,
):
    # Check if plugin is active
    if tach_state_key not in config.stash:
        return

    state = config.stash[tach_state_key]
    state.handler.tests_ran_to_completion = True

    # Only show validation results when skipping is NOT enabled
    # (i.e., when we ran all tests including would-be-skipped ones)
    if not state.skip_enabled and state.would_skip_paths:
        failed_reports: list[TestReport] = terminalreporter.stats.get("failed", [])
        failed_would_skip: list[str] = []

        # Resolve would-skip paths for comparison
        resolved_would_skip = {p.resolve() for p in state.would_skip_paths}

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
                "[Tach] These failures would be missed when using --tach:",
                yellow=True,
            )
            for nodeid in failed_would_skip:
                terminalreporter.write_line(f"[Tach]   - {nodeid}", yellow=True)

    # Record test durations for future estimation
    _record_test_durations(terminalreporter, config)


def _record_test_durations(terminalreporter: TerminalReporter, config: Config) -> None:
    """Record test durations to cache for future time estimation."""
    # Get all test reports (passed, failed, etc.)
    all_reports: list[TestReport] = []
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
