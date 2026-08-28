from __future__ import annotations

import json
from pathlib import Path
from typing import TYPE_CHECKING, cast
from unittest.mock import Mock

import pytest

from tach import cli, extension
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


def _deadcode_diagnostics_for(
    project_root: Path,
    *,
    entry_points: list[str] | None = None,
    files: bool = True,
    symbols: bool = False,
):
    project_config = parse_project_config(project_root)
    assert project_config is not None
    return extension.check_deadcode(
        project_root=project_root,
        project_config=project_config,
        entry_points=entry_points or [],
        files=files,
        symbols=symbols,
    )


JsonObject = dict[str, object]


def _diagnostic_payload(diagnostics: list[extension.Diagnostic]) -> list[JsonObject]:
    payload = cast(
        "object",
        json.loads(
            extension.serialize_diagnostics_json(diagnostics, pretty_print=False)
        ),
    )
    assert isinstance(payload, list)
    items = cast("list[object]", payload)
    return [cast("JsonObject", item) for item in items if isinstance(item, dict)]


def _json_object(value: object) -> JsonObject | None:
    if isinstance(value, dict):
        return cast("JsonObject", value)
    return None


def _deadcode_detail(
    diagnostic: JsonObject,
    kind: str,
) -> JsonObject | None:
    located = _json_object(diagnostic.get("Located"))
    if located is None:
        return None
    details = _json_object(located.get("details"))
    if details is None:
        return None
    code = _json_object(details.get("Code"))
    if code is None:
        return None
    return _json_object(code.get(kind))


def _dead_file_modules(diagnostics: list[extension.Diagnostic]) -> set[str]:
    modules: set[str] = set()
    for diagnostic in _diagnostic_payload(diagnostics):
        dead_file = _deadcode_detail(diagnostic, "DeadFile")
        if dead_file is None:
            continue
        module_path = dead_file.get("module_path")
        if isinstance(module_path, str):
            modules.add(module_path)
    return modules


def _dead_symbols(diagnostics: list[extension.Diagnostic]) -> set[tuple[str, str, str]]:
    symbols: set[tuple[str, str, str]] = set()
    for diagnostic in _diagnostic_payload(diagnostics):
        dead_symbol = _deadcode_detail(diagnostic, "DeadSymbol")
        if dead_symbol is None:
            continue
        module_path = dead_symbol.get("module_path")
        symbol_name = dead_symbol.get("symbol_name")
        symbol_kind = dead_symbol.get("symbol_kind")
        if (
            isinstance(module_path, str)
            and isinstance(symbol_name, str)
            and isinstance(symbol_kind, str)
        ):
            symbols.add((module_path, symbol_name, symbol_kind))
    return symbols


def test_deadcode_phase1_reports_unreachable_file(example_dir: Path) -> None:
    diagnostics = _deadcode_diagnostics_for(example_dir / "deadcode_phase1")

    assert _dead_file_modules(diagnostics) == {"pkg.dead"}
    assert all(diagnostic.is_warning() for diagnostic in diagnostics)
    assert any(
        (path := diagnostic.pyfile_path()) is not None and path.endswith("pkg/dead.py")
        for diagnostic in diagnostics
        if diagnostic.is_deadcode_error()
    )


def test_deadcode_phase1_entry_point_override_marks_file_reachable(
    example_dir: Path,
) -> None:
    diagnostics = _deadcode_diagnostics_for(
        example_dir / "deadcode_phase1",
        entry_points=["pkg.dead"],
    )

    assert "pkg.dead" not in _dead_file_modules(diagnostics)


def test_deadcode_phase1_module_symbol_entry_point_marks_file_reachable(
    example_dir: Path,
) -> None:
    diagnostics = _deadcode_diagnostics_for(
        example_dir / "deadcode_phase1",
        entry_points=["pkg.dead:unused"],
    )

    assert "pkg.dead" not in _dead_file_modules(diagnostics)


def test_deadcode_phase1_duplicate_entry_points_do_not_warn(
    example_dir: Path,
) -> None:
    diagnostics = _deadcode_diagnostics_for(
        example_dir / "deadcode_phase1",
        entry_points=["app.py"],
    )

    assert not any(
        "Deadcode entry point 'app.py' was not found" in diagnostic.to_string()
        for diagnostic in diagnostics
    )


