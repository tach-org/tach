use crate::checks::{ExternalDependencyChecker, IgnoreDirectivePostProcessor};
use crate::commands::check;
use crate::config::ProjectConfig;
use crate::dependencies::import::with_distribution_names;
use crate::diagnostics::{
    CodeDiagnostic, ConfigurationDiagnostic, Diagnostic, DiagnosticDetails, DiagnosticError,
    DiagnosticPipeline, FileChecker, FileProcessor, Result as DiagnosticResult,
};
use crate::filesystem::{self, ProjectFile};
use crate::interrupt::check_interrupt;
use crate::modules::{ModuleTree, ModuleTreeBuilder};
use crate::processors::ExternalDependencyExtractor;
use crate::processors::file_module::FileModule;
use crate::resolvers::{PackageResolver, SourceRootResolver};
use pyo3::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use dashmap::{DashMap, DashSet};
use rayon::prelude::*;

use super::error::CheckError;

pub type Result<T> = std::result::Result<T, CheckError>;

struct CheckExternalPipeline<'a> {
    module_mappings: &'a HashMap<String, Vec<String>>,
    excluded_external_modules: &'a HashSet<String>,
    seen_dependencies: DashMap<PathBuf, DashSet<String>>,
    package_resolver: &'a PackageResolver<'a>,
    dependency_extractor: ExternalDependencyExtractor<'a>,
    dependency_checker: ExternalDependencyChecker<'a>,
    ignore_directive_post_processor: IgnoreDirectivePostProcessor<'a>,
}

impl<'a> CheckExternalPipeline<'a> {
    pub fn new(
        source_roots: &'a [PathBuf],
        project_config: &'a ProjectConfig,
        module_tree: &'a ModuleTree,
        module_mappings: &'a HashMap<String, Vec<String>>,
        stdlib_modules: &'a HashSet<String>,
        excluded_external_modules: &'a HashSet<String>,
        package_resolver: &'a PackageResolver,
    ) -> Self {
        Self {
            module_mappings,
            excluded_external_modules,
            seen_dependencies: DashMap::new(),
            package_resolver,
            dependency_extractor: ExternalDependencyExtractor::new(
                source_roots,
                module_tree,
                project_config,
                package_resolver,
            ),
            dependency_checker: ExternalDependencyChecker::new(
                project_config,
                module_mappings,
                stdlib_modules,
                excluded_external_modules,
                package_resolver,
            ),
            ignore_directive_post_processor: IgnoreDirectivePostProcessor::new(project_config),
        }
    }
}

impl<'a> FileProcessor<'a, ProjectFile<'a>> for CheckExternalPipeline<'a> {
    type ProcessedFile = FileModule<'a>;

    fn process(&'a self, file_path: ProjectFile<'a>) -> DiagnosticResult<Self::ProcessedFile> {
        let package_root = match self
            .package_resolver
            .get_package_for_source_root(file_path.source_root)
            .map(|package| package.root.clone())
        {
            Some(package_root) => package_root,
            None => {
                return Err(DiagnosticError::PackageNotFound(
                    file_path.source_root.display().to_string(),
                ));
            }
        };
        let file_module = self.dependency_extractor.process(file_path)?;

        // Track all external dependencies seen in imports
        with_distribution_names(
            file_module.imports(),
            self.package_resolver,
            self.module_mappings,
        )
        .into_iter()
        .for_each(|import| {
            import
                .distribution_names
                .iter()
                .for_each(|distribution_name| {
                    self.seen_dependencies
                        .entry(package_root.clone())
                        .or_default()
                        .insert(distribution_name.clone());
                });
        });

        Ok(file_module)
    }
}

impl<'a> FileChecker<'a> for CheckExternalPipeline<'a> {
    type ProcessedFile = FileModule<'a>;
    type Output = Vec<Diagnostic>;

    fn check(&'a self, processed_file: &Self::ProcessedFile) -> DiagnosticResult<Self::Output> {
        let mut diagnostics = Vec::new();
        diagnostics.extend(self.dependency_checker.check(processed_file)?);

        self.ignore_directive_post_processor.process_diagnostics(
            &processed_file.ignore_directives,
            &mut diagnostics,
            processed_file.relative_file_path(),
        );

        Ok(diagnostics)
    }
}

struct CheckExternalMetadata {
    module_mappings: HashMap<String, Vec<String>>,
    stdlib_modules: Vec<String>,
}

