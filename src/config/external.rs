use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::ops::Not;

#[derive(Debug, Serialize, Default, Deserialize, Clone, PartialEq)]
#[pyclass(get_all, module = "tach.extension")]
pub struct ExternalDependencyConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rename: Vec<String>,
    #[serde(default, skip_serializing_if = "Not::not")]
    pub include_dependency_groups: bool,
}
