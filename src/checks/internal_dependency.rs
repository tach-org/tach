use crate::{
    config::{
        DependencyConfig, LayerConfig, ModuleConfig, ProjectConfig, RuleSetting,
        root_module::RootModuleTreatment,
    },
    dependencies::Dependency,
    diagnostics::{
        CodeDiagnostic, ConfigurationDiagnostic, Diagnostic, DiagnosticDetails, FileChecker,
        Result as DiagnosticResult,
    },
    modules::ModuleTree,
    processors::FileModule,
};

#[derive(Debug)]
enum LayerCheckResult {
    Ok,
    SameLayer,
    LayerNotSpecified,
    LayerViolation {
        source_layer: String,
        source_module: String,
        target_layer: String,
        target_module: String,
    },
    ClosedLayerViolation {
        source_layer: String,
        source_module: String,
        target_layer: String,
        target_module: String,
        closed_layer: String,
    },
    UnknownLayer {
        unknown_layer: String,
    },
}

pub struct InternalDependencyChecker<'a> {
    project_config: &'a ProjectConfig,
    module_tree: &'a ModuleTree,
}

impl<'a> InternalDependencyChecker<'a> {
    pub fn new(project_config: &'a ProjectConfig, module_tree: &'a ModuleTree) -> Self {
        Self {
            project_config,
            module_tree,
        }
    }

    fn check_layers(
        &self,
        layers: &[LayerConfig],
        source_module_config: &ModuleConfig,
        target_module_config: &ModuleConfig,
    ) -> LayerCheckResult {
        match (&source_module_config.layer, &target_module_config.layer) {
            (Some(source_layer), Some(target_layer)) => {
                let source_index = layers
                    .iter()
                    .position(|layer| layer.name.as_str() == source_layer);
                let target_index = layers
                    .iter()
                    .position(|layer| layer.name.as_str() == target_layer);

                match (source_index, target_index) {
                    (Some(source_index), Some(target_index)) => {
                        if source_index == target_index {
                            LayerCheckResult::SameLayer
                        } else if source_index < target_index {
                            // Check if there are any closed layers between source and target
                            for layer in &layers[source_index + 1..target_index] {
                                if layer.closed {
                                    return LayerCheckResult::ClosedLayerViolation {
                                        source_layer: source_layer.clone(),
                                        source_module: source_module_config.path.clone(),
                                        target_layer: target_layer.clone(),
                                        target_module: target_module_config.path.clone(),
                                        closed_layer: layer.name.clone(),
                                    };
                                }
                            }
                            LayerCheckResult::Ok
                        } else {
                            LayerCheckResult::LayerViolation {
                                source_layer: source_layer.clone(),
                                source_module: source_module_config.path.clone(),
                                target_layer: target_layer.clone(),
                                target_module: target_module_config.path.clone(),
                            }
                        }
                    }
                    // If either index is not found, the layer is unknown
                    (Some(_), None) => LayerCheckResult::UnknownLayer {
                        unknown_layer: target_layer.clone(),
                    },
                    (None, Some(_)) | (None, None) => LayerCheckResult::UnknownLayer {
                        unknown_layer: source_layer.clone(),
                    },
                }
            }
            _ => LayerCheckResult::LayerNotSpecified, // At least one module does not have a layer
        }
    }

