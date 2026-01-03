from __future__ import annotations

import pytest

pytest_plugins = ["pytester"]


def makepyfile(pytester: pytest.Pytester, *args: str | bytes, **kwargs: str | bytes):
    """workaround for https://github.com/pytest-dev/pytest/pull/14080"""
    _ = pytester.makepyfile(*args, **kwargs)  # pyright: ignore[reportUnknownMemberType]


@pytest.fixture
def tach_project(pytester: pytest.Pytester):
    """Create a basic tach project structure."""
    _ = pytester.makefile(".toml", tach='source_roots = ["."]')
    makepyfile(
        pytester,
        src_module="""
def add(a, b):
    return a + b

def subtract(a, b):
    return a - b
""",
        test_with_import="""
from src_module import add

def test_add_basic():
    assert add(1, 2) == 3

def test_add_zero():
    assert add(0, 0) == 0

def test_add_negative():
    assert add(-1, 1) == 0
""",
        test_no_import="""
def test_standalone_1():
    assert True

def test_standalone_2():
    assert 1 + 1 == 2
""",
    )
    # Initialize git repo
    _ = pytester.run("git", "init")
    _ = pytester.run("git", "config", "user.email", "test@test.com")
    _ = pytester.run("git", "config", "user.name", "Test")
    _ = pytester.run("git", "add", "-A")
    _ = pytester.run("git", "commit", "-m", "initial")
    return pytester


def run_pytest(pytester: pytest.Pytester, *args: str) -> pytest.RunResult:
    """Run pytest in subprocess to avoid PyO3 reinitialization issues.

    The tach plugin is auto-loaded via pytest11 entrypoint.
    """
    return pytester.runpytest_subprocess(*args)


class TestPytestPluginSkipping:
    def test_no_changes_skips_all_tests(self, tach_project: pytest.Pytester):
        """When there are no changes, all tests should be skipped."""
        result = run_pytest(tach_project, "--tach-base", "HEAD")
        result.assert_outcomes(passed=0)
        result.stdout.fnmatch_lines(["*Skipped 5 test* (2 file*"])

    def test_source_change_runs_dependent_tests(self, tach_project: pytest.Pytester):
        """When a source file changes, only tests that import it should run."""
        # Modify the source file
        makepyfile(
            tach_project,
            src_module="""
def add(a, b):
    return a + b

def subtract(a, b):
    return a - b

# Modified
""",
        )
        _ = tach_project.run("git", "add", "src_module.py")
        _ = tach_project.run("git", "commit", "-m", "modify source")

        result = run_pytest(tach_project, "--tach-base", "HEAD~1")
        result.assert_outcomes(passed=3)
        result.stdout.fnmatch_lines(
            [
                "*Skipped 2 test* (1 file*",
                "*test_no_import.py*",
            ]
        )

    def test_test_file_change_runs_that_file(self, tach_project: pytest.Pytester):
        """When a test file is directly modified, it should run."""
        # Modify a test file
        makepyfile(
            tach_project,
            test_no_import="""
def test_standalone_1():
    assert True

def test_standalone_2():
    assert 1 + 1 == 2

def test_standalone_3():
    assert "new test"
""",
        )
        _ = tach_project.run("git", "add", "test_no_import.py")
        _ = tach_project.run("git", "commit", "-m", "add test")

        result = run_pytest(tach_project, "--tach-base", "HEAD~1")
        result.assert_outcomes(passed=3)
        result.stdout.fnmatch_lines(["*Skipped 3 test* (1 file*"])


class TestPytestPluginDefaults:
    def test_default_mode_runs_all_tests_with_suggestion(
        self, tach_project: pytest.Pytester
    ):
        """Without --tach, all tests run but a skip suggestion is shown."""
        result = run_pytest(tach_project)
        # All tests should run
        result.assert_outcomes(passed=5)
        # Should show suggestion to skip using --tach
        result.stdout.fnmatch_lines(["*unaffected by changes*Skip with*--tach*"])

    def test_tach_flag_enables_skipping(self, tach_project: pytest.Pytester):
        """--tach should enable skipping with auto-detected base branch."""
        result = run_pytest(tach_project, "--tach")
        result.assert_outcomes(passed=0)
        result.stdout.fnmatch_lines(["*Skipped 5 test* (2 file*"])

    def test_disable_plugin_with_p_flag(self, tach_project: pytest.Pytester):
        """-p no:tach should disable the plugin entirely."""
        result = run_pytest(tach_project, "-p", "no:tach")
        # All tests should run
        result.assert_outcomes(passed=5)
        # Should NOT show any tach output
        assert "[Tach]" not in result.stdout.str()

    def test_verbose_mode_shows_details(self, tach_project: pytest.Pytester):
        """--tach-verbose should show changed files and would-skip paths."""
        result = run_pytest(tach_project, "--tach-verbose")
        # All tests should run
        result.assert_outcomes(passed=5)
        # Should show "Would skip" in verbose output
        result.stdout.fnmatch_lines(["*Would skip*"])


class TestPytestPluginCounting:
    def test_counts_all_tests_in_file(self, tach_project: pytest.Pytester):
        """Should correctly count all tests including parametrized ones."""
        makepyfile(
            tach_project,
            test_parametrized="""
import pytest

@pytest.mark.parametrize("x,y,expected", [
    (1, 2, 3),
    (2, 3, 5),
    (10, 20, 30),
])
def test_param_add(x, y, expected):
    assert x + y == expected

def test_regular():
    assert True
""",
        )
        _ = tach_project.run("git", "add", "test_parametrized.py")
        _ = tach_project.run("git", "commit", "--amend", "--no-edit")

        result = run_pytest(tach_project, "--tach-base", "HEAD")
        result.assert_outcomes(passed=0)
        # 3 (test_with_import) + 2 (test_no_import) + 4 (test_parametrized) = 9
        result.stdout.fnmatch_lines(["*Skipped 9 test* (3 file*"])

    def test_counts_tests_in_classes(self, tach_project: pytest.Pytester):
        """Should correctly count tests inside test classes."""
        makepyfile(
            tach_project,
            test_class="""
class TestGroup:
    def test_one(self):
        assert True

    def test_two(self):
        assert True

class TestAnotherGroup:
    def test_three(self):
        assert True
""",
        )
        _ = tach_project.run("git", "add", "test_class.py")
        _ = tach_project.run("git", "commit", "--amend", "--no-edit")

        result = run_pytest(tach_project, "--tach-base", "HEAD")
        result.assert_outcomes(passed=0)
        # 3 (test_with_import) + 2 (test_no_import) + 3 (test_class) = 8
        result.stdout.fnmatch_lines(["*Skipped 8 test* (3 file*"])


class TestPytestPluginDurations:
    def test_shows_estimated_duration_after_caching(
        self, tach_project: pytest.Pytester
    ):
        """After running once, estimated duration should be shown on subsequent runs."""
        # First run to cache durations (without --tach-base, so all tests run)
        result1 = run_pytest(tach_project)
        result1.assert_outcomes(passed=5)

        # Second run with --tach-base HEAD to skip tests
        result2 = run_pytest(tach_project, "--tach-base", "HEAD")
        result2.assert_outcomes(passed=0)
        # Should show estimated duration (format: ~X.Xs saved)
        result2.stdout.fnmatch_lines(["*~*s saved*"])
