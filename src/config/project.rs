use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::iter;
use std::ops::Not;
use std::path::PathBuf;

use crate::config::RespectGitIgnore;
use crate::filesystem::{self, module_path_is_included_in_paths};
use crate::resolvers::SourceRootResolver;

use super::cache::CacheConfig;
use super::domain::LocatedDomainConfig;
use super::edit::{ConfigEdit, ConfigEditor, EditError};
use super::error::ConfigError;
use super::external::ExternalDependencyConfig;
use super::interfaces::InterfaceConfig;
use super::layers::LayerConfig;
use super::map::MapConfig;
use super::modules::{DependencyConfig, ModuleConfig, deserialize_modules, serialize_modules};
use super::plugins::PluginsConfig;
use super::root_module::RootModuleTreatment;
use super::rules::RulesConfig;
use super::utils;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PyProjectWrapper {
    tool: ToolTable,
}

impl From<PyProjectWrapper> for ProjectConfig {
    fn from(val: PyProjectWrapper) -> Self {
        val.tool.tach
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ToolTable {
    tach: ProjectConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[pyclass(module = "tach.extension")]
pub struct ProjectConfig {
    #[serde(
        default,
        deserialize_with = "deserialize_modules",
        serialize_with = "serialize_modules"
    )]
    #[pyo3(get)]
    pub modules: Vec<ModuleConfig>,
    #[serde(default)]
    #[pyo3(get)]
    pub interfaces: Vec<InterfaceConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[pyo3(get)]
    pub layers: Vec<LayerConfig>,
    #[serde(default, skip_serializing_if = "utils::is_default")]
    #[pyo3(get)]
    pub cache: CacheConfig,
    #[serde(default, skip_serializing_if = "utils::is_default")]
    #[pyo3(get)]
    pub external: ExternalDependencyConfig,
    #[serde(default, skip_serializing_if = "utils::is_default")]
    #[pyo3(get)]
    pub map: MapConfig,
    #[serde(default)]
    #[pyo3(get, set)]
    pub exclude: Vec<String>,
    #[serde(default = "default_source_roots")]
    #[pyo3(get, set)]
    pub source_roots: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Not::not")]
    #[pyo3(get)]
    pub exact: bool,
    #[serde(
        default = "utils::default_true",
        skip_serializing_if = "utils::is_true"
    )]
    #[pyo3(get, set)]
    pub ignore_type_checking_imports: bool,
    #[serde(default, skip_serializing_if = "Not::not")]
    #[pyo3(get, set)]
    pub include_string_imports: bool,
    #[serde(default, skip_serializing_if = "Not::not")]
    #[pyo3(get)]
    pub forbid_circular_dependencies: bool,
    #[serde(default, skip_serializing_if = "Not::not")]
    #[pyo3(get, set)]
    pub layers_explicit_depends_on: bool,
    #[serde(default, skip_serializing_if = "utils::is_default")]
    #[pyo3(get)]
    pub respect_gitignore: RespectGitIgnore,
    #[serde(skip)]
    #[pyo3(get)]
    pub use_regex_matching: bool,
    #[serde(default, skip_serializing_if = "utils::is_default")]
    #[pyo3(get)]
    pub root_module: RootModuleTreatment,
    #[serde(default, skip_serializing_if = "utils::is_default")]
    #[pyo3(get)]
    pub rules: RulesConfig,
    #[serde(default, skip_serializing_if = "utils::is_default")]
    #[pyo3(get)]
    pub plugins: PluginsConfig,
    #[serde(skip)]
    pub domains: Vec<LocatedDomainConfig>,
    #[serde(skip)]
    pub pending_edits: Vec<ConfigEdit>,
    // If location is None, the config is not on disk
    #[serde(skip)]
    pub location: Option<PathBuf>,
}

pub fn default_source_roots() -> Vec<PathBuf> {
    vec![PathBuf::from(".")]
}

