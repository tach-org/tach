from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, cast
from unittest.mock import Mock

import pytest

from tach import cli
from tach.extension import ProjectConfig
from tach.parsing import dump_project_config_to_toml, parse_project_config

if TYPE_CHECKING:
    from _pytest.capture import CaptureFixture
    from pytest_mock import MockerFixture


class FakeDiagnostic:
    _is_error: bool

    def __init__(self, is_error: bool) -> None:
        self._is_error = is_error

    def is_error(self) -> bool:
        return self._is_error


@pytest.fixture
def mock_project_config(mocker: MockerFixture) -> ProjectConfig:
    config = ProjectConfig()
    _ = mocker.patch("tach.cli.parse_project_config", return_value=config)
    return config


@pytest.fixture
def mock_check_deadcode(mocker: MockerFixture) -> Mock:
    mock = Mock(return_value=[])
    _ = mocker.patch("tach.extension.check_deadcode", mock)
    return mock


def _mock_kwargs(mock: Mock) -> dict[str, object]:
    call_args = mock.call_args
    assert call_args is not None
    return cast("dict[str, object]", call_args.kwargs)


def test_parse_deadcode_defaults() -> None:
    args, _ = cli.parse_arguments(["deadcode"])

    assert cast("str", args.command) == "deadcode"
    assert cast("list[str]", args.entry_point) == []
    assert cast("bool", args.files) is False
    assert cast("bool", args.symbols) is False
    assert cast("bool", args.all) is False
    assert cast("str", args.output) == "text"


def test_parse_deadcode_options() -> None:
    args, _ = cli.parse_arguments(
        ["deadcode", "--entry-point", "app.py", "--symbols", "--output", "json"]
    )

    assert cast("str", args.command) == "deadcode"
    assert cast("list[str]", args.entry_point) == ["app.py"]
    assert cast("bool", args.files) is False
    assert cast("bool", args.symbols) is True
    assert cast("bool", args.all) is False
    assert cast("str", args.output) == "json"


def test_main_deadcode_calls_extension(
    mocker: MockerFixture,
    mock_project_config: ProjectConfig,
    mock_check_deadcode: Mock,
) -> None:
    _ = mocker.patch("tach.cli.cache.get_latest_version", return_value=None)

    with pytest.raises(SystemExit) as sys_exit:
        cli.main(["deadcode", "--entry-point", "app.py", "--files"])

    assert sys_exit.value.code == 0
    mock_check_deadcode.assert_called_once()
    kwargs = _mock_kwargs(mock_check_deadcode)
    assert kwargs["project_config"] is mock_project_config
    assert kwargs["entry_points"] == ["app.py"]
    assert kwargs["files"] is True
    assert kwargs["symbols"] is False


@pytest.mark.parametrize(("is_error", "expected_code"), [(False, 0), (True, 1)])
def test_deadcode_json_output_uses_serializer_and_error_exit(
    capfd: CaptureFixture[str],
    mocker: MockerFixture,
    is_error: bool,
    expected_code: int,
) -> None:
    diagnostics = [FakeDiagnostic(is_error)]
    _ = mocker.patch("tach.extension.check_deadcode", return_value=diagnostics)
    serialize: Mock = mocker.patch(
        "tach.extension.serialize_diagnostics_json", return_value='[{"kind":"dead"}]'
    )

    with pytest.raises(SystemExit) as sys_exit:
        cli.tach_deadcode(
            project_config=ProjectConfig(),
            project_root=Path(),
            entry_points=["app.py"],
            files=True,
            symbols=False,
            output_format="json",
        )

    captured = capfd.readouterr()
    assert sys_exit.value.code == expected_code
    assert captured.out.strip() == '[{"kind":"dead"}]'
    serialize.assert_called_once_with(diagnostics, pretty_print=True)


def test_deadcode_all_enables_files_and_symbols(mocker: MockerFixture) -> None:
    check_deadcode: Mock = mocker.patch(
        "tach.extension.check_deadcode", return_value=[]
    )

    with pytest.raises(SystemExit) as sys_exit:
        cli.tach_deadcode(
            project_config=ProjectConfig(),
            project_root=Path(),
            entry_points=[],
            files=False,
            symbols=False,
            all=True,
        )

    assert sys_exit.value.code == 0
    kwargs = _mock_kwargs(check_deadcode)
    assert kwargs["files"] is True
    assert kwargs["symbols"] is True


def test_deadcode_config_parses(tmp_path: Path) -> None:
    _ = tmp_path.joinpath("tach.toml").write_text(
        """
[deadcode]
entry_points = ["app.py", "pkg.cli:main"]
detect = ["files", "symbols"]
severity = "error"
exclude = ["generated"]
ignore = ["pkg.dead", "pkg.service:unused"]
public_modules = ["pkg.api"]
public_symbols = ["pkg.service:used"]
public_decorators = ["fastapi.get"]
protect_init_files = false
respect_all = false
include_test_usages = true
ignore_dynamic_modules = false
""".strip()
    )

    config = parse_project_config(tmp_path)

    assert config is not None
    assert config.deadcode.entry_points == ["app.py", "pkg.cli:main"]
    assert config.deadcode.detect == ["files", "symbols"]
    assert config.deadcode.severity == "error"
    assert config.deadcode.exclude == ["generated"]
    assert config.deadcode.ignore == ["pkg.dead", "pkg.service:unused"]
    assert config.deadcode.public_modules == ["pkg.api"]
    assert config.deadcode.public_symbols == ["pkg.service:used"]
    assert config.deadcode.public_decorators == ["fastapi.get"]
    assert config.deadcode.protect_init_files is False
    assert config.deadcode.respect_all is False
    assert config.deadcode.include_test_usages is True
    assert config.deadcode.ignore_dynamic_modules is False


def test_deadcode_config_unknown_field_fails(tmp_path: Path) -> None:
    _ = tmp_path.joinpath("tach.toml").write_text(
        """
[deadcode]
unknown = true
""".strip()
    )

    with pytest.raises(ValueError):
        _ = parse_project_config(tmp_path)


def test_deadcode_config_dump_omits_default_table() -> None:
    dumped = dump_project_config_to_toml(ProjectConfig())

    assert "[deadcode]" not in dumped
