use std::{fmt::Display, path::PathBuf};

use pyo3::prelude::*;
use serde::Serialize;
use thiserror::Error;

use crate::config::RuleSetting;

#[derive(Debug, Copy, Clone, Eq, PartialOrd, Ord, Serialize, PartialEq)]
#[pyclass(eq, eq_int, module = "tach.extension")]
pub enum Severity {
    Error,
    Warning,
}

impl Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "Error"),
            Severity::Warning => write!(f, "Warning"),
        }
    }
}

impl TryFrom<&RuleSetting> for Severity {
    type Error = ();

    fn try_from(setting: &RuleSetting) -> Result<Self, <Self as TryFrom<&RuleSetting>>::Error> {
        match setting {
            RuleSetting::Error => Ok(Self::Error),
            RuleSetting::Warn => Ok(Self::Warning),
            RuleSetting::Off => Err(()),
        }
    }
}

#[derive(Error, Debug, Clone, Serialize, PartialEq)]
#[pyclass(module = "tach.extension")]
pub enum ConfigurationDiagnostic {
    #[error("Module containing '{file_mod_path}' not found in project.")]
    ModuleNotFound { file_mod_path: String },

    #[error("Could not find module configuration for module '{module_path}'.")]
    ModuleConfigNotFound { module_path: String },

    #[error("Layer '{layer}' is not defined in the project.")]
    UnknownLayer { layer: String },

    #[error("No first-party imports were found. You may need to use 'tach mod' to update your Python source roots. Docs: https://detachhead.github.io/dtach/usage/configuration#source-roots")]
    NoFirstPartyImportsFound(),

    #[error("Unexpected error: No checks were enabled.")]
    NoChecksEnabled(),

    #[error("Skipped '{file_path}' due to a syntax error.")]
    SkippedFileSyntaxError { file_path: String },

    #[error("Skipped '{file_path}' due to an I/O error.")]
    SkippedFileIoError { file_path: String },

    #[error("Skipped '{file_path}' due to a parsing error.")]
    SkippedPyProjectParsingError { file_path: String },

    #[error("Skipped '{file_path}' due to an unknown error.")]
    SkippedUnknownError { file_path: String },
}

#[derive(Error, Debug, Clone, Serialize, PartialEq)]
#[pyclass(module = "tach.extension")]
pub enum CodeDiagnostic {
    #[error(
        "The path '{dependency}' is not part of the public interface for '{definition_module}'."
    )]
    PrivateDependency {
        dependency: String,
        definition_module: String,
        usage_module: String,
    },

    #[error("The dependency '{dependency}' (from module '{definition_module}') matches an interface but does not match the expected data type ('{expected_data_type}').")]
    InvalidDataTypeExport {
        dependency: String,
        definition_module: String,
        usage_module: String,
        expected_data_type: String,
    },

    #[error("Cannot use '{dependency}'. Module '{usage_module}' cannot depend on '{definition_module}'.")]
    UndeclaredDependency {
        dependency: String,
        usage_module: String,
        definition_module: String,
    },

    #[error("Dependency '{dependency}' is deprecated. Module '{usage_module}' should not depend on '{definition_module}'.")]
    DeprecatedDependency {
        dependency: String,
        usage_module: String,
        definition_module: String,
    },

    #[error("Cannot use '{dependency}'. Module '{usage_module}' cannot depend on '{definition_module}'.")]
    ForbiddenDependency {
        dependency: String,
        usage_module: String,
        definition_module: String,
    },

    #[error("Cannot use '{dependency}'. Layer '{usage_layer}' ('{usage_module}') is lower than layer '{definition_layer}' ('{definition_module}').")]
    LayerViolation {
        dependency: String,
        usage_module: String,
        usage_layer: String,
        definition_module: String,
        definition_layer: String,
    },

    #[error("Cannot use '{dependency}'. Layer '{usage_layer}' ('{usage_module}') cannot import from layer '{definition_layer}' ('{definition_module}') because layer '{closed_layer}' is a closed layer between them.")]
    ClosedLayerViolation {
        dependency: String,
        usage_module: String,
        usage_layer: String,
        definition_module: String,
        definition_layer: String,
        closed_layer: String,
    },

    #[error("Dependency '{dependency}' is unnecessarily ignored by a directive.")]
    UnnecessarilyIgnoredDependency { dependency: String },

    #[error("Ignore directive is unused.")]
    UnusedIgnoreDirective(),

    #[error("Ignore directive is missing a reason.")]
    MissingIgnoreDirectiveReason(),

    #[error("Dependency '{dependency}' is not declared in package '{package_name}'.")]
    UndeclaredExternalDependency {
        dependency: String,
        package_name: String,
    },

    #[error("External package '{package_module_name}' is not used in package '{package_name}'.")]
    UnusedExternalDependency {
        package_module_name: String,
        package_name: String,
    },

    #[error(
        "Module '{usage_module}' does not declare a dependency on external package '{dependency}'."
    )]
    ModuleUndeclaredExternalDependency {
        dependency: String,
        usage_module: String,
    },

    #[error("Module '{usage_module}' cannot depend on external package '{dependency}'.")]
    ModuleForbiddenExternalDependency {
        dependency: String,
        usage_module: String,
    },
}

