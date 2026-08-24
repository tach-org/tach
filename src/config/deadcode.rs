use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use super::RuleSetting;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
#[pyclass(get_all, module = "tach.extension")]
pub struct DeadCodeConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entry_points: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<String>,
    #[serde(
        default = "RuleSetting::warn",
        skip_serializing_if = "RuleSetting::is_warn"
    )]
    pub severity: RuleSetting,
}

impl Default for DeadCodeConfig {
    fn default() -> Self {
        Self {
            entry_points: vec![],
            ignore: vec![],
            severity: RuleSetting::warn(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProjectConfig;

    #[test]
    fn deadcode_table_parses() {
        let config: ProjectConfig = toml::from_str(
            r#"
[deadcode]
entry_points = ["app.py", "scripts/*.py", "pkg.cli"]
ignore = ["generated/**", "conftest.py"]
severity = "error"
"#,
        )
        .unwrap();

        assert_eq!(
            config.deadcode.entry_points,
            vec!["app.py", "scripts/*.py", "pkg.cli"]
        );
        assert_eq!(config.deadcode.ignore, vec!["generated/**", "conftest.py"]);
        assert_eq!(config.deadcode.severity, RuleSetting::Error);
    }

    #[test]
    fn deadcode_defaults() {
        let config: ProjectConfig = toml::from_str("").unwrap();

        assert!(config.deadcode.entry_points.is_empty());
        assert!(config.deadcode.ignore.is_empty());
        assert_eq!(config.deadcode.severity, RuleSetting::Warn);
    }

    #[test]
    fn unknown_deadcode_field_fails() {
        let result = toml::from_str::<ProjectConfig>(
            r#"
[deadcode]
unknown_option = true
"#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn default_config_omits_deadcode_table() {
        let dumped = toml::to_string(&ProjectConfig::default()).unwrap();

        assert!(!dumped.contains("[deadcode]"));
    }

    #[test]
    fn populated_deadcode_table_round_trips() {
        let toml_str = r#"
[deadcode]
entry_points = ["app.py"]
ignore = ["generated/**"]
severity = "error"
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        let dumped = toml::to_string(&config).unwrap();
        let reparsed: ProjectConfig = toml::from_str(&dumped).unwrap();

        assert_eq!(config.deadcode, reparsed.deadcode);
    }
}
