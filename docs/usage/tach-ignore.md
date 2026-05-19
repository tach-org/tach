# tach-ignore

To ignore a particular import which should be allowed unconditionally, use the `tach-ignore` comment directive.

```python
# tach-ignore
from core.main import private_function

from core.api import private_calculation  # tach-ignore

from core.package import (  # tach-ignore
    service_one,
    service_two
)
```

The directive can also be specific about the import to ignore, which is particularly useful when importing multiple packages.

```python
# tach-ignore private_function
from core.main import private_function, public_function

from core.api import private_calculation, public_service  # tach-ignore private_calculation

from core.package import (  # tach-ignore service_two
    service_one,
    service_two
)
```

Note: Names given to `tach-ignore` should match the alias as it is used in the subsequent import line, not the full module path from the project root.

## Deadcode ignores

For [`tach deadcode`](commands.md#tach-deadcode), use the [`[deadcode].ignore`](configuration.md#deadcode) list in `tach.toml` to suppress known-reachable files, modules, or symbols.

```toml
[deadcode]
ignore = [
  "pkg.legacy",                  # module path
  "pkg/generated.py",            # file path
  "pkg.service:dynamic_handler", # function symbol
  "pkg.api:UnusedClass",         # class symbol
  "pkg.constants:DEPRECATED",    # variable symbol
]
```

Use `public_modules`, `public_symbols`, or `public_decorators` instead when the code is part of your public API and should be treated as live, not ignored as an exception.

## Reasons

Tach also allows you to add a message next to the ignore directive, to document the reasoning for the ignore.

```python
# tach-ignore(Alternative API not yet available 11/26/24) private_api
from core.api import private_api
```
