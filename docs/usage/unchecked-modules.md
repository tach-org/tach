# Unchecked Modules

---

A module can be marked 'unchecked' by setting `unchecked: true` in its configuration within [`tach.toml`](configuration.md#tachtoml).

## How does it work?

An **unchecked** module should be thought of as a 'known' module which does not have any restriction on its dependencies.

The purpose of configuring an unchecked module is to better support **incremental adoption** of Tach.

!!! note
    Setting `unchecked: true` is different from omitting the `depends_on` field.

An unchecked module is allowed to import anything, from any module - even when the module declares a [public interface](configuration.md#interfaces).

In contrast, a module with `depends_on` omitted will still need to respect public interfaces.

## Example

Suppose you are adding Tach to an existing project, and you want to start by restricting dependencies for only one part of the codebase.

```
my_repo/
  tach.toml
  utils.py
  filesytem/
    service.py
  parsing/
    service.py
  core/
    module1.py
    module2/
      __init__.py
      service.py
    module3.py
  docs/
  tests/
```

Given the example project above, one might mark `utils`, `filesystem`, `parsing`, and `core.module(1,2,3)` as modules.

After doing this, `tach sync` would detect and add all the dependencies found between these modules, and `tach check` would enforce them.

However, suppose that `parsing`, `core.module2`, and `core.module3` are changing rapidly, and errors from `tach check` due to changing dependencies are unnecessary.
By default, Tach would require the configuration to be 'all-or-nothing' - these errors would be unavoidable if these modules are themselves dependencies of more stable modules.

```toml
[[modules]]
path = "utils"
utility = true
depends_on = []

[[modules]]
path = "filesystem"
depends_on = []

[[modules]]
# this module is not checked because its errors would be noisy
path = "parsing"
unchecked = true
depends_on = []

[[modules]]
# Tach will verify this module's dependencies,
# even though they are unchecked
path = "core.module1"
depends_on = ["core.module2", "core.module3", "parsing"]

[[modules]]
# this module is not checked because its errors would be noisy
path = "core.module2"
unchecked = true
depends_on = []

[[modules]]
# this module is not checked because its errors would be noisy
path = "core.module3"
unchecked = true
depends_on = []
```

Using `unchecked: true` in the configuration above allows restricting dependencies for `core.module1` at a fine-grained level without needing to restrict dependencies for all other modules.
