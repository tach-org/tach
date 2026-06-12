from __future__ import annotations

from tach.mcp.configuration import tach_configure
from tach.mcp.dependency_map import (  # noqa: F401
    delta_resource,
    dependency_map_resource,
    dependency_map_view_resource,
    tach_map,
)
from tach.mcp.imports import tach_imports
from tach.mcp.linting import diagnose_tach_boundaries, tach_lint  # noqa: F401
from tach.mcp.modularity import (  # noqa: F401
    modularity_report_resource,
    modularity_report_view_resource,
    plan_tach_modularization,
    tach_modularity,
)
from tach.mcp.module_graph import graph_resource, tach_graph  # noqa: F401
from tach.mcp.onboarding import (  # noqa: F401
    config_resource,
    project_summary_resource,
    tach_onboard,
    version_resource,
)
from tach.mcp.reporting import tach_report
from tach.mcp.server import MCP_PROTOCOL_VERSION, mcp, run
from tach.mcp.testing import tach_test

__all__ = [
    "mcp",
    "MCP_PROTOCOL_VERSION",
    "diagnose_tach_boundaries",
    "plan_tach_modularization",
    "run",
    "tach_configure",
    "tach_graph",
    "tach_imports",
    "tach_lint",
    "tach_map",
    "tach_modularity",
    "tach_onboard",
    "tach_report",
    "tach_test",
]