pub const DEFAULT_EXCLUDE_PATHS: [&str; 5] = [
    "**/tests",
    "**/docs",
    "**/*__pycache__",
    "**/*egg-info",
    "**/venv",
];

pub fn default_excludes() -> Vec<String> {
    DEFAULT_EXCLUDE_PATHS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            // special defaults
            exclude: default_excludes(),
            source_roots: default_source_roots(),
            ignore_type_checking_imports: true,
            // normal defaults
            modules: Default::default(),
            interfaces: Default::default(),
            layers: Default::default(),
            cache: Default::default(),
            external: Default::default(),
            exact: Default::default(),
            include_string_imports: Default::default(),
            forbid_circular_dependencies: Default::default(),
            layers_explicit_depends_on: Default::default(),
            respect_gitignore: RespectGitIgnore::True,
            use_regex_matching: Default::default(),
            root_module: Default::default(),
            rules: Default::default(),
            plugins: Default::default(),
            domains: Default::default(),
            pending_edits: Default::default(),
            location: Default::default(),
            map: Default::default(),
        }
    }
}

impl ProjectConfig {
    pub fn dependencies_for_module(&self, module: &str) -> Option<&Vec<DependencyConfig>> {
        self.all_modules()
            .find(|mod_config| mod_config.path == module)
            .map(|mod_config| mod_config.depends_on.as_ref())?
    }

    pub fn absolute_source_roots(&self) -> Result<Vec<PathBuf>, ConfigError> {
        let project_root = self
            .location
            .as_ref()
            .map(|path| path.parent().unwrap().to_path_buf())
            .ok_or(ConfigError::ConfigDoesNotExist)?;
        let file_walker =
            filesystem::FSWalker::try_new(&project_root, &self.exclude, self.respect_gitignore)?;
        let source_root_resolver = SourceRootResolver::new(&project_root, &file_walker);
        Ok(source_root_resolver.resolve(&self.source_roots)?)
    }

    pub fn with_dependencies_removed(&self) -> Self {
        Self {
            modules: self
                .modules
                .iter()
                .map(|module| module.with_dependencies_removed())
                .collect(),
            domains: self
                .domains
                .iter()
                .map(|domain| domain.with_dependencies_removed())
                .collect(),
            ..self.clone()
        }
    }

    pub fn add_domain(&mut self, domain: LocatedDomainConfig) {
        self.domains.push(domain);
    }

    pub fn add_root_module(&mut self) {
        self.modules.push(ModuleConfig::new_root_config());
    }

    pub fn all_modules(&self) -> impl Iterator<Item = &ModuleConfig> {
        self.modules
            .iter()
            .chain(self.domains.iter().flat_map(|domain| domain.modules()))
    }

    pub fn all_interfaces(&self) -> impl Iterator<Item = &InterfaceConfig> {
        self.interfaces
            .iter()
            .chain(self.domains.iter().flat_map(|domain| domain.interfaces()))
    }
}

