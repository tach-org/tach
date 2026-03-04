from __future__ import annotations

import os
import subprocess
import sys
import threading
from dataclasses import dataclass
from queue import Queue
from typing import IO, TYPE_CHECKING, Any

from tach.errors import TachSetupError

if TYPE_CHECKING:
    from pathlib import Path

    from tach.extension import ProjectConfig


def run_and_capture(cmd: list[str], **kwargs: Any) -> tuple[int, str, str]:
    stdout_queue: Queue[str] = Queue()
    stderr_queue: Queue[str] = Queue()

    def tee_output(pipe: IO[str], queue: Queue[str], terminal: Any):
        for line in pipe:
            queue.put_nowait(line)
            print(line, end="", file=terminal, flush=True)
        pipe.close()

    process_env = os.environ.copy()
    process_env["PYTEST_ADDOPTS"] = "--color=yes"
    process = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
        env=process_env,
        **kwargs,
    )

    stdout_thread = threading.Thread(
        target=tee_output, args=(process.stdout, stdout_queue, sys.stdout)
    )
    stderr_thread = threading.Thread(
        target=tee_output, args=(process.stderr, stderr_queue, sys.stderr)
    )

    stdout_thread.start()
    stderr_thread.start()

    returncode = process.wait()
    stdout_thread.join()
    stderr_thread.join()

    stdout_content = "".join(stdout_queue.queue)
    stderr_content = "".join(stderr_queue.queue)

    return returncode, stdout_content, stderr_content


@dataclass
class AffectedTestsResult:
    exit_code: int
    tests_ran_to_completion: bool
    stdout: str
    stderr: str


def run_affected_tests(
    project_root: Path,
    project_config: ProjectConfig,
    head: str,
    base: str,
    pytest_args: list[Any] | None = None,
) -> AffectedTestsResult:
    try:
        import pytest  # type: ignore  # noqa: F401
    except ImportError:
        raise TachSetupError("Cannot run tests, could not find 'pytest'.")

    cmd = ["pytest", "-p", "tach.pytest_plugin"]
    if pytest_args:
        cmd.extend(pytest_args)
    if base:
        cmd.extend(["--tach-base", base])
    if head:
        cmd.extend(["--tach-head", head])

    returncode, stdout, stderr = run_and_capture(cmd, cwd=project_root)
    tests_ran = returncode != pytest.ExitCode.NO_TESTS_COLLECTED
    exit_code = (
        pytest.ExitCode.OK
        if returncode == pytest.ExitCode.NO_TESTS_COLLECTED
        else returncode
    )

    return AffectedTestsResult(
        exit_code=exit_code,
        tests_ran_to_completion=tests_ran,
        stdout=stdout,
        stderr=stderr,
    )


__all__ = ["run_affected_tests"]
