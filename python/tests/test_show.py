from __future__ import annotations

from pathlib import Path

import pytest

from tach.parsing.config import parse_project_config
from tach.show import (
    generate_module_graph_dot_string,
    generate_module_graph_mermaid_string,
    generate_show_report,
)


def get_example_dir() -> Path:
    return Path(__file__).parent / "example"


def get_outputs_dir() -> Path:
    return Path(__file__).parent / "outputs"


def get_output_test_cases() -> list[tuple[str, str, str]]:
    """
    Discover all output test cases from the outputs directory.

    Returns:
        List of tuples: (example_name, output_type, expected_file_path)
        where output_type is either "dot" or "mermaid"
    """
    outputs_dir = get_outputs_dir()
    example_dir = get_example_dir()
    test_cases: list[tuple[str, str, str]] = []

    if not outputs_dir.exists():
        return test_cases

    for output_subdir in sorted(outputs_dir.iterdir()):
        if not output_subdir.is_dir() or output_subdir.name.startswith("."):
            continue

        example_name = output_subdir.name

        # Validate that the example directory exists
        if not (example_dir / example_name).exists():
            raise ValueError(
                f"Output directory '{example_name}' does not have a matching "
                f"example directory in '{example_dir}'. "
                f"Either create the example or remove the output directory."
            )

        # Check for DOT output file
        dot_file = output_subdir / "expected.dot"
        if dot_file.exists():
            test_cases.append((example_name, "dot", str(dot_file)))

        # Check for Mermaid output file
        mermaid_file = output_subdir / "expected.mmd"
        if mermaid_file.exists():
            test_cases.append((example_name, "mermaid", str(mermaid_file)))

    return test_cases


# Collect test cases at module load time
OUTPUT_TEST_CASES = get_output_test_cases()


@pytest.mark.parametrize(
    "example_name,output_type,expected_file_path",
    OUTPUT_TEST_CASES,
    ids=[f"{name}-{otype}" for name, otype, _ in OUTPUT_TEST_CASES],
)
def test_show_output_matches_expected(
    example_name: str,
    output_type: str,
    expected_file_path: str,
) -> None:
    """
    Test that generate_module_graph_dot_string or generate_module_graph_mermaid_string
    produces output matching the expected file.
    """
    example_dir = get_example_dir()
    project_root = example_dir / example_name
    project_config = parse_project_config(root=project_root)

    assert project_config is not None, f"Failed to parse config for {example_name}"
    assert not project_config.has_no_modules(), f"No modules found for {example_name}"

    # Generate the appropriate output
    if output_type == "dot":
        actual_output = generate_module_graph_dot_string(
            project_config=project_config,
            included_paths=[],
        )
    elif output_type == "mermaid":
        actual_output = generate_module_graph_mermaid_string(
            project_config=project_config,
            included_paths=[],
        )
    else:
        pytest.fail(f"Unknown output type: {output_type}")

    # Read expected output
    expected_output = Path(expected_file_path).read_text()

    # Compare
    assert actual_output == expected_output, (
        f"Output mismatch for {example_name} ({output_type})\n"
        f"To update expected output, run: python -m tests.generate_show_outputs"
    )


# Keep the original smoke test for generate_show_report
def test_many_features_example_dir(example_dir: Path) -> None:
    """
    Smoke test for generate_show_report.

    This example directory has Python files outside source roots,
    which has previously caused bugs.
    """
    project_root = example_dir / "many_features"
    project_config = parse_project_config(root=project_root)
    assert project_config is not None

    report = generate_show_report(
        project_root=project_root, project_config=project_config, included_paths=[]
    )
    assert report is not None
