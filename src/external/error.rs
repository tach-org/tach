use std::io;
use thiserror::Error;

use crate::filesystem::FileSystemError;

#[derive(Error, Debug)]
pub enum ParsingError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Filesystem error: {0}")]
    Filesystem(#[from] FileSystemError),
    #[error("TOML parsing error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("Missing field in TOML: {0}")]
    MissingField(String),
    #[error("Dependency group '{included}' included from '{from_group}' does not exist")]
    MissingDependencyGroup {
        included: String,
        from_group: String,
    },
    #[error("Circular dependency group reference: '{group}' includes itself")]
    CircularDependencyGroup { group: String },
}
