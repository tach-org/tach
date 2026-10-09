use std::collections::HashSet;
use std::{collections::HashMap, path::PathBuf};

use pyo3::{pyclass, pymethods};
use thiserror::Error;

use crate::config::{ModuleConfig, ProjectConfig};
use crate::filesystem::{self as fs};
use crate::modules::{ModuleTree, ModuleTreeBuilder};
use crate::resolvers::{SourceRootResolver, SourceRootResolverError};

use super::helpers::import::get_located_imports;

#[derive(Error, Debug)]
pub enum TestError {
    #[error("Filesystem error occurred.\n{0}")]
    Filesystem(#[from] fs::FileSystemError),
    #[error("Could not find module containing path: {0}")]
    ModuleNotFound(String),
    #[error("Source root resolution error: {0}")]
    SourceRootResolution(#[from] SourceRootResolverError),
}

pub type Result<T> = std::result::Result<T, TestError>;

#[pyclass(module = "tach.extension")]
pub struct TachPytestPluginHandler {
    project_root: PathBuf,
    source_roots: Vec<PathBuf>,
    project_config: ProjectConfig,
    module_tree: ModuleTree,
    affected_modules: HashSet<String>,
    #[pyo3(get)]
    all_affected_modules: HashSet<PathBuf>,
    #[pyo3(get)]
    removed_test_paths: HashSet<PathBuf>,
    #[pyo3(get, set)]
    num_removed_items: i32,
    #[pyo3(get, set)]
    tests_ran_to_completion: bool,
}

#[pymethods]
impl TachPytestPluginHandler {
    #[new]
    fn new(
        project_root: PathBuf,
        project_config: ProjectConfig,
        changed_files: Vec<PathBuf>,
        all_affected_modules: HashSet<PathBuf>,
    ) -> Self {
        // TODO: Remove unwraps
        let file_walker = fs::FSWalker::try_new(
            &project_root,
            &project_config.exclude,
            project_config.respect_gitignore,
        )
        .unwrap();
        let source_root_resolver = SourceRootResolver::new(&project_root, &file_walker);
        let source_roots = source_root_resolver
            .resolve(&project_config.source_roots)
            .unwrap();
        let module_tree_builder = ModuleTreeBuilder::new(
            &source_roots,
            &file_walker,
            project_config.forbid_circular_dependencies,
            project_config.root_module,
        );

        let (valid_modules, invalid_modules) =
            module_tree_builder.resolve_modules(project_config.all_modules());

        for invalid_module in invalid_modules {
            eprintln!(
                "Module '{}' not found. It will be ignored.",
                invalid_module.path
            );
        }

        // TODO: Remove unwraps
        let module_tree = module_tree_builder.build(valid_modules).unwrap();

        let affected_modules =
            get_affected_modules(&project_root, &project_config, changed_files, &module_tree)
                .unwrap();

        Self {
            project_root,
            source_roots,
            project_config,
            module_tree,
            affected_modules,
            all_affected_modules,
            removed_test_paths: HashSet::new(),
            num_removed_items: 0,
            tests_ran_to_completion: false,
        }
    }

    pub fn remove_test_path(&mut self, file_path: PathBuf) {
        self.removed_test_paths.insert(file_path);
    }

    pub fn should_remove_items(&self, file_path: PathBuf) -> bool {
        let imports = get_located_imports(
            &self.project_root,
            &self.source_roots,
            &file_path,
            &self.project_config,
            self.project_config.include_string_imports,
        )
        .unwrap_or_default();
        let mut should_remove = true;

        for import in imports {
            if let Some(nearest_module) = self.module_tree.find_nearest(import.module_path()) {
                if nearest_module.full_path.is_empty() {
                    continue;
                }
                if self.affected_modules.contains(&nearest_module.full_path) {
                    // If the module is affected, break early and don't remove the item
                    should_remove = false;
                    break;
                }
            }
        }
        should_remove
    }
}

fn build_module_consumer_map(modules: &Vec<ModuleConfig>) -> HashMap<&String, Vec<String>> {
    let mut consumer_map: HashMap<&String, Vec<String>> = HashMap::new();
    for module in modules {
        for dependency in module.dependencies_iter() {
            consumer_map
                .entry(&dependency.path)
                .or_default()
                .push(module.mod_path());
        }
    }
    consumer_map
}

fn get_changed_module_paths(
    project_root: &PathBuf,
    project_config: &ProjectConfig,
    changed_files: Vec<PathBuf>,
) -> Result<Vec<String>> {
    let file_walker = fs::FSWalker::try_new(
        project_root,
        &project_config.exclude,
        project_config.respect_gitignore,
    )?;
    let source_root_resolver = SourceRootResolver::new(project_root, &file_walker);
    let source_roots: Vec<PathBuf> = source_root_resolver.resolve(&project_config.source_roots)?;

    let changed_module_paths = changed_files
        .into_iter()
        .filter(|file| {
            file.extension().unwrap_or_default() == "py"
                && source_roots.iter().any(|root| file.starts_with(root))
        })
        .map(|file| fs::file_to_module_path(&source_roots, &file))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(changed_module_paths)
}

fn find_affected_modules(
    root_module_path: &String,
    module_consumers: &HashMap<&String, Vec<String>>,
    mut known_affected_modules: HashSet<String>,
) -> HashSet<String> {
    if let Some(consumers) = module_consumers.get(root_module_path) {
        for consumer in consumers {
            if !known_affected_modules.contains(consumer) {
                known_affected_modules.insert(consumer.clone());
                known_affected_modules.extend(find_affected_modules(
                    consumer,
                    module_consumers,
                    known_affected_modules.clone(),
                ));
            }
        }
    }
    known_affected_modules
}

pub fn get_affected_modules(
    project_root: &PathBuf,
    project_config: &ProjectConfig,
    changed_files: Vec<PathBuf>,
    module_tree: &ModuleTree,
) -> Result<HashSet<String>> {
    let changed_module_paths =
        get_changed_module_paths(project_root, project_config, changed_files)?;

    let mut affected_modules = HashSet::new();
    for changed_mod_path in changed_module_paths {
        let nearest_module = module_tree
            .find_nearest(&changed_mod_path)
            .ok_or(TestError::ModuleNotFound(changed_mod_path))?;
        affected_modules.insert(nearest_module.full_path.clone());
    }

    let modules = project_config.all_modules().cloned().collect();
    let module_consumers = build_module_consumer_map(&modules);
    for module in affected_modules.clone() {
        affected_modules = find_affected_modules(&module, &module_consumers, affected_modules);
    }

    Ok(affected_modules.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test::fixtures::module_tree;
    use crate::tests::test::fixtures::modules;
    use rstest::rstest;
    use std::env;

    #[rstest]
    #[case(&["python/tach/test.py"], "python", &["tach.test"])]
    #[case(&["tach/test.py", "tach/a/test.py"], ".", &["tach.test", "tach.a.test"])]
    #[case(&["tach/a/__init__.py"], ".", &["tach.a"])]
    fn test_get_changed_module_paths(
        #[case] changed_files: &[&str],
        #[case] source_root: &str,
        #[case] expected_mod_paths: &[&str],
    ) {
        let project_root = env::temp_dir();
        let project_config = ProjectConfig {
            source_roots: vec![PathBuf::from(source_root)],
            ..Default::default()
        };
        let changed_files = changed_files
            .iter()
            .map(|filepath| project_root.join(filepath))
            .collect();
        let expected_mod_paths = expected_mod_paths
            .iter()
            .map(|path| path.to_string())
            .collect::<HashSet<_>>();
        assert_eq!(
            expected_mod_paths,
            get_changed_module_paths(&project_root, &project_config, changed_files)
                .unwrap()
                .into_iter()
                .collect()
        );
    }

    #[rstest]
    #[case(&["tach/test.py"], &["tach.test", "tach.cli", "tach.__main__", "tach.start"])]
    #[case(
        &["tach/__init__.py"],
        &[
            "tach",
            "tach.cli",
            "tach.start",
            "tach.__main__",
            "tach.logging",
            "tach.cache"
        ]
    )]
    #[case(&[], &[])]
    fn test_affected_modules(
        #[case] changed_files: &[&str],
        #[case] expected_affected_modules: &[&str],
        module_tree: ModuleTree,
        modules: Vec<ModuleConfig>,
    ) {
        let project_root = env::temp_dir();
        let project_config = ProjectConfig {
            modules,
            ..Default::default()
        };
        let changed_files = changed_files
            .iter()
            .map(|filepath| project_root.join(filepath))
            .collect();
        let expected_affected_modules = expected_affected_modules
            .iter()
            .map(|path| path.to_string())
            .collect::<HashSet<_>>();

        // consider mocking get_changed_module_paths
        assert_eq!(
            expected_affected_modules,
            get_affected_modules(&project_root, &project_config, changed_files, &module_tree)
                .unwrap()
        );
    }
}
