use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;

use crate::{
    config::{ProjectConfig, RuleSetting},
    diagnostics::{
        CodeDiagnostic, ConfigurationDiagnostic, Diagnostic, DiagnosticDetails, Severity,
    },
    filesystem as fs,
    interrupt::check_interrupt,
    processors::import::get_normalized_imports_from_ast,
    python::parsing::parse_python_source,
    resolvers::{SourceRootResolver, glob::has_glob_syntax},
};

use super::check::CheckError;

/// Detect Python files which cannot be reached from the configured entry points.
///
/// This is a file-level reachability analysis: starting from the entry points,
/// imports are followed transitively (including imports guarded by
/// `if TYPE_CHECKING:`, since deleting a typing-only module would still break
/// the codebase), and any project file outside the reachable set is reported.
pub fn check_deadcode(
    project_root: &Path,
    project_config: &ProjectConfig,
    cli_entry_points: Option<Vec<String>>,
    cli_severity: Option<String>,
) -> Result<Vec<Diagnostic>, CheckError> {
    if !project_root.is_dir() {
        return Err(CheckError::InvalidDirectory(
            project_root.display().to_string(),
        ));
    }

    let Some(severity) = effective_severity(project_config, cli_severity.as_deref())? else {
        // Severity is 'off': skip all analysis.
        return Ok(vec![]);
    };

    let ignore_matcher = build_ignore_matcher(&project_config.deadcode.ignore)?;

    let project_root = project_root.to_path_buf();
    let file_walker = fs::FSWalker::try_new(
        &project_root,
        &project_config.exclude,
        project_config.respect_gitignore,
    )?;
    let source_root_resolver = SourceRootResolver::new(&project_root, &file_walker);
    let source_roots = source_root_resolver.resolve(&project_config.source_roots)?;

    let files = collect_project_files(&source_roots, &file_walker);
    check_interrupt().map_err(|_| CheckError::Interrupt)?;

    let (edges, unparsable_files) = parse_import_edges(&files, &source_roots);
    check_interrupt().map_err(|_| CheckError::Interrupt)?;

    let mut entry_points = project_config.deadcode.entry_points.clone();
    entry_points.extend(cli_entry_points.unwrap_or_default());
    let (entry_files, unmatched_entry_points) =
        resolve_entry_points(&project_root, &source_roots, &file_walker, &files, &entry_points);

    let mut diagnostics: Vec<Diagnostic> = unmatched_entry_points
        .into_iter()
        .map(|entry_point| {
            Diagnostic::new_global_warning(DiagnosticDetails::Configuration(
                ConfigurationDiagnostic::DeadCodeEntryPointNotFound { entry_point },
            ))
        })
        .collect();

    for (file, failure_severity) in &unparsable_files {
        let file_path = relative_display_path(&project_root, file);
        let details = match failure_severity {
            Severity::Error => ConfigurationDiagnostic::SkippedFileSyntaxError { file_path },
            Severity::Warning => ConfigurationDiagnostic::SkippedFileIoError { file_path },
        };
        diagnostics.push(Diagnostic::new_global(
            *failure_severity,
            DiagnosticDetails::Configuration(details),
        ));
    }

    if entry_files.is_empty() {
        diagnostics.push(Diagnostic::new_global_warning(
            DiagnosticDetails::Configuration(ConfigurationDiagnostic::DeadCodeNoEntryPoints()),
        ));
        return Ok(sorted(diagnostics));
    }

    let entry_roots = expand_entry_roots(&files, &source_roots, &entry_files);
    let reachable = reachable_files(&edges, &entry_roots);

    if unparsable_files.keys().any(|file| reachable.contains(file)) {
        // Imports of a reachable file are unknown, so reachability cannot be
        // trusted; report nothing rather than false positives.
        diagnostics.push(Diagnostic::new_global_warning(
            DiagnosticDetails::Configuration(
                ConfigurationDiagnostic::DeadCodeSkippedUnparsableFiles(),
            ),
        ));
        return Ok(sorted(diagnostics));
    }

    for (file, module_path) in &files {
        if reachable.contains(file)
            || unparsable_files.contains_key(file)
            // A package `__init__.py` exists to make its live siblings importable;
            // it is only reported once the rest of the package is gone.
            || is_init_file(file)
        {
            continue;
        }

        let relative_path = fs::relative_to(file, &project_root).unwrap_or_else(|_| file.clone());
        if ignore_matcher.is_match(&relative_path) {
            continue;
        }

        diagnostics.push(Diagnostic::new_located(
            severity,
            DiagnosticDetails::Code(CodeDiagnostic::UnreachableFile {
                module_path: module_path.clone(),
            }),
            relative_path,
            1,
            None,
        ));
    }

    Ok(sorted(diagnostics))
}

