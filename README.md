# detach

> [!IMPORTANT]
> this is a fork of [tach](https://github.com/gauge-sh/tach), which was abandoned. pypi release & updated docs coming soon. for now you can get wheels from [releases](https://github.com/DetachHead/detach/releases)

[![downloads](https://static.pepy.tech/badge/tach/month)](https://pepy.tech/project/tach)
[![version](https://img.shields.io/pypi/v/tach.svg)](https://pypi.Python.org/pypi/tach)
[![license](https://img.shields.io/pypi/l/tach.svg)](https://pypi.Python.org/pypi/tach)
[![python](https://img.shields.io/pypi/pyversions/tach.svg)](https://pypi.Python.org/pypi/tach)
[![ci](https://github.com/gauge-sh/tach/actions/workflows/ci.yml/badge.svg)](https://github.com/gauge-sh/tach/actions/workflows/ci.yml)
[![pyright](https://microsoft.github.io/pyright/img/pyright_badge.svg)](https://microsoft.github.io/pyright/)
[![ruff](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/ruff/main/assets/badge/v2.json)](https://github.com/astral-sh/ruff)

Tach is a Python tool to enforce dependencies and interfaces, written in Rust.

Tach is inspired by the [modular monolith](https://www.milanjovanovic.tech/blog/what-is-a-modular-monolith) architecture.

[Docs](https://docs.gauge.sh)

<div align="center">
    <img src="docs/assets/light_logo.png" alt="gauge-logo" width="30%"  style="border-radius: 50%; padding-bottom: 20px"/>
</div>

https://github.com/user-attachments/assets/11eec4a1-f80a-4f13-9ff3-91a9760133b6



Tach can enforce:

- 📋 Imports only come from [declared dependencies](https://docs.gauge.sh/usage/configuration#modules)
- 🤝 Cross-module calls use the [public interface](https://docs.gauge.sh/usage/configuration#interfaces)
- ⛓️‍💥 [No cycles](https://docs.gauge.sh/usage/configuration) in the dependency graph


Tach is:

- 🌎 Open source
- 🐍 Installable via pip
- 🔧 Able to be adopted incrementally
- ⚡ Implemented with no runtime impact
- ♾️ Interoperable with your existing systems

## Getting Started

### Installation

```bash
pip install tach
```

### Setup

Tach will guide you through initial project setup.

Run:
```bash
 tach init
```

After an introductory message, you will see a file tree interface allowing you to interactively configure your project.

Use the arrow keys to navigate, and mark each module boundary with 'Enter'. You can mark all of your top-level Python packages, or just a few that you want to track.

FAQ: [What is a module?](https://docs.gauge.sh/usage/faq#what-is-a-module%3F)

If your Python code lives below your project root, or if you are working in a monorepo with multiple Python packages, mark your Python [source roots](https://docs.gauge.sh/usage/configuration#source-roots) using the 's' key.

### Enforcement

Tach comes with a cli command to enforce the boundaries that you just set up! From the root of your Python project, run:

```bash
tach check
```

You will see:

```bash
✅ All modules validated!
```

You can validate that Tach is working by:

1. Removing an item from the `depends_on` key in `tach.toml`, or marking it as [deprecated](https://docs.gauge.sh/usage/deprecate)
2. By adding an import between modules that didn't previously import from each other.

Give both a try and run `tach check` again. This will generate an error:

```bash
❌ tach/check.py[L8]: Cannot use 'tach.filesystem'. Module 'tach' cannot depend on 'tach.filesystem'.
```

Each error indicates an import which violates your dependencies. If your terminal supports hyperlinks, click on the file path to go directly to the error.

When an error is detected, `tach check` will exit with a non-zero code. It can be easily integrated with CI/CD, [Pre-commit hooks](https://docs.gauge.sh/usage/commands#tach-install), and [VS Code](https://marketplace.visualstudio.com/items?itemName=Gauge.tach), and more!

### Extras

Visualize your dependency graph.

```bash
tach show [--web]
```

Tach will generate a graph of your dependencies. Here's what this looks like for Tach:

![tach show](docs/assets/tach_show.png)

Note that this graph is generated remotely with the contents of your `tach.toml` when running `tach show --web`.

If you would like to use the [GraphViz DOT format](https://graphviz.org/about/) locally, simply running `tach show` will generate `tach_module_graph.dot` in your working directory.

You can view the dependencies and usages for a given path:

```bash
tach report my_package/
# OR
tach report my_module.py
```

e.g.:

```bash
> tach report python/tach/filesystem
[Dependencies of 'python/tach/filesystem']
python/tach/filesystem/install.py[L6]: Import 'tach.hooks.build_pre_commit_hook_content'
python/tach/filesystem/project.py[L5]: Import 'tach.constants.CONFIG_FILE_NAME'
...
-------------------------------
[Usages of 'python/tach/filesystem']
python/tach/cache/access.py[L8]: Import 'tach.filesystem.find_project_config_root'
python/tach/cache/setup.py[L7]: Import 'tach.filesystem.find_project_config_root'
...
```

Tach also supports:

- [Public interfaces for modules](https://docs.gauge.sh/usage/interfaces/)
- [Deprecating individual dependencies](https://docs.gauge.sh/usage/deprecate)
- [Layered architecture](https://docs.gauge.sh/usage/layers)
- [Incremental adoption](https://docs.gauge.sh/usage/unchecked-modules)
- [Generating JSON dependency maps](https://docs.gauge.sh/usage/commands#tach-map)
- [Monorepos and namespace packages](https://docs.gauge.sh/usage/configuration#source-roots)
- [Domain ownership](https://docs.gauge.sh/usage/configuration#tach-domain-toml)
- [Inline 'ignore' comments](https://docs.gauge.sh/usage/tach-ignore)
- [Pre-commit hooks](https://docs.gauge.sh/usage/commands#tach-install)

More info in the [docs](https://docs.gauge.sh/). Tach logs anonymized usage statistics which can be [opted out](https://docs.gauge.sh/usage/faq/) of.
If you have any feedback, we'd love to talk!

If you have any questions or run into any issues, let us know by submitting a [Github Issue](https://github.com/gauge-sh/tach/issues)!

---

### Contributors

<a href="https://github.com/gauge-sh/tach/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=gauge-sh/tach" />
</a>
