# Getting Started

## Installation

#### uv (recommended)
```bash
uv add --dev dtach
```

#### pip
```bash
pip install dtach
```

## Quick Start

### 1. Initialize Your Project

```bash
tach init
```

This will guide you through setting up your module boundaries and will create a `tach.toml` file in your project root.

### 2. Define Module Boundaries

Use the interactive module editor to define your module boundaries:

```bash
tach mod
```

This opens an interactive terminal UI where you can navigate and mark your module boundaries:

- Use arrow keys to navigate the file tree
- Press `Enter` to mark/unmark a module
- Press `s` to mark a directory as a source root
- Press `u` to mark a module as a utility module (can be used anywhere)
- Press `Ctrl+a` to mark all siblings as modules
- Press `Ctrl+s` to save
- Press `Ctrl+c` to exit without saving

### 3. Sync Dependencies

Once your module boundaries are defined, sync your dependency rules with your actual code:

```bash
tach sync
```

This will analyze your codebase and automatically add dependency rules to your `tach.toml` file based on your actual imports.

### 4. Check Boundaries

Now you can check if your module boundaries are respected:

```bash
tach check
```

This command will report any violations of your module boundaries or interfaces.

### 5. Visualize Dependencies

To see a visualization of your module dependencies:

```bash
tach show
```

This will generate a graphical representation of your module dependencies.

## Integrating with Your Development Workflow

### Pre-commit Hook

You can add Tach to your pre-commit hooks to automatically check your module boundaries on each commit:

```bash
tach install --pre-commit
```

Or manually add it to your `.pre-commit-config.yaml`:

```yaml
-   repo: local
    hooks:
    -   id: tach
        name: tach
        entry: tach check
        language: system
        pass_filenames: false
```

### CI Pipeline

Add Tach to your CI pipeline to ensure your module boundaries are respected:

```yaml
- name: Check module boundaries
  run: tach check
```

## Next Steps

- Learn more about [Configuration](../usage/configuration.md)
- Explore the [Commands](../usage/commands.md) in detail
- Define [Public Interfaces](../usage/interfaces.md) for your modules
- Set up [Layers](../usage/layers.md) to enforce architectural patterns 