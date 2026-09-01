from __future__ import (  # noqa: EXE002 TODO: i dont think this file needs to be executable
    annotations,
)

from tach.constants import TOOL_NAME

template = """#!/bin/sh
# Pre-commit script that validates dependencies locally
set -e

{command}"""


def build_pre_commit_hook_content() -> str:
    return template.format(command=f"{TOOL_NAME} check")
