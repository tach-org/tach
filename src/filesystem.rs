use std::fs;
use std::io;
use std::path::StripPrefixError;
use std::path::{Path, PathBuf, MAIN_SEPARATOR, MAIN_SEPARATOR_STR};

use cached::proc_macro::cached;
use globset::Glob;
use globset::GlobSetBuilder;
use ignore;
use itertools::Itertools;
use thiserror::Error;

use crate::config::root_module::ROOT_MODULE_SENTINEL_TAG;
use crate::config::ModuleConfig;

#[derive(Error, Debug)]
pub enum FileSystemError {
    #[error("Encountered unexpected I/O error.\n{0}")]
    Io(#[from] io::Error),
    #[error("Path does not appear to be within project root.\n{0}")]
    StripPrefix(#[from] StripPrefixError),
    #[error("Error building exclude paths: {0}")]
    Exclusion(#[from] ignore::Error),
    #[error("{0}")]
    Other(String),
}
pub type Result<T> = std::result::Result<T, FileSystemError>;

pub fn relative_to<P: AsRef<Path>, R: AsRef<Path>>(path: P, root: R) -> Result<PathBuf> {
    let diff_path = path.as_ref().strip_prefix(root)?;
    Ok(diff_path.to_owned())
}

pub fn file_to_module_path(source_roots: &[PathBuf], file_path: &Path) -> Result<String> {
    // Find the matching source root
    let matching_root = source_roots
        .iter()
        .find(|&root| file_path.starts_with(root))
        .ok_or(FileSystemError::Other(format!(
            "No matching source root found for filepath: {:?}",
            file_path
        )))?;

    // Get the relative path from the matching root
    let relative_path = file_path.strip_prefix(matching_root)?;

    // If the relative path is empty, return an error
    // indicating that the path cannot be a source root itself
    if relative_path.as_os_str().is_empty() {
        return Err(FileSystemError::Other(
            "Filepath cannot be a source root.".to_string(),
        ));
    }

    // Convert the relative path to a module path
    let mut components: Vec<_> = relative_path
        .parent()
        .ok_or(FileSystemError::Other(format!(
            "Encountered invalid filepath: {:?}",
            relative_path
        )))?
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();

    // Get the file name
    let file_name = relative_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(FileSystemError::Other(format!(
            "Encountered invalid filepath: {:?}",
            relative_path
        )))?;

    // If the file is not __init__.py, add its name (without extension) to the components
    if file_name != "__init__.py" {
        if let Some(stem) = Path::new(file_name).file_stem().and_then(|s| s.to_str()) {
            components.push(stem);
        }
    }

    // Join the components with dots
    let module_path = components.join(".");

    // If the module_path is empty, return ".", otherwise return the module_path
    Ok(if module_path.is_empty() {
        ".".to_string()
    } else {
        module_path
    })
}

#[derive(Debug, Clone)]
pub struct ResolvedModule {
    pub source_root: PathBuf,
    pub file_path: PathBuf,
    pub member_name: Option<String>,
}

impl ResolvedModule {
    pub fn relative_file_path(&self) -> PathBuf {
        relative_to(&self.file_path, &self.source_root).unwrap_or_else(|_| self.file_path.clone())
    }
}

fn is_potential_python_module_path(s: &str) -> bool {
    !s.is_empty()
        && s.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '*')
        })
}

#[cached(
    key = "String",
    convert = r#"{
    format!(
        "{}{}{}",
        roots.iter().map(|p| p.to_string_lossy()).join(";"),
        mod_path,
        check_members
    )
}"#
)]
fn cached_module_to_file_path(
    roots: &[PathBuf],
    mod_path: &str,
    check_members: bool,
) -> Option<ResolvedModule> {
    // Fast check because this may run on every string literal in every source file
    if !is_potential_python_module_path(mod_path) {
        return None;
    }

    let mod_as_file_path = mod_path.replace('.', MAIN_SEPARATOR_STR);
    for root in roots {
        let fs_path = root.join(&mod_as_file_path);

        // Check for [package with .pyi, .py] file or [.pyi, .py] file itself
        for path in &[
            fs_path.join("__init__.pyi"),
            fs_path.join("__init__.py"),
            fs_path.with_extension("pyi"),
            fs_path.with_extension("py"),
        ] {
            if path.exists() {
                return Some(ResolvedModule {
                    file_path: path.to_path_buf(),
                    source_root: root.clone(),
                    member_name: None,
                });
            }
        }
        // If the original file path does not contain a separator (e.g. 'os', 'ast')
        // then we are done checking this root.
        if !mod_as_file_path.contains(MAIN_SEPARATOR) || !check_members {
            continue;
        }

        if let Some(last_sep_index) = mod_as_file_path.rfind(MAIN_SEPARATOR) {
            let member_name = &mod_as_file_path[last_sep_index + 1..];
            let base_fs_path = root.join(&mod_as_file_path[..last_sep_index]);

            for path in &[
                base_fs_path.join("__init__.pyi"),
                base_fs_path.join("__init__.py"),
                base_fs_path.with_extension("pyi"),
                base_fs_path.with_extension("py"),
            ] {
                if path.exists() {
                    return Some(ResolvedModule {
                        file_path: path.to_path_buf(),
                        source_root: root.clone(),
                        member_name: Some(member_name.to_string()),
                    });
                }
            }
        }
    }
    None
}