impl ConfigEditor for ProjectConfig {
    fn enqueue_edit(&mut self, edit: &ConfigEdit) -> Result<(), EditError> {
        // Enqueue the edit for any relevant domains
        let domain_results = self
            .domains
            .iter_mut()
            .map(|domain| domain.enqueue_edit(edit))
            .collect::<Vec<Result<(), EditError>>>();

        let result = match edit {
            ConfigEdit::CreateModule { .. }
            | ConfigEdit::DeleteModule { .. }
            | ConfigEdit::MarkModuleAsUtility { .. }
            | ConfigEdit::UnmarkModuleAsUtility { .. }
            | ConfigEdit::AddDependency { .. }
            | ConfigEdit::RemoveDependency { .. } => {
                if !domain_results.iter().any(|r| r.is_ok()) {
                    // If no domain accepted the edit, enqueue the edit
                    self.pending_edits.push(edit.clone());
                    Ok(())
                } else {
                    Err(EditError::NotApplicable)
                }
            }
            ConfigEdit::AddSourceRoot { .. } | ConfigEdit::RemoveSourceRoot { .. } => {
                // Source root edits are always applicable to project config
                self.pending_edits.push(edit.clone());
                Ok(())
            }
        };

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                // If any domain enqueued the edit, return Ok
                if domain_results.iter().any(|r| r.is_ok()) {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    fn apply_edits(&mut self) -> Result<(), EditError> {
        for domain in &mut self.domains {
            domain.apply_edits()?;
        }

        if self.pending_edits.is_empty() {
            return Ok(());
        }
        let config_path = self
            .location
            .as_ref()
            .ok_or(EditError::ConfigDoesNotExist)?;

        let toml_str =
            std::fs::read_to_string(config_path).map_err(|_| EditError::ConfigDoesNotExist)?;
        let mut doc = toml_str
            .parse::<toml_edit::DocumentMut>()
            .map_err(|_| EditError::ParsingFailed)?;

        let is_pyproject = config_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "pyproject.toml")
            .unwrap_or(false);

        let root_table = if is_pyproject {
            if !doc.contains_key("tool") {
                doc["tool"] = toml_edit::Item::Table(toml_edit::Table::new());
            }
            let tool = doc["tool"].as_table_mut().ok_or(EditError::ParsingFailed)?;
            if !tool.contains_key("tach") {
                tool.insert("tach", toml_edit::Item::Table(toml_edit::Table::new()));
            }
            tool["tach"]
                .as_table_mut()
                .ok_or(EditError::ParsingFailed)?
        } else {
            &mut doc
        };

        for edit in &self.pending_edits {
            match edit {
                ConfigEdit::CreateModule { path } => {
                    let mut module_table = toml_edit::Table::new();
                    module_table.insert("path", toml_edit::value(path));
                    module_table.insert("depends_on", toml_edit::value(toml_edit::Array::new()));

                    match root_table.get_mut("modules") {
                        // If modules is a regular array (modules = []) or doesn't exist, convert it to array of tables
                        None | Some(toml_edit::Item::Value(toml_edit::Value::Array(_))) => {
                            let mut array = toml_edit::ArrayOfTables::new();
                            array.push(module_table);
                            root_table["modules"] = toml_edit::Item::ArrayOfTables(array);
                        }
                        // If modules is already an array of tables ([[modules]]), just push
                        Some(toml_edit::Item::ArrayOfTables(array)) => {
                            array.push(module_table);
                        }
                        _ => return Err(EditError::ParsingFailed),
                    }
                }
                ConfigEdit::DeleteModule { path } => {
                    if let toml_edit::Item::ArrayOfTables(modules) = &mut root_table["modules"] {
                        let mut module_index = None;
                        for (i, table) in modules.iter_mut().enumerate() {
                            if table
                                .get("path")
                                .map(|p| p.as_str() == Some(path))
                                .unwrap_or(false)
                            {
                                module_index = Some(i);
                                break;
                            } else if table
                                .get("paths")
                                .map(|p| p.as_array().is_some())
                                .unwrap_or(false)
                            {
                                table["paths"]
                                    .as_array_mut()
                                    .unwrap()
                                    .retain(|p| p.as_str().unwrap() != path);
                                if table["paths"].as_array().unwrap().is_empty() {
                                    module_index = Some(i);
                                }
                                break;
                            }
                        }
                        if let Some(index) = module_index {
                            modules.remove(index);
                        }
                    }
                }
                ConfigEdit::MarkModuleAsUtility { path }
                | ConfigEdit::UnmarkModuleAsUtility { path } => {
                    if let toml_edit::Item::ArrayOfTables(modules) = &mut root_table["modules"] {
                        for table in modules.iter_mut() {
                            if table
                                .get("path")
                                .map(|p| p.as_str() == Some(path))
                                .unwrap_or(false)
                            {
                                match edit {
                                    ConfigEdit::MarkModuleAsUtility { .. } => {
                                        table.insert("utility", toml_edit::value(true));
                                    }
                                    ConfigEdit::UnmarkModuleAsUtility { .. } => {
                                        table.remove("utility");
                                    }
                                    _ => unreachable!(),
                                }
                            } else if table.get("paths").is_some_and(|p| {
                                p.as_array()
                                    .map(|p| p.iter().any(|p| p.as_str() == Some(path)))
                                    .unwrap_or(false)
                            }) {
                                return Err(EditError::NotImplemented(
                                    "Cannot mark utilities for multi-path modules".to_string(),
                                ));
                            }
                        }
                    }
                }
                ConfigEdit::AddDependency { path, dependency }
                | ConfigEdit::RemoveDependency { path, dependency } => {
                    if let toml_edit::Item::ArrayOfTables(modules) = &mut root_table["modules"] {
                        for table in modules.iter_mut() {
                            let is_target_module = table
                                .get("path")
                                .map(|p| p.as_str() == Some(path))
                                .unwrap_or(false)
                                || table
                                    .get("paths")
                                    .map(|p| {
                                        p.as_array().is_some_and(|p| {
                                            p.iter().any(|p| p.as_str() == Some(path))
                                        })
                                    })
                                    .unwrap_or(false);

                            if is_target_module {
                                match edit {
                                    ConfigEdit::AddDependency { .. } => {
                                        if let Some(toml_edit::Item::Value(
                                            toml_edit::Value::Array(array),
                                        )) = table.get_mut("depends_on")
                                        {
                                            // Check if dependency already exists
                                            let exists = array.iter().any(|item| {
                                                match item {
                                                    // Check for string match
                                                    toml_edit::Value::String(s) => {
                                                        s.value() == dependency
                                                    }
                                                    // Check for object with matching path
                                                    toml_edit::Value::InlineTable(t) => t
                                                        .get("path")
                                                        .and_then(|p| p.as_str())
                                                        .map(|p| p == dependency)
                                                        .unwrap_or(false),
                                                    _ => false,
                                                }
                                            });

                                            if !exists {
                                                array.push(dependency);
                                            }
                                        } else {
                                            table.insert(
                                                "depends_on",
                                                toml_edit::value(toml_edit::Array::from_iter(
                                                    iter::once(dependency),
                                                )),
                                            );
                                        }
                                    }
                                    ConfigEdit::RemoveDependency { .. } => {
                                        if let toml_edit::Item::Value(toml_edit::Value::Array(
                                            array,
                                        )) = &mut table["depends_on"]
                                        {
                                            array.retain(|dep| {
                                                dep.as_str()
                                                    .map(|d| d != dependency)
                                                    .unwrap_or(true)
                                            });
                                        }
                                    }
                                    _ => unreachable!(),
                                }
                            }
                        }
                    }
                }
                ConfigEdit::AddSourceRoot { filepath } => {
                    if let toml_edit::Item::Value(toml_edit::Value::Array(source_roots)) =
                        &mut root_table["source_roots"]
                    {
                        if !source_roots.iter().any(|root| {
                            root.as_str() == Some(filepath.as_os_str().to_str().unwrap())
                        }) {
                            source_roots.push(filepath.display().to_string());
                        }
                    }
                }
                ConfigEdit::RemoveSourceRoot { filepath } => {
                    if let toml_edit::Item::Value(toml_edit::Value::Array(source_roots)) =
                        &mut root_table["source_roots"]
                    {
                        source_roots.retain(|root| {
                            root.as_str()
                                .map(|s| s != filepath.as_os_str().to_str().unwrap())
                                .unwrap_or(true)
                        });
                    }
                }
            }
        }

        std::fs::write(config_path, doc.to_string()).map_err(|_| EditError::DiskWriteFailed)?;

        self.pending_edits.clear();
        Ok(())
    }
}

#[pymethods]
impl ProjectConfig {
    #[new]
    fn new() -> Self {
        ProjectConfig::default()
    }

