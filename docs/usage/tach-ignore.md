# tach-ignore

To ignore a particular import which should be allowed unconditionally, use the `tach-ignore` comment directive.

```python
# tach-ignore
from core.main import private_function

from core.api import private_calculation  # tach-ignore

from core.package import (  # tach-ignore
    service_one,
    service_two,
)
```

The directive can also be specific about the import to ignore, which is particularly useful when importing multiple packages.

```python
# tach-ignore private_function
from core.main import private_function, public_function

from core.api import (  # tach-ignore private_calculation
    private_calculation,
    public_service,
)

from core.package import (  # tach-ignore service_two
    service_one,
    service_two,
)
```

Note: Names given to `tach-ignore` should match the alias as it is used in the subsequent import line, not the full module path from the project root.

## Reasons

Tach also allows you to add a message next to the ignore directive, to document the reasoning for the ignore.

```python
# tach-ignore(Alternative API not yet available 11/26/24) private_api
from core.api import private_api
```
