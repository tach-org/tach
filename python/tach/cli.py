from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import TYPE_CHECKING, Any

from tach import __version__, cache, extension, icons
from tach import filesystem as fs
from tach.check_external import check_external
from tach.console import console_err
from tach.constants import CONFIG_FILE_NAME, DOCS_BASE_URL, TOOL_NAME
from tach.errors import (
    TachCircularDependencyError,
    TachClosedBetaError,
    TachError,
    TachSetupError,
    TachVisibilityError,
)
from tach.extension import Direction, ProjectConfig
from tach.filesystem import install_pre_commit
from tach.init import init_project
from tach.logging import CallInfo, init_logging, logger
from tach.modularity import export_report, upload_report_to_gauge
from tach.parsing import combine_exclude_paths, parse_project_config
from tach.report import external_dependency_report, report
from tach.show import (
    generate_module_graph_dot_file,
    generate_module_graph_dot_string,
    generate_module_graph_mermaid,
    generate_module_graph_mermaid_string,
    upload_show_report,
)
from tach.test import run_affected_tests

if TYPE_CHECKING:
    from tach.extension import UnusedDependencies


import signal


def handle_sigint(_signum: int, _frame: Any) -> None:
    print("Exiting...")
    sys.exit(1)


signal.signal(signal.SIGINT, handle_sigint)


def print_unused_dependencies(
    all_unused_dependencies: list[UnusedDependencies],
) -> None:
    constraint_messages = "\n".join(
        f"{icons.FAIL} [bold]'{unused_dependencies.path}'[/] does not depend on: [bold]{[dependency.path for dependency in unused_dependencies.dependencies]}[/]"
        for unused_dependencies in all_unused_dependencies
    )
    console_err.print(
        "[red bold]Unused Dependencies[/]\n" + f"[yellow]{constraint_messages}[/]"
    )
    console_err.print(
        f"\nRemove the unused dependencies from {CONFIG_FILE_NAME}.toml, "
        f"or consider running '{TOOL_NAME} sync' to update module configuration and "
        f"remove all unused dependencies.\n",
        style="yellow",
    )


def print_no_config_found(
    output_format: str = "text", *, config_file_name: str = CONFIG_FILE_NAME
) -> None:
    if output_format == "json":
        json.dump({"error": "No config file found"}, sys.stdout)
    else:
        console_err.print(
            f"Configuration file not found. Run [cyan]'{TOOL_NAME} init'[/] to get started!",
            style="yellow",
        )


def print_no_modules_found() -> None:
    console_err.print(
        "No modules have been defined yet. Run [cyan]'tach init'[/] to get started!",
        style="yellow",
    )


def print_no_dependencies_found() -> None:
    console_err.print(
        "No dependency rules were found for your modules. You may need to run [cyan]'tach sync'[/] or adjust your source root.",
        style="yellow",
    )


def print_show_web_suggestion(is_mermaid: bool = False) -> None:
    if is_mermaid:
        console_err.print(
            "NOTE: You are generating a Mermaid graph locally representing your module graph. For a remotely hosted visualization, use the '--web' argument.\nTo visualize your graph, you will need to use Mermaid.js: https://mermaid.js.org/config/usage.html\n",
            style="cyan",
        )
    else:
        console_err.print(
            "NOTE: You are generating a DOT file locally representing your module graph. For a remotely hosted visualization, use the '--web' argument.\nTo visualize your graph, you will need a program like GraphViz: https://www.graphviz.org/download/\n",
            style="cyan",
        )


def print_generated_module_graph_file(
    output_filepath: Path, is_mermaid: bool = False
) -> None:
    if is_mermaid:
        console_err.print(
            f"Generated a Mermaid file containing your module graph at '{output_filepath}'",
            style="green",
        )
    else:
        console_err.print(
            f"Generated a DOT file containing your module graph at '{output_filepath}'",
            style="green",
        )


def print_circular_dependency_error(
    module_paths: list[str], output_format: str = "text"
) -> None:
    if output_format == "json":
        json.dump(
            {"error": "Circular dependency", "dependencies": module_paths}, sys.stdout
        )
    else:
        console_err.print(
            "\n".join(
                [
                    f"{icons.FAIL} [red]Circular dependency detected for module [/]'{module_path}'"
                    for module_path in module_paths
                ]
            )
            + f"\n\n[yellow]Resolve circular dependencies.\n"
            f"Remove or unset 'forbid_circular_dependencies' from "
            f"'{CONFIG_FILE_NAME}.toml' to allow circular dependencies.[/]",
        )


