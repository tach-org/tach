use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value;

use super::error;

pub type Result<T> = std::result::Result<T, error::ParsingError>;

pub struct ProjectInfo {
    pub name: Option<String>,
    pub dependencies: HashSet<String>,
    pub source_paths: Vec<PathBuf>,
}

pub fn parse_pyproject_toml(
    pyproject_path: &Path,
    include_dependency_groups: bool,
) -> Result<ProjectInfo> {
    let content = fs::read_to_string(pyproject_path)?;
    let toml_value: Value = toml::from_str(&content)?;
    let name = extract_project_name(&toml_value);
    let dependencies = extract_dependencies(&toml_value, include_dependency_groups);
    let source_paths = extract_source_paths(&toml_value, pyproject_path.parent().unwrap());
    Ok(ProjectInfo {
        name,
        dependencies,
        source_paths,
    })
}

fn extract_project_name(toml_value: &Value) -> Option<String> {
    toml_value
        .get("project")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
}

fn extract_dependencies(toml_value: &Value, include_dependency_groups: bool) -> HashSet<String> {
    let mut dependencies = HashSet::new();

    // Extract dependencies from standard pyproject.toml format
    let has_project_deps = toml_value
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .is_some_and(|deps| {
            extract_deps_from_value(&mut dependencies, deps);
            true
        });

    let has_poetry_deps = toml_value
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .is_some();

    // Print warning if both formats are detected
    if has_project_deps && has_poetry_deps {
        eprintln!(
            "Warning: Both project dependencies and Poetry dependencies detected. Using project dependencies."
        );
    } else if has_poetry_deps {
        // Extract Poetry dependencies only if project dependencies are not present
        if let Some(deps) = toml_value
            .get("tool")
            .and_then(|tool| tool.get("poetry"))
            .and_then(|poetry| poetry.get("dependencies"))
        {
            extract_deps_from_value(&mut dependencies, deps)
        }
    }

    // Extract PEP 735 dependency groups if enabled
    if include_dependency_groups {
        if let Some(groups) = toml_value
            .get("dependency-groups")
            .and_then(|g| g.as_table())
        {
            extract_dependency_groups(&mut dependencies, groups);
        }
    }

    dependencies
}

fn extract_deps_from_value(dependencies: &mut HashSet<String>, deps: &Value) {
    const EXCLUDED_DEPS: [&str; 3] = ["python", "poetry", "poetry-core"];

    match deps {
        Value::Array(deps_array) => {
            for dep_str in deps_array.iter().filter_map(|dep| dep.as_str()) {
                let pkg_name = normalize_package_name(&extract_package_name(dep_str));
                if !EXCLUDED_DEPS.contains(&pkg_name.as_str()) {
                    dependencies.insert(pkg_name);
                }
            }
        }
        Value::Table(deps_table) => {
            for dep_name in deps_table.keys() {
                let pkg_name = normalize_package_name(&extract_package_name(dep_name));
                if !EXCLUDED_DEPS.contains(&pkg_name.as_str()) {
                    dependencies.insert(pkg_name);
                }
            }
        }
        _ => {}
    }
}

/// Extracts dependencies from PEP 735 [dependency-groups] table.
/// Handles both direct package names and {include-group = "..."} references.
fn extract_dependency_groups(
    dependencies: &mut HashSet<String>,
    groups: &toml::map::Map<String, Value>,
) {
    let mut visited = HashSet::new();
    for group_name in groups.keys() {
        extract_group_deps(dependencies, groups, group_name, &mut visited);
    }
}

/// Recursively extracts dependencies from a single group, resolving include-group references.
fn extract_group_deps(
    dependencies: &mut HashSet<String>,
    groups: &toml::map::Map<String, Value>,
    group_name: &str,
    visited: &mut HashSet<String>,
) {
    if !visited.insert(group_name.to_string()) {
        return;
    }

    let Some(group_value) = groups.get(group_name) else {
        return;
    };

    let Some(group_array) = group_value.as_array() else {
        return;
    };

    const EXCLUDED_DEPS: [&str; 3] = ["python", "poetry", "poetry-core"];

    for item in group_array {
        match item {
            // Direct package name: "pytest>7"
            Value::String(dep_str) => {
                let pkg_name = normalize_package_name(&extract_package_name(dep_str));
                if !EXCLUDED_DEPS.contains(&pkg_name.as_str()) {
                    dependencies.insert(pkg_name);
                }
            }
            // Include group reference: {include-group = "test"}
            Value::Table(table) => {
                if let Some(included_group) = table.get("include-group").and_then(|v| v.as_str()) {
                    extract_group_deps(dependencies, groups, included_group, visited);
                }
            }
            _ => {}
        }
    }
}

fn extract_package_name(dep_str: &str) -> String {
    // Split on common separators and take the first part
    dep_str
        .split(&[' ', '=', '<', '>', '~', ';', '['][..])
        .next()
        .unwrap_or(dep_str)
        .to_string()
}

/// This normalizes a Python distribution name according to PyPI standards
pub fn normalize_package_name(name: &str) -> String {
    name.to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>()
        .join("_")
}

