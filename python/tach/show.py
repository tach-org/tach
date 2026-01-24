from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from json.decoder import JSONDecodeError
from typing import TYPE_CHECKING
from urllib import error, request

from tach.constants import GAUGE_API_BASE_URL
from tach.modularity import (
    Module,
    Usage,
    build_modules,
    build_usages,
    serialize_diagnostics,
)

if TYPE_CHECKING:
    from pathlib import Path

    import pydot  # type: ignore

    from tach.extension import ProjectConfig


@dataclass
class ShowReportMetadata:
    version: str = "1.5"


@dataclass
class ShowReport:
    modules: list[Module]
    usages: list[Usage]
    diagnostics: str
    metadata: ShowReportMetadata = field(default_factory=ShowReportMetadata)


def generate_show_report(
    project_root: Path,
    project_config: ProjectConfig,
    included_paths: list[Path],
) -> ShowReport:
    modules = build_modules(
        project_config=project_config, included_paths=included_paths
    )
    usages = build_usages(
        project_root=project_root,
        project_config=project_config,
        included_paths=included_paths,
    )
    diagnostics = serialize_diagnostics(
        project_root=project_root,
        project_config=project_config,
    )
    return ShowReport(modules=modules, usages=usages, diagnostics=diagnostics)


def upload_show_report(
    project_root: Path,
    project_config: ProjectConfig,
    included_paths: list[Path],
) -> str | None:
    show_report = generate_show_report(
        project_root=project_root,
        project_config=project_config,
        included_paths=included_paths,
    )
    json_data = json.dumps(asdict(show_report))
    json_bytes = json_data.encode("utf-8")
    req = request.Request(
        f"{GAUGE_API_BASE_URL}/api/show/graph/1.5",
        data=json_bytes,
        headers={"Content-Type": "application/json"},
    )
    try:
        # Send the request and read the response
        with request.urlopen(req) as response:
            response_data = response.read().decode("utf-8")
            response_json = json.loads(response_data)
            uid = response_json.get("uid")
            return f"{GAUGE_API_BASE_URL}/show?uid={uid}"
    except (UnicodeDecodeError, JSONDecodeError, error.URLError) as e:
        print(f"Error: {e}")
        return None


def generate_module_graph_dot_string(
    project_config: ProjectConfig,
    included_paths: list[Path],
) -> str:
    # Local import because networkx takes about ~100ms to load
    import networkx as nx

    graph = nx.DiGraph()  # type: ignore

    def upsert_edge(
        graph: nx.DiGraph,  # type: ignore
        module: str,
        dependency: str,
        dashed: bool = False,
    ) -> None:
        if module not in graph:
            graph.add_node(module)  # type: ignore
        if dependency not in graph:
            graph.add_node(dependency)  # type: ignore
        if dashed:
            graph.add_edge(module, dependency, style="dashed")  # type: ignore
        else:
            graph.add_edge(module, dependency)  # type: ignore

    modules = project_config.filtered_modules(included_paths)

    # Sort modules by path for deterministic output
    for module in sorted(modules, key=lambda m: m.path):
        for dependency in sorted(module.depends_on or [], key=lambda d: d.path):
            upsert_edge(graph, module.path, dependency.path, dependency.deprecated)  # type: ignore

    pydot_graph: pydot.Dot = nx.nx_pydot.to_pydot(graph)  # type: ignore
    return str(pydot_graph.to_string())  # type: ignore


def generate_module_graph_mermaid_string(
    project_config: ProjectConfig,
    included_paths: list[Path],
) -> str:
    modules = project_config.filtered_modules(included_paths)
    edges: list[str] = []
    isolated: list[str] = []
    LINE_ARROW = "-->"
    DOTTED_ARROW = "-.->"
    # Sort modules by path for deterministic output
    for module in sorted(modules, key=lambda m: m.path):
        for dependency in sorted(module.depends_on or [], key=lambda d: d.path):
            module_name = module.path.strip("<>")
            dependency_name = dependency.path.strip("<>")
            if dependency.deprecated:
                edges.append(f"    {module_name} {DOTTED_ARROW} {dependency_name}")
            else:
                edges.append(f"    {module_name} {LINE_ARROW} {dependency_name}")
        if not module.depends_on:
            isolated.append(f"    {module.path.strip('<>')}")

    return "graph TD\n" + "\n".join(edges) + "\n" + "\n".join(isolated)


def generate_module_graph_dot_file(
    project_config: ProjectConfig,
    output_filepath: Path,
    included_paths: list[Path],
) -> None:
    dot_data = generate_module_graph_dot_string(project_config, included_paths)
    output_filepath.write_text(dot_data)


def generate_module_graph_mermaid(
    project_config: ProjectConfig,
    output_filepath: Path,
    included_paths: list[Path],
) -> None:
    mermaid_graph = generate_module_graph_mermaid_string(project_config, included_paths)
    output_filepath.write_text(mermaid_graph)


__all__ = [
    "upload_show_report",
    "generate_module_graph_dot_file",
    "generate_module_graph_mermaid",
    "generate_module_graph_dot_string",
    "generate_module_graph_mermaid_string",
]
