use std::collections::HashSet;
use std::path::{Path, PathBuf};

use globset::Glob;

use crate::filesystem::{self, FSWalker};
use crate::resolvers::glob::has_glob_syntax;

#[derive(Debug, Default)]
pub struct ResolvedEntryPoints {
    pub files: Vec<PathBuf>,
    pub unresolved: Vec<String>,
}

fn is_python_path(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("py")
}

fn looks_like_path_or_glob(entry_point: &str) -> bool {
    entry_point.contains('/')
        || entry_point.contains('\\')
        || entry_point.ends_with(".py")
        || has_glob_syntax(entry_point)
}

fn push_once(
    resolved: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    source_roots: &[PathBuf],
    file_walker: &FSWalker,
    file_path: PathBuf,
) -> bool {
    if !is_python_path(&file_path)
        || !source_roots.iter().any(|root| file_path.starts_with(root))
        || file_walker.is_path_excluded(&file_path, false)
    {
        return false;
    }

    if seen.insert(file_path.clone()) {
        resolved.push(file_path);
    }
    true
}

fn resolve_path_or_glob(
    project_root: &Path,
    source_roots: &[PathBuf],
    file_walker: &FSWalker,
    normalized: &str,
    resolved: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
) -> bool {
    if has_glob_syntax(normalized) {
        if Glob::new(normalized).is_err() {
            return false;
        }

        let mut matched = false;
        for match_path in file_walker.walk_globbed_files(
            project_root.to_str().unwrap_or("."),
            std::iter::once(normalized),
        ) {
            matched |= push_once(resolved, seen, source_roots, file_walker, match_path);
        }
        return matched;
    }

    let mut matched = false;
    let direct_path = project_root.join(normalized);
    if direct_path.exists() {
        matched |= push_once(resolved, seen, source_roots, file_walker, direct_path);
    }

    for source_root in source_roots {
        let source_root_entry = source_root.join(normalized);
        if source_root_entry.exists() {
            matched |= push_once(resolved, seen, source_roots, file_walker, source_root_entry);
        }
    }

    matched
}

fn resolve_module(
    source_roots: &[PathBuf],
    file_walker: &FSWalker,
    normalized: &str,
    resolved: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
) -> bool {
    if let Some(resolved_module) = filesystem::module_to_file_path(source_roots, normalized, false)
    {
        return push_once(
            resolved,
            seen,
            source_roots,
            file_walker,
            resolved_module.file_path,
        );
    }

    false
}

pub fn resolve_entry_points(
    project_root: &Path,
    source_roots: &[PathBuf],
    file_walker: &FSWalker,
    raw_entry_points: &[String],
) -> ResolvedEntryPoints {
    let mut resolved = Vec::<PathBuf>::new();
    let mut unresolved = Vec::<String>::new();
    let mut seen = HashSet::<PathBuf>::new();

    for raw_entry_point in raw_entry_points {
        if raw_entry_point.trim().is_empty() {
            continue;
        }

        let normalized = raw_entry_point
            .split(':')
            .next()
            .map(str::trim)
            .unwrap_or_default();

        if normalized.is_empty() {
            continue;
        }

        let matched = if looks_like_path_or_glob(normalized) {
            resolve_path_or_glob(
                project_root,
                source_roots,
                file_walker,
                normalized,
                &mut resolved,
                &mut seen,
            )
        } else {
            resolve_module(
                source_roots,
                file_walker,
                normalized,
                &mut resolved,
                &mut seen,
            )
        };

        if !matched {
            unresolved.push(raw_entry_point.clone());
        }
    }

    ResolvedEntryPoints {
        files: resolved,
        unresolved,
    }
}
