# MCP Server Benchmark

The [`tach mcp`](commands.md#tach-mcp) server returns compact, paginated
summaries by default so that AI agents can work on large codebases without
exhausting their context budgets. This page documents the benchmark behind the
numbers quoted in the docs, and how to reproduce it.

## Environment

- Tach with the MCP server (this repository, editable install)
- Target codebase: fresh shallow clone of [Django](https://github.com/django/django)
  (~2,900 Python files; Tach resolved 2,126 first-party files and 9,534
  file-level dependency edges across 14 declared modules)
- Transport: real MCP stdio session using the official `mcp` Python client —
  byte counts are the exact JSON content returned over the wire, not internal
  estimates. Tokens are approximated as bytes/4.

## Results

| Call | Default response | Full-mode / naive alternative | Savings |
|---|---|---|---|
| `tach_map mode="summary"` | 5,760 B (~1.4K tokens) | `mode="full"`: 520,696 B (~130K tokens) | **90x** |
| `tach_onboard` (summary) | 2,532 B | `view="full"`: 14,037 B | 5.5x |
| `tach_lint` (386 diagnostics) | 19,812 B first page (`limit=50`); 5,249 B at `limit=10` | ~150 KB unpaginated; raw `tach check-external` CLI output: 40,198 B | 2–29x |
| `tach_report` (one file) | 2,797 B (`report_bytes` reports true size; truncates at `max_bytes`) | unbounded report text | size-capped |
| `tach_map mode="delta"` (one ORM file changed) | 2,931 B conveying **2,078 affected files** (paginated) | reading the full closure | — |
| `tach_configure` write actions (`sync_dependencies`, `set_layers`, `set_module_layer`, `deprecate_dependency`) | 220–270 B each, < 0.6 s | — | — |

`sync_dependencies` wrote 90 module-level edges across the 14 Django modules
in 2.8 s end-to-end over MCP (CLI `tach sync` on the same checkout: 2.5 s).

Semantics checks on the same run: deprecating a real synced edge
(`django.forms` → `django.core`) moved `warning_count` from 0 to 13 (one per
importing file) with `error_count` unchanged, and the full dependency map —
the naive thing an agent might otherwise load — would by itself exceed most
agent context windows.

## Reproduce it

Clone a large target and run the driver below with the `mcp` Python package
installed (`pip install mcp`, already a Tach dependency):

```bash
git clone --depth 1 https://github.com/django/django.git /tmp/django-bench
python bench_mcp.py /tmp/django-bench
```

```python
# bench_mcp.py
import asyncio
import json
import sys
import time

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

ROOT = sys.argv[1]
MODULES = [
    "django.apps", "django.conf", "django.core", "django.db",
    "django.dispatch", "django.forms", "django.http", "django.middleware",
    "django.template", "django.templatetags", "django.test", "django.urls",
    "django.utils", "django.views",
]


def size_of(result):
    return len(json.dumps([c.model_dump() for c in result.content]).encode())


async def call(session, tool, label, **args):
    start = time.monotonic()
    result = await session.call_tool(tool, args)
    print(f"{label}: {size_of(result)} bytes, "
          f"{time.monotonic() - start:.2f}s, isError={result.isError}")
    return result.structuredContent


async def main():
    params = StdioServerParameters(command="tach", args=["mcp"], cwd=ROOT)
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            await call(session, "tach_onboard", "onboard_unconfigured",
                       project_root=ROOT)
            await call(session, "tach_configure", "create_config",
                       action="create_config", project_root=ROOT,
                       source_roots=["."], modules=MODULES)
            await call(session, "tach_configure", "sync",
                       action="sync_dependencies", project_root=ROOT)
            await call(session, "tach_onboard", "onboard_summary",
                       project_root=ROOT)
            await call(session, "tach_onboard", "onboard_full",
                       project_root=ROOT, view="full")
            await call(session, "tach_map", "map_summary",
                       project_root=ROOT, mode="summary")
            await call(session, "tach_map", "map_full",
                       project_root=ROOT, mode="full")
            await call(session, "tach_graph", "graph_preview",
                       project_root=ROOT)
            await call(session, "tach_lint", "lint_default",
                       project_root=ROOT)
            await call(session, "tach_lint", "lint_limit10",
                       project_root=ROOT, limit=10)
            await call(session, "tach_report", "report_default",
                       project_root=ROOT, path="django/db/models/query.py")
            await call(session, "tach_map", "delta_one_file",
                       project_root=ROOT, mode="delta",
                       changed=["django/db/models/query.py"])
            await call(session, "tach_configure", "set_layers",
                       action="set_layers", project_root=ROOT,
                       layers=["interface", "application", "core"])
            await call(session, "tach_configure", "deprecate",
                       action="deprecate_dependency", project_root=ROOT,
                       path="django.forms", dependency="django.core")
            await call(session, "tach_lint", "lint_after_deprecate",
                       project_root=ROOT)


asyncio.run(main())
```

Numbers vary slightly with the Django revision cloned; the ratios are stable.