def print_visibility_errors(
    visibility_errors: list[tuple[str, str, list[str]]], output_format: str = "text"
) -> None:
    if output_format == "json":
        json.dump(
            {"error": "Visibility error", "visibility_errors": visibility_errors},
            sys.stdout,
        )
    else:
        for dependent_module, dependency_module, visibility in visibility_errors:
            console_err.print(
                f"{icons.FAIL} [red]Module configuration error:[/] [yellow]'{dependent_module}' cannot depend on '{dependency_module}' because '{dependent_module}' does not match its visibility: {visibility}.[/]"
                "\n"
                f"[yellow]Adjust 'visibility' for '{dependency_module}' to include '{dependent_module}', or remove the dependency.[/]"
                "\n",
            )


def add_base_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "-e",
        "--exclude",
        required=False,
        type=str,
        metavar="file_or_path,...",
        help="Comma separated path list to exclude. tests/, ci/, etc.",
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog=TOOL_NAME,
        add_help=True,
    )
    parser.add_argument(
        "--version", action="version", version=f"{TOOL_NAME} {__version__}"
    )

    subparsers = parser.add_subparsers(title="commands", dest="command")

    ## tach mod
    mod_parser = subparsers.add_parser(
        "mod",
        prog=f"{TOOL_NAME} mod",
        help="Configure module boundaries interactively",
        description="Configure module boundaries interactively",
    )
    mod_parser.add_argument(
        "-d",
        "--depth",
        type=int,
        nargs="?",
        default=None,
        help="The number of child directories to expand from the root",
    )
    add_base_arguments(mod_parser)

    ## tach check
    check_parser = subparsers.add_parser(
        "check",
        prog=f"{TOOL_NAME} check",
        help="Check existing boundaries against your dependencies and module interfaces",
        description="Check existing boundaries against your dependencies and module interfaces",
    )
    check_parser.add_argument(
        "--exact",
        action="store_true",
        help="When checking dependencies, raise errors if any dependencies are unused.",
    )
    check_parser.add_argument(
        "--dependencies",
        action="store_true",
        help="Check dependency constraints between modules. When present, all checks must be explicitly enabled.",
    )
    check_parser.add_argument(
        "--interfaces",
        action="store_true",
        help="Check interface implementations. When present, all checks must be explicitly enabled.",
    )
    check_parser.add_argument(
        "--output",
        choices=["text", "json"],
        default="text",
        help="Output format (default: text)",
    )
    add_base_arguments(check_parser)

    ## tach check-external
    check_parser_external = subparsers.add_parser(
        "check-external",
        prog=f"{TOOL_NAME} check-external",
        help="Perform checks related to third-party dependencies",
        description="Perform checks related to third-party dependencies",
    )
    add_base_arguments(check_parser_external)

    ## tach sync
    sync_parser = subparsers.add_parser(
        "sync",
        prog=f"{TOOL_NAME} sync",
        help="Sync constraints with actual dependencies in your project.",
        description="Sync constraints with actual dependencies in your project.",
    )
    sync_parser.add_argument(
        "--add",
        action="store_true",
        help="Add any missing dependencies, but do not remove unused dependencies.",
    )
    add_base_arguments(sync_parser)

    ## tach report
    report_parser = subparsers.add_parser(
        "report",
        prog=f"{TOOL_NAME} report",
        help="Create a report of dependencies and usages.",
        description="Create a report of dependencies and usages.",
    )
    report_parser.add_argument(
        "path", help="The path or directory path used to generate the report."
    )
    # Report type flags
    report_parser.add_argument(
        "--dependencies",
        action="store_true",
        help="Generate dependency report. When present, all reports must be explicitly enabled.",
    )
    report_parser.add_argument(
        "--usages",
        action="store_true",
        help="Generate usage report. When present, all reports must be explicitly enabled.",
    )
    report_parser.add_argument(
        "--external",
        action="store_true",
        help="Generate external dependency report. When present, all reports must be explicitly enabled.",
    )
    # Report options
    report_parser.add_argument(
        "-d",
        "--dependency-modules",
        required=False,
        type=str,
        metavar="module_path,...",
        help="Comma separated module list of dependencies to include [includes everything by default]",
    )
    report_parser.add_argument(
        "-u",
        "--usage-modules",
        required=False,
        type=str,
        metavar="module_path,...",
        help="Comma separated module list of usages to include [includes everything by default]",
    )
    report_parser.add_argument(
        "--raw",
        action="store_true",
        help="Group lines by module and print each without any formatting.",
    )
    add_base_arguments(report_parser)

    ## tach show
    show_parser = subparsers.add_parser(
        "show",
        prog=f"{TOOL_NAME} show",
        help="Visualize the dependency graph of your project.",
        description="Visualize the dependency graph of your project.",
    )
    show_parser.add_argument(
        "included_paths",
        type=Path,
        nargs="*",
        help="Paths to include in the module graph. If not provided, the entire project is included.",
    )
    show_parser.add_argument(
        "--web",
        action="store_true",
        help="Open your dependency graph in a remote web viewer.",
    )
    show_parser.add_argument(
        "--mermaid",
        action="store_true",
        help="Generate a mermaid.js graph instead of a DOT file.",
    )
    show_parser.add_argument(
        "-o",
        "--out",
        type=str,
        nargs="?",
        default=None,
        help=f"Output file path. Use '-' for stdout (default: '{TOOL_NAME}_module_graph{{.mmd|.dot}}')",
    )

    ## tach install
    install_parser = subparsers.add_parser(
        "install",
        prog=f"{TOOL_NAME} install",
        help=f"Install {TOOL_NAME} into your workflow (e.g. as a pre-commit hook)",
        description=f"Install {TOOL_NAME} into your workflow (e.g. as a pre-commit hook)",
    )
    install_parser.add_argument(
        "target",
        choices=InstallTarget.choices(),
        help="What kind of installation to perform (e.g. pre-commit)",
    )

    ## tach test
    test_parser = subparsers.add_parser(
        "test",
        prog=f"{TOOL_NAME} test",
        help="Run tests on modules impacted by the current changes.",
        description="Run tests on modules impacted by the current changes.",
    )
    test_parser.add_argument(
        "--base",
        type=str,
        nargs="?",
        default="main",
        help="The base commit to use when determining which modules are impacted by changes. [default: 'main']",
    )
    test_parser.add_argument(
        "--head",
        type=str,
        nargs="?",
        default="",
        help="The head commit to use when determining which modules are impacted by changes. [default: current filesystem]",
    )
    test_parser.add_argument(
        "--disable-cache",
        action="store_true",
        help="Do not check cache for results, and do not push results to cache.",
    )
    test_parser.add_argument(
        "pytest_args",
        nargs=argparse.REMAINDER,
        help=f"Arguments forwarded to pytest. Use '--' to separate these arguments. Ex: '{TOOL_NAME} test -- -v'",
    )

    ## tach upload
    upload_parser = subparsers.add_parser(
        "upload",
        prog=f"{TOOL_NAME} upload",
        help="[CLOSED BETA] Upload a modularity report to Gauge",
        description="[CLOSED BETA] Upload a modularity report to Gauge",
    )
    upload_parser.add_argument(
        "-f",
        "--force",
        action="store_true",
        help="Ignore warnings and force the report to be generated.",
    )

    ## tach export
    export_parser = subparsers.add_parser(
        "export",
        prog=f"{TOOL_NAME} export",
        help="Export a modularity report to a local file",
        description="Export a modularity report to a local file",
    )
    export_parser.add_argument(
        "-o",
        "--output",
        type=Path,
        nargs="?",
        default=None,
        help="Specify an output path for the modularity report [DEFAULT: 'modularity_report.json']",
    )
    export_parser.add_argument(
        "-f",
        "--force",
        action="store_true",
        help="Ignore warnings and force the report to be generated.",
    )

    ## tach server
    server_parser = subparsers.add_parser(
        "server",
        prog=f"{TOOL_NAME} server",
        help="Start the Language Server Protocol (LSP) server",
        description="Start the Language Server Protocol (LSP) server",
    )
    server_parser.add_argument(
        "-c",
        "--config",
        type=Path,
        nargs="?",
        default=None,
        help="Path to the config file",
    )
    ## tach init
    init_parser = subparsers.add_parser(
        "init",
        prog=f"{TOOL_NAME} init",
        help="Initialize a new project",
        description="Initialize a new project",
    )
    init_parser.add_argument(
        "--force",
        action="store_true",
        help="Force re-initialization if project is already configured.",
    )

    ## tach map
    map_parser = subparsers.add_parser(
        "map",
        prog=f"{TOOL_NAME} map",
        help="Build a dependency map and write it to a file or stdout",
        description="Build a dependency map and write it to a file or stdout",
    )
    map_parser.add_argument(
        "-o",
        "--output",
        type=str,
        default="-",
        help="Output file path. Use '-' for stdout (default: '-')",
    )
    map_parser.add_argument(
        "--direction",
        choices=["dependencies", "dependents"],
        default="dependencies",
        help="Direction of the map (default: 'dependencies')",
    )
    map_parser.add_argument(
        "--closure",
        type=str,
        help="Get the closure for a specific file path",
    )

    return parser


