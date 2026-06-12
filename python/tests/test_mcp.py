from __future__ import annotations

import json
from pathlib import Path

from tach import mcp as tach_mcp

EXAMPLE_ROOT = Path(__file__).parent / "example" / "valid"


def test_tach_mcp_onboard_summary():
    result = tach_mcp.tach_onboard(str(EXAMPLE_ROOT))

    assert result["mcp_protocol"] == tach_mcp.MCP_PROTOCOL_VERSION
    assert result["configured"] is True
    assert result["project"]["module_count"] == 4
    assert result["resources"]["config"].startswith("tach://project-config/")


def test_tach_mcp_onboard_unconfigured(tmp_path):
    result = tach_mcp.tach_onboard(str(tmp_path), intent="bootstrap")

    assert result["configured"] is False
    assert "tach_configure" in result["next_actions"][0]


def test_tach_mcp_configure_project(tmp_path):
    project_root = tmp_path / "project"
    src = project_root / "src"
    src.mkdir(parents=True)
    (src / "module_one.py").write_text("import module_two\n")
    (src / "module_two.py").write_text("")
    (src / "module_three.py").write_text("")

    created = tach_mcp.tach_configure(
        "create_config",
        str(project_root),
        source_roots=["src"],
        modules=["module_one", "module_two"],
        utilities=["module_two"],
        dependencies=[{"path": "module_one", "dependency": "module_two"}],
    )
    assert created["ok"] is True
    assert (project_root / "tach.toml").exists()

    edited = tach_mcp.tach_configure(
        "edit_dependency",
        str(project_root),
        path="module_one",
        dependency="module_two",
        dependency_action="remove",
    )
    assert edited["ok"] is True

    module_edited = tach_mcp.tach_configure(
        "edit_module",
        str(project_root),
        path="module_three",
        module_action="create",
        include_config=True,
    )
    assert "module_three" in module_edited["config"]["module_paths"]


def test_tach_mcp_lint_defaults_to_strong_checks():
    result = tach_mcp.tach_lint(str(EXAMPLE_ROOT))

    assert result["ok"] is True
    assert result["checks"] == ["boundaries", "external", "unused"]
    assert result["warning_count"] == 1
    assert result["boundaries"]["diagnostics"]["items"][0]["severity"] == "warning"


def test_tach_mcp_analyze_imports_and_report():
    imports = tach_mcp.tach_imports(
        path="domain_one/__init__.py",
        project_root=str(EXAMPLE_ROOT),
    )
    assert imports["imports"][0]["module_path"] == "domain_two.x"

    report = tach_mcp.tach_report(path="domain_two", project_root=str(EXAMPLE_ROOT))
    assert report["mode"] == "report"
    assert "report" in report


def test_tach_mcp_analyze_graph_and_dependency_map():
    graph = tach_mcp.tach_graph(str(EXAMPLE_ROOT))

    assert graph["graph_preview"].startswith("graph TD")
    assert "resource_uri" in graph

    dep_map = tach_mcp.tach_map(str(EXAMPLE_ROOT), limit=1)
    assert dep_map["file_count"] > 1
    assert dep_map["entries"]["truncated"] is True
    assert "map" not in dep_map


def test_tach_mcp_analyze_closure_delta_and_changed_files():
    closure = tach_mcp.tach_map(
        str(EXAMPLE_ROOT),
        mode="closure",
        path="domain_two/some_file.py",
    )
    assert "domain_two/some_file.py" in closure["closure"]

    delta = tach_mcp.tach_map(
        str(EXAMPLE_ROOT),
        mode="delta",
        changed=["domain_two/other.py"],
    )
    assert delta["changed_files"] == ["domain_two/other.py"]
    assert "domain_two/some_file.py" in delta["affected"]["items"]


def test_tach_mcp_analyze_modularity_export(tmp_path):
    summary = tach_mcp.tach_modularity(str(Path.cwd()), force=True)
    assert summary["module_count"] > 1
    assert "full_configuration" not in summary

    output_path = tmp_path / "report.json"
    exported = tach_mcp.tach_modularity(
        str(Path.cwd()),
        mode="export",
        output_path=str(output_path),
        force=True,
    )
    assert exported["ok"] is True
    assert json.loads(output_path.read_text())["repo"] == "tach"


def test_tach_mcp_test_affected():
    result = tach_mcp.tach_test(
        str(Path.cwd()),
        pytest_args=["python/tests/test_cli.py", "-q"],
        max_bytes=1000,
    )

    assert result["ok"] is True
    assert result["stdout_bytes"] > 0