    fn __str__(&self) -> String {
        format!("{self:#?}")
    }

    fn serialize_json(&self) -> String {
        serde_json::to_string(&self).unwrap()
    }

    #[pyo3(name = "all_modules")]
    fn all_modules_py(&self) -> Vec<ModuleConfig> {
        self.all_modules().cloned().collect()
    }

    #[pyo3(name = "all_interfaces")]
    fn all_interfaces_py(&self) -> Vec<InterfaceConfig> {
        self.all_interfaces().cloned().collect()
    }

    pub fn exists(&self) -> bool {
        self.location.is_some()
    }

    pub fn set_location(&mut self, location: PathBuf) {
        self.location = Some(location);
    }

    pub fn has_no_modules(&self) -> bool {
        self.all_modules().next().is_none()
    }

    pub fn has_no_dependencies(&self) -> bool {
        self.all_modules().all(|module| {
            module
                .depends_on
                .as_ref()
                .map(|deps| deps.is_empty())
                .unwrap_or(true)
        })
    }

    pub fn has_root_module_reference(&self) -> bool {
        self.all_modules().any(|module| {
            module.path == "<root>"
                || module
                    .depends_on
                    .as_ref()
                    .map(|deps| deps.iter().any(|dep| dep.path == "<root>"))
                    .unwrap_or(false)
        })
    }

