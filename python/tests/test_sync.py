from __future__ import annotations

import shutil
import tempfile
from pathlib import Path

import pytest

from tach.cli import tach_sync
from tach.icons import SUCCESS
from tach.parsing.config import parse_project_config


def test_valid_example_dir(example_dir, capfd):
    project_root = example_dir / "valid"

    with tempfile.TemporaryDirectory() as temp_dir:
        temp_project_root = Path(temp_dir) / "valid"
        shutil.copytree(project_root, temp_project_root)

        project_config = parse_project_config(root=temp_project_root)
        assert project_config is not None

        with pytest.raises(SystemExit) as exc_info:
            tach_sync(
                project_root=temp_project_root,
                project_config=project_config,
                add=True,
            )

        assert exc_info.value.code == 0
        captured = capfd.readouterr()
        assert "✅" in captured.err  # success state


def test_distributed_config_dir(example_dir, capfd):
    project_root = example_dir / "distributed_config"

    with tempfile.TemporaryDirectory() as temp_dir:
        temp_project_root = Path(temp_dir) / "distributed_config"
        shutil.copytree(project_root, temp_project_root)

        project_config = parse_project_config(root=temp_project_root)
        assert project_config is not None

        modules = project_config.all_modules()
        assert len(modules) == 3

        top_level_module = next(
            module for module in modules if module.path == "project.top_level"
        )
        assert set(map(lambda dep: dep.path, top_level_module.depends_on)) == {
            "project.module_two"
        }

        with pytest.raises(SystemExit) as exc_info:
            tach_sync(
                project_root=temp_project_root,
                project_config=project_config,
            )

        assert exc_info.value.code == 0
        captured = capfd.readouterr()
        assert "✅" in captured.err  # success state

        project_config = parse_project_config(root=temp_project_root)
        assert project_config is not None

        modules = project_config.all_modules()
        assert len(modules) == 3

        top_level_module = next(
            module for module in modules if module.path == "project.top_level"
        )
        assert set(map(lambda dep: dep.path, top_level_module.depends_on)) == {
            "project.module_two",
            "project.module_one",
        }


def test_many_features_example_dir(example_dir, capfd):
    project_root = example_dir / "many_features"

    with tempfile.TemporaryDirectory() as temp_dir:
        temp_project_root = Path(temp_dir) / "many_features"
        shutil.copytree(project_root, temp_project_root)

        project_config = parse_project_config(root=temp_project_root)
        assert project_config is not None

        with pytest.raises(SystemExit) as exc_info:
            tach_sync(
                project_root=temp_project_root,
                project_config=project_config,
            )

        assert exc_info.value.code == 0
        captured = capfd.readouterr()
        assert "✅" in captured.err  # success state

        project_config = parse_project_config(root=temp_project_root)
        assert project_config is not None

        modules = project_config.all_modules()
        # This should be the number of statically defined modules, before globbing
        assert len(modules) == 17

        module2 = next(module for module in modules if module.path == "module2")
        assert set(map(lambda dep: dep.path, module2.depends_on)) == {"outer_module"}

        module3 = next(module for module in modules if module.path == "module3")
        assert set(map(lambda dep: dep.path, module3.depends_on)) == {"module1"}

        assert (
            '"//module1"'
            in (
                temp_project_root / "real_src" / "module3" / "tach.domain.toml"
            ).read_text()
        )

        assert (
            '"<domain_root>"'
            in (
                temp_project_root / "real_src" / "module3" / "tach.domain.toml"
            ).read_text()
        )


def test_layers_explicit_depends_on_sync(example_dir, capfd):
    """Test that tach sync populates depends_on and preserves layer config when layers_explicit_depends_on=true."""
    project_root = example_dir / "layers_explicit_depends_on"

    with tempfile.TemporaryDirectory() as temp_dir:
        temp_project_root = Path(temp_dir) / "layers_explicit_depends_on"
        shutil.copytree(project_root, temp_project_root)

        # Parse config before sync
        project_config = parse_project_config(root=temp_project_root)
        assert project_config is not None
        assert project_config.layers_explicit_depends_on is True

        # Verify initial state - api has layer but empty depends_on
        modules = project_config.all_modules()
        api_module = next(m for m in modules if m.path == "api")
        service_module = next(m for m in modules if m.path == "service")

        assert api_module.layer == "presentation"
        assert api_module.depends_on == []  # Empty before sync
        assert service_module.layer == "business"

        # Run sync
        with pytest.raises(SystemExit) as exc_info:
            tach_sync(
                project_root=temp_project_root,
                project_config=project_config,
                add=False,
            )

        # Should exit successfully
        assert exc_info.value.code == 0

        captured = capfd.readouterr()
        assert SUCCESS in captured.err

        # Parse config after sync
        project_config = parse_project_config(root=temp_project_root)
        modules = project_config.all_modules()

        # Verify sync populated depends_on
        api_module = next(m for m in modules if m.path == "api")
        service_module = next(m for m in modules if m.path == "service")

        # Sync should have detected the import from api to service
        assert set(map(lambda dep: dep.path, api_module.depends_on)) == {"service"}

        # CRITICAL: Layer configuration should be preserved
        assert api_module.layer == "presentation"
        assert service_module.layer == "business"

        # Verify the config file was written correctly
        config_file = temp_project_root / "tach.toml"
        config_content = config_file.read_text()

        # Verify layer assignments are still in the file
        assert 'layer = "presentation"' in config_content
        assert 'layer = "business"' in config_content

        # Verify depends_on was added
        assert 'depends_on = ["service"]' in config_content or "depends_on = ['service']" in config_content