fn extract_source_paths(toml_value: &Value, project_root: &Path) -> Vec<PathBuf> {
    let mut source_paths = Vec::new();

    // Check for setuptools configuration
    if let Some(packages) = toml_value
        .get("tool")
        .and_then(|t| t.get("setuptools"))
        .and_then(|setuptools| setuptools.get("packages"))
        .and_then(|p| p.as_array())
    {
        for package_name in packages.iter().filter_map(|package| package.as_str()) {
            source_paths.push(project_root.join(package_name));
        }
    }

    // Check for poetry configuration
    if let Some(packages) = toml_value
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("packages"))
        .and_then(|p| p.as_array())
    {
        for package in packages {
            if let Some(include) = package.get("include").and_then(|i| i.as_str()) {
                let from = package.get("from").and_then(|f| f.as_str()).unwrap_or("");
                source_paths.push(project_root.join(from).join(include));
            }
        }
    }

    // Check for maturin configuration
    if let Some(python_source) = toml_value
        .get("tool")
        .and_then(|t| t.get("maturin"))
        .and_then(|m| m.get("python-source"))
        .and_then(|ps| ps.as_str())
    {
        source_paths.push(project_root.join(python_source));
    }

    // If no specific configuration found, use conventional locations
    if source_paths.is_empty() {
        let src_dir = project_root.join("src");
        if src_dir.exists() {
            source_paths.push(src_dir);
        } else {
            source_paths.push(project_root.to_path_buf());
        }
    }

    source_paths
}

const REQUIREMENTS_TXT_EXCLUDED_DEPS: [&str; 3] = ["python", "poetry", "poetry-core"];

pub fn parse_requirements_txt(requirements_path: &Path) -> Result<HashSet<String>> {
    let content = fs::read_to_string(requirements_path)?;
    let mut dependencies = HashSet::new();

    for line in content.lines() {
        // Skip comments and empty lines
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Skip options (lines starting with -)
        if line.starts_with('-') {
            continue;
        }

        // Extract package name
        let package_name = extract_package_name(line);
        let normalized_name = normalize_package_name(&package_name);

        if !REQUIREMENTS_TXT_EXCLUDED_DEPS.contains(&normalized_name.as_str()) {
            dependencies.insert(normalized_name);
        }
    }

    Ok(dependencies)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_pyproject(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_dependency_groups_disabled_by_default() {
        let content = r#"
[project]
name = "test"
dependencies = ["requests"]

[dependency-groups]
test = ["pytest", "coverage"]
dev = ["ruff", "mypy"]
"#;
        let file = create_temp_pyproject(content);
        let result = parse_pyproject_toml(file.path(), false).unwrap();

        assert!(result.dependencies.contains("requests"));
        assert!(!result.dependencies.contains("pytest"));
        assert!(!result.dependencies.contains("coverage"));
        assert!(!result.dependencies.contains("ruff"));
        assert!(!result.dependencies.contains("mypy"));
    }

    #[test]
    fn test_dependency_groups_enabled() {
        let content = r#"
[project]
name = "test"
dependencies = ["requests"]

[dependency-groups]
test = ["pytest>=7", "coverage[toml]"]
dev = ["ruff~=0.1", "mypy"]
"#;
        let file = create_temp_pyproject(content);
        let result = parse_pyproject_toml(file.path(), true).unwrap();

        assert!(result.dependencies.contains("requests"));
        assert!(result.dependencies.contains("pytest"));
        assert!(result.dependencies.contains("coverage"));
        assert!(result.dependencies.contains("ruff"));
        assert!(result.dependencies.contains("mypy"));
    }

    #[test]
    fn test_dependency_groups_with_include_group() {
        let content = r#"
[project]
name = "test"
dependencies = ["requests"]

[dependency-groups]
coverage = ["coverage[toml]"]
test = ["pytest", {include-group = "coverage"}]
"#;
        let file = create_temp_pyproject(content);
        let result = parse_pyproject_toml(file.path(), true).unwrap();

        assert!(result.dependencies.contains("requests"));
        assert!(result.dependencies.contains("pytest"));
        assert!(result.dependencies.contains("coverage"));
    }

    #[test]
    fn test_dependency_groups_with_transitive_include() {
        let content = r#"
[project]
name = "test"

[dependency-groups]
base = ["base-pkg"]
mid = ["mid-pkg", {include-group = "base"}]
top = ["top-pkg", {include-group = "mid"}]
"#;
        let file = create_temp_pyproject(content);
        let result = parse_pyproject_toml(file.path(), true).unwrap();

        assert!(result.dependencies.contains("base_pkg"));
        assert!(result.dependencies.contains("mid_pkg"));
        assert!(result.dependencies.contains("top_pkg"));
    }

    #[test]
    fn test_dependency_groups_cycle_detection() {
        let content = r#"
[project]
name = "test"

[dependency-groups]
a = ["pkg-a", {include-group = "b"}]
b = ["pkg-b", {include-group = "a"}]
"#;
        let file = create_temp_pyproject(content);
        // Should not infinite loop
        let result = parse_pyproject_toml(file.path(), true).unwrap();

        assert!(result.dependencies.contains("pkg_a"));
        assert!(result.dependencies.contains("pkg_b"));
    }

    #[test]
    fn test_dependency_groups_missing_include_group() {
        let content = r#"
[project]
name = "test"

[dependency-groups]
test = ["pytest", {include-group = "nonexistent"}]
"#;
        let file = create_temp_pyproject(content);
        // Should not fail, just skip the missing group
        let result = parse_pyproject_toml(file.path(), true).unwrap();

        assert!(result.dependencies.contains("pytest"));
    }

    #[test]
    fn test_dependency_groups_normalizes_names() {
        let content = r#"
[project]
name = "test"

[dependency-groups]
test = ["My-Package", "another_package", "UPPER-case"]
"#;
        let file = create_temp_pyproject(content);
        let result = parse_pyproject_toml(file.path(), true).unwrap();

        assert!(result.dependencies.contains("my_package"));
        assert!(result.dependencies.contains("another_package"));
        assert!(result.dependencies.contains("upper_case"));
    }
}