/// Resolve the effective severity, preferring the CLI override over config.
/// Returns `None` when detection is disabled ('off').
fn effective_severity(
    project_config: &ProjectConfig,
    cli_severity: Option<&str>,
) -> Result<Option<Severity>, CheckError> {
    let setting = match cli_severity {
        Some("error") => RuleSetting::Error,
        Some("warn") => RuleSetting::Warn,
        Some("off") => RuleSetting::Off,
        Some(other) => {
            return Err(CheckError::Configuration(format!(
                "Invalid severity '{other}'. Expected 'error', 'warn', or 'off'."
            )));
        }
        None => project_config.deadcode.severity.clone(),
    };
    Ok(Severity::try_from(&setting).ok())
}

fn build_ignore_matcher(patterns: &[String]) -> Result<GlobSet, CheckError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|_| {
            CheckError::Configuration(format!(
                "Invalid glob pattern '{pattern}' in '[deadcode]' ignore."
            ))
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|_| CheckError::Configuration("Failed to build '[deadcode]' ignore.".to_string()))
}

/// All Python files under the source roots, mapped to their module paths.
fn collect_project_files(
    source_roots: &[PathBuf],
    file_walker: &fs::FSWalker,
) -> BTreeMap<PathBuf, String> {
    let mut files = BTreeMap::new();
    for source_root in source_roots {
        for relative_path in file_walker.walk_pyfiles(&source_root.display().to_string()) {
            let file_path = source_root.join(&relative_path);
            if let Ok(module_path) = fs::file_to_module_path(source_roots, &file_path) {
                files.insert(file_path, module_path);
            }
        }
    }
    files
}

/// Parse every file and resolve its imports to project files, in parallel.
/// Returns the import edges and the files which could not be parsed
/// (with the severity their skip diagnostic should carry).
fn parse_import_edges(
    files: &BTreeMap<PathBuf, String>,
    source_roots: &[PathBuf],
) -> (
    BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    BTreeMap<PathBuf, Severity>,
) {
    let results: Vec<(PathBuf, Result<BTreeSet<PathBuf>, Severity>)> = files
        .par_iter()
        .map(|(file, _)| (file.clone(), file_import_targets(files, source_roots, file)))
        .collect();

    let mut edges = BTreeMap::new();
    let mut unparsable = BTreeMap::new();
    for (file, result) in results {
        match result {
            Ok(targets) => {
                edges.insert(file, targets);
            }
            Err(failure_severity) => {
                unparsable.insert(file, failure_severity);
            }
        }
    }
    (edges, unparsable)
}

fn file_import_targets(
    files: &BTreeMap<PathBuf, String>,
    source_roots: &[PathBuf],
    file: &Path,
) -> Result<BTreeSet<PathBuf>, Severity> {
    let contents = fs::read_file_content(file).map_err(|_| Severity::Warning)?;
    let ast = parse_python_source(&contents).map_err(|_| Severity::Error)?;
    let imports = get_normalized_imports_from_ast(
        source_roots,
        file,
        &ast,
        // Never ignore `if TYPE_CHECKING:` imports here (regardless of the
        // project-wide setting): a module imported only for type annotations
        // is not dead code.
        false,
        project_includes_string_imports(),
    )
    .map_err(|_| Severity::Error)?;

    let mut targets = BTreeSet::new();
    for import in imports {
        let Some(resolved) = fs::module_to_file_path(source_roots, &import.module_path, true)
        else {
            continue;
        };
        if let Some(target) = project_file_for(files, &resolved.file_path) {
            targets.insert(target);
        }
        // Importing `a.b.c` also executes `a/__init__.py` and `a/b/__init__.py`.
        targets.extend(ancestor_init_files(files, source_roots, &import.module_path));
    }
    Ok(targets)
}

