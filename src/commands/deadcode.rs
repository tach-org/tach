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

    let graph = parse_import_edges(&files, &source_roots);
    check_interrupt().map_err(|_| CheckError::Interrupt)?;
    let unparsable_files = graph.unparsable;

    let mut entry_points = project_config.deadcode.entry_points.clone();
    entry_points.extend(cli_entry_points.unwrap_or_default());
    let (entry_files, unmatched_entry_points) = resolve_entry_points(
        &project_root,
        &source_roots,
        &file_walker,
        &files,
        &entry_points,
    )?;

    let mut diagnostics: Vec<Diagnostic> = unmatched_entry_points
        .into_iter()
        .map(|entry_point| {
            Diagnostic::new_global_warning(DiagnosticDetails::Configuration(
                ConfigurationDiagnostic::DeadCodeEntryPointNotFound { entry_point },
            ))
        })
        .collect();

    for (file, failure) in &unparsable_files {
        let file_path = relative_display_path(&project_root, file);
        let (failure_severity, details) = match failure {
            ParseFailure::Syntax => (
                Severity::Error,
                ConfigurationDiagnostic::SkippedFileSyntaxError { file_path },
            ),
            ParseFailure::Io => (
                Severity::Warning,
                ConfigurationDiagnostic::SkippedFileIoError { file_path },
            ),
        };
        diagnostics.push(Diagnostic::new_global(
            failure_severity,
            DiagnosticDetails::Configuration(details),
        ));
    }

    for file in &graph.unanalyzed_targets {
        diagnostics.push(Diagnostic::new_global_warning(
            DiagnosticDetails::Configuration(
                ConfigurationDiagnostic::DeadCodeUnanalyzedImportTarget {
                    file_path: relative_display_path(&project_root, file),
                },
            ),
        ));
    }

    if entry_files.is_empty() {
        diagnostics.push(Diagnostic::new_global_warning(
            DiagnosticDetails::Configuration(ConfigurationDiagnostic::DeadCodeNoEntryPoints()),
        ));
        return Ok(diagnostics);
    }

    let entry_roots = expand_entry_roots(&files, &source_roots, &entry_files);
    let reachable = reachable_files(&graph.edges, &entry_roots);

    if unparsable_files.keys().any(|file| reachable.contains(file)) {
        // Imports of a reachable file are unknown, so reachability cannot be
        // trusted; report nothing rather than false positives.
        diagnostics.push(Diagnostic::new_global_warning(
            DiagnosticDetails::Configuration(
                ConfigurationDiagnostic::DeadCodeSkippedUnparsableFiles(),
            ),
        ));
        return Ok(diagnostics);
    }

    for (file, module_path) in files.iter() {
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

    // Diagnostics are already in a stable order: global diagnostics are pushed
    // first, then located ones in `files` (BTreeMap) path order.
    Ok(diagnostics)
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
        builder.add(validate_glob(pattern, "ignore")?);
    }
    builder
        .build()
        .map_err(|_| CheckError::Configuration("Failed to build '[deadcode]' ignore.".to_string()))
}

/// Compile a user-supplied glob, reporting a configuration error rather than
/// panicking (`FSWalker::walk_globbed_files` unwraps invalid patterns).
fn validate_glob(pattern: &str, setting: &str) -> Result<Glob, CheckError> {
    Glob::new(pattern).map_err(|err| {
        CheckError::Configuration(format!(
            "Invalid glob pattern '{pattern}' in '[deadcode]' {setting}: {err}"
        ))
    })
}

/// Why a file could not be contributed to the import graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseFailure {
    Syntax,
    Io,
}

/// The analyzed Python files, keyed by absolute path, with a case-insensitive
/// index used to resolve imports on case-insensitive filesystems.
#[derive(Debug, Default)]
struct ProjectFiles {
    modules: BTreeMap<PathBuf, String>,
    by_lowercase: BTreeMap<String, PathBuf>,
}