/// Get metadata for checking external dependencies.
fn get_check_external_metadata(project_config: &ProjectConfig) -> Result<CheckExternalMetadata> {
    Python::with_gil(|py| {
        let external_utils = PyModule::import(py, "tach.utils.external")
            .expect("Failed to import tach.utils.external");
        let mut module_mappings: HashMap<String, Vec<String>> = external_utils
            .getattr("get_module_mappings")
            .expect("Failed to get module_mappings")
            .call0()
            .expect("Failed to call get_module_mappings")
            .extract()
            .expect("Failed to extract module_mappings");
        let stdlib_modules: Vec<String> = external_utils
            .getattr("get_stdlib_modules")
            .expect("Failed to get stdlib_modules")
            .call0()
            .expect("Failed to call get_stdlib_modules")
            .extract()
            .expect("Failed to extract stdlib_modules");

        if !project_config.external.rename.is_empty() {
            for rename_pair in project_config.external.rename.iter() {
                if let Some((module, name)) = rename_pair.split_once(':') {
                    module_mappings.insert(module.to_string(), vec![name.to_string()]);
                } else {
                    return Err(check::error::CheckError::Configuration(
                        "Invalid rename format: expected format is a list of 'module:name' pairs, e.g. ['PIL:pillow']".to_string()
                    ));
                }
            }
        }

        Ok(CheckExternalMetadata {
            module_mappings,
            stdlib_modules,
        })
    })
}

pub fn check(project_root: &PathBuf, project_config: &ProjectConfig) -> Result<Vec<Diagnostic>> {
    let metadata = get_check_external_metadata(project_config)?;
    check_with_modules(
        project_root,
        project_config,
        &metadata.module_mappings,
        &metadata.stdlib_modules,
    )
}