def parse_arguments(
    args: list[str],
) -> tuple[argparse.Namespace, argparse.ArgumentParser]:
    parser = build_parser()
    parsed_args = parser.parse_args(args)
    return parsed_args, parser


@dataclass
class CachedOutput:
    key: str
    output: list[tuple[int, str]] = field(default_factory=list)
    exit_code: int | None = None

    @property
    def exists(self) -> bool:
        return self.exit_code is not None

    def replay(self):
        for fd, output in self.output:
            if fd == 1:
                print(output, file=sys.stdout)
            elif fd == 2:
                print(output, file=sys.stderr)


def check_cache_for_action(
    project_root: Path, project_config: ProjectConfig, action: str
) -> CachedOutput:
    cache_key = extension.create_computation_cache_key(
        project_root=project_root,
        source_roots=[
            project_root / source_root for source_root in project_config.source_roots
        ],
        action=action,
        py_interpreter_version=f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
        file_dependencies=project_config.cache.file_dependencies,
        env_dependencies=project_config.cache.env_dependencies,
        backend=project_config.cache.backend,
        respect_gitignore=project_config.respect_gitignore,
    )
    cache_result = extension.check_computation_cache(
        project_root=project_root, cache_key=cache_key
    )
    if cache_result:
        return CachedOutput(
            key=cache_key,
            output=cache_result[0],
            exit_code=cache_result[1],
        )
    return CachedOutput(key=cache_key)


