pub mod cache;
pub mod checks;
pub mod cli;
pub mod colors;
pub mod commands;
pub mod config;
pub mod dep_map;
pub mod dependencies;
pub mod diagnostics;
pub mod external;
pub mod filesystem;
pub mod interfaces;
pub mod interrupt;
pub mod lsp;
pub mod modules;
pub mod parsing;
pub mod processors;
pub mod python;
pub mod resolvers;
pub mod tests;
use commands::{check, report, server, sync, test};
use diagnostics::serialize_diagnostics_json;
use pyo3::{prelude::*, types::PyTuple};
use std::path::PathBuf;

use pyo3::exceptions::{PyKeyboardInterrupt, PyOSError, PySyntaxError, PyValueError};

mod errors {
    pyo3::import_exception!(tach.errors, TachCircularDependencyError);
    pyo3::import_exception!(tach.errors, TachVisibilityError);
    pyo3::import_exception!(tach.errors, TachSetupError);
    pyo3::import_exception!(tach.errors, TachConfigError);
}

impl From<processors::import::ImportParseError> for PyErr {
    fn from(err: processors::import::ImportParseError) -> Self {
        match err {
            processors::import::ImportParseError::Parsing { file: _, source: _ } => {
                PySyntaxError::new_err(err.to_string())
            }
            _ => PyOSError::new_err(err.to_string()),
        }
    }
}

impl From<report::ReportCreationError> for PyErr {
    fn from(err: report::ReportCreationError) -> Self {
        PyValueError::new_err(err.to_string())
    }
}

impl From<cache::CacheError> for PyErr {
    fn from(err: cache::CacheError) -> Self {
        PyValueError::new_err(err.to_string())
    }
}

impl From<check::CheckError> for PyErr {
    fn from(err: check::CheckError) -> Self {
        match err {
            check::CheckError::Interrupt => PyKeyboardInterrupt::new_err(err.to_string()),
            check::CheckError::ModuleTree(modules::error::ModuleTreeError::CircularDependency(
                c,
            )) => errors::TachCircularDependencyError::new_err(c),
            check::CheckError::ModuleTree(
                modules::error::ModuleTreeError::VisibilityViolation(v),
            ) => errors::TachVisibilityError::new_err(v),
            check::CheckError::Configuration(err) => errors::TachConfigError::new_err(err),
            _ => PyValueError::new_err(err.to_string()),
        }
    }
}

impl From<python::error::ParsingError> for PyErr {
    fn from(err: python::error::ParsingError) -> Self {
        match err {
            python::error::ParsingError::PythonParse(err) => {
                PySyntaxError::new_err(err.to_string())
            }
            python::error::ParsingError::Io(err) => PyOSError::new_err(err.to_string()),
            python::error::ParsingError::Filesystem(err) => PyOSError::new_err(err.to_string()),
            python::error::ParsingError::InvalidSyntax => PySyntaxError::new_err(err.to_string()),
        }
    }
}

impl From<parsing::error::ParsingError> for PyErr {
    fn from(err: parsing::error::ParsingError) -> Self {
        match err {
            parsing::error::ParsingError::Io(err) => PyOSError::new_err(err.to_string()),
            parsing::error::ParsingError::Filesystem(err) => PyOSError::new_err(err.to_string()),
            parsing::error::ParsingError::TomlParse(err) => PyValueError::new_err(err.to_string()),
            parsing::error::ParsingError::MissingField(err) => PyValueError::new_err(err),
            parsing::error::ParsingError::ModulePath(err) => PyValueError::new_err(err),
            parsing::error::ParsingError::SourceRootResolution(err) => {
                PyValueError::new_err(err.to_string())
            }
        }
    }
}
impl From<sync::SyncError> for PyErr {
    fn from(err: sync::SyncError) -> Self {
        match err {
            sync::SyncError::FileWrite(err) => PyOSError::new_err(err.to_string()),
            sync::SyncError::TomlSerialize(err) => PyOSError::new_err(err.to_string()),
            sync::SyncError::CheckError(err) => err.into(),
            sync::SyncError::RootModuleViolation(err) => PyValueError::new_err(err.to_string()),
            sync::SyncError::EditError(err) => PyValueError::new_err(err.to_string()),
            sync::SyncError::SourceRootResolution(err) => PyValueError::new_err(err.to_string()),
            sync::SyncError::FileWalker(err) => PyOSError::new_err(err.to_string()),
        }
    }
}