pub fn module_to_file_path(
    roots: &[PathBuf],
    mod_path: &str,
    check_members: bool,
) -> Option<ResolvedModule> {
    cached_module_to_file_path(roots, mod_path, check_members)
}

pub fn module_to_pyfile_or_dir_path<P: AsRef<Path>>(
    roots: &[P],
    mod_path: &str,
) -> Option<PathBuf> {
    if mod_path.is_empty() {
        return None;
    }
    let base_path = mod_path.replace('.', MAIN_SEPARATOR_STR);

    // Iterate through each source root
    for source_root in roots {
        let source_root = source_root.as_ref();

        // Build paths
        let dir_path = source_root.join(&base_path);
        let pyinterface_path = source_root.join(format!("{}.pyi", base_path));
        let pyfile_path = source_root.join(format!("{}.py", base_path));

        if dir_path.is_dir() {
            return Some(dir_path);
        } else if pyinterface_path.exists() {
            return Some(pyinterface_path);
        } else if pyfile_path.exists() {
            return Some(pyfile_path);
        }
    }
    None
}

pub fn module_path_is_included_in_paths(
    source_roots: &[PathBuf],
    module_path: &str,
    included_paths: &[PathBuf],
) -> bool {
    module_to_pyfile_or_dir_path(source_roots, module_path).is_some_and(|path| {
        included_paths
            .iter()
            .any(|included_path| path.starts_with(included_path))
    })
}

pub fn read_file_content<P: AsRef<Path>>(path: P) -> Result<String> {
    fs::read_to_string(path).map_err(FileSystemError::Io)
}

pub fn is_hidden(entry: &ignore::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

fn is_pyfile(entry: &ignore::DirEntry) -> bool {
    match entry.path().extension() {
        Some(ext) => ext == "py",
        None => false,
    }
}

fn is_pymodule(entry: &ignore::DirEntry) -> bool {
    let path = entry.path();
    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
        path.join("__init__.py").exists() || path.join("__init__.pyi").exists()
    } else {
        // Check if the file is a .py or .pyi file and is not __init__.py (we will process the directory instead)
        matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("py" | "pyi")
        ) && path.file_stem().map(|s| s.to_str().unwrap_or("")) != Some("__init__")
    }
}

#[derive(Debug)]
pub struct ProjectFile<'a> {
    pub project_root: &'a Path,
    pub source_root: &'a Path,
    pub file_path: PathBuf,
    pub relative_file_path: PathBuf,
    pub contents: String,
}

impl<'a> ProjectFile<'a> {
    pub fn try_new(
        project_root: &'a Path,
        source_root: &'a Path,
        file_path: &'a Path,
    ) -> Result<Self> {
        let absolute_file_path = source_root.join(file_path);
        let contents = read_file_content(&absolute_file_path)?;
        Ok(Self {
            project_root,
            source_root,
            relative_file_path: relative_to(&absolute_file_path, project_root)?,
            file_path: absolute_file_path,
            contents,
        })
    }
}

impl AsRef<Path> for ProjectFile<'_> {
    fn as_ref(&self) -> &Path {
        &self.file_path
    }
}

#[derive(Debug, Clone)]
pub struct FSWalker {
    _project_root: PathBuf,
    overrides: ignore::overrides::Override,
    walk_builder: ignore::WalkBuilder,
}

impl FSWalker {
    pub fn try_new<P: AsRef<Path>>(
        project_root: P,
        exclude_paths: &[String],
        respect_gitignore: bool,
    ) -> Result<Self> {
        let mut walk_builder = ignore::WalkBuilder::new(project_root.as_ref());
        // Require git repository for git-related ignore rules
        // This ensures .gitignore files outside repository boundaries are not considered
        walk_builder.require_git(true);
        if !respect_gitignore {
            // Disable all ignore filters
            walk_builder.ignore(false);
            walk_builder.git_ignore(false);
            walk_builder.git_global(false);
        }

        let mut override_builder = ignore::overrides::OverrideBuilder::new(project_root.as_ref());
        for path in exclude_paths {
            override_builder.add(&format!("!{}", path))?;
        }
        let overrides = override_builder.build()?;
        walk_builder.overrides(overrides.clone());

        Ok(Self {
            _project_root: project_root.as_ref().to_path_buf(),
            overrides,
            walk_builder,
        })
    }

