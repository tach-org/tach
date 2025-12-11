use std::path::{Path, PathBuf};

use pyo3::prelude::*;
use ruff_linter::Locator;

use crate::config::ProjectConfig;
use crate::dependencies::import::LocatedImport;
use crate::filesystem;
use crate::processors::ignore_directive::get_ignore_directives;
use crate::processors::import::{Result, get_normalized_imports};
use crate::resolvers::{PackageResolution, PackageResolutionError, PackageResolver};

#[pyclass(get_all)]
pub struct PythonImport {
    pub module_path: String,
    pub line_number: usize,
}

impl<'py> IntoPyObject<'py> for LocatedImport {
    type Target = PythonImport;
    type Output = Bound<'py, Self::Target>;
    type Error = pyo3::PyErr;
    fn into_pyobject(self, py: Python<'py>) -> std::result::Result<Self::Output, Self::Error> {
        PythonImport {
            module_path: self.import.module_path,
            line_number: self.alias_line_number,
        }
        .into_pyobject(py)
    }
}

pub fn get_located_project_imports<P: AsRef<Path>>(
    project_root: &PathBuf,
    source_roots: &[PathBuf],
    file_path: P,
    project_config: &ProjectConfig,
) -> Result<Vec<LocatedImport>> {
    if !file_path.as_ref().starts_with(project_root) {
        return Ok(vec![]);
    }

    let file_contents = filesystem::read_file_content(file_path.as_ref())?;
    let line_index = Locator::new(&file_contents).to_index().clone();
    let normalized_imports = get_normalized_imports(
        source_roots,
        file_path.as_ref(),
        &file_contents,
        project_config.ignore_type_checking_imports,
        project_config.include_string_imports,
    )?;
    let ignore_directives = get_ignore_directives(&file_contents);
    let file_walker = filesystem::FSWalker::try_new(
        project_root,
        &project_config.exclude,
        project_config.respect_gitignore,
    )?;
    let package_resolver = PackageResolver::try_new(project_root, source_roots, &file_walker)?;
    let package = match package_resolver.resolve_file_path(file_path.as_ref()) {
        PackageResolution::Found { package, .. } => package,
        PackageResolution::NotFound | PackageResolution::Excluded => {
            return Err(PackageResolutionError::PackageRootNotFound(
                file_path.as_ref().display().to_string(),
            )
            .into());
        }
    };

    Ok(normalized_imports
        .into_iter()
        .map(|import| {
            LocatedImport::new(
                line_index.line_index(import.import_offset).get(),
                line_index.line_index(import.alias_offset).get(),
                import,
            )
        })
        .filter(|import| {
            if ignore_directives.is_ignored(import) {
                return false;
            }

            match package_resolver.resolve_module_path(import.module_path()) {
                PackageResolution::Found {
                    package: resolved_package,
                    ..
                } => package.root == resolved_package.root,
                PackageResolution::NotFound | PackageResolution::Excluded => false,
            }
        })
        .collect())
}

pub fn get_located_external_imports<P: AsRef<Path>>(
    project_root: &PathBuf,
    source_roots: &[PathBuf],
    file_path: P,
    project_config: &ProjectConfig,
) -> Result<Vec<LocatedImport>> {
    if !file_path.as_ref().starts_with(project_root) {
        return Ok(vec![]);
    }

    let file_contents = filesystem::read_file_content(file_path.as_ref())?;
    let line_index = Locator::new(&file_contents).to_index().clone();
    let normalized_imports = get_normalized_imports(
        source_roots,
        file_path.as_ref(),
        &file_contents,
        project_config.ignore_type_checking_imports,
        false,
    )?;
    let ignore_directives = get_ignore_directives(&file_contents);
    let file_walker = filesystem::FSWalker::try_new(
        project_root,
        &project_config.exclude,
        project_config.respect_gitignore,
    )?;
    let package_resolver = PackageResolver::try_new(project_root, source_roots, &file_walker)?;
    let package = match package_resolver.resolve_file_path(file_path.as_ref()) {
        PackageResolution::Found { package, .. } => package,
        PackageResolution::NotFound | PackageResolution::Excluded => {
            return Err(PackageResolutionError::PackageRootNotFound(
                file_path.as_ref().display().to_string(),
            )
            .into());
        }
    };

    Ok(normalized_imports
        .into_iter()
        .map(|import| {
            LocatedImport::new(
                line_index.line_index(import.import_offset).get(),
                line_index.line_index(import.alias_offset).get(),
                import,
            )
        })
        .filter(|import| {
            if ignore_directives.is_ignored(import) {
                return false;
            }

            match package_resolver.resolve_module_path(import.module_path()) {
                PackageResolution::Found {
                    package: resolved_package,
                    ..
                } => package.root != resolved_package.root,
                PackageResolution::NotFound => true,
                PackageResolution::Excluded => false,
            }
        })
        .collect())
}
