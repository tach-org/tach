from __future__ import annotations

import json
import shutil
from unittest.mock import NonCallableMagicMock

import pytest

from tach import icons
from tach.cli import tach_check, tach_check_external
from tach.errors import TachCircularDependencyError, TachVisibilityError
from tach.extension import Diagnostic
from tach.icons import FAIL, SUCCESS, WARNING
from tach.parsing.config import parse_project_config


def test_valid_example_dir(example_dir, capfd):
    project_root = example_dir / "valid"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None
    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 0
    captured = capfd.readouterr()
    assert SUCCESS in captured.err
    assert WARNING in captured.err or "WARN" in captured.err


def test_valid_example_dir_monorepo(example_dir):
    project_root = example_dir / "monorepo_namespace"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None
    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 0


def test_check_json_output(example_dir, capfd, mocker):
    project_root = example_dir / "valid"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    mock_diagnostics = [NonCallableMagicMock(spec=Diagnostic)]
    mock_diagnostics[0].is_error.return_value = False
    mocker.patch(
        "tach.extension.serialize_diagnostics_json",
        return_value=json.dumps([{"hello": "world"}]),
    )
    mocker.patch("tach.extension.check", return_value=mock_diagnostics)

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
            output_format="json",
        )
    assert exc_info.value.code == 0

    captured = capfd.readouterr()
    assert json.loads(captured.out) == [{"hello": "world"}]


def test_check_json_with_errors(example_dir, capfd, mocker):
    project_root = example_dir / "valid"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    mock_diagnostics = [NonCallableMagicMock(spec=Diagnostic)]
    mocker.patch(
        "tach.extension.serialize_diagnostics_json",
        return_value=json.dumps(
            {"errors": ["error1", "error2"], "warnings": ["warning1"]}
        ),
    )
    mocker.patch("tach.extension.check", return_value=mock_diagnostics)

    with pytest.raises(SystemExit):
        tach_check(
            project_root=project_root,
            project_config=project_config,
            output_format="json",
        )

    captured = capfd.readouterr()
    assert json.loads(captured.out) == {
        "errors": ["error1", "error2"],
        "warnings": ["warning1"],
    }


def test_check_circular_dependency_text(example_dir, capfd, mocker):
    project_root = example_dir / "valid"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    mocker.patch(
        "tach.extension.check",
        side_effect=TachCircularDependencyError(["mod1", "mod2", "mod1"]),
    )

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    assert "Circular dependency detected" in captured.err
    assert "'mod1'" in captured.err
    assert "'mod2'" in captured.err


def test_check_circular_dependency_json(example_dir, capfd, mocker):
    project_root = example_dir / "valid"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    mocker.patch(
        "tach.extension.check",
        side_effect=TachCircularDependencyError(["mod1", "mod2", "mod1"]),
    )

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
            output_format="json",
        )
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    result = json.loads(captured.out)
    assert result["error"] == "Circular dependency"
    assert result["dependencies"] == ["mod1", "mod2", "mod1"]


def test_check_visibility_error_text(example_dir, capfd, mocker):
    project_root = example_dir / "valid"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    visibility_errors = [("mod1", "mod2", ["public"])]
    mocker.patch(
        "tach.extension.check", side_effect=TachVisibilityError(visibility_errors)
    )

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    assert "Module configuration error" in captured.err
    assert "'mod1' cannot depend on 'mod2'" in captured.err
    assert "public" in captured.err


def test_check_visibility_error_json(example_dir, capfd, mocker):
    project_root = example_dir / "valid"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    visibility_errors = [("mod1", "mod2", ["public"])]
    mocker.patch(
        "tach.extension.check", side_effect=TachVisibilityError(visibility_errors)
    )

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
            output_format="json",
        )
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    result = json.loads(captured.out)
    assert result["error"] == "Visibility error"
    assert result["visibility_errors"] == [["mod1", "mod2", ["public"]]]