fn check_with_modules(
    project_root: &PathBuf,
    project_config: &ProjectConfig,
    module_mappings: &HashMap<String, Vec<String>>,
    stdlib_modules: &[String],
) -> Result<Vec<Diagnostic>> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let stdlib_modules: HashSet<String> = stdlib_modules.iter().cloned().collect();
    let excluded_external_modules: HashSet<String> =
        project_config.external.exclude.iter().cloned().collect();
    let file_walker = filesystem::FSWalker::try_new(
        project_root,
        &project_config.exclude,
        project_config.respect_gitignore,
    )?;
    let source_root_resolver = SourceRootResolver::new(project_root, &file_walker);
    let source_roots: Vec<PathBuf> = source_root_resolver.resolve(&project_config.source_roots)?;
    let package_resolver = PackageResolver::try_new(project_root, &source_roots, &file_walker)?;
    let module_tree_builder = ModuleTreeBuilder::new(
        &source_roots,
        &file_walker,
        project_config.forbid_circular_dependencies,
        project_config.root_module,
    );

    let (valid_modules, invalid_modules) =
        module_tree_builder.resolve_modules(project_config.all_modules());

    for module in &invalid_modules {
        diagnostics.push(Diagnostic::new_global_warning(
            DiagnosticDetails::Configuration(ConfigurationDiagnostic::ModuleNotFound {
                file_mod_path: module.path.to_string(),
            }),
        ));
    }

    check_interrupt().map_err(|_| CheckError::Interrupt)?;
    let module_tree = module_tree_builder.build(valid_modules)?;

    let pipeline = CheckExternalPipeline::new(
        &source_roots,
        project_config,
        &module_tree,
        module_mappings,
        &stdlib_modules,
        &excluded_external_modules,
        &package_resolver,
    );

    diagnostics.par_extend(source_roots.par_iter().flat_map(|source_root| {
        file_walker
            .walk_pyfiles(&source_root.display().to_string())
            .par_bridge()
            .flat_map(|file_path| {
                if check_interrupt().is_err() {
                    // Since files are being processed in parallel,
                    // this will essentially short-circuit all remaining files.
                    // Then, we check for an interrupt right after, and return the Err if it is set
                    return vec![];
                }

                let project_file = match ProjectFile::try_new(project_root, source_root, &file_path)
                {
                    Ok(project_file) => project_file,
                    Err(_) => {
                        return vec![Diagnostic::new_global_warning(
                            DiagnosticDetails::Configuration(
                                ConfigurationDiagnostic::SkippedFileIoError {
                                    file_path: file_path.display().to_string(),
                                },
                            ),
                        )];
                    }
                };

                match pipeline.diagnostics(project_file) {
                    Ok(diagnostics) => diagnostics,
                    Err(DiagnosticError::Io(_)) | Err(DiagnosticError::Filesystem(_)) => {
                        vec![Diagnostic::new_global_warning(
                            DiagnosticDetails::Configuration(
                                ConfigurationDiagnostic::SkippedFileIoError {
                                    file_path: file_path.display().to_string(),
                                },
                            ),
                        )]
                    }
                    Err(DiagnosticError::ImportParse(_)) => {
                        vec![Diagnostic::new_global_warning(
                            DiagnosticDetails::Configuration(
                                ConfigurationDiagnostic::SkippedFileSyntaxError {
                                    file_path: file_path.display().to_string(),
                                },
                            ),
                        )]
                    }
                    Err(_) => vec![Diagnostic::new_global_warning(
                        DiagnosticDetails::Configuration(
                            ConfigurationDiagnostic::SkippedUnknownError {
                                file_path: file_path.display().to_string(),
                            },
                        ),
                    )],
                }
            })
    }));

    if !project_config.rules.unused_external_dependencies.is_off() {
        for (package_root, seen_dependencies) in pipeline.seen_dependencies {
            let seen_dependencies: HashSet<String> = seen_dependencies.into_iter().collect();
            let package = match package_resolver.get_package_by_package_root(&package_root) {
                Some(package) => package,
                None => continue, // Skip packages we can't resolve dependencies for
            };

            let unused_dependency_diagnostics = package
                .dependencies
                .difference(&seen_dependencies)
                .filter(|&dep| !pipeline.excluded_external_modules.contains(dep)) // 'exclude' should hide unused errors unconditionally
                .map(|dep| {
                    Diagnostic::new_global(
                        (&project_config.rules.unused_external_dependencies)
                            .try_into()
                            .unwrap(),
                        DiagnosticDetails::Code(CodeDiagnostic::UnusedExternalDependency {
                            package_module_name: dep.clone(),
                            package_name: package
                                .name
                                .as_ref()
                                .map_or(package_root.display().to_string(), |name| {
                                    name.to_string()
                                }),
                        }),
                    )
                });

            diagnostics.extend(unused_dependency_diagnostics);
        }
    }

    if check_interrupt().is_err() {
        return Err(CheckError::Interrupt);
    }

    Ok(diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProjectConfig;
    use crate::diagnostics::Severity;
    use crate::tests::fixtures::example_dir;
    use rstest::*;

    #[fixture]
    fn project_config() -> ProjectConfig {
        ProjectConfig {
            source_roots: [
                "src/pack-a/src",
                "src/pack-b/src",
                "src/pack-c/src",
                "src/pack-d/src",
                "src/pack-e/src",
                "src/pack-f/src",
                "src/pack-g/src",
            ]
            .iter()
            .map(PathBuf::from)
            .collect(),
            ignore_type_checking_imports: true,
            ..Default::default()
        }
    }

    #[fixture]
    fn module_mapping() -> HashMap<String, Vec<String>> {
        HashMap::from([("git".to_string(), vec!["gitpython".to_string()])])
    }

    #[rstest]
    fn check_external_dependencies_multi_package_example(
        example_dir: PathBuf,
        project_config: ProjectConfig,
        module_mapping: HashMap<String, Vec<String>>,
    ) {
        let project_root = example_dir.join("multi_package");
        let result =
            check_with_modules(&project_root, &project_config, &module_mapping, &[]).unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(
            result[0],
            Diagnostic::Global {
                severity: Severity::Error,
                details: DiagnosticDetails::Code(CodeDiagnostic::UnusedExternalDependency { .. })
            }
        ));
        assert_eq!(
            result[0].details(),
            &DiagnosticDetails::Code(CodeDiagnostic::UnusedExternalDependency {
                package_module_name: "unused".to_string(),
                package_name: "myorg-pack-a".to_string()
            })
        );
    }

    #[rstest]
    fn check_external_dependencies_invalid_multi_package_example(
        example_dir: PathBuf,
        project_config: ProjectConfig,
    ) {
        let project_root = example_dir.join("multi_package");
        let result =
            check_with_modules(&project_root, &project_config, &HashMap::new(), &[]).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.iter().any(|d| d.details()
            == &DiagnosticDetails::Code(CodeDiagnostic::UndeclaredExternalDependency {
                dependency: "git".to_string(),
                package_name: "myorg-pack-a".to_string()
            })));
        assert!(result.iter().any(|d| d.details()
            == &DiagnosticDetails::Code(CodeDiagnostic::UnusedExternalDependency {
                package_module_name: "gitpython".to_string(),
                package_name: "myorg-pack-a".to_string()
            })));
        assert!(result.iter().any(|d| d.details()
            == &DiagnosticDetails::Code(CodeDiagnostic::UnusedExternalDependency {
                package_module_name: "unused".to_string(),
                package_name: "myorg-pack-a".to_string()
            })));
    }
}
