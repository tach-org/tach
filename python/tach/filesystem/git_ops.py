from __future__ import annotations

import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from tach.errors import TachError, TachSetupError

if TYPE_CHECKING:
    from git import Repo


@dataclass
class GitBranchInfo:
    repo: str
    name: str
    commit: str
    owner: str
    email: str


def is_github_actions():
    return os.environ.get("GITHUB_ACTIONS") == "true"


def _get_branch_name(repo: Repo) -> str:
    # GHA uses a detached HEAD / shallow clone in actions/checkout@v4
    if is_github_actions():
        event_name = os.environ.get("GITHUB_EVENT_NAME")
        # Different environment variables are used for PRs and pushes
        if event_name == "pull_request":
            return os.environ["GITHUB_HEAD_REF"]
        return os.environ["GITHUB_REF_NAME"]
    else:
        return repo.active_branch.name


def _get_commit(repo: Repo) -> str:
    # GHA uses a detached HEAD / shallow clone in actions/checkout@v4
    if is_github_actions():
        event_name = os.environ.get("GITHUB_EVENT_NAME")
        event_path = os.environ.get("GITHUB_EVENT_PATH")
        if event_path and event_name == "pull_request":
            with open(event_path) as f:
                event_info = json.load(f)
                # Pull commit from the PR event in GHA
                return event_info["pull_request"]["head"]["sha"]
    return repo.head.commit.hexsha


def _get_owner_and_repo_name(repo_url: str) -> tuple[str, str]:
    """
    Parse a git URL to extract owner and repo names, handling cases where
    either might contain slashes.

    Strategy:
    1. For HTTPS URLs, everything after the domain and before the last segment is the owner
    2. For SSH URLs, everything after the colon and before the last segment is the owner
    3. The last segment (minus .git) is always the repo name

    Args:
        url: Git remote URL (HTTPS or SSH format)

    Returns:
        Tuple of (owner, repo)

    Examples:
        >>> _get_owner_and_repo_name("https://github.com/apache/spark/spark-connector.git")
        ('apache/spark', 'spark-connector')
        >>> _get_owner_and_repo_name("git@github.com:facebook/react-native/docs.git")
        ('facebook/react-native', 'docs')
    """
    # Remove .git suffix and slashes
    url = repo_url.strip("/")

    if url.endswith(".git"):
        url = url[:-4].strip("/")

    if url.startswith("https://"):
        # HTTPS URL format
        # Remove https:// prefix and split by /
        parts = url[8:].split("/")
        # First part is domain (e.g., github.com)
        # Last part is repo name
        repo = parts[-1]
        # Everything between domain and repo is owner
        owner = "/".join(parts[1:-1])
    else:
        # SSH URL format
        # Split at the colon
        parts = url.split(":")
        if len(parts) != 2:
            raise TachError(f"Invalid SSH URL format: {url}")
        path_parts = parts[1].split("/")
        # Last part is repo name
        repo = path_parts[-1]
        # Everything else is owner
        owner = "/".join(path_parts[:-1])

    if not owner or not repo:
        raise TachError(f"Failed to parse owner or repo from URL: {url}")

    return owner, repo


def get_current_branch_info(
    project_root: Path, allow_dirty: bool = False
) -> GitBranchInfo:
    # Local import because git-python takes ~80ms to load
    from git import InvalidGitRepositoryError, NoSuchPathError, Repo

    try:
        repo = Repo(project_root, search_parent_directories=True)
    except (InvalidGitRepositoryError, NoSuchPathError):
        raise TachSetupError("The project does not appear to be a git repository!")

    # Fail if the branch is not clean
    if not allow_dirty and repo.is_dirty():
        raise TachError(
            "The current branch is not clean, please commit your changes before running this command!"
        )

    try:
        url = repo.remotes.origin.url
        owner_name, repo_name = _get_owner_and_repo_name(url)
        config_reader = repo.config_reader()
        email = str(config_reader.get_value("user", "email", default=""))
        branch = _get_branch_name(repo)
        commit = _get_commit(repo)

    except Exception as e:
        raise TachError(f"Failed to determine current branch information!\nError: {e}")

    return GitBranchInfo(
        repo=repo_name,
        owner=owner_name,
        name=branch,
        commit=commit,
        email=email,
    )


def get_changed_files(
    project_root: Path, head: str | None = None, base: str = "main"
) -> list[Path]:
    # Local import because git-python takes ~80ms to load
    from git import GitCommandError, InvalidGitRepositoryError, NoSuchPathError, Repo

    try:
        repo = Repo(project_root, search_parent_directories=True)
    except (InvalidGitRepositoryError, NoSuchPathError):
        raise TachSetupError(
            "The project does not appear to be a git repository, cannot determine changed files!"
        )

    try:
        if head:
            diff: str = repo.git.diff("--name-status", head, base)
        else:
            # If head is not provided, we can diff against 'base' from the current filesystem
            diff: str = repo.git.diff("--name-status", base)
    except GitCommandError:
        head_display = f"'{head}'" if head else "current filesystem"
        raise TachError(f"Failed to check diff between '{base}' and {head_display}!")

    diff_lines = diff.splitlines()
    changed_files: set[str] = set()

    for line in diff_lines:
        _, *files = line.split("\t")
        changed_files.update(files)

    if not head:
        # If we are using the current filesystem, there may be relevant changes in untracked files
        untracked_files: str = repo.git.ls_files("--others", "--exclude-standard")
        changed_files.update(untracked_files.splitlines())

    # return list of unique Paths
    git_root: str = repo.git.rev_parse("--show-toplevel")
    return [(Path(git_root) / filepath).resolve() for filepath in changed_files]


__all__ = ["get_changed_files", "get_current_branch_info"]
