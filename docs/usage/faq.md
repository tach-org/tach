# FAQ

### How does it work?

Tach works by analyzing the imports in your Python modules.
When you define dependencies in your project-level `tach.toml`, running `tach check` will verify that the imports in your modules match your expected dependencies.

### What is a module?

A 'module' is a Python module with dependencies configured in `tach.toml`. A module can be a python file or a directory.
The module is identified by its import path from the nearest source root (e.g. `a.b` for `<root>/a/b.py`),
and its dependencies can be listed in the `depends_on` key containing module paths in the same format.

[See more information on configuration here.](configuration.md)

### Can I declare a module without restricting its dependencies?

Yes, you can remove the `depends_on` key from the module in your `tach.toml` configuration. Tach will then be aware of the module boundary so that it can be referenced in other modules' dependencies,
even if you don't want to restrict the dependencies of the module itself.

```toml
[[modules]]
path = "my.module"
# no 'depends_on' key here means this module can depend on anything
```

### How can I isolate a module from the rest of the code?

To prevent any external usage of a given module, you can set `visibility: []` for the module in `tach.toml`. This means that no other module can declare an explicit dependency on this module.

[See more information on configuration here.](configuration.md)

### How can I declare modules which are freely usable by the rest of the code?

This is typically done for modules like `utils/`, `errors/` or `lib/`, which contain relatively isolated code which can be used throughout a project.

Tach allows marking these modules as **Utilities**, which means they can be used without being listed as an explicit dependency.

!!! note
    Marking a module with `utility: true` is different from `visibility: ['*']`.

  Even when a module has public visibility, other modules must declare an
  explicit dependency to use it (in fact, modules are publicly visible by
  default).

  In contrast, a utility module does not require its dependents to
  list an explicit dependency.

[See more information on configuration here.](configuration.md)

### How can I define a public interface for a module?

[Public interfaces](interfaces.md) are defined in [tach.toml](configuration.md#interfaces), and restrict the imports which are allowed from a given module.
This is useful when you want to expose a stable API from a module and prevent other modules from becoming deeply coupled to its implementation details.

[See more information on configuration here.](configuration.md)

### Are conditional imports checked?

Tach will check all imports in your source files, including those which are called conditionally.
The only exceptions are imports made within `TYPE_CHECKING` conditional blocks.
By default, Tach will **not** report errors from `TYPE_CHECKING` blocks. If you want to enable checks for
these imports, you can add `ignore_type_checking_imports: false` to your `tach.toml`.

[See more information on configuration here.](configuration.md)

### Can you catch dynamic references?

Since Tach uses the AST to find imports and public members, dynamic imports (e.g. using a string path) and dynamic names (e.g. using `setattr`, `locals`, `globals`) are not supported. If these usages cause Tach to report incorrect errors, the [ignore directive](tach-ignore.md) should be sufficient to reach a passing state.

### How can I make a feature request or file a bug?

This project uses [GitHub Issues](https://github.com/gauge-sh/tach/issues) to track bugs and feature requests. Search the existing
issues before filing new issues to avoid duplicates. File your bug or
feature request as a new Issue.

### How can I report a security issue?

Do not report security vulnerabilities through public GitHub issues. Instead, please email us at caelean@gauge.sh or evan@gauge.sh.

### What information does Tach track?

None