    pub fn empty<P: AsRef<Path>>(project_root: P) -> Self {
        Self::try_new(project_root, &[], false).unwrap()
    }

    pub fn is_path_excluded<P: AsRef<Path>>(&self, path: P, is_dir: bool) -> bool {
        self.overrides.matched(path.as_ref(), is_dir).is_ignore()
    }

    fn walk_non_excluded_paths(&self, root: &str) -> impl Iterator<Item = ignore::DirEntry> {
        let mut builder = self.walk_builder.clone();
        let owned_root = root.to_string();
        let overrides = self.overrides.clone();
        builder
            .filter_entry(move |e| {
                let path = e.path();
                if path.strip_prefix(&owned_root).is_ok() {
                    // We're at or below our target root - apply normal filters
                    !overrides
                        .matched(path, e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                        .is_ignore()
                } else {
                    // We're still traversing to reach our target - only check if this could be
                    // a parent directory of our target by seeing if our target starts with this path
                    owned_root.starts_with(path.to_str().unwrap_or(""))
                }
            })
            .build()
            .filter_map(|entry| entry.ok())
    }

    pub fn walk_dirs(&self, root: &str) -> impl Iterator<Item = PathBuf> {
        self.walk_non_excluded_paths(root)
            .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|entry| entry.into_path())
    }

    pub fn walk_pyfiles(&self, root: &str) -> impl Iterator<Item = PathBuf> {
        let prefix = root.to_string();
        self.walk_non_excluded_paths(root)
            .filter(|entry| {
                entry.file_type().map(|t| t.is_file()).unwrap_or(false) && is_pyfile(entry)
            })
            .map(move |entry| relative_to(entry.path(), &prefix).unwrap())
    }

    pub fn walk_pymodules(&self, root: &str) -> impl Iterator<Item = PathBuf> {
        let prefix = root.to_string();
        self.walk_non_excluded_paths(root)
            .filter(is_pymodule)
            .map(move |entry| relative_to(entry.path(), &prefix).unwrap())
    }

    pub fn walk_pyprojects(&self, root: &str) -> impl Iterator<Item = PathBuf> {
        let prefix = root.to_string();
        self.walk_non_excluded_paths(root)
            .filter(|entry| entry.file_name() == "pyproject.toml")
            .map(move |entry| relative_to(entry.path(), &prefix).unwrap())
    }
    pub fn walk_globbed_files<P, S>(&self, root: &str, patterns: P) -> impl Iterator<Item = PathBuf>
    where
        P: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut glob_builder = GlobSetBuilder::new();

        for pattern in patterns {
            glob_builder.add(Glob::new(pattern.as_ref()).unwrap());
        }

        let glob_set = glob_builder.build().unwrap();
        let prefix = root.to_string();
        self.walk_non_excluded_paths(root)
            .filter(move |entry| {
                entry.file_type().map(|t| t.is_file()).unwrap_or(false)
                    && glob_set.is_match(relative_to(entry.path(), &prefix).unwrap())
            })
            .map(|entry| entry.into_path())
    }

    pub fn walk_domain_config_files(&self, root: &str) -> impl Iterator<Item = PathBuf> {
        self.walk_non_excluded_paths(root)
            .filter(|entry| entry.file_name() == "tach.domain.toml")
            .map(|entry| entry.into_path())
    }
}

pub fn validate_module_path(source_roots: &[PathBuf], module_path: &str) -> bool {
    module_path == ROOT_MODULE_SENTINEL_TAG
        || module_to_pyfile_or_dir_path(source_roots, module_path).is_some()
}

/// Returns a tuple of (valid, invalid) modules
pub fn validate_project_modules(
    source_roots: &[PathBuf],
    modules: Vec<ModuleConfig>,
) -> (Vec<ModuleConfig>, Vec<ModuleConfig>) {
    let mut result = (Vec::new(), Vec::new());

    for module in modules {
        if validate_module_path(source_roots, &module.path) {
            // valid module
            result.0.push(module);
        } else {
            // invalid module
            result.1.push(module);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixtures::tests_dir;
    use rstest::rstest;

    #[rstest]
    #[case(&["."], "__init__.py", ".")]
    #[case(&["."], "domain_one/__init__.py", "domain_one")]
    #[case(&["."], "domain_one/interface.py", "domain_one.interface")]
    #[case(&["source/root"], "source/root/domain.py", "domain")]
    #[case(&["src1", "src2"], "src1/core/lib/cat.py", "core.lib.cat")]
    fn test_file_to_mod_path(
        tests_dir: PathBuf,
        #[case] roots: &[&str],
        #[case] file_path: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(
            file_to_module_path(
                roots
                    .iter()
                    .map(|r| tests_dir.join(r))
                    .collect::<Vec<_>>()
                    .as_slice(),
                &tests_dir.join(file_path)
            )
            .unwrap(),
            expected
        );
    }
}
