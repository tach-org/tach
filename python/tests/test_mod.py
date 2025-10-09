from __future__ import annotations

from pathlib import Path
from unittest.mock import patch

import pytest

from tach.extension import ProjectConfig
from tach.interactive import InteractiveModuleConfiguration
from tach.mod import mod_edit_interactive
from tach.parsing import parse_project_config


@pytest.fixture
def temp_project_dir(tmp_path):
    """Create a temporary project directory with initial structure."""
    project_root = tmp_path / "test_project"
    project_root.mkdir()
    return project_root


@pytest.fixture
def initial_project_config():
    """Create a clean project config."""
    return ProjectConfig()


def test_mod_edit_interactive_new_configuration(
    temp_project_dir, initial_project_config
):
    mock_config = InteractiveModuleConfiguration(
        source_roots=[temp_project_dir / "src"],
        module_paths=[
            temp_project_dir / "src" / "module1.py",
            temp_project_dir / "src" / "module2.py",
        ],
        utility_paths=[temp_project_dir / "src" / "utils.py"],
    )

    with patch(
        "tach.mod.get_selected_modules_interactive", return_value=mock_config
    ) as mock_get:
        success, errors = mod_edit_interactive(
            project_root=temp_project_dir,
            project_config=initial_project_config,
            exclude_paths=[],
        )

        assert success is True
        assert not errors
        mock_get.assert_called_once()

        # Verify the saved configuration
        saved_config = parse_project_config(temp_project_dir)
        assert set(saved_config.source_roots) == {Path("src")}
        assert set(saved_config.module_paths()) == {"module1", "module2", "utils"}
        assert set(saved_config.utility_paths()) == {"utils"}


def test_mod_edit_interactive_validation_error(
    temp_project_dir, initial_project_config
):
    # Create a configuration where a module is outside the source roots
    mock_config = InteractiveModuleConfiguration(
        source_roots=[temp_project_dir / "src"],
        module_paths=[temp_project_dir / "outside" / "module1.py"],  # Outside src
        utility_paths=[],
    )

    with patch(
        "tach.mod.get_selected_modules_interactive", return_value=mock_config
    ) as mock_get:
        success, errors = mod_edit_interactive(
            project_root=temp_project_dir,
            project_config=initial_project_config,
            exclude_paths=[],
        )

        assert success is False
        assert len(errors) == 1
        assert "not contained within any source root" in errors[0]
        mock_get.assert_called_once()


def test_mod_edit_interactive_user_cancelled(temp_project_dir, initial_project_config):
    with patch(
        "tach.mod.get_selected_modules_interactive", return_value=None
    ) as mock_get:
        success, errors = mod_edit_interactive(
            project_root=temp_project_dir,
            project_config=initial_project_config,
            exclude_paths=[],
        )

        assert success is False
        assert len(errors) == 1
        assert "No changes saved" in errors[0]
        mock_get.assert_called_once()


def test_mod_edit_interactive_update_existing(temp_project_dir, initial_project_config):
    # First round of edits
    mock_config = InteractiveModuleConfiguration(
        source_roots=[temp_project_dir / "src", temp_project_dir / "tests"],
        module_paths=[
            temp_project_dir / "src" / "new_module.py",
        ],
        utility_paths=[
            temp_project_dir / "src" / "new_utility.py",
        ],
    )

    with patch(
        "tach.mod.get_selected_modules_interactive", return_value=mock_config
    ) as mock_get:
        success, errors = mod_edit_interactive(
            project_root=temp_project_dir,
            project_config=initial_project_config,
            exclude_paths=[],
        )

        assert success is True
        assert not errors
        mock_get.assert_called_once()

        saved_config = parse_project_config(temp_project_dir)
        assert set(saved_config.source_roots) == {Path("src"), Path("tests")}
        assert set(saved_config.module_paths()) == {"new_module", "new_utility"}
        assert set(saved_config.utility_paths()) == {"new_utility"}

    # Second round of edits
    mock_config_2 = InteractiveModuleConfiguration(
        source_roots=[temp_project_dir / "src"],  # Remove tests directory
        module_paths=[
            temp_project_dir / "src" / "new_module.py",
            temp_project_dir / "src" / "another_module.py",  # Add new module
        ],
        utility_paths=[],  # Remove utility
    )

    with patch(
        "tach.mod.get_selected_modules_interactive", return_value=mock_config_2
    ) as mock_get:
        success, errors = mod_edit_interactive(
            project_root=temp_project_dir,
            project_config=saved_config,  # Use previously saved config
            exclude_paths=[],
        )

        assert success is True
        assert not errors
        mock_get.assert_called_once()

        # Verify final configuration
        final_config = parse_project_config(temp_project_dir)
        assert set(final_config.source_roots) == {Path("src")}
        assert set(final_config.module_paths()) == {"new_module", "another_module"}
        assert set(final_config.utility_paths()) == set()