def tach_check(
    project_config: ProjectConfig,
    project_root: Path,
    exact: bool = False,
    dependencies: bool = True,
    interfaces: bool = True,
    output_format: str = "text",
):
    logger.info(
        "tach check called",
        extra={
            "data": CallInfo(
                function="tach_check",
                parameters={"exact": exact, "output_format": output_format},
            ),
        },
    )
    try:
        exact |= project_config.exact

        diagnostics = extension.check(
            project_root=project_root,
            project_config=project_config,
            dependencies=dependencies,
            interfaces=interfaces,
        )
        has_errors = any(diagnostic.is_error() for diagnostic in diagnostics)

        if output_format == "json":
            try:
                print(
                    extension.serialize_diagnostics_json(diagnostics, pretty_print=True)
                )
            except ValueError as e:
                json.dump({"error": str(e)}, sys.stdout)
            sys.exit(1 if has_errors else 0)

        if diagnostics:
            print(
                extension.format_diagnostics(diagnostics=diagnostics),
                file=sys.stderr,
            )
        exit_code = 1 if has_errors else 0

        # If we're checking in exact mode, we want to verify that there are no unused dependencies
        if dependencies and exact:
            unused_dependencies = extension.detect_unused_dependencies(
                project_root=project_root,
                project_config=project_config,
            )
            if unused_dependencies:
                print_unused_dependencies(unused_dependencies)
                exit_code = 1

    except TachCircularDependencyError as e:
        print_circular_dependency_error(e.dependencies, output_format)
        sys.exit(1)
    except TachVisibilityError as e:
        print_visibility_errors(e.visibility_errors, output_format)
        sys.exit(1)
    except Exception as e:
        if output_format == "json":
            json.dump({"error": str(e)}, sys.stdout)
        else:
            print(str(e))
        sys.exit(1)

    if exit_code == 0 and output_format == "text":
        console_err.print(f"{icons.SUCCESS} All modules validated!", style="green")
    sys.exit(exit_code)


def tach_check_external(
    project_config: ProjectConfig,
    project_root: Path,
):
    logger.info(
        "tach check-external called",
        extra={
            "data": CallInfo(
                function="tach_check_external",
            ),
        },
    )
    try:
        diagnostics = check_external(
            project_root=project_root,
            project_config=project_config,
        )
        if diagnostics:
            print(
                extension.format_diagnostics(diagnostics=diagnostics),
                file=sys.stderr,
            )

        has_errors = any(diagnostic.is_error() for diagnostic in diagnostics)
        if has_errors:
            sys.exit(1)
        else:
            console_err.print(
                f"{icons.SUCCESS} All external dependencies validated!", style="green"
            )
            sys.exit(0)

    except Exception as e:
        print(str(e))
        sys.exit(1)