def test_deadcode_phase1_json_includes_dead_file(example_dir: Path) -> None:
    diagnostics = _deadcode_diagnostics_for(example_dir / "deadcode_phase1")

    payload = extension.serialize_diagnostics_json(diagnostics, pretty_print=False)

    assert '"DeadFile":{"module_path":"pkg.dead"}' in payload


def test_deadcode_phase1_unresolved_entry_point_warns_and_continues(
    example_dir: Path,
) -> None:
    diagnostics = _deadcode_diagnostics_for(
        example_dir / "deadcode_phase1",
        entry_points=["missing.py"],
    )

    assert _dead_file_modules(diagnostics) == {"pkg.dead"}
    assert any("missing.py" in diagnostic.to_string() for diagnostic in diagnostics)


def test_deadcode_phase1_no_entry_points_warns_without_dead_files(
    example_dir: Path,
) -> None:
    diagnostics = _deadcode_diagnostics_for(example_dir / "deadcode_phase1_no_entry")

    assert _dead_file_modules(diagnostics) == set()
    assert any(
        "No deadcode entry points resolved" in d.to_string() for d in diagnostics
    )


def test_deadcode_phase1_ignore_suppresses_file(example_dir: Path) -> None:
    diagnostics = _deadcode_diagnostics_for(example_dir / "deadcode_phase1_ignore")

    assert _dead_file_modules(diagnostics) == set()


def test_deadcode_phase1_imported_package_init_is_reachable(
    example_dir: Path,
) -> None:
    diagnostics = _deadcode_diagnostics_for(example_dir / "deadcode_phase1_init")

    assert "pkg" not in _dead_file_modules(diagnostics)
    assert _dead_file_modules(diagnostics) == {"pkg.dead"}


def test_deadcode_phase1_module_entry_point_marks_parent_init_reachable(
    example_dir: Path,
) -> None:
    diagnostics = _deadcode_diagnostics_for(
        example_dir / "deadcode_phase1_module_entry_init"
    )

    assert "pkg" not in _dead_file_modules(diagnostics)
    assert _dead_file_modules(diagnostics) == {"pkg.dead"}


def test_deadcode_phase1_source_root_relative_ignore_suppresses_file(
    example_dir: Path,
) -> None:
    diagnostics = _deadcode_diagnostics_for(example_dir / "deadcode_phase1_src_ignore")

    assert _dead_file_modules(diagnostics) == set()


def test_deadcode_phase1_syntax_error_is_skipped(example_dir: Path) -> None:
    diagnostics = _deadcode_diagnostics_for(example_dir / "deadcode_phase1_syntax")

    assert _dead_file_modules(diagnostics) == set()
    assert any("syntax error" in diagnostic.to_string() for diagnostic in diagnostics)


def test_deadcode_phase2_reports_only_dead_reachable_top_level_symbols(
    example_dir: Path,
) -> None:
    diagnostics = _deadcode_diagnostics_for(
        example_dir / "deadcode_phase2",
        files=False,
        symbols=True,
    )

    assert _dead_symbols(diagnostics) == {
        ("pkg.api", "UnusedClass", "class"),
        ("pkg.api", "unused_function", "function"),
        ("pkg.public_api", "_private_dead", "function"),
        ("pkg.service", "UNUSED_VALUE", "variable"),
        ("pkg.service", "unused_service", "function"),
    }


def test_deadcode_phase2_respects_public_and_dynamic_symbol_seeds(
    example_dir: Path,
) -> None:
    diagnostics = _deadcode_diagnostics_for(
        example_dir / "deadcode_phase2",
        files=False,
        symbols=True,
    )

    dead_symbol_names = {
        symbol_name for _, symbol_name, _ in _dead_symbols(diagnostics)
    }
    assert "configured_public" not in dead_symbol_names
    assert "decorated_endpoint" not in dead_symbol_names
    assert "exported_by_all" not in dead_symbol_names
    assert "public_func" not in dead_symbol_names
    assert "PublicClass" not in dead_symbol_names
    assert "dynamic_dead" not in dead_symbol_names
    assert "nested_unused" not in dead_symbol_names
    assert "method" not in dead_symbol_names


def test_deadcode_phase2_all_reports_dead_file_without_nested_dead_symbols(
    example_dir: Path,
) -> None:
    diagnostics = _deadcode_diagnostics_for(
        example_dir / "deadcode_phase2",
        files=True,
        symbols=True,
    )

    assert "pkg.unused_file" in _dead_file_modules(diagnostics)
    assert all(
        symbol_name != "unused_in_dead_file"
        for _, symbol_name, _ in _dead_symbols(diagnostics)
    )