impl From<lsp::error::ServerError> for PyErr {
    fn from(err: lsp::error::ServerError) -> Self {
        match err {
            lsp::error::ServerError::Initialize => errors::TachSetupError::new_err(err.to_string()),
            _ => PyOSError::new_err(err.to_string()),
        }
    }
}

impl From<config::edit::EditError> for PyErr {
    fn from(err: config::edit::EditError) -> Self {
        PyValueError::new_err(err.to_string())
    }
}

impl From<config::error::ConfigError> for PyErr {
    fn from(err: config::error::ConfigError) -> Self {
        PyValueError::new_err(err.to_string())
    }
}

impl<'py> IntoPyObject<'py> for modules::error::VisibilityErrorInfo {
    type Target = PyTuple;
    type Output = Bound<'py, Self::Target>;
    type Error = PyErr;
    fn into_pyobject(self, py: pyo3::prelude::Python<'py>) -> Result<Self::Output, Self::Error> {
        (
            self.dependent_module,
            self.dependency_module,
            self.visibility,
        )
            .into_pyobject(py)
    }
}

/// Parse project config
#[pyfunction]
fn parse_project_config(
    filepath: PathBuf,
) -> parsing::config::Result<(config::ProjectConfig, bool)> {
    parsing::config::parse_project_config(filepath)
}

/// Parse project config from pyproject.toml
#[pyfunction]
fn parse_project_config_from_pyproject(
    filepath: PathBuf,
) -> parsing::config::Result<config::ProjectConfig> {
    parsing::config::parse_project_config_from_pyproject(filepath)
}

#[pyfunction]
#[pyo3(signature = (config))]
fn dump_project_config_to_toml(
    config: &mut config::ProjectConfig,
) -> Result<String, sync::SyncError> {
    // TODO: Error handling hack
    parsing::config::dump_project_config_to_toml(config).map_err(sync::SyncError::TomlSerialize)
}

/// Get first-party imports from file_path
#[pyfunction]
fn get_project_imports(
    project_root: PathBuf,
    source_roots: Vec<PathBuf>,
    file_path: PathBuf,
    project_config: config::ProjectConfig,
) -> processors::import::Result<Vec<dependencies::LocatedImport>> {
    commands::helpers::import::get_located_project_imports(
        &project_root,
        &source_roots,
        &file_path,
        &project_config,
    )
}

/// Get third-party imports from file_path
#[pyfunction]
fn get_external_imports(
    project_root: PathBuf,
    source_roots: Vec<PathBuf>,
    file_path: PathBuf,
    project_config: config::ProjectConfig,
) -> processors::import::Result<Vec<dependencies::LocatedImport>> {
    commands::helpers::import::get_located_external_imports(
        &project_root,
        &source_roots,
        &file_path,
        &project_config,
    )
}

/// Validate external dependency imports against pyproject.toml dependencies
#[pyfunction]
fn check_external_dependencies(
    project_root: PathBuf,
    project_config: config::ProjectConfig,
) -> check::check_external::Result<Vec<diagnostics::Diagnostic>> {
    check::check_external::check(&project_root, &project_config)
}

/// Create a report of dependencies and usages of a given path
#[pyfunction]
#[pyo3(signature = (project_root, project_config, path, include_dependency_modules, include_usage_modules, skip_dependencies, skip_usages, raw))]
fn create_dependency_report(
    project_root: PathBuf,
    project_config: &config::ProjectConfig,
    path: PathBuf,
    include_dependency_modules: Option<Vec<String>>,
    include_usage_modules: Option<Vec<String>>,
    skip_dependencies: bool,
    skip_usages: bool,
    raw: bool,
) -> report::Result<String> {
    report::create_dependency_report(
        &project_root,
        project_config,
        &path,
        include_dependency_modules,
        include_usage_modules,
        skip_dependencies,
        skip_usages,
        raw,
    )
}