impl ProjectFiles {
    fn insert(&mut self, file_path: PathBuf, module_path: String) {
        self.by_lowercase
            .entry(file_path.to_string_lossy().to_lowercase())
            .or_insert_with(|| file_path.clone());
        self.modules.insert(file_path, module_path);
    }

    fn contains(&self, file_path: &Path) -> bool {
        self.modules.contains_key(file_path)
    }

    /// The analyzed file for `file_path`, tolerating a case mismatch. On macOS
    /// and Windows an import can resolve to a differently-cased path than the
    /// one the walker reported; at runtime those are the same file.
    fn get(&self, file_path: &Path) -> Option<&PathBuf> {
        if let Some((key, _)) = self.modules.get_key_value(file_path) {
            return Some(key);
        }
        self.by_lowercase
            .get(&file_path.to_string_lossy().to_lowercase())
    }

    fn iter(&self) -> impl Iterator<Item = (&PathBuf, &String)> {
        self.modules.iter()
    }
}

/// All Python files under the source roots, mapped to their module paths.
fn collect_project_files(source_roots: &[PathBuf], file_walker: &fs::FSWalker) -> ProjectFiles {
    let mut files = ProjectFiles::default();
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
/// Returns the import edges and the files which could not be parsed.
fn parse_import_edges(files: &ProjectFiles, source_roots: &[PathBuf]) -> ImportGraph {
    let results: Vec<(PathBuf, Result<ImportTargets, ParseFailure>)> = files
        .modules
        .par_iter()
        .map(|(file, _)| {
            if check_interrupt().is_err() {
                return (file.clone(), Err(ParseFailure::Io));
            }
            (file.clone(), file_import_targets(files, source_roots, file))
        })
        .collect();

    let mut graph = ImportGraph::default();
    for (file, result) in results {
        match result {
            Ok(targets) => {
                graph.edges.insert(file, targets.files);
                graph.unanalyzed_targets.extend(targets.unanalyzed);
            }
            Err(failure) => {
                graph.unparsable.insert(file, failure);
            }
        }
    }
    graph
}

#[derive(Debug, Default)]
struct ImportGraph {
    edges: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    unparsable: BTreeMap<PathBuf, ParseFailure>,
    /// Real files under a source root which imports point at, but which were
    /// not analyzed because they are excluded or gitignored.
    unanalyzed_targets: BTreeSet<PathBuf>,
}

fn file_import_targets(
    files: &ProjectFiles,
    source_roots: &[PathBuf],
    file: &Path,
) -> Result<ImportTargets, ParseFailure> {
    let contents = fs::read_file_content(file).map_err(|_| ParseFailure::Io)?;
    let ast = parse_python_source(&contents).map_err(|_| ParseFailure::Syntax)?;
    let imports = get_normalized_imports_from_ast(
        source_roots,
        file,
        &ast,
        // Never ignore `if TYPE_CHECKING:` imports here (regardless of the
        // project-wide setting): a module imported only for type annotations
        // is not dead code.
        false,
        // Follow string literals which look like dotted module paths: errs
        // toward keeping dynamically-imported files alive.
        true,
    )
    .map_err(|_| ParseFailure::Syntax)?;

    let mut targets = ImportTargets::default();
    for import in imports {
        let Some(resolved) = fs::module_to_file_path(source_roots, &import.module_path, true)
        else {
            continue;
        };
        match project_file_for(files, &resolved.file_path) {
            Some(target) => {
                targets.files.insert(target);
            }
            None if resolved
                .file_path
                .extension()
                .is_some_and(|ext| ext == "py") =>
            {
                // The module resolved to a real file under a source root which
                // was not analyzed (excluded, or hidden by gitignore). Its own
                // imports are unknown, so record it for reporting.
                targets.unanalyzed.insert(resolved.file_path.clone());
            }
            None => {}
        }
        // Importing `a.b.c` also executes `a/__init__.py` and `a/b/__init__.py`.
        targets.files.extend(ancestor_init_files(
            files,
            source_roots,
            &import.module_path,
        ));
    }
    Ok(targets)
}

/// Import targets of a single file: those inside the analyzed set, and those
/// which resolved to real but unanalyzed (excluded/gitignored) files.
#[derive(Debug, Default)]
struct ImportTargets {
    files: BTreeSet<PathBuf>,
    unanalyzed: BTreeSet<PathBuf>,
}

/// Map a resolved import target to the project file that represents it in the
/// graph. Stub files (`.pyi`) resolve to their sibling implementation, since
/// only `.py` files participate in the analysis.
fn project_file_for(files: &ProjectFiles, resolved: &Path) -> Option<PathBuf> {
    if let Some(file) = files.get(resolved) {
        return Some(file.clone());
    }
    if resolved.extension().is_some_and(|ext| ext == "pyi") {
        if let Some(file) = files.get(&resolved.with_extension("py")) {
            return Some(file.clone());
        }
    }
    None
}

/// The `__init__.py` files of every ancestor package of `module_path`.
fn ancestor_init_files(
    files: &ProjectFiles,
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
    files: &ProjectFiles,
    raw_entry_points: &[String],
) -> Result<(BTreeSet<PathBuf>, Vec<String>), CheckError> {
    let mut entry_files = BTreeSet::new();
    let mut unmatched = Vec::new();

    // Paths and globs are resolved against the project root and every source
    // root, so that both 'src/app.py' and 'app.py' work under source_roots=["src"].
    let candidate_roots: Vec<&Path> = std::iter::once(project_root)
        .chain(source_roots.iter().map(PathBuf::as_path))
        .collect();

    for raw_entry_point in raw_entry_points {
        let spec = match raw_entry_point.split(':').next() {
            Some(spec) => spec.trim(),
            None => "",
        };
        if spec.is_empty() {
            unmatched.push(raw_entry_point.clone());
            continue;
        }

        let mut matched = BTreeSet::new();
        if has_glob_syntax(spec) {
            validate_glob(spec, "entry_points")?;
            for root in &candidate_roots {
                for path in file_walker
                    .walk_globbed_files(&root.display().to_string(), std::iter::once(spec))
                {
                    matched.extend(project_file_for(files, &path));
                }
            }
        } else {
            for root in &candidate_roots {
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

    Ok((entry_files, unmatched))
}

/// Entry files plus the `__init__.py` of each of their ancestor packages
/// (running `python -m pkg.app` imports `pkg` first).
fn expand_entry_roots(
    files: &ProjectFiles,
    source_roots: &[PathBuf],
    entry_files: &BTreeSet<PathBuf>,
) -> BTreeSet<PathBuf> {
    let mut roots = entry_files.clone();
    for entry_file in entry_files {
        if let Some(module_path) = files.modules.get(entry_file) {
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
                (
                    "pkg/used.py",
                    "from pkg.helper import help\ndef run(): ...\n",
                ),
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
        write_files(
            temp.path(),
            &[("app.py", "x = 1\n"), ("other.py", "y = 2\n")],
        );
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
    fn invalid_entry_point_glob_is_a_configuration_error_not_a_panic() {
        // FSWalker::walk_globbed_files unwraps invalid patterns, so entry-point
        // globs must be validated before they reach it.
        let temp = TempDir::new().unwrap();
        write_files(temp.path(), &[("app.py", "x = 1\n")]);
        let config = config_with_entry_points(&["bad["]);

        let result = check_deadcode(temp.path(), &config, None, None);

        assert!(matches!(result, Err(CheckError::Configuration(_))));
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
            DiagnosticDetails::Configuration(
                ConfigurationDiagnostic::SkippedFileSyntaxError { .. }
            )
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
            DiagnosticDetails::Configuration(
                ConfigurationDiagnostic::SkippedFileSyntaxError { .. }
            )
        )));
    }

    #[test]
    fn severity_off_short_circuits() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[("app.py", "x = 1\n"), ("dead.py", "y = 2\n")],
        );
        let mut config = config_with_entry_points(&["app.py"]);
        config.deadcode.severity = RuleSetting::Off;

        let diagnostics = run(temp.path(), &config);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn cli_severity_overrides_config() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[("app.py", "x = 1\n"), ("dead.py", "y = 2\n")],
        );
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
            &[
                ("app.py", "x = 1\n"),
                ("extra.py", "y = 2\n"),
                ("dead.py", "z = 3\n"),
            ],
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
            &[
                ("app.py", "x = 1\n"),
                ("vendored/lib.py", "y = 2\n"),
                // Positive control: proves the analysis actually ran.
                ("still_dead.py", "z = 3\n"),
            ],
        );
        let mut config = config_with_entry_points(&["app.py"]);
        config.exclude = vec!["vendored".to_string()];

        let diagnostics = run(temp.path(), &config);

        assert_eq!(unreachable_modules(&diagnostics), modules(&["still_dead"]));
    }

    #[test]
    fn import_through_an_excluded_file_warns_about_the_unanalyzed_target() {
        // An excluded (or gitignored) file in the middle of a live import chain
        // severs reachability. That is inherent to excluding it, but the user
        // is told rather than silently handed a false deletion candidate.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "import generated\n"),
                ("generated.py", "import helper\n"),
                ("helper.py", "def h(): ...\n"),
            ],
        );
        let mut config = config_with_entry_points(&["main.py"]);
        config.exclude = vec!["generated.py".to_string()];

        let diagnostics = run(temp.path(), &config);

        assert_eq!(unreachable_modules(&diagnostics), modules(&["helper"]));
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.details(),
            DiagnosticDetails::Configuration(
                ConfigurationDiagnostic::DeadCodeUnanalyzedImportTarget { file_path }
            ) if file_path == "generated.py"
        )));
    }

    #[test]
    fn duplicate_module_in_two_roots_resolves_to_the_first_configured_root() {
        // Deterministic, and matching Python's sys.path semantics: the first
        // configured source root wins, so the shadowed copy is the dead one.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("appdir/main.py", "import util\n"),
                ("appdir/util.py", "A = 1\n"),
                ("libdir/util.py", "B = 2\n"),
            ],
        );
        let mut config = config_with_entry_points(&["appdir/main.py"]);
        config.source_roots = vec![PathBuf::from("appdir"), PathBuf::from("libdir")];

        for _ in 0..5 {
            let diagnostics = run(temp.path(), &config);
            let paths: Vec<PathBuf> = diagnostics
                .iter()
                .filter(|d| d.is_deadcode())
                .filter_map(|d| d.file_path().cloned())
                .collect();
            assert_eq!(paths, vec![PathBuf::from("libdir/util.py")]);
        }
    }

    #[test]
    fn case_mismatched_import_resolves_on_case_insensitive_filesystems() {
        // On macOS/Windows `module_to_file_path` can resolve to a differently
        // cased path than the walker reported (both name the same file), which
        // must not sever reachability and must not be mistaken for an
        // unanalyzed target. On a case-sensitive filesystem the import simply
        // does not resolve, and the file is reported dead instead.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "from pkg.Toolset import Tool\nTool()\n"),
                ("pkg/__init__.py", ""),
                ("pkg/toolset.py", "class Tool: ...\n"),
                ("pkg/dead.py", "x = 1\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        let unreachable = unreachable_modules(&diagnostics);
        assert!(unreachable.contains("pkg.dead"));
        // Never a spurious "unanalyzed import target" warning for a case twin.
        assert!(!diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.details(),
            DiagnosticDetails::Configuration(
                ConfigurationDiagnostic::DeadCodeUnanalyzedImportTarget { .. }
            )
        )));
        if cfg!(any(target_os = "macos", target_os = "windows")) {
            assert!(!unreachable.contains("pkg.toolset"));
        }
    }

    #[test]
    fn namespace_packages_without_init_are_traversed() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "from nsa.nsb.mod import f\nf()\n"),
                ("nsa/nsb/mod.py", "def f(): ...\n"),
                ("nsa/nsb/dead.py", "x = 1\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["nsa.nsb.dead"])
        );
    }

    #[test]
    fn source_root_that_is_itself_a_package_is_handled() {
        // A root-level __init__.py has module path "."; it must not be reported
        // and must not break ancestor resolution.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("__init__.py", "import sibling\n"),
                ("main.py", "from sibling import f\nf()\n"),
                ("sibling.py", "def f(): ...\n"),
                ("dead.py", "q = 1\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert_eq!(unreachable_modules(&diagnostics), modules(&["dead"]));
    }

    #[test]
    fn dunder_main_is_reachable_only_when_declared_as_an_entry_point() {
        // `python -m pkg` runs pkg/__main__.py, which nothing imports. It is a
        // convention the import graph cannot see, so it must be declared.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("pkg/__init__.py", ""),
                ("pkg/__main__.py", "from pkg.cli import main\nmain()\n"),
                ("pkg/cli.py", "def main(): ...\n"),
            ],
        );

        let config = config_with_entry_points(&["pkg/__init__.py"]);
        let diagnostics = run(temp.path(), &config);
        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["pkg.__main__", "pkg.cli"])
        );

        let config = config_with_entry_points(&["pkg/__main__.py"]);
        let diagnostics = run(temp.path(), &config);
        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn platform_conditional_imports_keep_every_branch_alive() {
        // Both branches of a platform guard are followed: the module not used
        // on this machine is still needed on the other platform.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                (
                    "main.py",
                    "import os\n\nif os.name == \"nt\":\n    from impl_win32 import read\nelse:\n    from impl_posix import read\n",
                ),
                ("impl_win32.py", "def read(): ...\n"),
                ("impl_posix.py", "def read(): ...\n"),
                ("dead.py", "x = 1\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert_eq!(unreachable_modules(&diagnostics), modules(&["dead"]));
    }

    #[test]
    fn optional_dependency_fallback_imports_are_followed() {
        // try/except ImportError accelerator fallbacks: both the fast path and
        // the pure-Python fallback are alive.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                (
                    "main.py",
                    "try:\n    from fast_impl import run\nexcept ImportError:\n    from slow_impl import run\n",
                ),
                ("fast_impl.py", "def run(): ...\n"),
                ("slow_impl.py", "def run(): ...\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn side_effect_only_import_keeps_the_registering_module_alive() {
        // `from . import handlers` purely for @register side effects has no
        // symbol usage, but the file-level edge is real.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "import app\n"),
                ("app/__init__.py", "from app import handlers\n"),
                (
                    "app/handlers.py",
                    "from app.registry import register\n\n@register\ndef pay(): ...\n",
                ),
                ("app/registry.py", "def register(fn): return fn\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn pkgutil_plugin_discovery_is_flagged_and_glob_entry_point_is_the_remedy() {
        // A package that imports its own children via pkgutil.iter_modules +
        // import_module(f"...") leaves no static edge to the plugins.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "from plugins import load_all\nload_all()\n"),
                (
                    "plugins/__init__.py",
                    "import importlib\nimport pkgutil\n\ndef load_all():\n    for _, name, _ in pkgutil.iter_modules(__path__):\n        importlib.import_module(f\"{__name__}.{name}\")\n",
                ),
                ("plugins/csv_export.py", "def run(): ...\n"),
                ("plugins/json_export.py", "def run(): ...\n"),
            ],
        );

        let config = config_with_entry_points(&["main.py"]);
        let diagnostics = run(temp.path(), &config);
        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["plugins.csv_export", "plugins.json_export"])
        );

        let config = config_with_entry_points(&["main.py", "plugins/*.py"]);
        let diagnostics = run(temp.path(), &config);
        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn dead_island_cluster_is_reported_despite_mutual_imports() {
        // Every file in the cluster has an inbound import edge, so a naive
        // "is anything importing this?" check would call all of them alive.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "from pkg.active import go\ngo()\n"),
                ("pkg/__init__.py", ""),
                ("pkg/active.py", "def go(): ...\n"),
                ("pkg/old_flow.py", "from pkg.old_helpers import fmt\n"),
                (
                    "pkg/old_helpers.py",
                    "from pkg.old_flow import Step\n\ndef fmt(): ...\n",
                ),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["pkg.old_flow", "pkg.old_helpers"])
        );
    }

    #[test]
    fn importer_inside_an_excluded_tree_does_not_keep_its_target_alive() {
        // A migration runner (excluded from analysis, as migrations usually are)
        // is the only importer of a live helper. The helper is reported: the
        // importing file is outside the analyzed set, so the edge is invisible.
        // The remedy is to declare the migration tree as an entry point.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "print(\"serve\")\n"),
                ("app/__init__.py", ""),
                ("app/ddl_gen.py", "def view_sql(): return \"CREATE VIEW\"\n"),
                (
                    "migrations/001_add_view.py",
                    "from app.ddl_gen import view_sql\n\ndef upgrade(): view_sql()\n",
                ),
            ],
        );
        let mut config = config_with_entry_points(&["main.py"]);
        config.exclude = vec!["migrations".to_string()];

        let diagnostics = run(temp.path(), &config);
        assert_eq!(unreachable_modules(&diagnostics), modules(&["app.ddl_gen"]));

        // Declaring the excluded runner as an entry point cannot help while it
        // is excluded from the walk; un-excluding it and seeding it does.
        let mut config = config_with_entry_points(&["main.py", "migrations/*.py"]);
        config.exclude = vec![];
        let diagnostics = run(temp.path(), &config);
        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn module_path_inside_a_non_python_config_file_is_not_followed() {
        // Plugin wiring that lives in YAML/TOML consumed by an external tool is
        // outside the import graph entirely.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                (
                    "main.py",
                    "import subprocess\n\nsubprocess.run([\"external-tool\", \"--conf\", \"conf/profiles.yml\"])\n",
                ),
                ("plugins/__init__.py", ""),
                ("plugins/fancy_writer.py", "class Plugin: ...\n"),
                (
                    "conf/profiles.yml",
                    "plugins:\n  - module: plugins.fancy_writer\n",
                ),
            ],
        );

        let config = config_with_entry_points(&["main.py"]);
        let diagnostics = run(temp.path(), &config);
        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["plugins.fancy_writer"])
        );

        let mut config = config_with_entry_points(&["main.py"]);
        config.deadcode.ignore = vec!["plugins/**".to_string()];
        let diagnostics = run(temp.path(), &config);
        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn statically_registered_versions_are_all_alive() {
        // The healthy variant of v1/v2 parallelism: a factory imports every
        // version explicitly and picks at runtime. Older versions look
        // superseded but are alive, and must never be reported.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                (
                    "main.py",
                    "from domain.builder import build\nbuild(\"2025-02-01\")\n",
                ),
                ("domain/__init__.py", ""),
                ("domain/versions/__init__.py", ""),
                (
                    "domain/versions/v1.py",
                    "class QV1:\n    min_version = \"2025-01-01\"\n",
                ),
                (
                    "domain/versions/v2.py",
                    "class QV2:\n    min_version = \"2025-06-01\"\n",
                ),
                (
                    "domain/builder.py",
                    "from domain.versions.v1 import QV1\nfrom domain.versions.v2 import QV2\n\nFACTORY = [QV2, QV1]\n\ndef build(v):\n    return next(c for c in FACTORY if v >= c.min_version)()\n",
                ),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn console_script_module_must_be_declared_as_an_entry_point() {
        // pyproject.toml [project.scripts] targets have no importer; tach does
        // not read pyproject entry points, so they are declared explicitly.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("myapp/__init__.py", ""),
                (
                    "myapp/cli.py",
                    "from myapp.core import work\n\ndef main(): work()\n",
                ),
                ("myapp/core.py", "def work(): ...\n"),
                ("myapp/unused.py", "def leftover(): ...\n"),
            ],
        );
        let config = config_with_entry_points(&["myapp.cli:main"]);

        let diagnostics = run(temp.path(), &config);

        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["myapp.unused"])
        );
    }

    // The tests below encode deceptive liveness patterns observed in real
    // codebases: code that looks dead but is alive (dynamic loading, external
    // runners) and code that looks alive but is dead (transitive death,
    // test-only imports). They pin both the detection behavior and the
    // documented remedy (entry_points / ignore).

    #[test]
    fn dynamic_file_loading_flags_targets_and_entry_point_glob_heals_transitively() {
        // Pattern: a live loader uses importlib.util.spec_from_file_location
        // with a runtime-built path to load per-client scripts; the scripts
        // (and everything only they import) look dead to the import graph.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                (
                    "main.py",
                    "import importlib.util\n\ndef load(client):\n    spec = importlib.util.spec_from_file_location(client, f\"clients/{client}/script.py\")\n    return spec\n",
                ),
                ("clients/__init__.py", ""),
                ("clients/acme/__init__.py", ""),
                (
                    "clients/acme/script.py",
                    "from shared import helper\nhelper()\n",
                ),
                ("shared.py", "def helper(): ...\n"),
                ("truly_dead.py", "x = 1\n"),
            ],
        );

        // Without configuration, the dynamically-loaded subgraph is flagged.
        let config = config_with_entry_points(&["main.py"]);
        let diagnostics = run(temp.path(), &config);
        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["clients.acme.script", "shared", "truly_dead"])
        );

        // Declaring the dynamic targets as entry points revives them AND their
        // transitive imports — the documented remedy.
        let config = config_with_entry_points(&["main.py", "clients/*/script.py"]);
        let diagnostics = run(temp.path(), &config);
        assert_eq!(unreachable_modules(&diagnostics), modules(&["truly_dead"]));
    }

    #[test]
    fn two_dot_string_reference_keeps_target_alive_transitively() {
        // String literals with >= 2 dots are treated as imports (conservative);
        // shorter strings are not.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                (
                    "main.py",
                    "PLUGIN = \"pkg.plugins.mailer\"\nSHORT = \"pkg.orphan\"\n",
                ),
                ("pkg/__init__.py", ""),
                ("pkg/plugins/__init__.py", ""),
                (
                    "pkg/plugins/mailer.py",
                    "from pkg.plugins.transport import send\n",
                ),
                ("pkg/plugins/transport.py", "def send(): ...\n"),
                ("pkg/orphan.py", "x = 1\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        // The 2-dot reference keeps mailer alive, and mailer's own imports are
        // then followed; the 1-dot reference does not count.
        assert_eq!(unreachable_modules(&diagnostics), modules(&["pkg.orphan"]));
    }

    #[test]
    fn runtime_built_module_names_are_flagged_and_ignorable() {
        // f-strings and concatenated module names cannot be followed; the
        // targets are flagged, and the ignore glob is the remedy when the
        // files should stay.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                (
                    "main.py",
                    "import importlib\n\ndef load(name):\n    return importlib.import_module(f\"plugins.{name}\")\n",
                ),
                ("plugins/__init__.py", ""),
                ("plugins/emailer.py", "def notify(): ...\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);
        let diagnostics = run(temp.path(), &config);
        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["plugins.emailer"])
        );

        let mut config = config_with_entry_points(&["main.py"]);
        config.deadcode.ignore = vec!["plugins/**".to_string()];
        let diagnostics = run(temp.path(), &config);
        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn test_only_imports_do_not_keep_code_alive() {
        // Pattern: a module's only importer is an excluded test file. The
        // import exists, so the module looks alive to a grep — but it has no
        // production path and is correctly reported.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "from pkg.used import run\nrun()\n"),
                ("pkg/__init__.py", ""),
                ("pkg/used.py", "def run(): ...\n"),
                ("pkg/only_tested.py", "def helper(): ...\n"),
                (
                    "tests/test_helper.py",
                    "from pkg.only_tested import helper\n\ndef test_helper(): helper()\n",
                ),
            ],
        );
        let mut config = config_with_entry_points(&["main.py"]);
        config.exclude = vec!["tests".to_string()];

        let diagnostics = run(temp.path(), &config);

        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["pkg.only_tested"])
        );
    }

    #[test]
    fn transitive_death_chain_is_fully_reported() {
        // Pattern: b.py and c.py both have importers, so they look alive to a
        // grep — but their only importers are themselves dead.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "x = 1\n"),
                ("dead_root.py", "import dead_mid\n"),
                ("dead_mid.py", "import dead_leaf\n"),
                ("dead_leaf.py", "def f(): ...\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);

        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["dead_root", "dead_mid", "dead_leaf"])
        );
    }

    #[test]
    fn wide_init_facade_keeps_siblings_alive_at_file_level() {
        // Boundary of file-level analysis, pinned from both sides: a sibling
        // the facade does NOT re-export is reported, and adding the re-export
        // silences the report for exactly the same dead code. Distinguishing
        // those requires symbol-level analysis (out of scope).
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("main.py", "from pkg import UsedThing\nUsedThing()\n"),
                ("pkg/__init__.py", "from pkg.used import UsedThing\n"),
                ("pkg/used.py", "class UsedThing: ...\n"),
                // Consumed by nobody, and not re-exported by the facade.
                ("pkg/legacy.py", "PALETTE = {}\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);

        let diagnostics = run(temp.path(), &config);
        assert_eq!(unreachable_modules(&diagnostics), modules(&["pkg.legacy"]));

        // Same dead code, now re-exported: the file-level graph can no longer
        // see that nothing consumes it.
        write_files(
            temp.path(),
            &[(
                "pkg/__init__.py",
                "from pkg.used import UsedThing\nfrom pkg.legacy import PALETTE\n",
            )],
        );

        let diagnostics = run(temp.path(), &config);
        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn packaging_metadata_plugin_needs_an_entry_point_and_takes_its_subtree_with_it() {
        // A module loaded through installed-package entry-point metadata (a
        // pytest11 plugin, for example) has no importer at all, and everything
        // only it imports goes down with it.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("app/__init__.py", ""),
                ("app/main.py", "print(\"app\")\n"),
                ("testkit/__init__.py", ""),
                (
                    "testkit/fixtures.py",
                    "from testkit.db import make_db\n\ndef db(): return make_db()\n",
                ),
                ("testkit/db.py", "def make_db(): ...\n"),
            ],
        );

        let config = config_with_entry_points(&["app/main.py"]);
        let diagnostics = run(temp.path(), &config);
        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["testkit.fixtures", "testkit.db"])
        );

        let config = config_with_entry_points(&["app/main.py", "testkit.fixtures"]);
        let diagnostics = run(temp.path(), &config);
        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn subprocess_script_references_are_not_followed() {
        // A path string like "jobs/nightly.py" is not a module path, so a
        // subprocess invocation does not create an edge; the remedy is to
        // declare the script as an entry point.
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                (
                    "main.py",
                    "import subprocess\n\ndef cron():\n    subprocess.run([\"python\", \"jobs/nightly.py\"])\n",
                ),
                ("jobs/__init__.py", ""),
                ("jobs/nightly.py", "from jobs.shared import work\nwork()\n"),
                ("jobs/shared.py", "def work(): ...\n"),
            ],
        );
        let config = config_with_entry_points(&["main.py"]);
        let diagnostics = run(temp.path(), &config);
        assert_eq!(
            unreachable_modules(&diagnostics),
            modules(&["jobs.nightly", "jobs.shared"])
        );

        let config = config_with_entry_points(&["main.py", "jobs/nightly.py"]);
        let diagnostics = run(temp.path(), &config);
        assert!(unreachable_modules(&diagnostics).is_empty());
    }

    #[test]
    fn diagnostics_carry_project_relative_paths() {
        let temp = TempDir::new().unwrap();
        write_files(
            temp.path(),
            &[
                ("app.py", "x = 1\n"),
                ("pkg/__init__.py", ""),
                ("pkg/dead.py", "y = 2\n"),
            ],
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