def tach_mod(
    project_root: Path,
    depth: int | None = 1,
    exclude_paths: list[str] | None = None,
):
    logger.info(
        "tach mod called",
        extra={
            "data": CallInfo(
                function="tach_mod",
                parameters={"depth": depth},
            ),
        },
    )
    # Local import because prompt_toolkit takes about ~80ms to load
    from tach.mod import mod_edit_interactive

    try:
        project_config = parse_project_config(root=project_root) or ProjectConfig()
        exclude_paths = combine_exclude_paths(exclude_paths, project_config.exclude)
        saved_changes, warnings = mod_edit_interactive(
            project_root=project_root,
            project_config=project_config,
            exclude_paths=exclude_paths,
            depth=depth,
        )
    except Exception as e:
        console_err.print(str(e))
        sys.exit(1)

    if warnings:
        console_err.print("\n".join(warnings))
    if saved_changes:
        console_err.print(
            f"{icons.SUCCESS} Set modules! You may want to run '{TOOL_NAME} sync' "
            f"to automatically set boundaries.",
            style="green",
        )
    sys.exit(0)


def tach_sync(
    project_config: ProjectConfig,
    project_root: Path,
    add: bool = False,
):
    logger.info(
        "tach sync called",
        extra={
            "data": CallInfo(
                function="tach_sync",
                parameters={"add": add},
            ),
        },
    )
    try:
        extension.sync_project(
            project_root=project_root,
            project_config=project_config,
            add=add,
        )
    except Exception as e:
        print(str(e))
        sys.exit(1)

    console_err.print(f"{icons.SUCCESS} Synced dependencies.", style="green")
    sys.exit(0)


class InstallTarget(Enum):
    PRE_COMMIT = "pre-commit"

    @classmethod
    def choices(cls) -> list[str]:
        return [item.value for item in cls]


def tach_install(project_root: Path, target: InstallTarget) -> None:
    logger.info(
        "tach install called",
        extra={
            "data": CallInfo(
                function="tach_install",
            ),
        },
    )
    try:
        if target == InstallTarget.PRE_COMMIT:
            installed, warning = install_pre_commit(project_root=project_root)
        else:
            raise NotImplementedError(f"Target {target} is not supported by 'install'.")
    except Exception as e:
        print(str(e))
        sys.exit(1)

    if installed:
        console_err.print(
            f"{icons.SUCCESS} Pre-commit hook installed to '.git/hooks/pre-commit'.",
            style="green",
        )
        sys.exit(0)
    else:
        console_err.print(
            f"Pre-commit hook could not be installed: {warning}",
            style="yellow",
        )
        sys.exit(1)


def tach_report(
    project_config: ProjectConfig,
    project_root: Path,
    path: str,
    include_dependency_modules: list[str] | None = None,
    include_usage_modules: list[str] | None = None,
    dependencies: bool = False,
    usages: bool = False,
    external: bool = False,
    raw: bool = False,
):
    logger.info(
        "tach report called",
        extra={
            "data": CallInfo(
                function="tach_report",
                parameters={
                    "dependencies": dependencies,
                    "usages": usages,
                    "external": external,
                },
            ),
        },
    )
    try:
        # Generate reports based on flags
        generate_all = not (dependencies or usages or external)
        generate_dependencies = generate_all or dependencies
        generate_usages = generate_all or usages
        generate_external = generate_all or external

        reports: list[str] = []
        if generate_dependencies or generate_usages:
            reports.append(
                report(
                    project_root,
                    Path(path),
                    project_config=project_config,
                    include_dependency_modules=include_dependency_modules,
                    include_usage_modules=include_usage_modules,
                    skip_dependencies=not generate_dependencies,
                    skip_usages=not generate_usages,
                    raw=raw,
                )
            )

        if generate_external:
            reports.append(
                external_dependency_report(
                    project_root,
                    Path(path),
                    raw=raw,
                    project_config=project_config,
                )
            )

        print("\n".join(reports))
        sys.exit(0)
    except TachError as e:
        print(f"Report failed: {e}", file=sys.stderr)
        sys.exit(1)


