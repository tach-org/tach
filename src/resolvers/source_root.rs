use std::{collections::HashSet, path::PathBuf};

use globset;
use thiserror::Error;

use crate::filesystem;

use super::glob;

#[derive(Error, Debug)]
pub enum SourceRootResolverError {
    #[error("Invalid source root: {0}")]
    InvalidSourceRoot(String),
    #[error("Failed to handle glob: {0}")]
    GlobError(#[from] globset::Error),
}

pub struct SourceRootResolver<'a> {
    project_root: &'a PathBuf,
    file_walker: &'a filesystem::FSWalker,
}

impl<'a> SourceRootResolver<'a> {
    pub fn new(project_root: &'a PathBuf, file_walker: &'a filesystem::FSWalker) -> Self {
        Self {
            project_root,
            file_walker,
        }
    }

    pub fn resolve(
        &self,
        source_roots: &[PathBuf],
    ) -> Result<Vec<PathBuf>, SourceRootResolverError> {
        // Configured order is preserved: when the same module path exists under
        // more than one source root, the first configured root wins (mirroring
        // how Python resolves imports along 'sys.path'). Duplicates are removed.
        let mut seen = HashSet::new();
        let mut resolved = Vec::new();
        for root in source_roots {
            let expanded = if root.as_os_str().to_str() == Some(".") {
                // Don't want to construct a path like: "<project_root>/."
                vec![self.project_root.to_path_buf()]
            } else {
                match root.as_os_str().to_str() {
                    Some(s) => {
                        if glob::has_glob_syntax(s) {
                            let mut matches = glob::find_matching_directories(
                                self.project_root,
                                s,
                                self.file_walker,
                            )
                            .map_err(SourceRootResolverError::GlobError)?;
                            // Walk order is not guaranteed; sort for determinism.
                            matches.sort();
                            matches
                        } else {
                            vec![self.project_root.join(root)]
                        }
                    }
                    None => {
                        return Err(SourceRootResolverError::InvalidSourceRoot(
                            root.display().to_string(),
                        ));
                    }
                }
            };

            for path in expanded {
                if seen.insert(path.clone()) {
                    resolved.push(path);
                }
            }
        }
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_directory() -> TempDir {
        let temp_dir = TempDir::with_prefix("tach-test").unwrap();
        let root_path = temp_dir.path();

        // Create a directory structure for testing
        fs::create_dir_all(root_path.join("src/main")).unwrap();
        fs::create_dir_all(root_path.join("src/lib")).unwrap();
        fs::create_dir_all(root_path.join("tests")).unwrap();
        fs::create_dir_all(root_path.join("examples/one")).unwrap();
        fs::create_dir_all(root_path.join("examples/two")).unwrap();
        fs::create_dir_all(root_path.join("docs")).unwrap();

        temp_dir
    }

    #[test]
    fn test_resolve_single_directory() {
        let temp_dir = setup_test_directory();
        let project_root = PathBuf::from(temp_dir.path());
        let file_walker = filesystem::FSWalker::empty(&project_root);
        let resolver = SourceRootResolver::new(&project_root, &file_walker);

        let source_roots = vec![PathBuf::from("src")];
        let resolved = resolver.resolve(&source_roots).unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0], project_root.join("src"));
    }

    #[test]
    fn test_resolve_current_directory() {
        let temp_dir = setup_test_directory();
        let project_root = PathBuf::from(temp_dir.path());
        let file_walker = filesystem::FSWalker::empty(&project_root);
        let resolver = SourceRootResolver::new(&project_root, &file_walker);

        let source_roots = vec![PathBuf::from(".")];
        let resolved = resolver.resolve(&source_roots).unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0], project_root);
    }

    #[test]
    fn test_resolve_glob_pattern() {
        let temp_dir = setup_test_directory();
        let project_root = PathBuf::from(temp_dir.path());
        let file_walker = filesystem::FSWalker::empty(&project_root);
        let resolver = SourceRootResolver::new(&project_root, &file_walker);

        let source_roots = vec![PathBuf::from("examples/*")];
        let resolved = resolver.resolve(&source_roots).unwrap();

        assert_eq!(resolved.len(), 2);
        assert!(resolved.contains(&project_root.join("examples/one")));
        assert!(resolved.contains(&project_root.join("examples/two")));
    }

    #[test]
    fn test_resolve_multiple_patterns() {
        let temp_dir = setup_test_directory();
        let project_root = PathBuf::from(temp_dir.path());
        let file_walker = filesystem::FSWalker::empty(&project_root);
        let resolver = SourceRootResolver::new(&project_root, &file_walker);

        let source_roots = vec![PathBuf::from("src/*"), PathBuf::from("tests")];
        let resolved = resolver.resolve(&source_roots).unwrap();

        assert_eq!(resolved.len(), 3);
        assert!(resolved.contains(&project_root.join("src/main")));
        assert!(resolved.contains(&project_root.join("src/lib")));
        assert!(resolved.contains(&project_root.join("tests")));
    }

    #[test]
    fn test_resolve_deduplicates_paths() {
        let temp_dir = setup_test_directory();
        let project_root = PathBuf::from(temp_dir.path());
        let file_walker = filesystem::FSWalker::empty(&project_root);
        let resolver = SourceRootResolver::new(&project_root, &file_walker);

        let source_roots = vec![
            PathBuf::from("src"),
            PathBuf::from("src"), // Duplicate
        ];
        let resolved = resolver.resolve(&source_roots).unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0], project_root.join("src"));
    }
}