/// String literals which look like module paths are treated as imports.
/// This errs on the side of keeping dynamically-imported files alive.
fn project_includes_string_imports() -> bool {
    true
}

/// Map a resolved import target to the project file that represents it in the
/// graph. Stub files (`.pyi`) resolve to their sibling implementation, since
/// only `.py` files participate in the analysis.
fn project_file_for(files: &BTreeMap<PathBuf, String>, resolved: &Path) -> Option<PathBuf> {
    if files.contains_key(resolved) {
        return Some(resolved.to_path_buf());
    }
    if resolved.extension().is_some_and(|ext| ext == "pyi") {
        let sibling = resolved.with_extension("py");
        if files.contains_key(&sibling) {
            return Some(sibling);
        }
    }
    None
}

/// The `__init__.py` files of every ancestor package of `module_path`.
fn ancestor_init_files(
    files: &BTreeMap<PathBuf, String>,
    source_roots: &[PathBuf],
    module_path: &str,
) -> Vec<PathBuf> {
    let parts: Vec<&str> = module_path.split('.').collect();
    (1..parts.len())
        .filter_map(|end| {
            let package_path = parts[..end].join(".");
            let resolved = fs::module_to_file_path(source_roots, &package_path, false)?;
            let target = project_file_for(files, &resolved.file_path)?;
            is_init_file(&target).then_some(target)
        })
        .collect()
}

fn is_init_file(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "__init__.py")
}

/// Resolve raw entry point specs to project files. Each spec may be a file
/// path (relative to the project root or a source root), a glob pattern, or a
/// module path. A trailing ':symbol' qualifier is tolerated and ignored.
/// Specs which match no project files are returned separately for reporting.
fn resolve_entry_points(
    project_root: &Path,
    source_roots: &[PathBuf],
    file_walker: &fs::FSWalker,
    files: &BTreeMap<PathBuf, String>,
    raw_entry_points: &[String],
) -> (BTreeSet<PathBuf>, Vec<String>) {
    let mut entry_files = BTreeSet::new();
    let mut unmatched = Vec::new();

    for raw_entry_point in raw_entry_points {
        let spec = raw_entry_point
            .split(':')
            .next()
            .unwrap_or_default()
            .trim();
        if spec.is_empty() {
            unmatched.push(raw_entry_point.clone());
            continue;
        }

        let mut matched = BTreeSet::new();
        if has_glob_syntax(spec) {
            let roots = std::iter::once(project_root).chain(source_roots.iter().map(PathBuf::as_path));
            for root in roots {
                for path in
                    file_walker.walk_globbed_files(&root.display().to_string(), std::iter::once(spec))
                {
                    matched.extend(project_file_for(files, &path));
                }
            }
        } else {
            // Literal path, tried against the project root and each source root.
            let candidate_roots =
                std::iter::once(project_root).chain(source_roots.iter().map(PathBuf::as_path));
            for root in candidate_roots {
                matched.extend(project_file_for(files, &root.join(spec)));
            }
            // Module path.
            if let Some(resolved) = fs::module_to_file_path(source_roots, spec, false) {
                matched.extend(project_file_for(files, &resolved.file_path));
            }
        }

        if matched.is_empty() {
            unmatched.push(raw_entry_point.clone());
        } else {
            entry_files.extend(matched);
        }
    }

    (entry_files, unmatched)
}

/// Entry files plus the `__init__.py` of each of their ancestor packages
/// (running `python -m pkg.app` imports `pkg` first).
fn expand_entry_roots(
    files: &BTreeMap<PathBuf, String>,
    source_roots: &[PathBuf],
    entry_files: &BTreeSet<PathBuf>,
) -> BTreeSet<PathBuf> {
    let mut roots = entry_files.clone();
    for entry_file in entry_files {
        if let Some(module_path) = files.get(entry_file) {
            roots.extend(ancestor_init_files(files, source_roots, module_path));
        }
    }
    roots
}

