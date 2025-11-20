# Tach

Tach is a tool that helps you control dependencies between your Python modules. It allows modules to define explicit public interfaces to prevent deep coupling, creating a more modular architecture.

## Key Features

- 🌎 Open source
- 🐍 Installable via pip
- 🔧 Able to be adopted incrementally
- ⚡ Implemented with no runtime impact
- ♾️ Interoperable with your existing systems (cli, hooks, ci, etc.)

## How it Works

Tach checks that no module imports from another module that is not listed as a dependency. When a module has a public interface, any import which does not go through the public interface will cause Tach to report an error.

Dependencies can be additionally marked as 'deprecated', in which case Tach will surface their usage without reporting an error.

## Core Commands

- [`tach init`](usage/commands.md#tach-init) - Interactively define module boundaries.
- [`tach check`](usage/commands.md#tach-check) - Check that boundaries and interfaces are respected.
- [`tach sync`](usage/commands.md#tach-sync) - Sync constraints with the actual dependencies in your project.
- [`tach show`](usage/commands.md#tach-show) - Visualize your project's dependency graph.

## Getting Started

Ready to get started with Tach? Check out our [Getting Started Guide](getting-started/getting-started.md) or the [Overview](getting-started/introduction.md) to learn more. 