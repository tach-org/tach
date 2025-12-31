use std::path::PathBuf;

use crate::config::ProjectConfig;
use crate::config::root_module::RootModuleTreatment;
use crate::dependencies::Dependency;
use crate::diagnostics::{
    CodeDiagnostic, ConfigurationDiagnostic, Diagnostic, DiagnosticDetails, FileChecker,
    Result as DiagnosticResult,
};
use crate::interfaces::compiled::CompiledInterfaces;
use crate::interfaces::data_types::{TypeCheckCache, TypeCheckResult};
use crate::interfaces::error::InterfaceError;
use crate::modules::ModuleTree;
use crate::processors::file_module::FileModule;

#[derive(Debug)]
pub enum InterfaceCheckResult {
    Exposed { type_check_result: TypeCheckResult },
    NotExposed,
    NoInterfaces,
    TopLevelModule,
}

pub struct InterfaceChecker<'a> {
    project_config: &'a ProjectConfig,
    source_roots: &'a [PathBuf],
    module_tree: &'a ModuleTree,
    interfaces: CompiledInterfaces,
    type_check_cache: Option<TypeCheckCache>,
}

impl<'a> InterfaceChecker<'a> {
    pub fn new(
        project_config: &'a ProjectConfig,
        module_tree: &'a ModuleTree,
        source_roots: &'a [PathBuf],
    ) -> Self {
        let compiled = CompiledInterfaces::build(project_config.all_interfaces());

        Self {
            project_config,
            source_roots,
            module_tree,
            interfaces: compiled,
            type_check_cache: None,
        }
    }

    pub fn with_type_check_cache(mut self) -> Result<Self, InterfaceError> {
        let type_check_cache = TypeCheckCache::build(
            &self.interfaces,
            &self.module_tree.module_paths(),
            self.source_roots,
        )?;
        self.type_check_cache = Some(type_check_cache);
        Ok(self)
    }

    fn check_member(
        &self,
        member: &str,
        definition_module: &str,
        usage_module: &str,
    ) -> InterfaceCheckResult {
        if member.is_empty() {
            return InterfaceCheckResult::TopLevelModule;
        }

        let any_interfaces_exist = self
            .interfaces
            .interfaces_for_module(definition_module)
            .next()
            .is_some();

        let matching_interfaces = self
            .interfaces
            .get_visible_interfaces(definition_module, usage_module);

        if matching_interfaces.is_empty() {
            if any_interfaces_exist {
                // Interfaces exist but none are visible due to visibility constraints
                return InterfaceCheckResult::NotExposed;
            }
            return InterfaceCheckResult::NoInterfaces;
        }

        for interface in matching_interfaces {
            if interface.is_exposed_to(member, usage_module) {
                return InterfaceCheckResult::Exposed {
                    type_check_result: self
                        .type_check_cache
                        .as_ref()
                        .map(|cache| cache.get_result(member))
                        .unwrap_or(TypeCheckResult::Unknown),
                };
            }
        }

        InterfaceCheckResult::NotExposed
    }

    fn check_interfaces(
        &self,
        dependency: &Dependency,
        file_module: &FileModule,
    ) -> DiagnosticResult<Vec<Diagnostic>> {
        if let Some(dependency_module_config) = self
            .module_tree
            .find_nearest(dependency.module_path())
            .as_ref()
            .and_then(|module| module.config.as_ref())
        {
            if dependency_module_config == file_module.module_config() {
                return Ok(vec![]);
            }

            if dependency_module_config.is_root()
                && self.project_config.root_module == RootModuleTreatment::Ignore
            {
                return Ok(vec![]);
            }

            let import_member = dependency
                .module_path()
                .strip_prefix(&dependency_module_config.path)
                .and_then(|s| s.strip_prefix('.'))
                .unwrap_or("");
            let check_result = self.check_member(
                import_member,
                &dependency_module_config.path,
                &file_module.module_config().path,
            );
            match check_result {
                InterfaceCheckResult::NotExposed => {
                    let diagnostic = DiagnosticDetails::Code(CodeDiagnostic::PrivateDependency {
                        dependency: dependency.module_path().to_string(),
                        usage_module: file_module.module_config().path.to_string(),
                        definition_module: dependency_module_config.path.to_string(),
                    });

                    if let Dependency::Import(import) = dependency {
                        if !import.is_global_scope {
                            if let Ok(severity) =
                                (&self.project_config.rules.local_imports).try_into()
                            {
                                return Ok(vec![Diagnostic::new_located(
                                    severity,
                                    diagnostic,
                                    file_module.relative_file_path().to_path_buf(),
                                    file_module.line_number(dependency.offset()),
                                    dependency
                                        .original_line_offset()
                                        .map(|offset| file_module.line_number(offset)),
                                )]);
                            }
                            return Ok(vec![]);
                        }
                    }

                    Ok(vec![Diagnostic::new_located_error(
                        file_module.relative_file_path().to_path_buf(),
                        file_module.line_number(dependency.offset()),
                        dependency
                            .original_line_offset()
                            .map(|offset| file_module.line_number(offset)),
                        diagnostic,
                    )])
                }
                InterfaceCheckResult::Exposed {
                    type_check_result: TypeCheckResult::DidNotMatchInterface { expected },
                } => {
                    let diagnostic =
                        DiagnosticDetails::Code(CodeDiagnostic::InvalidDataTypeExport {
                            dependency: dependency.module_path().to_string(),
                            usage_module: file_module.module_config().path.to_string(),
                            definition_module: dependency_module_config.path.to_string(),
                            expected_data_type: expected.to_string(),
                        });

                    if let Dependency::Import(import) = dependency {
                        if !import.is_global_scope {
                            if let Ok(severity) =
                                (&self.project_config.rules.local_imports).try_into()
                            {
                                return Ok(vec![Diagnostic::new_located(
                                    severity,
                                    diagnostic,
                                    file_module.relative_file_path().to_path_buf(),
                                    file_module.line_number(dependency.offset()),
                                    dependency
                                        .original_line_offset()
                                        .map(|offset| file_module.line_number(offset)),
                                )]);
                            }
                            return Ok(vec![]);
                        }
                    }

                    Ok(vec![Diagnostic::new_located_error(
                        file_module.relative_file_path().to_path_buf(),
                        file_module.line_number(dependency.offset()),
                        dependency
                            .original_line_offset()
                            .map(|offset| file_module.line_number(offset)),
                        diagnostic,
                    )])
                }
                InterfaceCheckResult::Exposed {
                    type_check_result: TypeCheckResult::MatchedInterface { .. },
                }
                | InterfaceCheckResult::Exposed {
                    type_check_result: TypeCheckResult::Unknown,
                }
                | InterfaceCheckResult::NoInterfaces
                | InterfaceCheckResult::TopLevelModule => Ok(vec![]),
            }
        } else {
            Ok(vec![Diagnostic::new_global_error(
                DiagnosticDetails::Configuration(ConfigurationDiagnostic::ModuleConfigNotFound {
                    module_path: dependency.module_path().to_string(),
                }),
            )])
        }
    }
}

impl<'a> FileChecker<'a> for InterfaceChecker<'a> {
    type ProcessedFile = FileModule<'a>;
    type Output = Vec<Diagnostic>;

    fn check(&'a self, input: &Self::ProcessedFile) -> DiagnosticResult<Self::Output> {
        let mut diagnostics = vec![];
        for dependency in input.dependencies.iter() {
            diagnostics.extend(self.check_interfaces(dependency, input)?);
        }

        Ok(diagnostics)
    }
}
