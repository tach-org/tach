from __future__ import annotations

import uuid
from unittest.mock import patch

from tach.cache.access import get_latest_version, get_uid
from tach.cache.setup import get_cache_path, resolve_cache_path


def test_resolve_cache_path(tmp_path):
    project_path = tmp_path / "project"
    project_path.mkdir(parents=True, exist_ok=True)
    version = "1.0.0"

    with patch("tach.cache.setup.__version__", version):
        result = resolve_cache_path(project_path)

    cache_path = get_cache_path(project_path)
    assert cache_path.exists()
    assert (cache_path / "tach.info").exists()
    assert (cache_path / "tach.info").read_text().strip() != ""
    assert (cache_path / ".gitignore").exists()
    assert (cache_path / ".latest-version").exists()
    assert (cache_path / ".latest-version").read_text().strip() == version
    assert result == cache_path


@patch("tach.cache.access.resolve_cache_path")
def test_get_uid(mock_resolve_cache_path, tmp_path):
    project_path = tmp_path / "project"
    tach_info_path = project_path / ".tach" / "tach.info"
    tach_info_path.parent.mkdir(parents=True, exist_ok=True)
    uid = uuid.uuid4()
    tach_info_path.write_text(str(uid))

    result = get_uid(project_path)
    assert result == uid


def test_get_latest_version(tmp_path):
    project_path = tmp_path / "project"
    cache_path = get_cache_path(project_path)
    latest_version_path = cache_path / ".latest-version"
    latest_version_path.parent.mkdir(parents=True, exist_ok=True)
    version = "1.0.0"
    latest_version_path.write_text(version)

    result = get_latest_version(project_path)
    assert result == version
