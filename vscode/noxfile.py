# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.
"""All the action we need during build"""

from __future__ import annotations

import json
import os
import pathlib
import urllib.request as url_lib

import nox


def _install_bundle(session: nox.Session) -> None:
    session.run(
        "rm",
        "-rf",
        "./bundled/libs",
        external=True,
    )
    session.install(
        "-v",
        "--target",
        "./bundled/libs",
        "..",
    )


def _check_files(names: list[str]) -> None:
    root_dir = pathlib.Path(__file__).parent
    for name in names:
        file_path = root_dir / name
        lines: list[str] = file_path.read_text().splitlines()
        if any(line for line in lines if line.startswith("# TODO:")):
            raise Exception(f"Please update {os.fspath(file_path)}.")


def _update_pip_packages(session: nox.Session) -> None:
    session.run(
        "uv",
        "pip",
        "compile",
        "--generate-hashes",
        "--resolver=backtracking",
        "--upgrade",
        "./requirements.in",
    )
    session.run(
        "uv",
        "pip",
        "compile",
        "--generate-hashes",
        "--resolver=backtracking",
        "--upgrade",
        "./src/test/python_tests/requirements.in",
    )


def _get_package_data(package):
    json_uri = f"https://registry.npmjs.org/{package}"
    with url_lib.urlopen(json_uri) as response:
        return json.loads(response.read())


def _update_npm_packages(session: nox.Session) -> None:
    pinned = {
        "vscode-languageclient",
        "@types/vscode",
        "@types/node",
    }
    package_json_path = pathlib.Path(__file__).parent / "package.json"
    package_json = json.loads(package_json_path.read_text(encoding="utf-8"))

    for package in package_json["dependencies"]:
        if package not in pinned:
            data = _get_package_data(package)
            latest = "^" + data["dist-tags"]["latest"]
            package_json["dependencies"][package] = latest

    for package in package_json["devDependencies"]:
        if package not in pinned:
            data = _get_package_data(package)
            latest = "^" + data["dist-tags"]["latest"]
            package_json["devDependencies"][package] = latest

    # Ensure engine matches the package
    if (
        package_json["engines"]["vscode"]
        != package_json["devDependencies"]["@types/vscode"]
    ):
        print(
            "Please check VS Code engine version and @types/vscode version in package.json."
        )

    new_package_json = json.dumps(package_json, indent=4)
    # JSON dumps uses \n for line ending on all platforms by default
    if not new_package_json.endswith("\n"):
        new_package_json += "\n"
    package_json_path.write_text(new_package_json, encoding="utf-8")
    session.run("npm", "install", external=True)


def _setup_template_environment(session: nox.Session) -> None:
    session.install("wheel", "pip-tools")
    session.run("uv", "sync", "--active", "--group", "vscode")
    _install_bundle(session)


@nox.session()
def setup(session: nox.Session) -> None:
    """Sets up the template for development."""
    _setup_template_environment(session)


@nox.session()
def tests(session: nox.Session) -> None:
    """Runs all the tests for the extension."""
    session.run("pytest", "-svv", "src/test/python_tests")


@nox.session()
def lint(session: nox.Session) -> None:
    # check typescript code
    session.run("npm", "run", "lint", external=True)


@nox.session()
def build_package(session: nox.Session) -> None:
    """Builds VSIX package for publishing."""
    _check_files(["README.md", "LICENSE", "SECURITY.md", "SUPPORT.md"])
    _setup_template_environment(session)
    session.run("npm", "install", external=True)
    session.run("npm", "run", "vsce-package", external=True)


@nox.session()
def update_packages(session: nox.Session) -> None:
    """Update pip and npm packages."""
    session.install("wheel", "pip-tools")
    _update_pip_packages(session)
    _update_npm_packages(session)