    pub fn module_paths(&self) -> Vec<String> {
        self.all_modules()
            .map(|module| module.path.clone())
            .collect()
    }

    fn utility_paths(&self) -> Vec<String> {
        self.all_modules()
            .filter(|module| module.utility)
            .map(|module| module.path.clone())
            .collect()
    }

    pub fn filtered_modules(
        &self,
        included_paths: Vec<PathBuf>,
    ) -> Result<Vec<ModuleConfig>, ConfigError> {
        let absolute_source_roots = self.absolute_source_roots()?;
        Ok(self
            .all_modules()
            .filter(|module| {
                included_paths.is_empty()
                    || module_path_is_included_in_paths(
                        &absolute_source_roots,
                        &module.path,
                        &included_paths,
                    )
            })
            .map(|module| {
                module.with_filtered_dependencies(&absolute_source_roots, &included_paths)
            })
            .collect())
    }

    pub fn create_module(&mut self, path: String) -> Result<(), EditError> {
        self.enqueue_edit(&ConfigEdit::CreateModule { path })
    }

    pub fn delete_module(&mut self, path: String) -> Result<(), EditError> {
        self.enqueue_edit(&ConfigEdit::DeleteModule { path })
    }

    pub fn mark_module_as_utility(&mut self, path: String) -> Result<(), EditError> {
        self.enqueue_edit(&ConfigEdit::MarkModuleAsUtility { path })
    }

    pub fn unmark_module_as_utility(&mut self, path: String) -> Result<(), EditError> {
        self.enqueue_edit(&ConfigEdit::UnmarkModuleAsUtility { path })
    }

    pub fn add_dependency(&mut self, path: String, dependency: String) -> Result<(), EditError> {
        self.enqueue_edit(&ConfigEdit::AddDependency { path, dependency })
    }

    pub fn remove_dependency(&mut self, path: String, dependency: String) -> Result<(), EditError> {
        self.enqueue_edit(&ConfigEdit::RemoveDependency { path, dependency })
    }

    pub fn add_source_root(&mut self, filepath: PathBuf) -> Result<(), EditError> {
        self.enqueue_edit(&ConfigEdit::AddSourceRoot { filepath })
    }

    pub fn remove_source_root(&mut self, filepath: PathBuf) -> Result<(), EditError> {
        self.enqueue_edit(&ConfigEdit::RemoveSourceRoot { filepath })
    }

    pub fn save_edits(&mut self) -> Result<(), EditError> {
        self.apply_edits()
    }
}
