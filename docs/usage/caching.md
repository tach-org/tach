# Caching

Tach makes use of a 'computation cache' to speed up certain tasks, such as [testing](commands.md#tach-test).

When Tach finds cached results for a given task, the terminal output is enclosed in:

```
============ Cached results found!  ============
...
============ END Cached results  ============
```

Caching is done at the command level. This means a single invocation of `tach test` can only ever result in a single cache hit or miss. Individual tests are not cached separately.

## Cache content

The computation cache contains the output from `stdout` and `stderr` from a previous task invocation.

This is done to enable 'replaying' cached tasks so that their output can be reused seamlessly.

## Determining cache hits

Tach uses several pieces of information to determine cache hits:

- Python interpreter version (`major.minor.micro`)
- All Python file contents beneath your [source roots](configuration.md#source-roots)
- Declared versions of 3rd party dependencies in project requirements (`requirements.txt` or `pyproject.toml`)
- File contents of explicitly configured [file dependencies](configuration.md#cache)
- Explicitly configured [environment variable values](configuration.md#cache)

When all of these match a previous cache entry, the cached results are printed directly to the terminal.

## Cache storage

The computation cache exists within the `.tach` directory in your project root. The directory is managed by Tach, and your cached results are stored on-disk on each machine where tasks are run.

We are currently working on a _remote cache_ backend, which will allow multiple developers and CI environments to share a centralized cache to maximize the hit rate. If you are interested in this functionality, reach out through a [GitHub issue](https://github.com/detachhead/dtach/issues) or via email: [evan@gauge.sh](mailto://evan@gauge.sh); [caelean@gauge.sh](mailto://caelean@gauge.sh)!

## Disabling the cache

The computation cache is enabled by default for commands such as [tach test](commands.md#tach-test). It can be disabled using `--disable-cache`. This will prevent all access to the cache and run the underlying command unconditionally.
