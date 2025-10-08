use pyo3::{prelude::*, types::PyString};
use serde::{Deserialize, Serialize};

pub const ROOT_MODULE_SENTINEL_TAG: &str = "<root>";

#[derive(Debug, Serialize, Default, Deserialize, Copy, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RootModuleTreatment {
    Allow,
    Forbid,
    #[default]
    Ignore,
    DependenciesOnly,
}

impl<'py> IntoPyObject<'py> for RootModuleTreatment {
    type Target = PyString;
    type Output = Bound<'py, Self::Target>;
    type Error = std::convert::Infallible;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self {
            Self::Allow => "allow".into_pyobject(py),
            Self::Forbid => "forbid".into_pyobject(py),
            Self::Ignore => "ignore".into_pyobject(py),
            Self::DependenciesOnly => "dependenciesonly".into_pyobject(py),
        }
    }
}