def test_distributed_config_example_dir(example_dir, capfd):
    project_root = example_dir / "distributed_config"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    assert FAIL in captured.err or "FAIL" in captured.err
    assert "Cannot use 'project.module_one.module_one'" in captured.err
    assert (
        "Module 'project.top_level' cannot depend on 'project.module_one'"
        in captured.err
    )
    assert "project/top_level.py" in captured.err


def _check_expected_messages_unordered(
    section_text: str, expected_messages: list[tuple]
) -> None:
    """Helper to verify all expected messages appear in a section of output text.

    Args:
        section_text: The text section to check
        expected_messages: List of tuples containing substrings that should appear together in a line
    """
    lines = iter(section_text.split("\n"))
    substr_tuples = set(expected_messages)
    for line in lines:
        if (
            "[WARN]" in line
            or "[FAIL]" in line
            or icons.FAIL in line
            or icons.WARNING in line
        ):
            matched = False
            for substr_tuple in substr_tuples:
                if all(substr.lower() in line.lower() for substr in substr_tuple):
                    substr_tuples.remove(substr_tuple)
                    matched = True
                    break
            if not matched:
                assert False, f"Unexpected warning/error line: {line}"

    assert not substr_tuples, (
        f"Not all expected messages were found: {list(substr_tuples)} in section: {section_text}"
    )


def test_many_features_example_dir(example_dir, capfd):
    project_root = example_dir / "many_features"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    general_header = captured.err.index("General\n")
    interfaces_header = captured.err.index("Interfaces\n")
    dependencies_header = captured.err.index("Internal Dependencies\n")
    unused_header = captured.err.index("Unused Dependencies")

    general_section = captured.err[general_header:interfaces_header]
    interfaces_section = captured.err[interfaces_header:dependencies_header]
    dependencies_section = captured.err[dependencies_header:unused_header]
    unused_section = captured.err[unused_header:]

    expected_general = [
        (
            "[WARN]",
            "other_src_root/module1/api.py",
            "ignore directive",
            "missing a reason",
        ),
        (
            "[WARN]",
            "other_src_root/module4/service.py",
            "ignore directive",
            "missing a reason",
        ),
        ("[WARN]", "real_src/main.py", "ignore directive", "missing a reason"),
        (
            "[FAIL]",
            "other_src_root/module4/service.py:6",
            "ignore directive",
            "unused",
        ),
        (
            "[FAIL]",
            "other_src_root/module4/service.py:12",
            "ignore directive",
            "unused",
        ),
        ("[FAIL]", "other_src_root/module1/api.py", "ignore directive", "unused"),
        ("[FAIL]", "real_src/main.py", "ignore directive", "unused"),
    ]

    expected_interfaces = [
        (
            "[FAIL]",
            "real_src/module1/__init__.py",
            "module3.anything",
            "public interface",
        ),
        (
            "[FAIL]",
            "real_src/module1/controller.py",
            "module5.something",
            "public interface",
        ),
        (
            "[FAIL]",
            "real_src/module1/controller.py",
            "module3.anything",
            "public interface",
        ),
        (
            "[FAIL]",
            "other_src_root/module5/__init__.py",
            "module1.api.something",
            "public interface",
        ),
    ]

    expected_dependencies = [
        ("[FAIL]", "real_src/module2/service.py", "outer_module", "module2"),
        ("[FAIL]", "real_src/module3/__init__.py", "'low'", "lower than", "'mid'"),
        (
            "[FAIL]",
            "real_src/globbed/other/module.py",
            "'hightest'",
            "lower than",
            "'high'",
        ),
        (
            "[FAIL]",
            "other_src_root/module4/service.py",
            "cannot depend on",
            "globbed.other.module.something",
        ),
        (
            "[FAIL]",
            "real_src/module1/controller.py",
            "'hightest'",
            "lower than",
            "'mid'",
            "module5",
        ),
        (
            "[FAIL]",
            "real_src/module1/controller.py:5",
            "'hightest'",
            "lower than",
            "'low'",
            "module3",
        ),
        (
            "[FAIL]",
            "real_src/module1/controller.py:6",
            "'hightest'",
            "lower than",
            "'low'",
            "module3",
        ),
        (
            "[FAIL]",
            "real_src/module3/submodule1/__init__.py",
            "'module3.submodule1'",
            "cannot depend on",
            "'module3.submodule2'",
        ),
        (
            "[FAIL]",
            "real_src/module2/__init__.py:8",
            "Cannot use",
            "'module3.submodule3.lowest_open.something_else'",
            "closed",
        ),
    ]

    expected_unused = [
        ("module1", "module5"),
    ]

    _check_expected_messages_unordered(general_section, expected_general)
    _check_expected_messages_unordered(interfaces_section, expected_interfaces)
    _check_expected_messages_unordered(dependencies_section, expected_dependencies)
    _check_expected_messages_unordered(unused_section, expected_unused)


