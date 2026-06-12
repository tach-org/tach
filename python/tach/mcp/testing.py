from __future__ import annotations

from contextlib import redirect_stderr, redirect_stdout
from io import StringIO
from typing import Any

from tach.mcp.payloads import DEFAULT_MAX_BYTES, tail_text
from tach.mcp.project import project_config, resolve_project_root
from tach.mcp.server import mcp
from tach.test import run_affected_tests


@mcp.tool()
def tach_test(
    project_root: str | None = None,
    base: str = "main",
    head: str = "",
    pytest_args: list[str] | str | None = None,
    max_bytes: int = DEFAULT_MAX_BYTES,
) -> dict[str, Any]:
    """Run pytest with Tach affected-test filtering and compact output tails."""
    root = resolve_project_root(project_root)
    config = project_config(root)
    normalized_pytest_args = (
        pytest_args.split() if isinstance(pytest_args, str) else pytest_args or []
    )
    stdout_buffer = StringIO()
    stderr_buffer = StringIO()
    with redirect_stdout(stdout_buffer), redirect_stderr(stderr_buffer):
        result = run_affected_tests(
            project_root=root,
            project_config=config,
            head=head,
            base=base,
            pytest_args=normalized_pytest_args,
        )
    stdout = result.stdout or stdout_buffer.getvalue()
    stderr = result.stderr or stderr_buffer.getvalue()
    stdout_tail = tail_text(stdout, max_bytes=max_bytes)
    stderr_tail = tail_text(stderr, max_bytes=max_bytes)
    return {
        "ok": result.exit_code == 0,
        "exit_code": result.exit_code,
        "tests_ran_to_completion": result.tests_ran_to_completion,
        "stdout_tail": stdout_tail["text"],
        "stdout_bytes": stdout_tail["bytes"],
        "stdout_truncated": stdout_tail["truncated"],
        "stderr_tail": stderr_tail["text"],
        "stderr_bytes": stderr_tail["bytes"],
        "stderr_truncated": stderr_tail["truncated"],
    }
