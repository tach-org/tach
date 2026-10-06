from __future__ import annotations

from pathlib import Path

from tach import __version__
from tach.utils.toml import tomllib


def test_version_consistency():
    """Verify that the version in pyproject.toml and Cargo.toml match the CLI version."""
    # Read version from pyproject.toml
    pyproject_path = Path(__file__).parent.parent.parent / "pyproject.toml"
    with open(pyproject_path, "rb") as f:
        pyproject_data = tomllib.load(f)
        pyproject_version = pyproject_data["project"]["version"]

    # Read version from Cargo.toml
    cargo_path = Path(__file__).parent.parent.parent / "Cargo.toml"
    with open(cargo_path, "rb") as f:
        cargo_data = tomllib.load(f)
        cargo_version = cargo_data["package"]["version"]

    # Compare versions
    assert pyproject_version == __version__, (
        f"Version mismatch: pyproject.toml has {pyproject_version} but CLI reports {__version__}"
    )
    assert cargo_version == __version__, (
        f"Version mismatch: Cargo.toml has {cargo_version} but CLI reports {__version__}"
    )
