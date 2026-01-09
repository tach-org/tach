# Layers

An ordered list of layers can be configured at the top level of [`tach.toml`](configuration.md#layers),
and [modules](configuration.md#modules) can each be assigned to a specific layer.

## How does it work?

[Layered architecture](https://www.oreilly.com/library/view/software-architecture-patterns/9781491971437/ch01.html) is often
an effective starting point for modularizing an application.

The idea is straightforward:
**Higher layers may import from lower layers, but lower layers may NOT import from higher layers.**

Defining this architecture is more concise and flexible than specifying all module dependencies
with `depends_on`, which makes it easier to adopt in an existing project.

Tach allows defining and enforcing a layered architecture with any number of vertically-stacked layers.

When a module is assigned to a layer, this module:

- may freely depend on modules in **lower layers**, *without declaring these dependencies*
- must explicitly declare dependencies in **its own layer**
- may never depend on modules in **higher layers**, *even if they are declared*

## Requiring Explicit Dependencies

By default, modules in higher layers can freely depend on modules in lower layers without declaring them in `depends_on`. However, you can require explicit declarations for all cross-layer dependencies by setting `layers_explicit_depends_on = true` in your `tach.toml`:

```toml
layers_explicit_depends_on = true

layers = ["presentation", "business", "data"]

[[modules]]
path = "api"
layer = "presentation"
depends_on = ["user_service"]  # Must explicitly declare dependency on modules in lower layers

[[modules]]
path = "user_service"
layer = "business"
depends_on = ["user_repo"]  # Must explicitly declare dependency on modules in lower layers

[[modules]]
path = "user_repo"
layer = "data"
depends_on = []
```

With this flag enabled:

- Modules must explicitly declare **all** cross-layer dependencies in `depends_on`
- The layer hierarchy still prevents lower layers from depending on higher layers
- [Utility modules](configuration.md#modules) remain accessible without explicit declaration
- This provides stricter control over dependencies while maintaining architectural constraints

This is useful when you want:

- Full visibility of all module dependencies in your configuration
- Finer-grained control over which lower-layer modules each module can access
- To gradually migrate toward explicit dependency management

## Closed Layers

By default, modules in higher layers can import from any lower layer. However, you can mark a layer as "closed" to prevent modules in higher layers from importing modules in lower layers.

For example, with layers A, B, and C (high to low), if B is a closed layer, then A cannot import C directly, even though C is lower than A. This is useful for enforcing strict boundaries between architectural tiers.

When a layer is closed, it acts as an intermediary - modules in higher layers must go through the closed layer to access modules in lower layers. This helps enforce architectural boundaries and prevents direct coupling between distant layers.

You can specify a closed layer by using an in-line object in the `layers` array:

```toml
# Shorthand format (defaults to closed = false)
layers = ["ui", "commands", "core"]

# Object format with closed property
layers = [
    "ui",
    { name = "commands", closed = true },
    "core"
]
```

In this example, if `commands` is a closed layer:
- `ui` modules can import from `commands` modules
- `commands` modules can import from `core` modules
- `ui` modules CANNOT import directly from `core` modules
- `ui` modules must go through `commands` modules to access `core` functionality

## Example

We can use the Tach codebase itself as an example of a 3-tier layered architecture:

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
depends_on = ["tach.filesystem"]
layer = "core"

[[modules]]
path = "tach.filesystem"
depends_on = []
layer = "core"
```

In the configuration above, three layers are defined.
They are similar to the classic `Presentation` - `Business Logic` - `Data` which are often found in web applications,
but a bit different given that Tach is a CLI program.

In Tach, the highest layer is `UI`, which includes code related to the CLI and other entrypoints to start the program.

Just below this, the `Commands` layer contains high-level business logic which implements each of the CLI commands.

At the bottom is the `Core` layer, which contains utilities, libraries, and broadly relevant data structures.

Given this configuration, `tach.check` does not need to declare a dependency on `tach.cache` or `tach.filesystem` to use it,
because the `Commands` layer is higher than the `Core` layer.

However, `tach.cache` needs to explicitly declare its dependency on `tach.filesystem`, because they
are *both* in the `Core` layer.