impl CodeDiagnostic {
    pub fn is_ignore_directive_related(&self) -> bool {
        matches!(
            self,
            CodeDiagnostic::UnusedIgnoreDirective()
                | CodeDiagnostic::MissingIgnoreDirectiveReason()
        )
    }

    pub fn dependency(&self) -> Option<&str> {
        match self {
            CodeDiagnostic::PrivateDependency { dependency, .. }
            | CodeDiagnostic::InvalidDataTypeExport { dependency, .. }
            | CodeDiagnostic::UndeclaredDependency { dependency, .. }
            | CodeDiagnostic::DeprecatedDependency { dependency, .. }
            | CodeDiagnostic::ForbiddenDependency { dependency, .. }
            | CodeDiagnostic::LayerViolation { dependency, .. }
            | CodeDiagnostic::ClosedLayerViolation { dependency, .. }
            | CodeDiagnostic::UnnecessarilyIgnoredDependency { dependency, .. } => Some(dependency),
            CodeDiagnostic::UnusedIgnoreDirective() => None,
            CodeDiagnostic::MissingIgnoreDirectiveReason() => None,
            CodeDiagnostic::UndeclaredExternalDependency { dependency, .. }
            | CodeDiagnostic::ModuleUndeclaredExternalDependency { dependency, .. }
            | CodeDiagnostic::ModuleForbiddenExternalDependency { dependency, .. } => {
                Some(dependency)
            }
            CodeDiagnostic::UnusedExternalDependency {
                package_module_name,
                ..
            } => Some(package_module_name),
        }
    }

    pub fn usage_module(&self) -> Option<&str> {
        match self {
            CodeDiagnostic::PrivateDependency { usage_module, .. }
            | CodeDiagnostic::InvalidDataTypeExport { usage_module, .. }
            | CodeDiagnostic::UndeclaredDependency { usage_module, .. }
            | CodeDiagnostic::DeprecatedDependency { usage_module, .. }
            | CodeDiagnostic::ForbiddenDependency { usage_module, .. }
            | CodeDiagnostic::LayerViolation { usage_module, .. }
            | CodeDiagnostic::ClosedLayerViolation { usage_module, .. }
            | CodeDiagnostic::ModuleUndeclaredExternalDependency { usage_module, .. }
            | CodeDiagnostic::ModuleForbiddenExternalDependency { usage_module, .. } => {
                Some(usage_module)
            }
            _ => None,
        }
    }

