use pyo3::{prelude::*, types::PyString};
use serde::{Deserialize, Serialize};

use super::RuleSetting;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeadcodeDetection {
    Files,
    Symbols,
}

impl<'py> IntoPyObject<'py> for DeadcodeDetection {
    type Target = PyString;
    type Output = Bound<'py, Self::Target>;
    type Error = std::convert::Infallible;

    fn into_pyobject(
        self,
        py: Python<'py>,
    ) -> Result<Self::Output, <Self as IntoPyObject<'py>>::Error> {
        match self {
            Self::Files => "files".into_pyobject(py),
            Self::Symbols => "symbols".into_pyobject(py),
        }
    }
}

fn default_detect() -> Vec<DeadcodeDetection> {
    vec![DeadcodeDetection::Files]
}

fn is_default_detect(detect: &[DeadcodeDetection]) -> bool {
    detect == [DeadcodeDetection::Files]
}

fn default_severity() -> RuleSetting {
    RuleSetting::Warn
}

fn is_default_severity(severity: &RuleSetting) -> bool {
    *severity == RuleSetting::Warn
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
#[pyclass(get_all, module = "tach.extension")]
pub struct DeadcodeConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entry_points: Vec<String>,
    #[serde(default = "default_detect", skip_serializing_if = "is_default_detect")]
    pub detect: Vec<DeadcodeDetection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_modules: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_symbols: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_decorators: Vec<String>,
    #[serde(
        default = "default_severity",
        skip_serializing_if = "is_default_severity"
    )]
    pub severity: RuleSetting,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub protect_init_files: bool,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub respect_all: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_test_usages: bool,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub ignore_dynamic_modules: bool,
}

impl Default for DeadcodeConfig {
    fn default() -> Self {
        Self {
            entry_points: vec![],
            detect: default_detect(),
            exclude: vec![],
            ignore: vec![],
            public_modules: vec![],
            public_symbols: vec![],
            public_decorators: vec![],
            severity: default_severity(),
            protect_init_files: true,
            respect_all: true,
            include_test_usages: false,
            ignore_dynamic_modules: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProjectConfig;

    #[test]
    fn default_detects_files_only() {
        let config = DeadcodeConfig::default();

        assert_eq!(config.detect, vec![DeadcodeDetection::Files]);
        assert_eq!(config.severity, RuleSetting::Warn);
        assert!(config.protect_init_files);
        assert!(config.respect_all);
        assert!(!config.include_test_usages);
        assert!(config.ignore_dynamic_modules);
    }

    #[test]
    fn project_config_parses_deadcode_table() {
        let config: ProjectConfig = toml::from_str(
            r#"
[deadcode]
entry_points = ["app.py", "pkg.cli:main"]
detect = ["files", "symbols"]
severity = "error"
exclude = ["generated"]
ignore = ["pkg.dead", "pkg.service:unused"]
public_modules = ["pkg.api"]
public_symbols = ["pkg.service:used"]
public_decorators = ["fastapi.get"]
protect_init_files = false
respect_all = false
include_test_usages = true
ignore_dynamic_modules = false
"#,
        )
        .unwrap();

        assert_eq!(
            config.deadcode.detect,
            vec![DeadcodeDetection::Files, DeadcodeDetection::Symbols]
        );
        assert_eq!(config.deadcode.entry_points, vec!["app.py", "pkg.cli:main"]);
        assert_eq!(config.deadcode.severity, RuleSetting::Error);
        assert_eq!(config.deadcode.exclude, vec!["generated"]);
        assert_eq!(
            config.deadcode.ignore,
            vec!["pkg.dead", "pkg.service:unused"]
        );
        assert_eq!(config.deadcode.public_modules, vec!["pkg.api"]);
        assert_eq!(config.deadcode.public_symbols, vec!["pkg.service:used"]);
        assert_eq!(config.deadcode.public_decorators, vec!["fastapi.get"]);
        assert!(!config.deadcode.protect_init_files);
        assert!(!config.deadcode.respect_all);
        assert!(config.deadcode.include_test_usages);
        assert!(!config.deadcode.ignore_dynamic_modules);
    }

    #[test]
    fn unknown_deadcode_field_fails() {
        let result = toml::from_str::<ProjectConfig>(
            r#"
[deadcode]
unknown = true
"#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn default_project_config_omits_deadcode_table() {
        let dumped = toml::to_string(&ProjectConfig::default()).unwrap();

        assert!(!dumped.contains("[deadcode]"));
    }
}
