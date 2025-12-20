from __future__ import annotations

from pathlib import Path

import pytest


@pytest.fixture
def example_dir() -> Path:
    current_dir = Path(__file__).parent
    return current_dir / "example"


@pytest.fixture(autouse=True, scope="function")
def no_color(monkeypatch: pytest.MonkeyPatch) -> None:
    # According to https://bixense.com/clicolors/, NO_COLOR=1 should be enough.
    # "console", however, does not respect "NO_COLOR" as of this writing,
    # and requires CLICOLOR_FORCE being unset.
    monkeypatch.setenv("NO_COLOR", "1")
    monkeypatch.delenv("CLICOLOR_FORCE", raising=False)