def tach_show(
    project_config: ProjectConfig,
    project_root: Path,
    included_paths: list[Path] | None = None,
    is_web: bool = False,
    is_mermaid: bool = False,
    output_filepath: str | None = None,
):
    logger.info(
        "tach show called",
        extra={
            "data": CallInfo(
                function="tach_show",
                parameters={"is_web": is_web, "is_mermaid": is_mermaid},
            ),
        },
    )

    if is_web and is_mermaid:
        console_err.print(
            "Passing --web generates a remote graph; ignoring '--mermaid' flag.",
            style="yellow",
        )

    if project_config.has_no_modules():
        print_no_modules_found()
        sys.exit(1)

    if project_config.has_no_dependencies():
        print_no_dependencies_found()
        sys.exit(1)
    try:
        included_paths = list(
            map(lambda path: project_root / path, included_paths or [])
        )
        if is_web:
            result = upload_show_report(
                project_root=project_root,
                project_config=project_config,
                included_paths=included_paths,
            )
            if result:
                console_err.print("View your dependency graph here:")
                console_err.print(result)
                sys.exit(0)
            else:
                sys.exit(1)
        else:
            print_show_web_suggestion(is_mermaid=is_mermaid)
            if is_mermaid:
                if output_filepath == "-":
                    print(
                        generate_module_graph_mermaid_string(
                            project_config, included_paths
                        )
                    )
                else:
                    output_path = (
                        Path(output_filepath)
                        if output_filepath
                        else Path(f"{TOOL_NAME}_module_graph.mmd")
                    )
                    generate_module_graph_mermaid(
                        project_config,
                        included_paths=included_paths,
                        output_filepath=output_path,
                    )
                    print_generated_module_graph_file(output_path, is_mermaid=True)
            else:
                if output_filepath == "-":
                    print(
                        generate_module_graph_dot_string(project_config, included_paths)
                    )
                else:
                    output_path = (
                        Path(output_filepath)
                        if output_filepath
                        else Path(f"{TOOL_NAME}_module_graph.dot")
                    )
                    generate_module_graph_dot_file(
                        project_config,
                        included_paths=included_paths,
                        output_filepath=output_path,
                    )
                    print_generated_module_graph_file(output_path)
            sys.exit(0)
    except TachError as e:
        print(f"Failed to show module graph: {e}")
        sys.exit(1)


def tach_test(
    project_config: ProjectConfig,
    project_root: Path,
    head: str,
    base: str,
    disable_cache: bool,
    pytest_args: list[Any],
):
    logger.info(
        "tach test called",
        extra={
            "data": CallInfo(
                function="tach_test",
                parameters={
                    "disable_cache": disable_cache,
                    "pytest_args": pytest_args,
                },
            ),
        },
    )

    if pytest_args and pytest_args[0] != "--":
        console_err.print(
            f"[red]Unknown arguments received. Use '--' to separate arguments for pytest. Ex: '{TOOL_NAME} test -- -v'[/]"
        )
        sys.exit(1)

    try:
        if disable_cache:
            # If cache disabled, just run affected tests and exit
            results = run_affected_tests(
                project_root=project_root,
                project_config=project_config,
                head=head,
                base=base,
                pytest_args=pytest_args[1:],  # Remove '--' pseudo-argument
            )
            sys.exit(results.exit_code)

        # Below this line caching is enabled
        cached_output = check_cache_for_action(
            project_root, project_config, f"tach-test,{head},{base},{pytest_args}"
        )
        if cached_output.exists:
            # Early exit, cached terminal output was found
            console_err.print(
                "============ Cached results found!  ============",
                style="green",
            )
            cached_output.replay()
            console_err.print(
                "============ END Cached results  ============",
                style="green",
            )
            sys.exit(cached_output.exit_code)

        # Cache missed, capture terminal output while tests run so we can update the cache

        results = run_affected_tests(
            project_root=project_root,
            project_config=project_config,
            head=head,
            base=base,
            pytest_args=pytest_args[1:],  # Remove '--' pseudo-argument
        )

        if results.tests_ran_to_completion:
            extension.update_computation_cache(
                project_root,
                cache_key=cached_output.key,
                value=(
                    [
                        *(
                            (1, stdout_line)
                            for stdout_line in results.stdout.split("\n")
                        ),
                        *(
                            (2, stderr_line)
                            for stderr_line in results.stderr.split("\n")
                        ),
                    ],
                    results.exit_code,
                ),
            )
        sys.exit(results.exit_code)
    except TachError as e:
        console_err.print(f"[red]Report failed: {e}[/]")
        sys.exit(1)


