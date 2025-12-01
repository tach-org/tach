# Configuration

Aside from running `tach mod` and `tach sync`, you can configure Tach by creating or modifying the configuration file as described below.

## `tach.toml`

This is the project-level configuration file which should be in the root of your project.

`modules` defines the modules in your project - [see details](#modules).

`interfaces` defines the interfaces of modules in your project (optional) - [see details](#interfaces).

`layers` defines the layers of modules in your project (optional) - [see details](#layers).

`exclude` accepts a list of directory patterns to exclude from checking. These should be glob paths which match from the beginning of a given file path. For example: `project/*.tests` would match any path beginning with `project/` and ending with `.tests`.

!!! note
    Tach uses forward slashes to match path separators, even on Windows.

`ignore_type_checking_imports` (default: **true**) is a flag which silences `tach check` failures caused by imports under a `TYPE_CHECKING` conditional block.

`exact` (default: **false**) is a flag which causes `tach check` to fail if any declared dependencies are found to be unused.

`forbid_circular_dependencies` (default: **false**) is a flag which causes `tach check` to fail if any circular dependencies are detected.

`respect_gitignore` (default: **true**) is a flag which causes Tach to exclude files and directories matched by `.gitignore`. Possible values:

- `true` - always respect `.gitignore` files
- `"if_git_repo"` - only respect a `.gitignore` file if it's part of the git repository associated with the directory tach is executed in, otherwise ignore it.
- `false` - never respect `.gitignore` files

`root_module` takes a string enum value, and determines how Tach treats code which lives within the project but is not covered by an explicit module. This is described in detail [below](#the-root-module)

`rules` allows precise configuration of the severity of certain types of issues. See [below](#rules) for more details.

```toml
exclude = [
    "**/*__pycache__",
    "build/",
    "dist/",
    "docs/",
    "python/tests/",
    "tach.egg-info/",
    "venv/",
]
source_roots = ["python"]
exact = true
ignore_type_checking_imports = true
forbid_circular_dependencies = true

layers = [
  "ui",
  "commands",
  "core"
]

[[modules]]
path = "tach"
depends_on = []

[[modules]]
path = "tach.__main__"
layer = "ui"

[[modules]]
path = "tach.errors"
depends_on = []
utility = true

[[modules]]
path = "tach.parsing"
depends_on = ["tach", "tach.filesystem"]
layer = "core"
visibility = ["tach.check"]

[[modules]]
path = "tach.check"
depends_on = [
    "tach.extension",
    "tach.filesystem",
    "tach.parsing",
]
layer = "commands"

[[interfaces]]
expose = ["types.*"]

[[interfaces]]
expose = [
    "parse_project_config",
    "dump_project_config_to_toml",
]
from = [
    "tach.parsing",
]

...

[cache]
file_dependencies = ["python/tests/**", "src/*.rs"]

[external]
exclude = ["pytest"]

[rules]
unused_ignore_directives = "warn"
```

## Modules

Each module listed under the `modules` key above can accept the following attributes:

- `path` the Python import path to the module (e.g. `a.b` for `<root>/a/b.py`)

    !!! note
        Glob patterns are allowed. The pattern `"libs.**"` would define the default configuration for any module under the `libs` namespace.<br/><br/>This can be overridden for specific modules by defining them later in the file using a concrete pattern like `"libs.module"`.

    !!! note
        A module can also define `paths` as a shorthand for multiple module definitions. This allows specifying allowed dependencies and other attributes as a group. Example: `paths = ["a.b", "a.c"]`

- `depends_on` a list of module paths which this module can import from

    !!! note
        Glob patterns are allowed. The pattern `"libs.**"` would allow dependencies on any module under the `libs` namespace.

    !!! note
        Omitting the `depends_on` field means the module will be allowed to import from any other module. However, it will still be subject to those modules' [public interfaces](#interfaces).

- `cannot_depend_on` a list of module paths which this module cannot import from

    !!! note
        This takes precedence over `depends_on`. In other words, if `cannot_depend_on = ["module"]`, then `depends_on = ["module"]` will have no effect.

- `layer` (optional) the [**layer**](#layers) which holds this module
- `visibility` (optional) a list of other modules which can import from this module
- `utility` (default: `false`) marks this module as a **Utility**, meaning all other modules may import from it without declaring an explicit dependency
- `unchecked` (default: `false`) marks this module as [**unchecked**](unchecked-modules.md), meaning Tach will not check its imports

!!! note
    Tach also supports [deprecating individual dependencies](deprecate.md).

## Interfaces

Public interfaces are defined separately from modules, and define the imports that are allowed from that module.

For example, if a module should expose everything from a nested 'services' folder, the config would look like:

```toml
[[interfaces]]
expose = ["services.*"]
from = ["my_module"]
```

More specifically:

- `expose`: a list of regex patterns which define the public interface
- `from` (optional): a list of regex patterns which define the modules which adopt this interface
- `visibility` (optional): a list of modules which can use this interface
- `exclusive` (default: `false`): when paired with `visibility`, requires that matching modules use _only_ this interface

[More details here.](interfaces.md)

!!! note
    If an interface entry does not specify `from`, all modules will adopt the interface.

!!! note
    A module can match multiple interface entries - if an import matches _any_ of the entries, it will be considered valid.

## Layers

An ordered list of layers can be configured at the top level of `tach.toml`,
and [modules](#modules) can each be assigned to a specific layer.

```toml
layers = [
  "ui",
  "commands",
  "core"
]

[[modules]]
path = "tach.check"
layer = "commands"

[[modules]]
path = "tach.cache"
layer = "core"
```

The configuration above defines three layers, with `ui` being the highest layer, and `core` being the lowest layer.
It also tags `tach.check` as a module in the `commands` layer, and `tach.cache` in `core`.

[More details here.](layers.md)


## The Root Module

By default, Tach checks all of the source files beneath all of the configured [source roots](#source-roots), and will ignore dependencies which are not contained by [modules](#modules).

However, Tach allows configuration of how to treat code which is within a source root, but not contained by a module.

For example, given the file tree below:

```
my_repo/
  tach.toml
  script.py
  lib/
    module1.py
    module2/
      __init__.py
      service.py
    module3.py
  docs/
  tests/
```

If `lib.module1`, `lib.module2`, and `lib.module3` are the only configured modules, then the code in `script.py` would be automatically part of the `<root>` module.

This module can declare its own dependencies with `depends_on` and use the rest of the available module configuration.
Further, other modules would need to declare an explicit dependency on `<root>` to use code which rolls up to the root.

Tach allows configuring how the root module should be treated through the `root_module` key in `tach.toml`. It may take one of the following values:

- **(permissive default)** `"ignore"`: Disable all checks related to the `<root>` module. `tach check` will never fail due to code in the `<root>` module, and `tach sync` will never add `<root>` to `tach.toml`
- **(stricter)** `"allow"`: Treat `<root>` as a catch-all rollup module which must be explicitly declared as a dependency and must declare its own dependencies on other modules.
- **(stricter)** `"dependenciesonly"`: Forbid any module from listing `<root>` as a dependency, but allow `<root>` to declare its own dependencies.
- **(strictest)** `"forbid"`: Forbid any reference to the `<root>` module in tach.toml. This means that all code in [source roots](#source-roots) MUST be contained within an explicitly configured [module](#modules).

## Source Roots

The `source_roots` key is required for Tach to understand the imports within your project.
If it is not set explicitly, `source_roots` defaults to your project root path: `['.']`.
This means Tach will expect that your Python imports are resolved relative to the directory in which `tach.toml` exists.

Below are typical cases in which modifying `source_roots` is necessary.

### Example: Python below project root

Suppose your repository contains a subfolder where all of your Python code lives. This could be a web server, a collection of serverless functions, or even utility scripts.
In this example we will assume the Python code in our repo lives in the `backend/` folder.

```
my_repo/
  tach.toml
  backend/
    module1.py
    module2/
      __init__.py
      service.py
    module3.py
  docs/
  tests/
```

In a Python module such as `backend/module1.py`, we can see imports from other modules.

```python
# In backend/module1.py

import module3
from module2.service import MyService
```

Notice that these import paths (`module3`, `module2.service.MyService`) are rooted in the `backend/` folder, NOT the project root.

To indicate this structure to Tach, set:

```toml
source_roots = ["backend"]
```

in your `tach.toml`, or use [`tach mod`](commands.md#tach-mod) and mark the `backend` folder as the only source root.

### Example: Monorepo - Namespace Packages

Suppose you work on a 'monorepo', in which Python packages which import from each other are located in distinct project directories.
You may package your utility libraries in a `utility`folder, while your core packages live in `core_one` and `core_two`.
You may also use a [namespace package](https://peps.python.org/pep-0420/) to share a common top-level namespace. In this example we'll use `myorg` as the namespace package.

The file tree in a case like this might look like:

```
my_repo/
  tach.toml
  utility/
    pyproject.toml
    src/
      myorg/
        utils/
          __init__.py
  core_one/
    pyproject.toml
    src/
      myorg/
        core_one/
          __init__.py
          module1.py
          module2/
            __init__.py
            service.py
          module3.py
  core_two/
    pyproject.toml
    src/
      myorg/
        core_two/
          __init__.py
          module1.py
          module2/
            __init__.py
            service.py
          module3.py
  docs/
  tests/
```

In a Python module such as `core_one/src/myorg/core_one/module1.py`, there may be imports from other packages:

```python
# In core_one/src/myorg/core_one/module1.py

from myorg.utils import utility_fn
```

Notice that this import path (`myorg.utils.utility_fn`) is rooted in the `utility/src` folder, NOT the project root.

To indicate the project structure to Tach, you would set:

```toml
source_roots = [
  "utility/src",
  "core_one/src",
  "core_two/src"
]
```

in your `tach.toml`, or use [`tach mod`](commands.md#tach-mod) and mark the same folders as source roots.

!!! note
    In `tach.toml`, each entry in `source_roots` is interpreted as a relative path from the project root.

After configuring your source roots as above, you can use `tach check-external`
to validate that any dependencies between the packages are declared explicitly in the corresponding `pyproject.toml`.

For example, given the import shown above (`core_one` importing from `utility`),

```python
# In core_one/src/myorg/core_one/module1.py

from myorg.utils import utility_fn
```

Tach would validate that this is declared in `core_one/pyproject.toml`:

```toml
# In core_one/pyproject.toml
[project]
dependencies = ["myorg-utility"]
```

Note that this also assumes the `name` of the `utility` package has been set to `myorg-utility`:

```toml
# In utility/pyproject.toml
[project]
name = "myorg-utility"
```


### Example: Monorepo - Workspace Packages

Suppose you work on a 'monorepo', in which Python packages which import from each other are located in distinct project directories.
You may package your utility libraries in a `utility`folder, while your core packages live in `core_one` and `core_two`.
You may also use something like [uv workspaces](https://docs.astral.sh/uv/concepts/projects/workspaces/#getting-started) to organize these packages.

The file tree in a case like this might look like:

```
my_repo/
  tach.toml
  utility/
    pyproject.toml
    src/
      utility/
        __init__.py
  core_one/
    pyproject.toml
    src/
      core_one/
        __init__.py
        module1.py
        module2/
          __init__.py
          service.py
        module3.py
  core_two/
    pyproject.toml
    src/
      core_two/
        __init__.py
        module1.py
        module2/
          __init__.py
          service.py
        module3.py
  docs/
  tests/
```

In a Python module such as `core_one/src/core_one/module1.py`, there may be imports from other packages:

```python
# In core_one/src/core_one/module1.py

from utility import utility_fn
```

Notice that this import path (`utility.utility_fn`) is rooted in the `utility/src` folder, NOT the project root.

To indicate the project structure to Tach, you would set:

```toml
source_roots = [
  "utility/src",
  "core_one/src",
  "core_two/src"
]
```

in your `tach.toml`, or use [`tach mod`](commands.md#tach-mod) and mark the same folders as source roots.

!!! note
    In `tach.toml`, each entry in `source_roots` is interpreted as a relative path from the project root.

After configuring your source roots as above, you can use `tach check-external`
to validate that any dependencies between the packages are declared explicitly in the corresponding `pyproject.toml`.

For example, given the import shown above (`core_one` importing from `utility`),

```python
# In core_one/src/core_one/module1.py

from utility import utility_fn
```

Tach would validate that this is declared in `core_one/pyproject.toml`:

```toml
# In core_one/pyproject.toml
[project]
dependencies = ["myorg-utility"]
```

Note that this also assumes the `name` of the `utility` package has been set to `myorg-utility`:

```toml
# In utility/pyproject.toml
[project]
name = "myorg-utility"
```

## `tach.domain.toml`

Tach allows splitting your configuration into 'domains', or sub-folders of your project.
You can define modules and interfaces in a `tach.domain.toml` file which lives right next to the module code itself.

This enables multiple developers or teams to independently own and maintain their modules and interfaces.

### Modules

Within a `tach.domain.toml` file, you can define [modules](#modules) similar to modules in `tach.toml`.
The key difference is that paths are relative to the location of the domain by default, and there is a `[root]` module syntax for describing the parent folder itself.

For example, a `tach.toml` file containing the module definitions:
```toml
[[modules]]
path = "tach.filesystem"
depends_on = [
  "tach.hooks",
  "tach.filesystem.service",
]

[[modules]]
path = "tach.filesystem.service"
depends_on = []
```

could be expressed instead in a `tach.domain.toml` file at `<source root>/tach/filesystem/tach.domain.toml`:

```toml
[root]  # This defines "tach.filesystem"
depends_on = [
  "//tach.hooks",  # This refers to "tach.hooks" (outside of this domain)
  "service",  # This refers to "tach.filesystem.service"
]

[[modules]]
path = "service"  # This defines "tach.filesystem.service"
depends_on = []
```

Note that the `tach.domain.toml` file uses its location relative to the nearest source root to implicitly determine its `[root]` module path (`"tach.filesystem"`).

The domain configuration also uses special syntax to refer to dependencies on module paths outside of the domain, prefixing the absolute path with `"//"`.

Module paths are generally interpreted relative to the location of the `tach.domain.toml` file, which can be seen in the definition for the `"service"` module.
This path is interpreted relative to the domain root, meaning it refers to the `"tach.filesystem.service"` module.

### Interfaces

Interfaces are defined in largely the same way as in [`tach.toml`](#interfaces), with the key difference being the treatment of paths in the `from` field.
These will be interpreted as paths relative to the domain root.

For example, a `tach.toml` file containing the interface definitions:
```toml
[[interfaces]]
expose = ["service.*"]
from = ["tach.filesystem"]

[[interfaces]]
expose = ["client.*"]
from = ["tach.filesystem.git_ops"]
```

could be expressed instead in a `tach.domain.toml` file at `<source root>/tach/filesystem/tach.domain.toml`:

```toml
[[interfaces]]
expose = ["service.*"]
from = ["<domain_root>"]  # This matches "tach.filesystem"

[[interfaces]]
expose = ["client.*"]
from = ["git_ops"]  # This matches "tach.filesystem.git_ops"
```

### Example: `CODEOWNERS`

Tach domain configuration files enable smooth integration with [`CODEOWNERS`](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners):

```CODEOWNERS
# Domain ownership for different teams

/tach.toml    @platform-team

/payments/tach.domain.toml    @payments-team
/auth/tach.domain.toml    @auth-team
/data/analytics/tach.domain.toml    @analytics-team @data-team
/mobile/tach.domain.toml    @mobile-team
/libs/shared/tach.domain.toml    @platform-team
```

This allows a team to own their public interface, without imposing a bottleneck on other teams' configuration changes.

## External

When running [`check-external`](commands.md#tach-check-external), Tach allows excluding certain modules from validation.

Adding the top level module name to the `exclude` key (underneath the `external` key) will allow all usages of the corresponding module.

Example:
```toml
[external]
exclude = ["PIL"]
```

Tach also allows supplying a `rename` field to handle cases where the top level module name does not match the name of the package.

For example, the `pillow` package supplies the `PIL` module, so Tach needs to map imports from `PIL` to the `pillow` package specifier in your requirements.

```toml
[external]
rename = [
  # Format "[module name]:[package name]"
  "PIL:pillow",
  ...
]
```

In most cases you should not need to specify `rename` manually (see the Note below).

!!! note
    It is recommended to run Tach within a virtual environment containing all of
    your dependencies across all packages. This is because Tach uses the
    distribution metadata to map module names like 'git' to their distributions
    ('GitPython').

## Rules

Tach allows configuring the severity of certain issues. Each entry in the `rules` table can be set to `error`, `warn`, or `off`.

The available rules and their defaults are listed below.

- `unused_ignore_directives` (**default**: `warn`): catch `tach-ignore` comments which are unused (e.g. do not suppress any errors or warnings)
- `require_ignore_directive_reasons` (**default**: `off`): require every `tach-ignore` comment to have a reason
- `unused_external_dependencies` (**default**: `error`): catch declared 3rd party dependencies which are not imported in your code


## Cache

Tach allows configuration of the [computation cache](caching.md) it uses to speed up tasks like [testing](commands.md#tach-test).

The `file_dependencies` key accepts a list of glob patterns to indicate additional file contents that should be considered when [checking for cache hits](caching.md#determining-cache-hits). This should typically include files outside of your [source roots](#source-roots) which affect your project's behavior under test, including the tests themselves. Additionally, if you have non-Python files which affect your project's behavior (such as Rust or C extensions), these should be included as well.

The `env_dependencies` key accepts a list of environment variable names whose values affect your project's behavior under test. This may include a `DEBUG` flag, or database connection parameters in the case of tests which use a configurable database.
