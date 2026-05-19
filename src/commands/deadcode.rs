use std::path::Path;

use crate::{config::ProjectConfig, diagnostics::Diagnostic};

use super::check::CheckError;

pub fn check_deadcode(
    project_root: &Path,
    project_config: &ProjectConfig,
    entry_points: Option<Vec<String>>,
    files: bool,
    symbols: bool,
) -> Result<Vec<Diagnostic>, CheckError> {
    let _ = (project_root, project_config, entry_points, files, symbols);
    Ok(vec![])
}
