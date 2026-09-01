from __future__ import annotations

import json
import os
import re
from dataclasses import asdict, dataclass, field
from http.client import HTTPConnection, HTTPSConnection
from typing import TYPE_CHECKING, Any
from urllib import parse

from tach import extension
from tach import filesystem as fs
from tach.console import console
from tach.constants import GAUGE_API_BASE_URL
from tach.errors import TachClosedBetaError, TachError
from tach.extension import (
    ProjectConfig,
    check,
    get_project_imports,
)
from tach.filesystem.git_ops import get_current_branch_info

if TYPE_CHECKING:
    from pathlib import Path

REPORT_VERSION = "1.6"


def export_report(
    project_root: Path,
    project_config: ProjectConfig,
    output_path: Path | None = None,
    force: bool = False,
):
    """Export a modularity report to a local file."""
    report = generate_modularity_report(project_root, project_config, force=force)
    output_path = output_path or project_root / "modularity_report.json"
    output_path.write_text(json.dumps(asdict(report), indent=2))


def upload_report_to_gauge(
    project_root: Path,
    project_config: ProjectConfig,
    force: bool = False,
):
    """Upload a modularity report to Gauge."""
    report = generate_modularity_report(project_root, project_config, force=force)
    console.print("[cyan] > Uploading report...[/]")
    response_data = post_json_to_gauge_api(asdict(report))
    console.print("[green] > Report uploaded![/]")
    if response_data.get("url"):
        console.print(f"[blue] > {GAUGE_API_BASE_URL}{response_data['url']}[/]")


GAUGE_API_KEY = os.getenv("GAUGE_API_KEY", "")
GAUGE_UPLOAD_URL = f"{GAUGE_API_BASE_URL}/api/client/tach-upload/{REPORT_VERSION}"


def post_json_to_gauge_api(
    data: dict[str, Any], user_name: str | None = None
) -> dict[str, str]:
    if not GAUGE_API_KEY:
        raise TachClosedBetaError(
            f"[yellow]Modularity is currently in closed beta. Visit {GAUGE_API_BASE_URL}/closed-beta to request access.[/]"
            "\n\n"
            f"[cyan]Already have access? Set the GAUGE_API_KEY environment variable to continue.[/]"
        )
    headers = {
        "Content-Type": "application/json",
        "Authorization": GAUGE_API_KEY,
    }
    if user_name:
        data["user_name"] = user_name
    json_data = json.dumps(data)
    conn = None
    try:
        url_parts: parse.ParseResult = parse.urlparse(GAUGE_UPLOAD_URL)
        if GAUGE_UPLOAD_URL.startswith("https://"):
            conn = HTTPSConnection(url_parts.netloc, timeout=10)
        else:
            conn = HTTPConnection(url_parts.netloc, timeout=10)
        conn.request("POST", url_parts.path, body=json_data, headers=headers)
        response = conn.getresponse()
        response_data = response.read().decode("utf-8")
        # If key is unbound, prompt user to provide username to bind key
        if response.status == 422 and not user_name:
            # Prompt user to provide username
            conn.close()
            user_name = input("Enter your GitHub username: ").strip()
            return post_json_to_gauge_api(data, user_name)
        if response.status != 200:
            raise TachError(
                f"API request failed with status {response.status}: {response_data}"
            )
    except Exception as e:
        raise TachError(f"Failed to upload modularity report: {e}")
    finally:
        if conn is not None:
            conn.close()
    return json.loads(response_data)


# NOTE: these usages are all imports
@dataclass
class Usage:
    # The configured Module being used
    module_path: str
    # The full import path to the member being used
    full_path: str
    # The file containing the usage
    filepath: str
    # 1-indexed location of the usage in the file
    line: int
    # [1.1] The Module that contains the Usage (None indicates Usage outside of any Module)
    containing_module_path: str | None


@dataclass
class Dependency:
    path: str
    deprecated: bool = False


@dataclass
class Module:
    path: str
    # [1.2] Replaces 'is_strict'
    has_interface: bool = False
    interface_members: list[str] = field(default_factory=list)
    # [1.3] Adds 'depends_on'
    depends_on: list[Dependency] = field(default_factory=list)
    # [1.4] Adds 'layer'
    layer: str | None = None


@dataclass
class ReportMetadata:
    version: str = REPORT_VERSION
    configuration_format: str = "json"


