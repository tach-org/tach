"""
Script to generate expected output files for tach show tests.

Run this script to populate/update the expected output files:
    python -m tests.generate_show_outputs

This will generate expected.dot and expected.mmd files
for each example directory that has a valid project configuration
(either tach.toml or pyproject.toml).
"""

from __future__ import annotations

import shutil
import tempfile
from pathlib import Path

from tach.extension import sync_project
from tach.parsing.config import parse_project_config
from tach.show import (
    generate_module_graph_dot_string,
    generate_module_graph_mermaid_string,
)


def get_example_dir() -> Path:
    return Path(__file__).parent / "example"


def get_outputs_dir() -> Path:
    return Path(__file__).parent / "outputs"


def generate_outputs_for_example(example_name: str) -> tuple[str | None, str | None]:
    """
    Generate DOT and Mermaid outputs for a given example directory.

    Copies the example to a temp directory to isolate side effects of sync_project.

    Returns:
        Tuple of (dot_output, mermaid_output) or (None, None) if invalid config.
    """
    source_path = get_example_dir() / example_name

    # Copy to temp directory to isolate side effects of sync_project
    with tempfile.TemporaryDirectory() as tmp_dir:
        example_path = Path(tmp_dir) / example_name
        _ = shutil.copytree(source_path, example_path)

        try:
            project_config = parse_project_config(root=example_path)
        except Exception as e:
            print(f"Error parsing config for {example_name}: {e}")
            return None, None

        if project_config is None:
            return None, None

        # Check if there are modules to show
        if project_config.has_no_modules():
            return None, None

        # Sync project to ensure dependencies are up to date (modifies config file on disk)
        sync_project(example_path, project_config)

        # Re-parse config to get the updated version after sync
        project_config = parse_project_config(root=example_path)
        if project_config is None:
            print(f"Error re-parsing config for {example_name} after sync")
            return None, None

        try:
            dot_output = generate_module_graph_dot_string(
                project_config=project_config,
                included_paths=[],
            )
            mermaid_output = generate_module_graph_mermaid_string(
                project_config=project_config,
                included_paths=[],
            )
            return dot_output, mermaid_output
        except Exception as e:
            print(f"Error generating output for {example_name}: {e}")
            return None, None


def main() -> None:
    example_dir = get_example_dir()
    outputs_dir = get_outputs_dir()

    # Get all example directories
    example_names = sorted(
        [
            d.name
            for d in example_dir.iterdir()
            if d.is_dir() and not d.name.startswith(".")
        ]
    )

    for example_name in example_names:
        dot_output, mermaid_output = generate_outputs_for_example(example_name)

        if dot_output is None or mermaid_output is None:
            print(f"Skipping {example_name}: no valid configuration or no modules")
            continue

        # Create output directory
        output_subdir = outputs_dir / example_name
        output_subdir.mkdir(parents=True, exist_ok=True)

        # Write DOT output
        dot_file = output_subdir / "expected.dot"
        _ = dot_file.write_text(dot_output)
        print(f"Generated {dot_file}")

        # Write Mermaid output
        mermaid_file = output_subdir / "expected.mmd"
        _ = mermaid_file.write_text(mermaid_output)
        print(f"Generated {mermaid_file}")

    print("\nDone! Review the generated files before committing.")


if __name__ == "__main__":
    main()
