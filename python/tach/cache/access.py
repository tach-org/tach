from __future__ import annotations

import uuid
from typing import TYPE_CHECKING

from tach.cache.setup import get_cache_path, resolve_cache_path

if TYPE_CHECKING:
    from pathlib import Path


def get_uid(project_root: Path) -> uuid.UUID | None:
    info_path = get_cache_path(project_root) / "tach.info"
    if not info_path.exists():
        resolve_cache_path(project_root)
    contents = info_path.read_text().strip()
    uid = uuid.UUID(contents)
    return uid


def get_latest_version(project_root: Path) -> str | None:
    latest_version_path = get_cache_path(project_root) / ".latest-version"
    if not latest_version_path.exists():
        return
    version = latest_version_path.read_text().strip()
    return version