    pub fn definition_module(&self) -> Option<&str> {
        match self {
            CodeDiagnostic::PrivateDependency {
                definition_module, ..
            }
            | CodeDiagnostic::InvalidDataTypeExport {
                definition_module, ..
            }
            | CodeDiagnostic::UndeclaredDependency {
                definition_module, ..
            }
            | CodeDiagnostic::DeprecatedDependency {
                definition_module, ..
            }
            | CodeDiagnostic::ForbiddenDependency {
                definition_module, ..
            }
            | CodeDiagnostic::LayerViolation {
                definition_module, ..
            }
            | CodeDiagnostic::ClosedLayerViolation {
                definition_module, ..
            } => Some(definition_module),
            _ => None,
        }
    }

    pub fn usage_layer(&self) -> Option<&str> {
        match self {
            CodeDiagnostic::LayerViolation { usage_layer, .. }
            | CodeDiagnostic::ClosedLayerViolation { usage_layer, .. } => Some(usage_layer),
            _ => None,
        }
    }

    pub fn definition_layer(&self) -> Option<&str> {
        match self {
            CodeDiagnostic::LayerViolation {
                definition_layer, ..
            }
            | CodeDiagnostic::ClosedLayerViolation {
                definition_layer, ..
            } => Some(definition_layer),
            _ => None,
        }
    }

