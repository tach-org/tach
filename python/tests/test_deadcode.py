from __future__ import annotations

import json
from typing import TYPE_CHECKING

import pytest

from tach.cli import build_parser, main, tach_deadcode
from tach.extension import (
    check_deadcode,
    dump_project_config_to_toml,
)
from tach.icons import SUCCESS
from tach.parsing.config import parse_project_config

if TYPE_CHECKING:
    from pathlib import Path

    from tach.extension import Diagnostic, ProjectConfig


def _unreachable_files(diagnostics: list[Diagnostic]) -> set[str]:
    return {
        diagnostic.pyfile_path() or ""
        for diagnostic in diagnostics
        if diagnostic.is_deadcode()
    }


def _project_config(example_dir: Path, name: str) -> tuple[Path, ProjectConfig]:
    project_root = example_dir / name
    project_config = parse_project_config(root=project_root)
    assert project_config is not None
    return project_root, project_config


def test_deadcode_reports_expected_files_with_relative_paths(example_dir):
    project_root, project_config = _project_config(example_dir, "deadcode")

    diagnostics = check_deadcode(
        project_root=project_root,
        project_config=project_config,
    )

    # Exact set: proves detection ran, the TYPE_CHECKING-only module stayed
    # alive, the ignore glob and the exclude were selective, and paths are
    # project-root-relative.
    assert _unreachable_files(diagnostics) == {"pkg/dead.py", "orphan.py"}


def test_deadcode_type_checking_only_module_is_not_reported(example_dir):
    project_root, project_config = _project_config(example_dir, "deadcode")

    diagnostics = check_deadcode(
        project_root=project_root,
        project_config=project_config,
    )

    assert "pkg/models.py" not in _unreachable_files(diagnostics)


def test_deadcode_cli_entry_points_extend_config(example_dir):
    project_root, project_config = _project_config(example_dir, "deadcode")

    diagnostics = check_deadcode(
        project_root=project_root,
        project_config=project_config,
        entry_points=["pkg.dead", "orphan.py"],
    )

    assert _unreachable_files(diagnostics) == set()


def test_deadcode_module_entry_point_in_src_layout(example_dir):
    project_root, project_config = _project_config(example_dir, "deadcode_src")

    diagnostics = check_deadcode(
        project_root=project_root,
        project_config=project_config,
    )

    assert _unreachable_files(diagnostics) == {"src/myapp/unused.py"}


def test_deadcode_severity_error_makes_findings_errors(example_dir):
    project_root, project_config = _project_config(example_dir, "deadcode")

    diagnostics = check_deadcode(
        project_root=project_root,
        project_config=project_config,
        severity="error",
    )

    deadcode_diagnostics = [d for d in diagnostics if d.is_deadcode()]
    assert deadcode_diagnostics
    assert all(d.is_error() for d in deadcode_diagnostics)


def test_deadcode_severity_off_reports_nothing(example_dir):
    project_root, project_config = _project_config(example_dir, "deadcode")

    diagnostics = check_deadcode(
        project_root=project_root,
        project_config=project_config,
        severity="off",
    )

    assert diagnostics == []


def test_deadcode_cli_warn_exits_zero_and_prints_group(example_dir, capfd):
    project_root, project_config = _project_config(example_dir, "deadcode")

    with pytest.raises(SystemExit) as exc_info:
        tach_deadcode(
            project_config=project_config,
            project_root=project_root,
        )

    assert exc_info.value.code == 0
    captured = capfd.readouterr()
    assert "Dead Code" in captured.err
    assert "pkg/dead.py" in captured.err


def test_deadcode_cli_severity_error_exits_one(example_dir, capfd):
    project_root, project_config = _project_config(example_dir, "deadcode")

    with pytest.raises(SystemExit) as exc_info:
        tach_deadcode(
            project_config=project_config,
            project_root=project_root,
            severity="error",
        )

    assert exc_info.value.code == 1
    captured = capfd.readouterr()
    assert "pkg/dead.py" in captured.err


def test_deadcode_cli_success_message_when_clean(example_dir, capfd):
    project_root, project_config = _project_config(example_dir, "deadcode")

    with pytest.raises(SystemExit) as exc_info:
        tach_deadcode(
            project_config=project_config,
            project_root=project_root,
            entry_points=["pkg.dead", "orphan.py"],
        )

    assert exc_info.value.code == 0
    captured = capfd.readouterr()
    assert SUCCESS in captured.err


def test_deadcode_cli_severity_off_prints_disabled_note(example_dir, capfd):
    project_root, project_config = _project_config(example_dir, "deadcode")

    with pytest.raises(SystemExit) as exc_info:
        tach_deadcode(
            project_config=project_config,
            project_root=project_root,
            severity="off",
        )

    assert exc_info.value.code == 0
    captured = capfd.readouterr()
    assert "disabled" in captured.err


