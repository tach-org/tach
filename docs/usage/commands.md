# Commands

## tach init

Tach provides a guided setup process in `tach init`. This command will provide guidance and perform validation,
while walking through `tach mod`, `tach sync` and `tach show`.

New users should start with this command.

```
usage: tach init [-h] [--force]

Initialize a new project

options:
  -h, --help  show this help message and exit
  --force     Force re-initialization if project is already configured.
```

## tach mod

Tach provides an interactive editor for configuring your module boundaries - `tach mod`.

```
usage: tach mod [-h] [-d [DEPTH]] [-e file_or_path,...]

Configure module boundaries interactively

options:
  -h, --help            show this help message and exit
  -d [DEPTH], --depth [DEPTH]
                        The number of child directories to expand from the root
  -e file_or_path,..., --exclude file_or_path,...
                        Comma separated path list to exclude. tests/, ci/, etc.
```

Running `tach mod` will open an editor in your terminal where you can mark your module boundaries.

You can navigate with the arrow keys, mark individual modules with `Enter`, and mark all siblings
as modules with `Ctrl + a`.

You can also mark your Python [source roots](configuration.md#source-roots) by pressing `s`.
This allows Tach to understand module paths and correctly identify first-party imports.

You can mark modules as [utilities](configuration.md#modules) by pressing `u`. This is appropriate for modules like `utils/`, which can be freely used by the rest of the code.

To save your modules, use `Ctrl + s`. Otherwise, to exit without saving, use `Ctrl + c`.

Any time you make changes with `tach mod`, run [`tach sync`](commands.md#tach-sync)
to automatically configure dependency rules.

## tach sync

Tach can automatically sync your project configuration (`tach.toml`) with your project's actual dependencies.

```
usage: tach sync [-h] [--add] [-e file_or_path,...]

Sync constraints with actual dependencies in your project.

options:
  -h, --help            show this help message and exit
  --add                 add all existing constraints and re-sync dependencies.
  -e file_or_path,..., --exclude file_or_path,...
                        Comma separated path list to exclude. tests/, ci/, etc.
```

When this command runs, Tach will analyze the imports in your Python project.

Any undeclared dependencies will be automatically resolved by
adding the corresponding dependencies to your `tach.toml` file.

With `--add`,
any missing dependencies in your `tach.toml` will be added, but does not remove unused dependencies.

When run without the `--add` flag, `tach sync` will remove modules from the `tach.yml` file that do not exist in the project's source roots.

## tach check

Tach will flag any unwanted imports between modules. We recommend you run `tach check` like a linter or test runner, e.g. in pre-commit hooks, on-save hooks, and in CI pipelines.

```
usage: tach check [-h] [--exact] [--dependencies] [--interfaces] [-e file_or_path,...]

Check existing boundaries against your dependencies and module interfaces

options:
  -h, --help            show this help message and exit
  --exact               When checking dependencies, raise errors if any dependencies are unused.
  --dependencies        Check dependency constraints between modules. When present, all checks must be explicitly enabled.
  --interfaces          Check interface implementations. When present, all checks must be explicitly enabled.
  -e file_or_path,..., --exclude file_or_path,...
                        Comma separated path list to exclude. tests/, ci/, etc.
```

Using the `--dependencies` or `--interfaces` flag will limit the checks performed to the respective category.
By default, all checks will be performed.

### Dependency Errors
An error will indicate:

- the file path in which the error was detected
- the module associated with that file
- the module associated with the attempted import

If `--exact` is provided, additional errors will be raised if a dependency exists in `tach.toml` that does not exist in the code.

Example:

```bash
> tach check
❌ tach/check.py[L8]: Cannot import 'tach.filesystem'. Module 'tach' cannot depend on 'tach.filesystem'.
```

NOTE: If your terminal supports hyperlinks, you can click on the failing file path to go directly to the error.


### Interface Errors
An error will indicate:

- the file path in which the error was detected
- the module associated with that file
- the module associated with the attempted import
- the non-public member associated with the attempted import

Example:

```bash
❌  tach/mod.py[L13]: Module 'tach.interactive' has a defined public interface. Only imports from the public interface of this module are allowed. The import 'tach.interactive.get_selected_modules_interactive' (in module 'tach.mod') is not public.
```

NOTE: If your terminal supports hyperlinks, you can click on the failing file path to go directly to the error.

## tach check-external

Tach can validate that the external imports in your Python packages match your declared package dependencies in `pyproject.toml` or `requirements.txt`.

```
usage: tach check-external [-h] [-e file_or_path,...]

Perform checks related to third-party dependencies

options:
  -h, --help  show this help message and exit
  -e file_or_path,..., --exclude file_or_path,...
                        Comma separated path list to exclude. tests/, ci/, etc.
```

For all Python files in each [source root](configuration.md#source-roots), Tach will determine which package it belongs to,
and compare its dependencies to those declared in `pyproject.toml` or `requirements.txt`.
Tach will report an error for any external import which is not satisfied by the declared dependencies.

This also means that, for monorepos which contain multiple Python packages, Tach will detect when an import comes from a source root in another package,
and verify that this dependency is declared. Make sure to configure [`source_roots`](configuration.md#source-roots) for every package (globs are coming soon!).

This is typically useful if you are developing more than one Python package from a single virtual environment.
Although your local environment may contain the dependencies for all your packages, when an end-user installs each package they will only install the dependencies listed in the `pyproject.toml`.

This means that, although tests may pass in your shared environment, an invalid import can still cause errors at runtime for your users.

In case you would like to explicitly allow a certain external module, this can be configured in your [`tach.toml`](configuration.md#external)

!!! note
    It is recommended to run Tach within a virtual environment containing all of
    your dependencies across all packages. This is because Tach uses the
    distribution metadata to map module names like 'git' to their distributions
    ('GitPython').

## tach report

Tach can generate a report showing all the dependencies and usages of a given module.

```
usage: tach report [-h] [--dependencies] [--usages] [--external] [-d module_path,...] [-u module_path,...] [--raw] [-e file_or_path,...] path

Create a report of dependencies and usages.

positional arguments:
  path                  The path or directory path used to generate the report.

options:
  -h, --help            show this help message and exit
  --dependencies        Generate dependency report. When present, all reports must be explicitly enabled.
  --usages              Generate usage report. When present, all reports must be explicitly enabled.
  --external            Generate external dependency report. When present, all reports must be explicitly enabled.
  -d module_path,..., --dependency-modules module_path,...
                        Comma separated module list of dependencies to include [includes everything by default]
  -u module_path,..., --usage-modules module_path,...
                        Comma separated module list of usages to include [includes everything by default]
  --raw                 Group lines by module and print each without any formatting.
  -e file_or_path,..., --exclude file_or_path,...
                        Comma separated path list to exclude. tests/, ci/, etc.
```

By default, this will generate a textual report showing the file and line number of each module dependency, module usage, and external dependency. Each section corresponds to a command line flag.

The given `path` can be a directory or a file path. The [module](configuration.md#modules) which contains the given path will be used to determine which imports to include in the report.
Generally, if an import points to a file which is contained by a different module, it will be included.

The `--dependencies` flag includes module dependencies, meaning any import which targets a different module within your project. For example, if `core.api` and `core.services` are marked as modules,
then an import of `core.api.member` from within `core.services` would be included in a report for `core/services`.

The `--usages` flag includes module usages, meaning any import which comes from a different module within your project. For example, if `core.api` and `core.services` are marked as modules,
then an import of `core.services.member` from within `core.api` would be included in a report for `core/services`.

The `--external` flag includes external (3rd party) dependencies, meaning any import which targets a module outside of your project. For example, importing `pydantic` or `tomli` would be included in this report.

!!! note
    It is recommended to run Tach within a virtual environment containing all of
    your dependencies across all packages. This is because Tach uses the
    distribution metadata to map 3rd party module names like 'git' to their distributions ('GitPython').

Supplying the `--raw` flag will group the results by module name and eliminate formatting, making the output more easily machine-readable.

## tach show

Tach will generate a visual representation of your dependency graph!

```
usage: tach show [-h] [--web] [--mermaid] [-o [OUT]] [included_paths ...]

Visualize the dependency graph of your project.

positional arguments:
  included_paths        Paths to include in the module graph. If not provided, the entire project is
                        included.

options:
  -h, --help            show this help message and exit
  --web                 Open your dependency graph in a remote web viewer.
  --mermaid             Generate a mermaid.js graph instead of a DOT file.
  -o [OUT], --out [OUT]
                        Specify an output path for a locally generated module graph file.
```

These are the results of `tach show --web` on the Tach codebase itself:
![tach show](../assets/tach_show.png)

## tach map

Tach can generate a JSON dependency map showing the relationships between files in your project.
```
usage: tach map [-h] [-o OUTPUT] [--direction {dependencies,dependents}] [--closure CLOSURE]

Build a dependency map and write it to a file or stdout

options:
  -h, --help            show this help message and exit
  -o OUTPUT, --output OUTPUT
                        Output file path. Use '-' for stdout (default: '-')
  --direction {dependencies,dependents}
                        Direction of the map (default: 'dependencies')
  --closure CLOSURE     Get the closure for a specific file path
```

By default, `tach map` outputs to stdout and shows dependencies. The output is a JSON object where each key is a file path and its value is an array of file paths it depends on.

Example output:
```json
{
  "src/core.py": ["src/utils.py", "src/config.py"],
  "src/utils.py": [],
  "src/config.py": ["src/utils.py"]
}
```

This map is particularly useful for build tools, test runners, and development servers that need to understand file dependencies.

For example, it can help with test selection by identifying affected files, or support hot-reloading by finding all files that need to be reloaded when a dependency changes.

### With jq
You can use [`jq`](https://jqlang.org/download/) to query this output. Here are some useful examples:

```bash
# Get dependencies for a specific file
tach map | jq '."src/core.py"'

# Find all files that depend on utils.py (using dependents direction)
tach map --direction dependents | jq '."src/utils.py"'

# Count dependencies for each file
tach map | jq 'map_values(length)'

# Find files with no dependencies
tach map | jq 'to_entries | map(select(.value | length == 0)) | map(.key)'
```

### Closures
The `--closure` flag can be used to find all transitive dependencies for a specific file path. For example:

```bash
# Get all direct and indirect dependencies of core.py
tach map --closure src/core.py
```

Example output with closure:
```json
[
  "src/core.py",
  "src/utils.py",
  "src/config.py",
  "src/constants.py"
]
```

The output includes the target file itself and all files that are either directly or indirectly required by it. In this example, if `src/core.py` imports `config.py` which in turn imports `constants.py`, all of these files will appear in the closure.


## tach test

Tach also functions as an intelligent test runner.

```
usage: tach test [-h] [--base [BASE]] [--head [HEAD]] [--disable-cache] ...
Run tests on modules impacted by the current changes.
positional arguments:
  pytest_args      Arguments forwarded to pytest. Use '--' to separate
                   these arguments. Ex: 'tach test -- -v'
options:
  -h, --help       show this help message and exit
  --base [BASE]    The base commit to use when determining which modules
                   are impacted by changes. [default: 'main']
  --head [HEAD]    The head commit to use when determining which modules
                   are impacted by changes. [default: current filesystem]
  --disable-cache  Do not check cache for results, and
                   do not push results to cache.
```

Using `pytest`, running `tach test` will perform [impact analysis](https://martinfowler.com/articles/rise-test-impact-analysis.html) on the changes between your current filesystem and your `main` branch to determine which test files need to be run.
This can dramatically speed up your test suite in CI, particularly when you make a small change to a large codebase.
This command also takes advantage of Tach's [computation cache](caching.md).

### Using the pytest plugin directly

When tach is installed, the pytest plugin is automatically loaded. Just run pytest normally:

```bash
pytest
```

By default, the plugin runs all tests but reports how many could be skipped based on impact analysis. This allows you to see the potential benefit without committing to skipping tests:

```
[Tach] 42 tests in 8 files unaffected by changes (~15.3s could be saved). Skip with: pytest --tach
```

To actually skip unaffected tests, provide the `--tach` flag:

```bash
pytest --tach
```

The plugin auto-detects whether your default branch is `main` or `master`.

**Options:**

- `--tach`: Enable test skipping using the auto-detected base branch
- `--tach-base <commit>`: Set the base commit explicitly (also enables skipping)
- `--tach-head <commit>`: Head commit to compare against (also enables skipping. default: current filesystem)
- `--tach-verbose`: Show detailed output including changed files and skipped/would-skip test paths

To disable the plugin entirely, use pytest's built-in plugin disabling:

```bash
pytest -p no:tach
```

You can also disable it permanently in `pyproject.toml`:

```toml
[tool.pytest]
addopts = ["-p", "no:tach"]
```

#### Duration estimation

The plugin caches test durations and estimates time saved when skipping tests. After running your test suite once, subsequent runs will show estimated time saved:

```
[Tach] Skipped 42 tests (5 files) (~12.3s saved) - unaffected by current changes.
```

#### Validating impact analysis

By default (without `--tach`), the plugin runs all tests but tracks which would be skipped. If any "would-be-skipped" test fails, you'll see a warning:

```
[Tach] WARNING: 2 test(s) failed that would be skipped by impact analysis!
[Tach] These failures would be missed when using --tach:
[Tach]   - test_module.py::test_that_unexpectedly_failed
```

This helps validate impact analysis accuracy before enabling test skipping in CI.

#### Using in CI

Most CI systems use shallow clones by default. To enable impact analysis, ensure the base branch is fetched (e.g., `git fetch origin main:main` or configure a full clone). If the base branch is unavailable, the plugin disables itself and all tests run normally.

## tach install

Tach can be installed into your development workflow automatically as a pre-commit hook.

### With pre-commit framework

If you use the [pre-commit framework](https://github.com/pre-commit/pre-commit), you can add the following to your `.pre-commit-hooks.yaml`:

```yaml
repos:
  - repo: https://github.com/gauge-sh/tach-pre-commit
    rev: v0.30.0 # change this to the latest tag!
    hooks:
      - id: tach
```

Note that you should specify the version you are using in the `rev` key.

### Standard install

If you don't already have pre-commit hooks set up, you can run:

```bash
tach install pre-commit
```

The command above will install `tach check` as a pre-commit hook, directly into `.git/hooks/pre-commit`.

If that file already exists, you will need to manually add `tach check` to your existing `.git/hooks/pre-commit` file.