def test_many_features_example_dir__external(example_dir, capfd):
    project_root = example_dir / "many_features"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    with pytest.raises(SystemExit) as exc_info:
        tach_check_external(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    general_header = captured.err.index("General\n")
    external_header = captured.err.index("External Dependencies\n")

    general_section = captured.err[general_header:external_header]
    external_section = captured.err[external_header:]

    expected_general = [
        ("[WARN]", "real_src/main.py", "ignore directive", "missing a reason"),
        (
            "[WARN]",
            "real_src/module1/__init__.py",
            "ignore directive",
            "missing a reason",
        ),
        ("[FAIL]", "real_src/module1/__init__.py", "ignore directive", "unused"),
    ]

    expected_external = [
        ("[FAIL]", "prompt_toolkit", "not used"),
        ("[FAIL]", "importlib_metadata", "not used"),
        ("[FAIL]", "tomli_w", "not used"),
        ("[FAIL]", "pydot", "not used"),
        ("[FAIL]", "rich", "not used"),
        ("[FAIL]", "stdlib_list", "not used"),
        ("[FAIL]", "other_src_root/module5/__init__.py", "module5", "networkx"),
        ("[FAIL]", "real_src/module1/__init__.py", "module1", "tomli"),
        ("[FAIL]", "real_src/django_settings.py", "django", "not declared"),
        ("[FAIL]", "real_src/module3/content.py", "django", "not declared"),
    ]

    _check_expected_messages_unordered(general_section, expected_general)
    _check_expected_messages_unordered(external_section, expected_external)


def test_monorepo_workspace_example_dir(example_dir, capfd):
    project_root = example_dir / "monorepo_workspace"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 0

    captured = capfd.readouterr()
    configuration_header = captured.err.index("Configuration\n")

    configuration_section = captured.err[configuration_header:]

    expected_configuration = [
        ("[WARN]", "No first-party imports"),
    ]

    _check_expected_messages_unordered(configuration_section, expected_configuration)


def test_monorepo_workspace_example_dir_external(example_dir, capfd):
    project_root = example_dir / "monorepo_workspace"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    with pytest.raises(SystemExit) as exc_info:
        tach_check_external(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    external_header = captured.err.index("External Dependencies\n")

    external_section = captured.err[external_header:]

    expected_external = [
        ("[FAIL]", "requests", "not used"),
        ("[FAIL]", "src/albatross/__init__.py", "package2", "not declared"),
    ]

    _check_expected_messages_unordered(external_section, expected_external)


def test_visibility_error_example_dir(example_dir, capfd):
    project_root = example_dir / "visibility_error"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    assert "Module configuration error" in captured.err
    assert "'module2' cannot depend on 'module3'" in captured.err
    assert "module3" in captured.err
    assert "['module1']" in captured.err


def test_many_features_example_dir_with_gitignore(example_dir, capfd, tmp_path):
    project_root = tmp_path / "many_features"
    shutil.copytree(example_dir / "many_features", project_root)

    (project_root / ".gitignore").write_text("""
# Ignore module3 entirely
real_src/module3/
# Ignore specific files
real_src/module1/controller.py
other_src_root/module4/service.py
""")

    (project_root / "real_src" / ".gitignore").write_text("""
# Ignore all python files in globbed directory
globbed/**/*.py
""")

    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    general_header = captured.err.index("General\n")
    interfaces_header = captured.err.index("Interfaces\n")
    dependencies_header = captured.err.index("Internal Dependencies\n")
    unused_header = captured.err.index("Unused Dependencies")

    general_section = captured.err[general_header:interfaces_header]
    interfaces_section = captured.err[interfaces_header:dependencies_header]
    dependencies_section = captured.err[dependencies_header:unused_header]
    unused_section = captured.err[unused_header:]

    # Files that are gitignored should not appear in the diagnostics
    assert "real_src/module3/" not in captured.err
    assert "real_src/module1/controller.py" not in captured.err
    assert "other_src_root/module4/service.py" not in captured.err
    assert "real_src/globbed/" not in captured.err

    expected_general = [
        (
            "[WARN]",
            "other_src_root/module1/api.py",
            "ignore directive",
            "missing a reason",
        ),
        ("[WARN]", "real_src/main.py", "ignore directive", "missing a reason"),
        ("[FAIL]", "other_src_root/module1/api.py", "ignore directive", "unused"),
        ("[FAIL]", "real_src/main.py", "ignore directive", "unused"),
    ]

    expected_interfaces = [
        (
            "[FAIL]",
            "other_src_root/module5/__init__.py",
            "module1.api.something",
            "public interface",
        ),
    ]

    expected_dependencies = [
        ("[FAIL]", "real_src/module2/service.py", "outer_module", "module2"),
    ]

    expected_unused = [
        ("module1", "module5"),
    ]

    _check_expected_messages_unordered(general_section, expected_general)
    _check_expected_messages_unordered(interfaces_section, expected_interfaces)
    _check_expected_messages_unordered(dependencies_section, expected_dependencies)
    _check_expected_messages_unordered(unused_section, expected_unused)


def test_many_features_example_dir_with_gitignore__external(
    example_dir, capfd, tmp_path
):
    project_root = tmp_path / "many_features"
    shutil.copytree(example_dir / "many_features", project_root)

    (project_root / ".gitignore").write_text("""
# Ignore module3 entirely
real_src/module3/
# Ignore specific files
real_src/module1/controller.py
other_src_root/module4/service.py
""")

    (project_root / "real_src" / ".gitignore").write_text("""
# Ignore all python files in globbed directory
globbed/**/*.py
""")

    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    with pytest.raises(SystemExit) as exc_info:
        tach_check_external(
            project_root=project_root,
            project_config=project_config,
        )
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    general_header = captured.err.index("General\n")
    external_header = captured.err.index("External Dependencies\n")

    general_section = captured.err[general_header:external_header]
    external_section = captured.err[external_header:]

    # Files that are gitignored should not appear in the diagnostics
    assert "real_src/module3/" not in captured.err
    assert "real_src/module1/controller.py" not in captured.err
    assert "other_src_root/module4/service.py" not in captured.err
    assert "real_src/globbed/" not in captured.err

    expected_general = [
        ("[WARN]", "real_src/main.py", "ignore directive", "missing a reason"),
        (
            "[WARN]",
            "real_src/module1/__init__.py",
            "ignore directive",
            "missing a reason",
        ),
        ("[FAIL]", "real_src/module1/__init__.py", "ignore directive", "unused"),
    ]

    expected_external = [
        ("[FAIL]", "prompt_toolkit", "not used"),
        ("[FAIL]", "importlib_metadata", "not used"),
        ("[FAIL]", "tomli_w", "not used"),
        ("[FAIL]", "pydot", "not used"),
        ("[FAIL]", "rich", "not used"),
        ("[FAIL]", "stdlib_list", "not used"),
        ("[FAIL]", "other_src_root/module5/__init__.py", "module5", "networkx"),
        ("[FAIL]", "real_src/module1/__init__.py", "module1", "tomli"),
        ("[FAIL]", "real_src/django_settings.py", "django", "not declared"),
    ]

    _check_expected_messages_unordered(general_section, expected_general)
    _check_expected_messages_unordered(external_section, expected_external)


def test_layers_explicit_depends_on(tmp_path, capfd):
    """Test the layers_explicit_depends_on flag requires explicit depends_on declarations."""
    # Create a test project with layers
    project_root = tmp_path / "test_project"
    project_root.mkdir()

    # Create tach.toml with layers_explicit_depends_on enabled
    config_file = project_root / "tach.toml"
    config_file.write_text("""
layers_explicit_depends_on = true

layers = ["api", "service", "repo"]

[[modules]]
path = "api"
layer = "api"
depends_on = []

[[modules]]
path = "service"
layer = "service"
depends_on = []

[[modules]]
path = "repo"
layer = "repo"
depends_on = []

[[modules]]
path = "utils"
utility = true
depends_on = []
""")

    # Create module directories
    api_dir = project_root / "api"
    service_dir = project_root / "service"
    repo_dir = project_root / "repo"
    utils_dir = project_root / "utils"

    for dir_path in [api_dir, service_dir, repo_dir, utils_dir]:
        dir_path.mkdir()
        (dir_path / "__init__.py").write_text("")

    # Create a file in api that imports from service (should fail without explicit depends_on)
    (api_dir / "handler.py").write_text("from service import something\n")

    # Create a file in api that imports from utils (should pass, utility modules are exempt)
    (api_dir / "helper.py").write_text("from utils import tool\n")

    # Create the imported modules
    (service_dir / "__init__.py").write_text("something = 1\n")
    (utils_dir / "__init__.py").write_text("tool = lambda: None\n")

    # Parse config and run check - should fail due to undeclared dependency
    project_config = parse_project_config(root=project_root)
    assert project_config is not None
    assert project_config.layers_explicit_depends_on is True

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
        )

    # Should exit with error code due to undeclared dependency
    assert exc_info.value.code == 1

    captured = capfd.readouterr()
    # Should report undeclared dependency from api to service
    assert "api/handler.py" in captured.err
    assert "service" in captured.err
    # Should NOT report utils (utility modules are exempt)
    assert "utils" not in captured.err or "api/helper.py" not in captured.err


def test_layers_explicit_depends_on_disabled(tmp_path, capfd):
    """Test that layers_explicit_depends_on defaults to false and allows implicit dependencies."""
    # Create a test project with layers but flag disabled
    project_root = tmp_path / "test_project"
    project_root.mkdir()

    # Create tach.toml WITHOUT layers_explicit_depends_on (defaults to false)
    config_file = project_root / "tach.toml"
    config_file.write_text("""
layers = ["api", "service"]

[[modules]]
path = "api"
layer = "api"
depends_on = []

[[modules]]
path = "service"
layer = "service"
depends_on = []
""")

    # Create module directories
    api_dir = project_root / "api"
    service_dir = project_root / "service"

    for dir_path in [api_dir, service_dir]:
        dir_path.mkdir()
        (dir_path / "__init__.py").write_text("")

    # Create a file in api that imports from service
    (api_dir / "handler.py").write_text("from service import something\n")
    (service_dir / "__init__.py").write_text("something = 1\n")

    # Parse config and run check - should pass
    project_config = parse_project_config(root=project_root)
    assert project_config is not None
    assert project_config.layers_explicit_depends_on is False

    with pytest.raises(SystemExit) as exc_info:
        tach_check(
            project_root=project_root,
            project_config=project_config,
        )

    # Should exit successfully (implicit layer dependencies allowed)
    assert exc_info.value.code == 0
    captured = capfd.readouterr()
    assert SUCCESS in captured.err
