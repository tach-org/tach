use pyo3::{prelude::*, types::PyString};
use serde::{Deserialize, Serialize};

use super::utils;

#[derive(Debug, Serialize, Default, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CacheBackend {
    #[default]
    Disk,
}

impl<'py> IntoPyObject<'py> for CacheBackend {
    type Target = PyString;
    type Output = Bound<'py, Self::Target>;
    type Error = std::convert::Infallible;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self {
            Self::Disk => "disk".into_pyobject(py),
        }
    }
}

#[derive(Debug, Serialize, Default, Deserialize, Clone, PartialEq)]
#[pyclass(get_all, module = "tach.extension")]
pub struct CacheConfig {
    #[serde(default, skip_serializing_if = "utils::is_default")]
    pub backend: CacheBackend,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_dependencies: Vec<String>,
}