#[pyfunction]
fn create_computation_cache_key(
    project_root: PathBuf,
    source_roots: Vec<PathBuf>,
    action: String,
    py_interpreter_version: String,
    file_dependencies: Vec<String>,
    env_dependencies: Vec<String>,
    backend: String,
    respect_gitignore: bool,
) -> String {
    cache::create_computation_cache_key(
        &project_root,
        &source_roots,
        action,
        py_interpreter_version,
        file_dependencies,
        env_dependencies,
        backend,
        respect_gitignore,
    )
}

#[pyfunction]
fn check_computation_cache(
    project_root: PathBuf,
    cache_key: String,
) -> cache::Result<Option<cache::ComputationCacheValue>> {
    cache::check_computation_cache(&project_root, cache_key)
}

#[pyfunction]
fn update_computation_cache(
    project_root: PathBuf,
    cache_key: String,
    value: cache::ComputationCacheValue,
) -> cache::Result<Option<cache::ComputationCacheValue>> {
    cache::update_computation_cache(&project_root, cache_key, value)
}

#[pyfunction]
#[pyo3(name = "check")]
fn check_internal(
    project_root: PathBuf,
    project_config: &config::ProjectConfig,
    dependencies: bool,
    interfaces: bool,
) -> check::check_internal::Result<Vec<diagnostics::Diagnostic>> {
    check::check_internal(&project_root, project_config, dependencies, interfaces)
}

#[pyfunction]
pub fn format_diagnostics(diagnostics: Vec<diagnostics::Diagnostic>) -> String {
    check::format::format_diagnostics(&diagnostics)
}

#[pyfunction]
fn detect_unused_dependencies(
    project_root: PathBuf,
    project_config: &mut config::ProjectConfig,
) -> Result<Vec<sync::UnusedDependencies>, sync::SyncError> {
    sync::detect_unused_dependencies(project_root, project_config)
}

#[pyfunction]
#[pyo3(signature = (project_root, project_config, add = false))]
pub fn sync_project(
    project_root: PathBuf,
    project_config: config::ProjectConfig,
    add: bool,
) -> Result<(), sync::SyncError> {
    sync::sync_project(project_root, project_config, add)
}

#[pyfunction]
fn run_server(
    project_root: PathBuf,
    project_config: config::ProjectConfig,
) -> Result<(), lsp::error::ServerError> {
    server::run_server(project_root, project_config)
}

#[pyfunction]
fn serialize_modules_json(modules: Vec<config::ModuleConfig>) -> String {
    config::serialize_modules_json(&modules)
}

#[pymodule]
fn extension(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    interrupt::setup_interrupt_handler();
    m.add_class::<config::ProjectConfig>()?;
    m.add_class::<config::ModuleConfig>()?;
    m.add_class::<config::InterfaceConfig>()?;
    m.add_class::<config::RulesConfig>()?;
    m.add_class::<config::DependencyConfig>()?;
    m.add_class::<config::MapConfig>()?;
    m.add_class::<diagnostics::Diagnostic>()?;
    m.add_class::<dep_map::PyDependentMap>()?;
    m.add_class::<dep_map::PyDirection>()?;
    m.add_class::<test::TachPytestPluginHandler>()?;
    m.add_function(wrap_pyfunction!(parse_project_config, m)?)?;
    m.add_function(wrap_pyfunction!(parse_project_config_from_pyproject, m)?)?;
    m.add_function(wrap_pyfunction!(get_project_imports, m)?)?;
    m.add_function(wrap_pyfunction!(get_external_imports, m)?)?;
    m.add_function(wrap_pyfunction!(check_external_dependencies, m)?)?;
    m.add_function(wrap_pyfunction!(create_dependency_report, m)?)?;
    m.add_function(wrap_pyfunction!(create_computation_cache_key, m)?)?;
    m.add_function(wrap_pyfunction!(check_computation_cache, m)?)?;
    m.add_function(wrap_pyfunction!(update_computation_cache, m)?)?;
    m.add_function(wrap_pyfunction!(dump_project_config_to_toml, m)?)?;
    m.add_function(wrap_pyfunction!(check_internal, m)?)?;
    m.add_function(wrap_pyfunction!(format_diagnostics, m)?)?;
    m.add_function(wrap_pyfunction!(detect_unused_dependencies, m)?)?;
    m.add_function(wrap_pyfunction!(sync_project, m)?)?;
    m.add_function(wrap_pyfunction!(run_server, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_modules_json, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_diagnostics_json, m)?)?;
    Ok(())
}