def test_tach_mcp_create_config_emits_editable_format(tmp_path):
    # The Rust config editor (sync_project/save_edits) silently no-ops on
    # inline `modules = [{...}]` arrays, so generated configs must use
    # [[modules]] array-of-tables format.
    project_root = tmp_path / "project"
    src = project_root / "src"
    src.mkdir(parents=True)
    (src / "module_one.py").write_text("import module_two\n")
    (src / "module_two.py").write_text("")

    tach_mcp.tach_configure(
        "create_config",
        str(project_root),
        source_roots=["src"],
        modules=["module_one", "module_two"],
    )
    assert "[[modules]]" in (project_root / "tach.toml").read_text()

    synced = tach_mcp.tach_configure("sync_dependencies", str(project_root))
    assert synced["changed"] is True
    assert 'depends_on = ["module_two"]' in (project_root / "tach.toml").read_text()


def test_tach_mcp_configure_architecture_rules(tmp_path):
    project_root = tmp_path / "project"
    src = project_root / "src"
    src.mkdir(parents=True)
    (src / "api.py").write_text("import services\n")
    (src / "services.py").write_text("import models\n")
    (src / "models.py").write_text("")

    tach_mcp.tach_configure(
        "create_config",
        str(project_root),
        source_roots=["src"],
        modules=["api", "services", "models"],
        dependencies=[
            {"path": "api", "dependency": "services"},
            {"path": "services", "dependency": "models"},
        ],
    )

    layered = tach_mcp.tach_configure(
        "set_layers",
        str(project_root),
        layers=["ui", "commands", "core"],
    )
    assert layered["ok"] is True
    tach_mcp.tach_configure(
        "set_module_layer", str(project_root), path="api", layer="ui"
    )
    visible = tach_mcp.tach_configure(
        "set_module_visibility",
        str(project_root),
        path="models",
        visibility=["services"],
    )
    assert visible["ok"] is True

    deprecated = tach_mcp.tach_configure(
        "deprecate_dependency",
        str(project_root),
        path="services",
        dependency="models",
    )
    assert deprecated["deprecated"] is True

    added = tach_mcp.tach_configure(
        "add_interface",
        str(project_root),
        interface_expose=["get_model"],
        interface_from=["models"],
        interface_data_types="primitive",
    )
    assert added["ok"] is True

    config_text = (project_root / "tach.toml").read_text()
    assert 'layers = [\n    "ui",' in config_text
    assert 'layer = "ui"' in config_text
    assert "deprecated = true" in config_text
    assert "[[interfaces]]" in config_text

    lint = tach_mcp.tach_lint(str(project_root), checks="boundaries")
    assert lint["warning_count"] == 1  # deprecated services -> models usage

    removed = tach_mcp.tach_configure(
        "remove_interface",
        str(project_root),
        interface_expose=["get_model"],
        interface_from=["models"],
    )
    assert removed["removed"] == 1


def test_tach_mcp_lint_reports_circular_dependencies_structured(tmp_path):
    project_root = tmp_path / "project"
    src = project_root / "src"
    src.mkdir(parents=True)
    (src / "module_one.py").write_text("import module_two\n")
    (src / "module_two.py").write_text("import module_one\n")

    tach_mcp.tach_configure(
        "create_config",
        str(project_root),
        source_roots=["src"],
        modules=["module_one", "module_two"],
        dependencies=[
            {"path": "module_one", "dependency": "module_two"},
            {"path": "module_two", "dependency": "module_one"},
        ],
        forbid_circular_dependencies=True,
    )
    assert (
        "forbid_circular_dependencies = true"
        in (project_root / "tach.toml").read_text()
    )

    result = tach_mcp.tach_lint(str(project_root), checks="boundaries")

    assert result["ok"] is False
    assert result["boundaries"]["ok"] is False
    assert "module_one" in result["boundaries"]["circular_dependencies"]
    assert result["next_actions"]


def test_tach_mcp_report_is_size_bounded():
    result = tach_mcp.tach_report(
        path="domain_two",
        project_root=str(EXAMPLE_ROOT),
        max_bytes=50,
    )

    assert result["report_truncated"] is True
    assert len(result["report"].encode()) <= 50
    assert result["report_bytes"] > 50


def test_tach_mcp_prompts():
    prompt = tach_mcp.diagnose_tach_boundaries(str(EXAMPLE_ROOT))

    assert "tach_lint" in prompt