    pub fn closed_layer(&self) -> Option<&str> {
        match self {
            CodeDiagnostic::ClosedLayerViolation { closed_layer, .. } => Some(closed_layer),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[pyclass(module = "tach.extension")]
pub enum DiagnosticDetails {
    Code(CodeDiagnostic),
    Configuration(ConfigurationDiagnostic),
}

impl Display for DiagnosticDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnosticDetails::Code(code) => write!(f, "{}", code),
            DiagnosticDetails::Configuration(config) => write!(f, "{}", config),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[pyclass(module = "tach.extension")]
pub enum Diagnostic {
    Global {
        severity: Severity,
        details: DiagnosticDetails,
    },
    Located {
        file_path: PathBuf,
        line_number: usize, // Line number where the diagnostic should be attached
        original_line_number: Option<usize>, // Optional line number to point to the origin of the diagnostic
        severity: Severity,
        details: DiagnosticDetails,
    },
}

impl Diagnostic {
    pub fn new_global(severity: Severity, details: DiagnosticDetails) -> Self {
        Self::Global { severity, details }
    }

    pub fn new_located(
        severity: Severity,
        details: DiagnosticDetails,
        file_path: PathBuf,
        line_number: usize,
        original_line_number: Option<usize>,
    ) -> Self {
        Self::Located {
            severity,
            details,
            file_path,
            line_number,
            original_line_number,
        }
    }

    pub fn new_located_error(
        file_path: PathBuf,
        line_number: usize,
        original_line_number: Option<usize>,
        details: DiagnosticDetails,
    ) -> Self {
        Self::Located {
            file_path,
            line_number,
            original_line_number,
            severity: Severity::Error,
            details,
        }
    }

    pub fn new_located_warning(
        file_path: PathBuf,
        line_number: usize,
        original_line_number: Option<usize>,
        details: DiagnosticDetails,
    ) -> Self {
        Self::Located {
            file_path,
            line_number,
            original_line_number,
            severity: Severity::Warning,
            details,
        }
    }

    pub fn new_global_error(details: DiagnosticDetails) -> Self {
        Self::Global {
            severity: Severity::Error,
            details,
        }
    }

    pub fn new_global_warning(details: DiagnosticDetails) -> Self {
        Self::Global {
            severity: Severity::Warning,
            details,
        }
    }

    pub fn details(&self) -> &DiagnosticDetails {
        match self {
            Self::Global { details, .. } => details,
            Self::Located { details, .. } => details,
        }
    }

    pub fn message(&self) -> String {
        self.details().to_string()
    }

    pub fn severity(&self) -> Severity {
        match self {
            Self::Global { severity, .. } => *severity,
            Self::Located { severity, .. } => *severity,
        }
    }

    pub fn file_path(&self) -> Option<&PathBuf> {
        match self {
            Self::Global { .. } => None,
            Self::Located { file_path, .. } => Some(file_path),
        }
    }

    pub fn line_number(&self) -> Option<usize> {
        match self {
            Self::Global { .. } => None,
            Self::Located { line_number, .. } => Some(*line_number),
        }
    }

    pub fn original_line_number(&self) -> Option<usize> {
        match self {
            Self::Global { .. } => None,
            Self::Located {
                original_line_number,
                ..
            } => *original_line_number,
        }
    }

    pub fn is_ignore_directive_related(&self) -> bool {
        match self.details() {
            DiagnosticDetails::Code(details) => details.is_ignore_directive_related(),
            _ => false,
        }
    }

    pub fn dependency(&self) -> Option<&str> {
        match self.details() {
            DiagnosticDetails::Code(details) => details.dependency(),
            _ => None,
        }
    }

    pub fn usage_module(&self) -> Option<&str> {
        match self.details() {
            DiagnosticDetails::Code(details) => details.usage_module(),
            _ => None,
        }
    }

    pub fn definition_module(&self) -> Option<&str> {
        match self.details() {
            DiagnosticDetails::Code(details) => details.definition_module(),
            _ => None,
        }
    }

    pub fn usage_layer(&self) -> Option<&str> {
        match self.details() {
            DiagnosticDetails::Code(details) => details.usage_layer(),
            _ => None,
        }
    }

    pub fn definition_layer(&self) -> Option<&str> {
        match self.details() {
            DiagnosticDetails::Code(details) => details.definition_layer(),
            _ => None,
        }
    }

    pub fn closed_layer(&self) -> Option<&str> {
        match self.details() {
            DiagnosticDetails::Code(details) => details.closed_layer(),
            _ => None,
        }
    }
}

#[pymethods]
impl Diagnostic {
    pub fn is_code(&self) -> bool {
        matches!(self.details(), DiagnosticDetails::Code { .. })
    }

    pub fn is_configuration(&self) -> bool {
        matches!(self.details(), DiagnosticDetails::Configuration { .. })
    }

    pub fn is_dependency_error(&self) -> bool {
        matches!(
            self.details(),
            DiagnosticDetails::Code(CodeDiagnostic::UndeclaredDependency { .. })
                | DiagnosticDetails::Code(CodeDiagnostic::DeprecatedDependency { .. })
                | DiagnosticDetails::Code(CodeDiagnostic::ForbiddenDependency { .. })
                | DiagnosticDetails::Code(CodeDiagnostic::LayerViolation { .. })
        )
    }

    pub fn is_interface_error(&self) -> bool {
        matches!(
            self.details(),
            DiagnosticDetails::Code(CodeDiagnostic::PrivateDependency { .. })
                | DiagnosticDetails::Code(CodeDiagnostic::InvalidDataTypeExport { .. })
        )
    }

    pub fn is_deprecated(&self) -> bool {
        matches!(
            self.details(),
            DiagnosticDetails::Code(CodeDiagnostic::DeprecatedDependency { .. })
        )
    }

    pub fn is_error(&self) -> bool {
        matches!(self.severity(), Severity::Error)
    }

    pub fn is_warning(&self) -> bool {
        matches!(self.severity(), Severity::Warning)
    }

    #[pyo3(name = "to_string")]
    pub fn to_pystring(&self) -> String {
        self.message()
    }

    pub fn pyfile_path(&self) -> Option<String> {
        self.file_path()
            .map(|path| path.to_string_lossy().to_string())
    }

    pub fn pyline_number(&self) -> Option<usize> {
        self.line_number()
    }
}

#[pyfunction(signature = (diagnostics, pretty_print = false))]
pub fn serialize_diagnostics_json(diagnostics: Vec<Diagnostic>, pretty_print: bool) -> String {
    if pretty_print {
        serde_json::to_string_pretty(&diagnostics).unwrap()
    } else {
        serde_json::to_string(&diagnostics).unwrap()
    }
}