    fn check_dependency_rules(
        &self,
        file_module: &FileModule,
        dependency: &Dependency,
        dependency_module_config: &ModuleConfig,
        layers: &[LayerConfig],
    ) -> DiagnosticResult<Vec<Diagnostic>> {
        let file_module_config = file_module.module_config();
        if dependency_module_config == file_module_config {
            return Ok(vec![]);
        }

        let relative_file_path = file_module.relative_file_path();
        // Layer check should take precedence over other depends_on checks
        match self.check_layers(layers, file_module_config, dependency_module_config) {
            LayerCheckResult::Ok => {
                // If layers_explicit_depends_on is enabled and target is not a utility,
                // don't return early - fall through to check explicit depends_on declaration
                if !self.project_config.layers_explicit_depends_on || dependency_module_config.utility {
                    return Ok(vec![]);
                }
                // Fall through to check forbidden deps and depends_on
            }
            LayerCheckResult::LayerViolation {
                source_layer,
                source_module,
                target_layer,
                target_module,
            } => {
                let details = DiagnosticDetails::Code(CodeDiagnostic::LayerViolation {
                    dependency: dependency.module_path().to_string(),
                    usage_module: source_module,
                    usage_layer: source_layer,
                    definition_module: target_module,
                    definition_layer: target_layer,
                });

                if let Dependency::Import(import) = dependency {
                    if !import.is_global_scope {
                        if let Ok(severity) = (&self.project_config.rules.local_imports).try_into()
                        {
                            return Ok(vec![Diagnostic::new_located(
                                severity,
                                details,
                                relative_file_path.to_path_buf(),
                                file_module.line_number(dependency.offset()),
                                dependency
                                    .original_line_offset()
                                    .map(|offset| file_module.line_number(offset)),
                            )]);
                        }
                        return Ok(vec![]);
                    }
                }

                return Ok(vec![Diagnostic::new_located_error(
                    relative_file_path.to_path_buf(),
                    file_module.line_number(dependency.offset()),
                    dependency
                        .original_line_offset()
                        .map(|offset| file_module.line_number(offset)),
                    details,
                )]);
            }
            LayerCheckResult::ClosedLayerViolation {
                source_layer,
                source_module,
                target_layer,
                target_module,
                closed_layer,
            } => {
                let details = DiagnosticDetails::Code(CodeDiagnostic::ClosedLayerViolation {
                    dependency: dependency.module_path().to_string(),
                    usage_module: source_module,
                    usage_layer: source_layer,
                    definition_module: target_module,
                    definition_layer: target_layer,
                    closed_layer,
                });

                if let Dependency::Import(import) = dependency {
                    if !import.is_global_scope {
                        if let Ok(severity) = (&self.project_config.rules.local_imports).try_into()
                        {
                            return Ok(vec![Diagnostic::new_located(
                                severity,
                                details,
                                relative_file_path.to_path_buf(),
                                file_module.line_number(dependency.offset()),
                                dependency
                                    .original_line_offset()
                                    .map(|offset| file_module.line_number(offset)),
                            )]);
                        }
                        return Ok(vec![]);
                    }
                }

                return Ok(vec![Diagnostic::new_located_error(
                    relative_file_path.to_path_buf(),
                    file_module.line_number(dependency.offset()),
                    dependency
                        .original_line_offset()
                        .map(|offset| file_module.line_number(offset)),
                    details,
                )]);
            }
            LayerCheckResult::UnknownLayer { unknown_layer } => {
                let details =
                    DiagnosticDetails::Configuration(ConfigurationDiagnostic::UnknownLayer {
                        layer: unknown_layer,
                    });

                if let Dependency::Import(import) = dependency {
                    if !import.is_global_scope {
                        if let Ok(severity) = (&self.project_config.rules.local_imports).try_into()
                        {
                            return Ok(vec![Diagnostic::new_global(severity, details)]);
                        }
                        return Ok(vec![]);
                    }
                }

                return Ok(vec![Diagnostic::new_global_error(details)]);
            }
            LayerCheckResult::SameLayer | LayerCheckResult::LayerNotSpecified => (), // We need to do further processing to determine if the dependency is allowed
        };

        let file_nearest_module_path = &file_module_config.path;
        let dependency_nearest_module_path = &dependency_module_config.path;

        if let Some(DependencyConfig { .. }) = file_module_config
            .forbidden_dependencies_iter()
            .find(|dep| dep.matches(dependency_nearest_module_path))
        {
            let diagnostic = DiagnosticDetails::Code(CodeDiagnostic::ForbiddenDependency {
                dependency: dependency.module_path().to_string(),
                usage_module: file_nearest_module_path.to_string(),
                definition_module: dependency_nearest_module_path.to_string(),
            });

            if let Dependency::Import(import) = dependency {
                if !import.is_global_scope {
                    if let Ok(severity) = (&self.project_config.rules.local_imports).try_into() {
                        return Ok(vec![Diagnostic::new_located(
                            severity,
                            diagnostic,
                            relative_file_path.to_path_buf(),
                            file_module.line_number(dependency.offset()),
                            dependency
                                .original_line_offset()
                                .map(|offset| file_module.line_number(offset)),
                        )]);
                    }
                    return Ok(vec![]);
                }
            }

            return Ok(vec![Diagnostic::new_located_error(
                relative_file_path.to_path_buf(),
                file_module.line_number(dependency.offset()),
                dependency
                    .original_line_offset()
                    .map(|offset| file_module.line_number(offset)),
                diagnostic,
            )]);
        }

        if file_module_config.depends_on.is_none() {
            return Ok(vec![]);
        }

        if dependency_module_config.utility {
            return Ok(vec![]);
        }

        match file_module_config
            .dependencies_iter()
            .find(|dep| dep.matches(dependency_nearest_module_path))
        {
            Some(DependencyConfig {
                deprecated: true, ..
            }) => {
                let diagnostic = DiagnosticDetails::Code(CodeDiagnostic::DeprecatedDependency {
                    dependency: dependency.module_path().to_string(),
                    usage_module: file_nearest_module_path.to_string(),
                    definition_module: dependency_nearest_module_path.to_string(),
                });

                if let Dependency::Import(import) = dependency {
                    if !import.is_global_scope {
                        match &self.project_config.rules.local_imports {
                            RuleSetting::Warn | RuleSetting::Error => {
                                return Ok(vec![Diagnostic::new_located_warning(
                                    relative_file_path.to_path_buf(),
                                    file_module.line_number(dependency.offset()),
                                    dependency
                                        .original_line_offset()
                                        .map(|offset| file_module.line_number(offset)),
                                    diagnostic,
                                )]);
                            }
                            _ => return Ok(vec![]),
                        }
                    }
                }

                Ok(vec![Diagnostic::new_located_warning(
                    relative_file_path.to_path_buf(),
                    file_module.line_number(dependency.offset()),
                    dependency
                        .original_line_offset()
                        .map(|offset| file_module.line_number(offset)),
                    diagnostic,
                )])
            }
            Some(_) => Ok(vec![]),
            None => {
                let diagnostic = DiagnosticDetails::Code(CodeDiagnostic::UndeclaredDependency {
                    dependency: dependency.module_path().to_string(),
                    usage_module: file_nearest_module_path.to_string(),
                    definition_module: dependency_nearest_module_path.to_string(),
                });

                if let Dependency::Import(import) = dependency {
                    if !import.is_global_scope {
                        if let Ok(severity) = (&self.project_config.rules.local_imports).try_into()
                        {
                            return Ok(vec![Diagnostic::new_located(
                                severity,
                                diagnostic,
                                relative_file_path.to_path_buf(),
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
                    relative_file_path.to_path_buf(),
                    file_module.line_number(dependency.offset()),
                    dependency
                        .original_line_offset()
                        .map(|offset| file_module.line_number(offset)),
                    diagnostic,
                )])
            }
        }
    }

    fn check_dependency(
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
            if dependency_module_config.is_root()
                && self.project_config.root_module == RootModuleTreatment::Ignore
            {
                return Ok(vec![]);
            }

            self.check_dependency_rules(
                file_module,
                dependency,
                dependency_module_config,
                &self.project_config.layers,
            )
        } else {
            Ok(vec![Diagnostic::new_global_error(
                DiagnosticDetails::Configuration(ConfigurationDiagnostic::ModuleConfigNotFound {
                    module_path: dependency.module_path().to_string(),
                }),
            )])
        }
    }
}

impl<'a> FileChecker<'a> for InternalDependencyChecker<'a> {
    type ProcessedFile = FileModule<'a>;
    type Output = Vec<Diagnostic>;

    fn check(&'a self, processed_file: &Self::ProcessedFile) -> DiagnosticResult<Self::Output> {
        let mut diagnostics = Vec::new();
        for dependency in processed_file.dependencies.iter() {
            diagnostics.extend(self.check_dependency(dependency, processed_file)?);
        }

        Ok(diagnostics)
    }
}
