---
name: tach
description: Enforce and fix Python module boundaries with the Tach MCP server. Use when a repo contains tach.toml, when import/dependency violations are reported, or when asked to modularize a Python codebase. Drives tach_onboard, tach_lint, tach_configure, tach_report, tach_map, tach_graph, and tach_test efficiently.
---

# Tach: module boundary enforcement

Tach enforces declared dependencies, public interfaces, and acyclicity between
Python modules. This skill drives the `tach` MCP server (configure the client
to run `tach mcp`; see docs/usage/commands.md).

## Workflow

1. **Always start with `tach_onboard`.** It returns whether the project is
   configured, a compact summary, and the exact next actions. Never guess
   project state.
2. **Unconfigured repo**: call `tach_configure action="create_config"` with
   `source_roots`, `modules` (top-level packages), intended `dependencies`,
   and `forbid_circular_dependencies=true` for new projects. Then
   `tach_configure action="sync_dependencies"` to align rules with reality.
3. **Check**: `tach_lint` runs boundary, interface, external, and unused
   dependency checks in one call. Read `error_count` first; diagnostics are
   paginated (`limit`/`offset`) — page through only when fixing.
4. **Diagnose a violation**: `tach_report` with the failing file shows its
   dependencies and usages. `tach_map mode="closure"` shows everything a file
   pulls in; `tach_map mode="delta"` scopes the blast radius of changed files
   before large edits.
5. **Fix**: either move the import to respect the declared direction, or — if
   the architecture genuinely changed — `tach_configure
   action="edit_dependency"` to update the rules. Prefer fixing code over
   loosening rules. For a violation that must stay temporarily, add a
   `# tach-ignore(reason)` comment on the import line instead of deleting the
   rule — unused or reason-less directives are themselves lintable.
6. **Architecture rules**: `tach_configure` also writes the higher-level
   constraints — `action="set_layers"` (ordered layer list, optionally
   `layers_explicit_depends_on`), `"set_module_layer"`,
   `"set_module_visibility"`, `"add_interface"` / `"remove_interface"`
   (public interface per module), and `"deprecate_dependency"` (keep an edge
   legal but warn on use while migrating off it).
7. **Verify**: re-run `tach_lint`, then `tach_test` (affected tests only) to
   confirm nothing broke.

## Token discipline

- Default modes are compact and paginated; stay on them. Use `view="full"`,
  `mode="full"`, or `include_full=true` only when you must read an entire
  payload, and prefer the `tach://...` resource URIs for bulk data.
- `tach_lint`, `tach_report`, `tach_graph`, and `tach_test` accept
  `limit`/`offset` or `max_bytes` — lower them when triaging large repos.

## Interpreting results

- `boundaries.circular_dependencies` (list of module paths) means the declared
  dependency graph has a cycle: remove one edge with
  `tach_configure action="edit_dependency", dependency_action="remove"`.
- Many identical diagnostics against one module usually mean a utility/god
  module: mark it with `tach_configure action="edit_module",
  module_action="mark_utility"` only if it is genuinely interface-free shared
  code; otherwise split it.
- `unused.unused_dependencies` are declared edges no code uses — remove them
  with `sync_dependencies` or `edit_dependency`.

## Hooks

If a `PostToolUse` hook runs `tach check` after edits, a non-zero hook result
quoting boundary diagnostics is authoritative — fix the violation before
moving on instead of retrying the edit. Outside hook-enabled environments,
`tach_configure action="install_pre_commit"` gives the same guarantee at
commit time.