fn reachable_files(
    edges: &BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    roots: &BTreeSet<PathBuf>,
) -> BTreeSet<PathBuf> {
    let mut reachable = roots.clone();
    let mut queue: VecDeque<PathBuf> = roots.iter().cloned().collect();
    while let Some(current) = queue.pop_front() {
        let Some(targets) = edges.get(&current) else {
            continue;
        };
        for target in targets {
            if reachable.insert(target.clone()) {
                queue.push_back(target.clone());
            }
        }
    }
    reachable
}

fn relative_display_path(project_root: &Path, file: &Path) -> String {
    fs::relative_to(file, project_root)
        .unwrap_or_else(|_| file.to_path_buf())
        .display()
        .to_string()
}

fn sorted(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by(|left, right| match (left.file_path(), right.file_path()) {
        (None, None) => left.message().cmp(&right.message()),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(left_path), Some(right_path)) => left_path
            .cmp(right_path)
            .then_with(|| left.message().cmp(&right.message())),
    });
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::TempDir;

    fn write_files(project_root: &Path, files: &[(&str, &str)]) {
        for (path, contents) in files {
            let file_path = project_root.join(path);
            std_fs::create_dir_all(file_path.parent().unwrap()).unwrap();
            std_fs::write(file_path, contents).unwrap();
        }
    }

    fn config_with_entry_points(entry_points: &[&str]) -> ProjectConfig {
        let mut config = ProjectConfig {
            exclude: vec![],
            ..Default::default()
        };
        config.deadcode.entry_points = entry_points.iter().map(|s| s.to_string()).collect();
        config
    }

    fn run(project_root: &Path, config: &ProjectConfig) -> Vec<Diagnostic> {
        check_deadcode(project_root, config, None, None).unwrap()
    }

    fn unreachable_modules(diagnostics: &[Diagnostic]) -> BTreeSet<String> {
        diagnostics
            .iter()
            .filter_map(|diagnostic| match diagnostic.details() {
                DiagnosticDetails::Code(CodeDiagnostic::UnreachableFile { module_path }) => {
                    Some(module_path.clone())
                }
                _ => None,
            })
            .collect()
    }

    fn modules(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reports_unreachable_files_and_keeps_reachable_chain() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "from pkg.used import run\nrun()\n"),
                ("pkg/__init__.py", ""),
                ("pkg/used.py", "from pkg.helper import help\ndef run(): ...\n"),
                ("pkg/helper.py", "def help(): ...\n"),
                ("pkg/dead.py", "def gone(): ...\n"),
                ("orphan.py", "x = 1\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["pkg.dead", "orphan"])
        );
    }

    #[test]
    fn cycles_do_not_hang_or_leak() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "import a\n"),
                ("a.py", "import b\n"),
                ("b.py", "import a\n"),
                ("dead.py", "import dead_friend\n"),
                ("dead_friend.py", "import dead\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["dead", "dead_friend"])
        );
    }

    #[test]
    fn type_checking_imports_keep_files_alive() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                (
                    "main.py",
                    "from typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    from models import User\ndef f(user: \"User\") -> None: ...\n",
                ),
                ("models.py", "class User: ...\n"),
                ("dead.py", "x = 1\n"),
            ],
        );
        let mut config = config_with_entry_points(&["main.py"]);
        // Even with the project-wide setting asking to ignore type-checking
        // imports for boundary checks, dead code detection must include them.
        config.ignore_type_checking_imports = true;

        let diagnostics = run(temp.path(), &config);

        assert_eq!(unreachable_modules(&diagnostics), modules(&["dead"]));
    }

    #[test]
    fn pyi_stub_resolves_to_sibling_implementation() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "from impl import f\nf()\n"),
                ("impl.py", "def f(): ...\n"),
                ("impl.pyi", "def f() -> None: ...\n"),
                ("dead.py", "x = 1\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert_eq!(unreachable_modules(&diagnostics), modules(&["dead"]));
    }

    #[test]
    fn importing_a_submodule_keeps_ancestor_packages_alive() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "from pkg.sub.mod import f\nf()\n"),
                ("pkg/__init__.py", "from pkg.init_dep import x\n"),
                ("pkg/init_dep.py", "x = 1\n"),
                ("pkg/sub/__init__.py", ""),
                ("pkg/sub/mod.py", "def f(): ...\n"),
                ("pkg/dead.py", "y = 2\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        // pkg/__init__.py runs on import, so its own imports are alive too.
        assert_eq!(unreachable_modules(&diagnostics), modules(&["pkg.dead"]));
    }

    #[test]
    fn entry_point_forms_path_glob_and_module() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("scripts/one.py", "x = 1\n"),
                ("scripts/two.py", "y = 2\n"),
                ("pkg/__init__.py", ""),
                ("pkg/cli.py", "def main(): ...\n"),
                ("app.py", "z = 3\n"),
                ("dead.py", "d = 4\n"),
            ],
        );
        let config = config_with_entry_points(&["app.py", "scripts/*.py", "pkg.cli:main"]);

        let diagnostics = run(temp.path(), &config);

        assert_eq!(unreachable_modules(&diagnostics), modules(&["dead"]));
    }

    #[test]
    fn globs_match_inside_source_roots() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("src/scripts/entry.py", "from lib import f\nf()\n"),
                ("src/lib.py", "def f(): ...\n"),
                ("src/dead.py", "x = 1\n"),
            ],
        );
        let mut config = config_with_entry_points(&["scripts/*.py"]);
        config.source_roots = vec![PathBuf::from("src")];

        let diagnostics = run(temp.path(), &config);

        assert_eq!(unreachable_modules(&diagnostics), modules(&["dead"]));
    }

    #[test]
    fn unmatched_entry_point_warns_instead_of_flagging_everything() {
        let temp = TempDir::new().unwrap();
        write_files(temp.path(), &[("app.py", "x = 1\n"), ("other.py", "y = 2\n")]);
        let config = config_with_entry_points(&["app.py", "missing.py"]);

        let diagnostics = run(temp.path(), &config);

        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.details(),
            DiagnosticDetails::Configuration(
                ConfigurationDiagnostic::DeadCodeEntryPointNotFound { entry_point }
            ) if entry_point == "missing.py"
        )));
        assert_eq!(unreachable_modules(&diagnostics), modules(&["other"]));
    }

    #[test]
    fn no_entry_points_reports_configuration_warning_and_no_findings() {
        let temp = TempDir::new().unwrap();
        write_files(temp.path(), &[("app.py", "x = 1\n")]);
        let config = config_with_entry_points(&[]);

        let diagnostics = run(temp.path(), &config);

        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.details(),
            DiagnosticDetails::Configuration(ConfigurationDiagnostic::DeadCodeNoEntryPoints())
        )));
        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn ignore_globs_suppress_selectively() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("app.py", "x = 1\n"),
                ("generated/schema.py", "s = 1\n"),
                ("still_dead.py", "y = 2\n"),
            ],
        );
        let mut config = config_with_entry_points(&["app.py"]);
        config.deadcode.ignore = vec!["generated/**".to_string()];

        let diagnostics = run(temp.path(), &config);

        assert_eq!(unreachable_modules(&diagnostics), modules(&["still_dead"]));
    }

    #[test]
    fn invalid_ignore_glob_is_a_configuration_error() {
        let temp = TempDir::new().unwrap();
        write_files(temp.path(), &[("app.py", "x = 1\n")]);
        let mut config = config_with_entry_points(&["app.py"]);
        config.deadcode.ignore = vec!["bad[".to_string()];

        let result = check_deadcode(temp.path(), &config, None, None);

        assert!(matches!(result, Err(CheckError::Configuration(_))));
    }

    #[test]
    fn init_files_are_never_reported() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("app.py", "x = 1\n"),
                ("deadpkg/__init__.py", ""),
                ("deadpkg/mod.py", "y = 2\n"),
            ],
        );
        let config = config_with_entry_points(&["app.py"]);

        let diagnostics = run(temp.path(), &config);

        assert_eq!(unreachable_modules(&diagnostics), modules(&["deadpkg.mod"]));
    }

    #[test]
    fn reachable_syntax_error_suppresses_findings_with_explanation() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "import broken\n"),
                ("broken.py", "def f(:\n"),
                ("dead.py", "x = 1\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert!(unreachable_modules(&diagnostics).is_empty());
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.details(),
            DiagnosticDetails::Configuration(
                ConfigurationDiagnostic::DeadCodeSkippedUnparsableFiles()
            )
        )));
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.details(),
            DiagnosticDetails::Configuration(ConfigurationDiagnostic::SkippedFileSyntaxError { .. })
        )));
    }

    #[test]
    fn unreachable_syntax_error_does_not_suppress_findings() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "x = 1\n"),
                ("broken.py", "def f(:\n"),
                ("dead.py", "y = 2\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        // The broken file is reported as skipped, not as dead; other dead files
        // are still reported.
        assert_eq!(unreachable_modules(&diagnostics), modules(&["dead"]));
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.details(),
            DiagnosticDetails::Configuration(ConfigurationDiagnostic::SkippedFileSyntaxError { .. })
        )));
    }

    #[test]
    fn severity_off_short_circuits() {
        let temp = TempDir::new().unwrap();
        write_files(temp.path(), &[("app.py", "x = 1\n"), ("dead.py", "y = 2\n")]);
        let mut config = config_with_entry_points(&["app.py"]);
        config.deadcode.severity = RuleSetting::Off;

        let diagnostics = run(temp.path(), &config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn cli_severity_overrides_config() {
        let temp = TempDir::new().unwrap();
        write_files(temp.path(), &[("app.py", "x = 1\n"), ("dead.py", "y = 2\n")]);
        let config = config_with_entry_points(&["app.py"]);

        let diagnostics =
            check_deadcode(temp.path(), &config, None, Some("error".to_string())).unwrap();
        assert!(diagnostics.iter().any(|d| d.is_error() && d.is_deadcode()));

        let diagnostics =
            check_deadcode(temp.path(), &config, None, Some("off".to_string())).unwrap();
        assert!(diagnostics.is_empty());

        let result = check_deadcode(temp.path(), &config, None, Some("loud".to_string()));
        assert!(matches!(result, Err(CheckError::Configuration(_))));
    }

    #[test]
    fn cli_entry_points_extend_config_entry_points() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[("app.py", "x = 1\n"), ("extra.py", "y = 2\n"), ("dead.py", "z = 3\n")],
        );
        let config = config_with_entry_points(&["app.py"]);

        let diagnostics = check_deadcode(
            temp.path(),
            &config,
            Some(vec!["extra.py".to_string()]),
            None,
        )
        .unwrap();

        assert_eq!(unreachable_modules(&diagnostics), modules(&["dead"]));
    }

    #[test]
    fn multiple_source_roots_reach_across_roots() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("apps/app.py", "from shared.util import f\nf()\n"),
                ("libs/shared/__init__.py", ""),
                ("libs/shared/util.py", "def f(): ...\n"),
                ("libs/orphan.py", "x = 1\n"),
            ],
        );
        let mut config = config_with_entry_points(&["apps/app.py"]);
        config.source_roots = vec![PathBuf::from("apps"), PathBuf::from("libs")];

        let diagnostics = run(temp.path(), &config);

        assert_eq!(unreachable_modules(&diagnostics), modules(&["orphan"]));
    }

    #[test]
    fn excluded_paths_are_not_analyzed() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[("app.py", "x = 1\n"), ("vendored/lib.py", "y = 2\n")],
        );
        let mut config = config_with_entry_points(&["app.py"]);
        config.exclude = vec!["vendored".to_string()];

        let diagnostics = run(temp.path(), &config);

        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn diagnostics_carry_project_relative_paths() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[("app.py", "x = 1\n"), ("pkg/__init__.py", ""), ("pkg/dead.py", "y = 2\n")],
        );
        let config = config_with_entry_points(&["app.py"]);

        let diagnostics = run(temp.path(), &config);

        let paths: Vec<PathBuf> = diagnostics
            .iter()
            .filter_map(|d| d.file_path().cloned())
            .collect();
        assert_eq!(paths, vec![PathBuf::from("pkg/dead.py")]);
    }
}