def tach_export(
    project_config: ProjectConfig,
    project_root: Path,
    output_path: Path | None = None,
    force: bool = False,
):
    logger.info(
        "tach export called",
        extra={
            "data": CallInfo(
                function="tach_export",
                parameters={"force": force},
            ),
        },
    )

    try:
        export_report(
            project_root=project_root,
            project_config=project_config,
            output_path=output_path,
            force=force,
        )
    except TachError as e:
        print(f"Failed to export modularity report: {e}")
        sys.exit(1)


def tach_upload(
    project_config: ProjectConfig,
    project_root: Path,
    force: bool = False,
):
    logger.info(
        "tach upload called",
        extra={
            "data": CallInfo(
                function="tach_upload",
                parameters={"force": force},
            ),
        },
    )

    try:
        upload_report_to_gauge(
            project_root=project_root,
            project_config=project_config,
            force=force,
        )
    except TachClosedBetaError as e:
        console_err.print(str(e))
        sys.exit(1)
    except TachError as e:
        console_err.print(f"Failed to upload modularity report: {e}")
        sys.exit(1)


def tach_server(
    project_config: ProjectConfig,
    project_root: Path,
) -> None:
    logger.info(
        "tach server called",
        extra={
            "data": CallInfo(function="tach_server"),
        },
    )
    try:
        extension.run_server(project_root, project_config)
    except TachSetupError as e:
        print(f"Failed to setup LSP server: {e}")
        sys.exit(1)


def tach_map(
    project_config: ProjectConfig,
    project_root: Path,
    output_path: str,
    direction: str,
    closure_path: str | None = None,
):
    logger.info(
        "tach map called",
        extra={
            "data": CallInfo(
                function="tach_map",
                parameters={
                    "is_stdout": output_path == "-",
                    "direction": direction,
                    "closure_path": closure_path,
                },
            ),
        },
    )
    try:
        dependent_map = extension.DependentMap(
            project_root,
            project_config,
            Direction.Dependencies
            if direction == "dependencies"
            else Direction.Dependents,
        )

        if closure_path:
            closure_file_path = Path(closure_path).resolve().relative_to(project_root)
            closure = dependent_map.get_closure([closure_file_path])
            output_json = json.dumps(
                {str(closure_file_path): sorted(list(closure))}, indent=2
            )
            if output_path == "-":
                print(output_json)
            else:
                with open(output_path, "w") as f:
                    f.write(output_json)
                console_err.print(
                    f"{icons.SUCCESS} Closure written to '{output_path}'",
                    style="green",
                )
            sys.exit(0)

        if output_path == "-":
            dependent_map.write_to_stdout()
        else:
            dependent_map.write_to_file(Path(output_path))
            console_err.print(
                f"{icons.SUCCESS} Dependency map written to '{output_path}'",
                style="green",
            )
        sys.exit(0)
    except Exception as e:
        print(f"Failed to build dependency map: {e}")
        sys.exit(1)


def tach_init(project_root: Path, force: bool = False):
    logger.info(
        "tach init called",
        extra={"data": CallInfo(function="tach_init")},
    )
    try:
        init_project(project_root, force=force)
    except TachError as e:
        # Error may contain rich markup
        console_err.print(str(e))
        sys.exit(1)


def current_version_is_behind(latest_version: str) -> bool:
    try:
        current_version_parts = list(map(int, __version__.split(".")[:3]))
        latest_version_parts = list(map(int, latest_version.split(".")[:3]))
        return current_version_parts < latest_version_parts
    except Exception:
        return False


def try_parse_project_config(
    project_root: Path,
    *,
    file_name: str = CONFIG_FILE_NAME,
) -> ProjectConfig | None:
    try:
        return parse_project_config(project_root, file_name=file_name)
    except Exception as e:
        print(f"Failed to parse project config: {e}")
        sys.exit(1)


