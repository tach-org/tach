use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DeadcodeFile {
    pub file_path: PathBuf,
    pub module_path: String,
}

impl DeadcodeFile {
    pub fn new(file_path: PathBuf, module_path: String) -> Self {
        Self {
            file_path,
            module_path,
        }
    }
}

pub fn is_init_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "__init__.py")
}
