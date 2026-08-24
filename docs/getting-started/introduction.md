# Overview

## What is Tach?

Tach allows you to control dependencies between your Python modules.
Modules can also define an explicit public interface to prevent deep coupling.

This creates a modular architecture, which makes development easier.

If a module tries to import from another module that is not listed as a dependency, Tach will report an error.

When a module has a [public interface](../usage/interfaces.md), any import which does not go through the public interface will cause Tach to report an error.

Dependencies can be additionally marked as ['deprecated'](../usage/deprecate.md). Tach will not report an error but will surface usages of the deprecated dependency.

Tach is a CLI tool, and is ideal for pre-commit hooks and CI checks.

Tach is:

- 🌎 Open source
- 🐍 Installable via pip
- 🔧 Able to be adopted incrementally
- ⚡ Implemented with no runtime impact
- ♾️ Interoperable with your existing systems (cli, hooks, ci, etc.)

## Commands

- [`tach init`](../usage/commands.md#tach-init) - Initialize your project and select module boundaries
- [`tach mod`](../usage/commands.md#tach-mod) - Interactively edit module boundaries.
- [`tach check`](../usage/commands.md#tach-check) - Check that boundaries and interfaces are respected.
- [`tach check-external`](../usage/commands.md#tach-check-external) - Check 3rd party imports match dependencies in your packages.
- [`tach deadcode`](../usage/commands.md#tach-deadcode) - Find Python files which cannot be reached from your entry points.
- [`tach sync`](../usage/commands.md#tach-sync) - Sync constraints with the actual dependencies in your project.
- [`tach show`](../usage/commands.md#tach-show) - Visualize your project's dependency graph.
- [`tach map`](../usage/commands.md#tach-map) - Generate a JSON dependency map between files in your codebase.
- [`tach report`](../usage/commands.md#tach-report) - Generate a dependency report for a file or module.
- [`tach test`](../usage/commands.md#tach-test) - Run only the tests impacted by your changes
- [`tach install`](../usage/commands.md#tach-install) - Install Tach into your development workflow (e.g. pre-commit) 