def main(argv: list[str] = sys.argv[1:]) -> None:
    args, parser = parse_arguments(argv)
    project_root = fs.find_project_config_root() or Path.cwd()
    using_custom_config = args.command == "server" and args.config
    config_file_name = CONFIG_FILE_NAME if not using_custom_config else args.config.stem
    if using_custom_config:
        project_root = args.config.parent.resolve()
        project_config = try_parse_project_config(
            project_root, file_name=args.config.stem
        )
    else:
        project_config = try_parse_project_config(project_root)

    if project_config is None or not project_config.disable_logging:
        init_logging(project_root)

    latest_version = cache.get_latest_version(project_root)
    if latest_version and current_version_is_behind(latest_version):
        console_err.print(
            f"WARNING: there is a new {TOOL_NAME} version available"
            f" ({__version__} -> {latest_version}). Upgrade to remove this warning.",
            style="yellow",
        )

    exclude_paths = args.exclude.split(",") if getattr(args, "exclude", None) else None

    # Some commands can run without project config
    if args.command == "mod":
        tach_mod(
            project_root=project_root,
            depth=args.depth,
            exclude_paths=exclude_paths,
        )
        return
    elif args.command == "init":
        tach_init(project_root, force=args.force)
        return
    elif args.command == "install":
        try:
            install_target = InstallTarget(args.target)
        except ValueError:
            print(f"{args.target} is not a valid installation target.")
            sys.exit(1)
        tach_install(project_root=project_root, target=install_target)
        return
    elif args.command == "map":
        tach_map(
            project_config=project_config
            or ProjectConfig(),  # Project config is optional for 'map', affects filesystem visibility
            project_root=project_root,
            output_path=args.output,
            direction=args.direction,
            closure_path=args.closure,
        )

    # All other commands require project config
    if project_config is None:
        print_no_config_found(config_file_name=config_file_name)
        sys.exit(1)

    # Deprecation warnings
    if project_config.use_regex_matching:
        console_err.print(
            "WARNING: regex matching for exclude paths is deprecated. Exclude paths are always interpreted as glob patterns."
            + f"Update your exclude paths in {CONFIG_FILE_NAME}.toml to use glob patterns instead, and remove the 'use_regex_matching' setting."
            + "\n",
            style="yellow",
        )
    if (
        project_config.root_module == "ignore"
        and project_config.has_root_module_reference()
    ):
        console_err.print(
            "WARNING: root module treatment is set to 'ignore' (default as of 0.23.0), but '<root>' appears in your configuration."
            + f"\n\nRun '{TOOL_NAME} sync' to remove the root module from your dependencies,"
            + f" or update 'root_module' in {CONFIG_FILE_NAME}.toml to 'allow' or 'forbid' instead."
            + f"\nDocumentation: {DOCS_BASE_URL}/usage/configuration#the-root-module"
            + "\n",
            style="yellow",
        )

    # Exclude paths on the CLI extend those from the project config
    project_config.exclude = combine_exclude_paths(
        exclude_paths, project_config.exclude
    )

    if args.command == "sync":
        tach_sync(
            project_config=project_config,
            project_root=project_root,
            add=args.add,
        )
    elif args.command == "check":
        if args.dependencies or args.interfaces:
            tach_check(
                project_config=project_config,
                project_root=project_root,
                dependencies=args.dependencies,
                interfaces=args.interfaces,
                exact=args.exact,
                output_format=args.output,
            )
        else:
            tach_check(
                project_config=project_config,
                project_root=project_root,
                exact=args.exact,
                output_format=args.output,
            )
    elif args.command == "check-external":
        tach_check_external(
            project_config=project_config,
            project_root=project_root,
        )
    elif args.command == "report":
        include_dependency_modules = (
            args.dependency_modules.split(",") if args.dependency_modules else None
        )
        include_usage_modules = (
            args.usage_modules.split(",") if args.usage_modules else None
        )
        tach_report(
            project_config=project_config,
            project_root=project_root,
            path=args.path,
            include_dependency_modules=include_dependency_modules,
            include_usage_modules=include_usage_modules,
            dependencies=args.dependencies,
            usages=args.usages,
            external=args.external,
            raw=args.raw,
        )
    elif args.command == "show":
        tach_show(
            project_config=project_config,
            project_root=project_root,
            included_paths=args.included_paths,
            output_filepath=args.out,
            is_web=args.web,
            is_mermaid=args.mermaid,
        )
    elif args.command == "test":
        tach_test(
            project_config=project_config,
            project_root=project_root,
            head=args.head,
            base=args.base,
            disable_cache=args.disable_cache,
            pytest_args=args.pytest_args,
        )
    elif args.command == "export":
        tach_export(
            project_config=project_config,
            project_root=project_root,
            output_path=args.output,
            force=args.force,
        )
    elif args.command == "upload":
        tach_upload(
            project_config=project_config,
            project_root=project_root,
            force=args.force,
        )
    elif args.command == "server":
        tach_server(
            project_config=project_config,
            project_root=project_root,
        )
    else:
        print("Unrecognized command")
        parser.print_help()
        sys.exit(1)


__all__ = ["main"]
