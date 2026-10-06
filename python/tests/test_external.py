from __future__ import annotations

from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

from tach.utils.external import get_module_mappings, get_package_name

if TYPE_CHECKING:
    import pytest


class _Distribution:
    metadata: dict[str, str]
    files: list[PurePosixPath] | None
    _located: Path

    def __init__(
        self, name: str, files: list[PurePosixPath] | None, located: Path
    ) -> None:
        self.metadata = {"Name": name}
        self.files = files
        self._located = located

    def locate_file(self, _path: object) -> Path:
        return self._located


def _patch_metadata(
    monkeypatch: pytest.MonkeyPatch,
    packages: dict[str, list[str]],
    dists: list[_Distribution],
) -> None:
    get_module_mappings.cache_clear()
    monkeypatch.setattr("importlib.metadata.packages_distributions", lambda: packages)
    monkeypatch.setattr("importlib.metadata.distributions", lambda: dists)


def test_editable_namespace_without_top_level_txt(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    src = tmp_path / "src"
    (src / "google" / "foo").mkdir(parents=True)
    (src / "google" / "foo" / "__init__.py").touch()
    pth = tmp_path / "google_foo.pth"
    _ = pth.write_text(f"{src}\n")
    dist = _Distribution("google-foo", [PurePosixPath("google_foo.pth")], pth)
    packages = {"google": ["google-cloud-storage"]}
    _patch_metadata(monkeypatch, packages, [dist])
    try:
        mappings = get_module_mappings()
        assert packages["google"] == ["google-cloud-storage"]
        assert mappings["google"] == [
            "google-cloud-storage",
            "google-foo",
        ]
        assert get_package_name("google.foo") == "google-cloud-storage"
    finally:
        get_module_mappings.cache_clear()


def test_editable_finder_is_not_executed(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    pth = tmp_path / "google_foo.pth"
    _ = pth.write_text(
        "import __editable___google_foo_finder; "
        "__editable___google_foo_finder.install()\n"
    )
    finder = tmp_path / "__editable___google_foo_finder.py"
    _ = finder.write_text(
        'raise RuntimeError("finder must not be executed")\n'
        'NAMESPACES = {"google": ["/tmp/src/google"]}\n'
        'MAPPING = {"google.foo": "/tmp/src/google/foo"}\n'
    )
    dist = _Distribution(
        "google-foo",
        [PurePosixPath("google_foo.pth")],
        pth,
    )
    _patch_metadata(
        monkeypatch,
        {"google": ["google-cloud-storage"]},
        [dist],
    )
    try:
        assert get_module_mappings()["google"] == [
            "google-cloud-storage",
            "google-foo",
        ]
    finally:
        get_module_mappings.cache_clear()


def test_editable_pth_ignores_non_modules(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    src = tmp_path / "src"
    (src / "alpha").mkdir(parents=True)
    (src / "alpha" / "__init__.py").touch()
    (src / "beta.py").touch()
    (src / "__init__.py").touch()
    (src / "notes").mkdir()
    (src / "notes" / "readme.txt").touch()
    not_a_directory = tmp_path / "not-a-directory"
    not_a_directory.touch()
    pth = tmp_path / "app.pth"
    _ = pth.write_text(
        f"\n# comment\n{tmp_path / 'missing-src'}\n{not_a_directory}\n{src}\n"
    )
    dist = _Distribution("my-app", [PurePosixPath("app.pth")], pth)
    _patch_metadata(monkeypatch, {}, [dist])
    try:
        assert get_module_mappings() == {
            "alpha": ["my-app"],
            "beta": ["my-app"],
        }
    finally:
        get_module_mappings.cache_clear()


def test_distribution_already_mapped_is_not_duplicated(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    src = tmp_path / "src"
    (src / "otherpkg").mkdir(parents=True)
    (src / "otherpkg" / "__init__.py").touch()
    pth = tmp_path / "google_foo.pth"
    _ = pth.write_text(f"{src}\n")
    dist = _Distribution("google-foo", [PurePosixPath("google_foo.pth")], pth)
    _patch_metadata(monkeypatch, {"google": ["google-foo"]}, [dist])
    try:
        assert get_module_mappings() == {"google": ["google-foo"]}
    finally:
        get_module_mappings.cache_clear()


def test_missing_files_leave_packages_distributions(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    dist = _Distribution("google-foo", None, Path("missing.pth"))
    _patch_metadata(
        monkeypatch,
        {"google": ["google-cloud-storage"]},
        [dist],
    )
    try:
        assert get_module_mappings() == {"google": ["google-cloud-storage"]}
    finally:
        get_module_mappings.cache_clear()


def test_missing_pth_leaves_packages_distributions(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    dist = _Distribution(
        "google-foo",
        [PurePosixPath("google_foo.pth")],
        tmp_path / "missing.pth",
    )
    _patch_metadata(
        monkeypatch,
        {"google": ["google-cloud-storage"]},
        [dist],
    )
    try:
        assert get_module_mappings() == {"google": ["google-cloud-storage"]}
    finally:
        get_module_mappings.cache_clear()


def test_unreadable_pth_leaves_packages_distributions(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    src = tmp_path / "src"
    (src / "google" / "foo").mkdir(parents=True)
    (src / "google" / "foo" / "__init__.py").touch()
    pth = tmp_path / "google_foo.pth"
    _ = pth.write_text(f"{src}\n")
    pth.chmod(0)
    dist = _Distribution("google-foo", [PurePosixPath("google_foo.pth")], pth)
    _patch_metadata(
        monkeypatch,
        {"google": ["google-cloud-storage"]},
        [dist],
    )
    try:
        assert get_module_mappings() == {"google": ["google-cloud-storage"]}
    finally:
        pth.chmod(0o644)
        get_module_mappings.cache_clear()


def test_invalid_finder_leaves_packages_distributions(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    pth = tmp_path / "google_foo.pth"
    _ = pth.write_text(
        "import __editable___google_foo_finder; "
        "__editable___google_foo_finder.install()\n"
    )
    _ = (tmp_path / "__editable___google_foo_finder.py").write_text("def (\n")
    dist = _Distribution("google-foo", [PurePosixPath("google_foo.pth")], pth)
    _patch_metadata(
        monkeypatch,
        {"google": ["google-cloud-storage"]},
        [dist],
    )
    try:
        assert get_module_mappings() == {"google": ["google-cloud-storage"]}
    finally:
        get_module_mappings.cache_clear()
