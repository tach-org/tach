from __future__ import annotations

import sys

if sys.version_info >= (3, 11):
    # ruff - intentional explicit re-export
    # pyright - https://github.com/DetachHead/basedpyright/issues/8
    import tomllib as tomllib  # pyright: ignore[reportUnreachable]  # noqa: PLC0414
else:
    import tomli

    tomllib = tomli