def test_deadcode_json_output_is_parseable_with_relative_paths(example_dir, capfd):
    project_root, project_config = _project_config(example_dir, "deadcode")

    with pytest.raises(SystemExit) as exc_info:
        tach_deadcode(
            project_config=project_config,
            project_root=project_root,
            output_format="json",
        )

    assert exc_info.value.code == 0
    captured = capfd.readouterr()
    payload = json.loads(captured.out)
    located = [entry["Located"] for entry in payload if "Located" in entry]
    file_paths = {entry["file_path"] for entry in located}
    assert "pkg/dead.py" in file_paths
    assert not any(path.startswith("/") for path in file_paths)


def test_deadcode_config_survives_toml_round_trip(example_dir):
    _, project_config = _project_config(example_dir, "deadcode")

    dumped = dump_project_config_to_toml(project_config)

    assert 'entry_points = ["main.py"]' in dumped
    assert 'ignore = ["scratch/**"]' in dumped


def test_deadcode_config_from_pyproject_toml(example_dir):
    project_root, project_config = _project_config(example_dir, "deadcode_pyproject")

    assert project_config.deadcode.entry_points == ["main.py"]
    assert project_config.deadcode.severity == "error"

    diagnostics = check_deadcode(
        project_root=project_root,
        project_config=project_config,
    )

    assert _unreachable_files(diagnostics) == {"orphan.py"}
    assert all(d.is_error() for d in diagnostics if d.is_deadcode())


def test_deadcode_severity_off_from_config_prints_disabled_note(example_dir, capfd):
    project_root, project_config = _project_config(example_dir, "deadcode_off")

    with pytest.raises(SystemExit) as exc_info:
        tach_deadcode(
            project_config=project_config,
            project_root=project_root,
        )

    assert exc_info.value.code == 0
    captured = capfd.readouterr()
    assert "disabled" in captured.err


def test_deadcode_json_payload_shape_is_stable(example_dir, capfd):
    project_root, project_config = _project_config(example_dir, "deadcode")

    with pytest.raises(SystemExit):
        tach_deadcode(
            project_config=project_config,
            project_root=project_root,
            output_format="json",
        )

    payload = json.loads(capfd.readouterr().out)
    dead = next(
        entry["Located"]
        for entry in payload
        if "Located" in entry and entry["Located"]["file_path"] == "pkg/dead.py"
    )
    assert dead["details"] == {"Code": {"UnreachableFile": {"module_path": "pkg.dead"}}}
    assert dead["severity"] == "Warning"
    assert dead["line_number"] == 1


def test_deadcode_main_dispatch_passes_cli_flags(example_dir, monkeypatch, capfd):
    monkeypatch.chdir(example_dir / "deadcode")

    with pytest.raises(SystemExit) as exc_info:
        main(["deadcode", "--severity", "error"])

    assert exc_info.value.code == 1
    captured = capfd.readouterr()
    assert "pkg/dead.py" in captured.err


def test_deadcode_main_dispatch_honors_shared_exclude_flag(example_dir, monkeypatch, capfd):
    monkeypatch.chdir(example_dir / "deadcode")

    with pytest.raises(SystemExit) as exc_info:
        main(["deadcode", "-e", "pkg"])

    assert exc_info.value.code == 0
    captured = capfd.readouterr()
    # Excluding 'pkg' removes pkg/dead.py from the analysis, leaving only orphan.
    assert "pkg/dead.py" not in captured.err
    assert "orphan.py" in captured.err


def test_deadcode_invalid_entry_point_glob_reports_config_error(example_dir, capfd):
    project_root, project_config = _project_config(example_dir, "deadcode")

    with pytest.raises(SystemExit) as exc_info:
        tach_deadcode(
            project_config=project_config,
            project_root=project_root,
            entry_points=["bad["],
        )

    assert exc_info.value.code == 1
    captured = capfd.readouterr()
    assert "Invalid glob pattern" in captured.out


def test_deadcode_parser_accepts_documented_flags():
    parser = build_parser()

    args = parser.parse_args(
        [
            "deadcode",
            "--entry-point",
            "main.py",
            "--entry-point",
            "scripts/*.py",
            "--severity",
            "error",
            "--output",
            "json",
        ]
    )

    assert args.command == "deadcode"
    assert args.entry_point == ["main.py", "scripts/*.py"]
    assert args.severity == "error"
    assert args.output == "json"


def test_deadcode_parser_rejects_unknown_severity():
    parser = build_parser()

    with pytest.raises(SystemExit):
        parser.parse_args(["deadcode", "--severity", "loud"])