@dataclass
class Report:
    email: str
    user_name: str
    owner: str
    repo: str
    branch: str
    commit: str
    # [1.1] The full configuration encoded as JSON
    full_configuration: str
    modules: list[Module] = field(default_factory=list)
    usages: list[Usage] = field(default_factory=list)
    # [1.5] Diagnostics (changed from list[UsageError] to str)
    diagnostics: str = "[]"
    metadata: ReportMetadata = field(default_factory=ReportMetadata)


def build_modules(
    project_config: ProjectConfig,
    included_paths: list[Path] | None = None,
) -> list[Module]:
    config_modules = (
        project_config.filtered_modules(included_paths)
        if included_paths
        else project_config.all_modules()
    )
    modules: list[Module] = []
    for module in config_modules:
        if module.mod_path() == ".":
            # Skip <root>
            continue

        has_interface = False
        interface_members: set[str] = set()
        for interface in project_config.all_interfaces():
            if any(
                re.match(r"^" + pattern + r"$", module.path)
                for pattern in interface.from_modules
            ):
                has_interface = True
                interface_members.update(interface.expose)
        dependencies = [
            Dependency(path=dep.path, deprecated=dep.deprecated)
            for dep in module.depends_on
            or []  # TODO: platform should differentiate None vs. []
        ]
        modules.append(
            Module(
                path=module.path,
                has_interface=has_interface,
                interface_members=list(interface_members),
                depends_on=dependencies,
                layer=module.layer,
            )
        )
    return modules


def build_usages(
    project_root: Path,
    project_config: ProjectConfig,
    included_paths: list[Path] | None = None,
) -> list[Usage]:
    source_roots = [project_root / root for root in project_config.source_roots]
    modules = (
        project_config.filtered_modules(included_paths)
        if included_paths
        else project_config.all_modules()
    )
    module_paths = sorted(
        (module.path for module in modules),
        key=lambda path: len(path.split(".")),
        reverse=True,
    )

    def get_containing_module(mod_path: str) -> str | None:
        return next(
            (
                module_path
                for module_path in module_paths
                if mod_path == module_path or mod_path.startswith(f"{module_path}.")
            ),
            None,
        )

    usages: list[Usage] = []
    for source_root in source_roots:
        for pyfile in fs.walk_pyfiles(
            source_root,
            project_root=project_root,
            exclude_paths=project_config.exclude,
        ):
            pyfile_mod_path = fs.file_to_module_path(
                tuple(source_roots), source_root / pyfile
            )
            pyfile_containing_module = get_containing_module(pyfile_mod_path)
            imports = get_project_imports(
                project_root=project_root,
                source_roots=source_roots,
                file_path=source_root / pyfile,
                project_config=project_config,
            )
            for project_import in imports:
                import_containing_module = get_containing_module(
                    project_import.module_path
                )
                if (
                    import_containing_module is None
                    or import_containing_module == pyfile_containing_module
                ):
                    continue

                usages.append(
                    Usage(
                        module_path=import_containing_module,
                        full_path=project_import.module_path,
                        filepath=str((source_root / pyfile).relative_to(project_root)),
                        line=project_import.line_number,
                        containing_module_path=pyfile_containing_module,
                    )
                )

    return usages


def serialize_diagnostics(
    project_root: Path,
    project_config: ProjectConfig,
) -> str:
    check_diagnostics = check(
        project_root=project_root,
        project_config=project_config,
        dependencies=True,
        interfaces=True,
    )
    return extension.serialize_diagnostics_json(check_diagnostics, pretty_print=False)


def generate_modularity_report(
    project_root: Path, project_config: ProjectConfig, force: bool = False
) -> Report:
    console.print("[cyan] > Generating report...[/]")
    branch_info = get_current_branch_info(project_root, allow_dirty=force)
    report = Report(
        user_name="",  # only needed for binding a new api key
        email=branch_info.email,
        owner=branch_info.owner,
        repo=branch_info.repo,
        branch=branch_info.name,
        commit=branch_info.commit,
        full_configuration=project_config.serialize_json(),
    )

    report.modules = build_modules(project_config)
    report.usages = build_usages(project_root, project_config)
    report.diagnostics = serialize_diagnostics(
        project_root=project_root,
        project_config=project_config,
    )

    console.print("[green] > Report generated![/]")
    return report


__all__ = ["export_report", "upload_report_to_gauge"]
