# Deprecate Dependencies

A dependency can be marked as `deprecated` - this means that the intention is to remove it over time, but it is still allowed.
`tach check` will not error on deprecated dependencies, but it will surface each import that uses the deprecated dependency.

## Example

Given modules called 'core' and 'parsing':

```toml
[[modules]]
path = "parsing"
depends_on = [
    { path = "core", deprecated = true }
]

[[modules]]
path = "core"
depends_on = []
```

Then, in `parsing.py`:

```python
from core.main import get_data  # we want to remove this!

get_data()
```

This import won't fail `tach check`, instead you'll see:
```shell
‼️ parsing.py[L1]: Import 'core.get_data' is deprecated. 'parsing' should not depend on 'core'.
✅ All modules validated!
```

Note that we still see that all module dependencies are valid! To fail on the dependency, simply remove it from the `depends_on` key.

