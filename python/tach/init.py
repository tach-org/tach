from __future__ import annotations

from typing import TYPE_CHECKING

from rich.panel import Panel
from rich.prompt import Confirm

from tach import errors
from tach import filesystem as fs
from tach.console import console
from tach.constants import (
    CONFIG_FILE_NAME,
    DOCS_BASE_URL,
    GAUGE_API_BASE_URL,
    TOOL_NAME,
)
from tach.extension import ProjectConfig, parse_project_config, sync_project
from tach.mod import mod_edit_interactive
from tach.show import upload_show_report

if TYPE_CHECKING:
    from pathlib import Path


def mark_modules(project_root: Path, project_config: ProjectConfig) -> ProjectConfig:
    saved, _ = mod_edit_interactive(
        project_root=project_root,
        project_config=project_config,
        exclude_paths=project_config.exclude,
    )
    if not saved:
        raise errors.TachInitCancelledError()

    project_config_path = fs.build_project_config_path(project_root)
    project_config, _ = parse_project_config(project_config_path)
    return project_config


def sync_modules(project_root: Path, project_config: ProjectConfig) -> ProjectConfig:
    sync_project(project_root, project_config)

    project_config_path = fs.build_project_config_path(project_root)
    project_config, _ = parse_project_config(project_config_path)
    return project_config


def prompt_to_re_select_modules() -> bool:
    console.print(
        Panel(
            "No dependencies found between the selected modules.\n"
            "This might mean that your source root is not set correctly,"
            " or you did not select modules which depend on each other.",
            style="yellow",
        )
    )
    return Confirm.ask(
        "[cyan]Would you like to re-select modules?[/]\n",
        default=True,
        show_default=False,
    )


def prompt_to_show_project() -> bool:
    console.print(
        Panel(
            "Would you like to visualize your dependency graph?\n"
            f"This will upload your module configuration in [cyan]'{CONFIG_FILE_NAME}.toml'[/] to Gauge ([blue underline]{GAUGE_API_BASE_URL}[/])",
            style="yellow",
        )
    )
    return Confirm.ask("", default=False, show_default=False)


def show_project(project_config: ProjectConfig, project_root: Path):
    if prompt_to_show_project():
        show_url = upload_show_report(project_root, project_config, included_paths=[])
        if show_url:
            console.print(
                "\n[cyan]View your dependency graph here:[/]\n"
                f"[blue underline]{show_url}[/]"
            )
        else:
            console.print(
                "[red]Failed to generate show URL. Please try again later.[/]"
            )


def setup_modules(project_root: Path, project_config: ProjectConfig) -> ProjectConfig:
    project_config = mark_modules(project_root, project_config)
    project_config = sync_modules(project_root, project_config)

    while project_config.has_no_dependencies():
        wants_to_re_select = prompt_to_re_select_modules()
        if wants_to_re_select:
            project_config = mark_modules(project_root, project_config)
            project_config = sync_modules(project_root, project_config)
        else:
            console.print("[cyan]Continuing with selected modules.[/]")
            break
    return project_config


def get_all_existing_config_files(project_root: Path) -> list[Path]:
    current_config_path = fs.get_project_config_path(project_root)

    def exclude_self(path: Path) -> bool:
        # This is a quick proxy to exclude Tach's own config files in the venv
        return "/tach/" not in str(path)

    return list(
        filter(
            None,
            (
                current_config_path,
                *filter(exclude_self, project_root.rglob("tach.domain.toml")),
            ),
        ),
    )


def init_project(project_root: Path, force: bool = False):
    config_files = get_all_existing_config_files(project_root)
    if config_files:
        if not force:
            raise errors.TachError(
                f"[yellow]Project already initialized. Use [cyan]`{TOOL_NAME} init --force`[/] to reinitialize.[/]"
            )

        console.print(
            Panel(
                "Project already initialized. Would you like to reinitialize?\n"
                "This will overwrite the current configuration.",
                style="yellow",
            )
        )

        if Confirm.ask("", default=False, show_default=False):
            try:
                for config_file in config_files:
                    config_file.unlink()
            except OSError:
                raise errors.TachError("[red]Failed to remove configuration file.[/]")
        else:
            raise errors.TachError(
                "[red]Refusing to overwrite existing project configuration.[/]"
            )

    console.print(
        Panel(
            "Welcome to Tach! Let's get started by selecting the modules you want to include in your project.\n"
            f"We will use [cyan]'tach mod'[/] to interactively mark which files or folders should be tracked. You can learn more at [blue underline]{DOCS_BASE_URL}/usage/faq[/]",
            style="yellow",
        )
    )
    console.print("\nPress [cyan]'Enter'[/] to get started.", style="cyan")
    console.input()

    project_config = ProjectConfig()

    try:
        project_config = setup_modules(project_root, project_config)
    except errors.TachInitCancelledError:
        console.print("[yellow]Initialization cancelled.[/]")
        return

    show_project(project_config, project_root)

    console.print(
        "\n[green]Tach is now configured for this project!"
        " Run [cyan]'tach check'[/] to validate your configuration.[/]\n\n"
        f"[green]Full documentation is available at [blue underline]{DOCS_BASE_URL}[/]"
    )
