use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::{
    config::{
        DeadcodeDetection, ProjectConfig, project::DEFAULT_EXCLUDE_PATHS,
        root_module::ROOT_MODULE_SENTINEL_TAG,
    },
    deadcode::{
        DeadSymbolAnalysisInput, FileImportGraph, find_dead_symbols, resolve_entry_points,
        types::DeadcodeFile, types::is_init_file,
    },
    diagnostics::{CodeDiagnostic, ConfigurationDiagnostic, Diagnostic, DiagnosticDetails},
    filesystem as fs,
    processors::import::get_normalized_imports_from_ast,
    python::{error::ParsingError, parsing::parse_python_source},
    resolvers::SourceRootResolver,
};

use super::check::CheckError;

pub fn check_deadcode(
    project_root: &Path,
    project_config: &ProjectConfig,
    entry_points: Option<Vec<String>>,
    files: bool,
    symbols: bool,
) -> Result<Vec<Diagnostic>, CheckError> {
    if !project_root.is_dir() {
        return Err(CheckError::InvalidDirectory(
            project_root.display().to_string(),
        ));
    }

    let (detect_files, detect_symbols) = if files || symbols {
        (files, symbols)
    } else {
        (
            project_config
                .deadcode
                .detect
                .contains(&DeadcodeDetection::Files),
            project_config
                .deadcode
                .detect
                .contains(&DeadcodeDetection::Symbols),
        )
    };

    if !detect_files && !detect_symbols {
        return Ok(vec![]);
    }

    let project_root = project_root.to_path_buf();
    let exclude_paths = deadcode_exclude_paths(project_config);
    let file_walker = fs::FSWalker::try_new(
        &project_root,
        &exclude_paths,
        project_config.respect_gitignore,
    )?;
    let source_root_resolver = SourceRootResolver::new(&project_root, &file_walker);
    let source_roots = source_root_resolver.resolve(&project_config.source_roots)?;

    let mut effective_entry_points = project_config.deadcode.entry_points.clone();
    if let Some(cli_entry_points) = entry_points {
        effective_entry_points.extend(cli_entry_points);
    }

    let resolved_entry_points = resolve_entry_points(
        &project_root,
        &source_roots,
        &file_walker,
        &effective_entry_points,
    );

    let mut diagnostics: Vec<Diagnostic> = resolved_entry_points
        .unresolved
        .iter()
        .map(|entry_point| deadcode_entry_point_not_found_warning(entry_point))
        .collect();

    let project_files = collect_project_files(&source_roots, &file_walker);

    let mut graph = FileImportGraph::new();
    for file in &project_files {
        graph.add_file(file.file_path.clone(), file.module_path.clone());
    }

    let mut parse_failures: BTreeSet<PathBuf> = BTreeSet::new();

    for file in &project_files {
        let content = match fs::read_file_content(&file.file_path) {
            Ok(content) => content,
            Err(_) => {
                push_skipped_file_io_warning(
                    &mut diagnostics,
                    &mut parse_failures,
                    &file.file_path,
                );
                continue;
            }
        };

        let python_source = match parse_python_source(&content) {
            Ok(source) => source,
            Err(ParsingError::PythonParse(_) | ParsingError::InvalidSyntax) => {
                push_skipped_file_syntax_error(
                    &mut diagnostics,
                    &mut parse_failures,
                    &file.file_path,
                );
                continue;
            }
            Err(ParsingError::Io(_) | ParsingError::Filesystem(_)) => {
                push_skipped_file_io_warning(
                    &mut diagnostics,
                    &mut parse_failures,
                    &file.file_path,
                );
                continue;
            }
        };

        let imports = match get_normalized_imports_from_ast(
            &source_roots,
            &file.file_path,
            &python_source,
            project_config.ignore_type_checking_imports,
            project_config.include_string_imports,
        ) {
            Ok(imports) => imports,
            Err(_) => {
                push_skipped_file_syntax_error(
                    &mut diagnostics,
                    &mut parse_failures,
                    &file.file_path,
                );
                continue;
            }
        };

        for import in imports {
            if let Some(resolved_import) =
                fs::module_to_file_path(&source_roots, &import.module_path, true)
            {
                if resolved_import
                    .file_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    != Some("py")
                {
                    continue;
                }
                add_import_edges(
                    &mut graph,
                    &source_roots,
                    &file.file_path,
                    &import.module_path,
                    &resolved_import.file_path,
                );
            }
        }
    }

    add_extra_dependency_edges(
        &mut graph,
        &project_root,
        &file_walker,
        &project_config.map.extra_dependencies,
    );

    let entry_file_roots =
        expand_entry_file_roots(&graph, &source_roots, resolved_entry_points.files);

    if entry_file_roots.is_empty() {
        diagnostics.push(Diagnostic::new_global_warning(
            DiagnosticDetails::Configuration(ConfigurationDiagnostic::DeadcodeNoEntryPoints()),
        ));
        return Ok(sort_diagnostics(diagnostics));
    }

    if detect_files {
        emit_dead_file_diagnostics(
            &mut diagnostics,
            project_config,
            &project_root,
            &source_roots,
            &graph,
            &entry_file_roots,
            &parse_failures,
        );
    }

    if detect_symbols {
        emit_dead_symbol_diagnostics(
            &mut diagnostics,
            DeadSymbolDiagnosticContext {
                project_config,
                project_root: &project_root,
                source_roots: &source_roots,
                file_walker: &file_walker,
                graph: &graph,
                entry_file_roots: &entry_file_roots,
                parse_failures: &parse_failures,
                entry_points: &effective_entry_points,
            },
        );
    }

    Ok(sort_diagnostics(diagnostics))
}

fn deadcode_entry_point_not_found_warning(entry_point: &str) -> Diagnostic {
    Diagnostic::new_global_warning(DiagnosticDetails::Configuration(
        ConfigurationDiagnostic::DeadcodeEntryPointNotFound {
            entry_point: entry_point.to_string(),
        },
    ))
}

fn collect_project_files(
    source_roots: &[PathBuf],
    file_walker: &fs::FSWalker,
) -> Vec<DeadcodeFile> {
    let mut project_files = Vec::new();

    for source_root in source_roots {
        project_files.extend(
            file_walker
                .walk_pyfiles(&source_root.display().to_string())
                .filter_map(|file_path| {
                    let abs_path = source_root.join(&file_path);
                    fs::file_to_module_path(source_roots, &abs_path)
                        .ok()
                        .map(|module_path| DeadcodeFile::new(abs_path, module_path))
                }),
        );
    }

    project_files.sort_by_key(|file| file.file_path.clone());
    project_files.dedup_by_key(|file| file.file_path.clone());
    project_files
}

fn push_skipped_file_syntax_error(
    diagnostics: &mut Vec<Diagnostic>,
    parse_failures: &mut BTreeSet<PathBuf>,
    file_path: &Path,
) {
    parse_failures.insert(file_path.to_path_buf());
    diagnostics.push(Diagnostic::new_global_error(
        DiagnosticDetails::Configuration(ConfigurationDiagnostic::SkippedFileSyntaxError {
            file_path: file_path.display().to_string(),
        }),
    ));
}

fn push_skipped_file_io_warning(
    diagnostics: &mut Vec<Diagnostic>,
    parse_failures: &mut BTreeSet<PathBuf>,
    file_path: &Path,
) {
    parse_failures.insert(file_path.to_path_buf());
    diagnostics.push(Diagnostic::new_global_warning(
        DiagnosticDetails::Configuration(ConfigurationDiagnostic::SkippedFileIoError {
            file_path: file_path.display().to_string(),
        }),
    ));
}

fn deadcode_exclude_paths(project_config: &ProjectConfig) -> Vec<String> {
    let mut exclude_paths = project_config.exclude.clone();

    if project_config.deadcode.include_test_usages && is_default_exclude_set(&exclude_paths) {
        exclude_paths.retain(|path| path != "tests" && path != "**/tests");
    }

    exclude_paths.extend(project_config.deadcode.exclude.clone());
    exclude_paths
}

fn is_default_exclude_set(exclude_paths: &[String]) -> bool {
    let configured = exclude_paths
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let defaults = DEFAULT_EXCLUDE_PATHS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    configured == defaults
}

fn expand_entry_file_roots(
    graph: &FileImportGraph,
    source_roots: &[PathBuf],
    entry_files: Vec<PathBuf>,
) -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();

    for entry_file in entry_files {
        if !graph.has_file(&entry_file) {
            continue;
        }

        roots.insert(entry_file.clone());
        let Some(module_path) = graph.module_path(&entry_file) else {
            continue;
        };

        let module_parts = module_path.split('.').collect::<Vec<_>>();
        for end in 1..module_parts.len() {
            let parent_module = module_parts[..end].join(".");
            let Some(parent_package) = fs::module_to_file_path(source_roots, &parent_module, false)
            else {
                continue;
            };

            if is_init_file(&parent_package.file_path) && graph.has_file(&parent_package.file_path)
            {
                roots.insert(parent_package.file_path);
            }
        }
    }

    roots.into_iter().collect()
}

fn add_import_edges(
    graph: &mut FileImportGraph,
    source_roots: &[PathBuf],
    from: &Path,
    imported_module_path: &str,
    imported_file: &Path,
) {
    if !graph.has_file(imported_file) {
        return;
    }
    graph.add_import(from, imported_file);

    let module_parts = imported_module_path.split('.').collect::<Vec<_>>();
    for end in 1..module_parts.len() {
        let parent_module = module_parts[..end].join(".");
        let Some(parent_package) = fs::module_to_file_path(source_roots, &parent_module, false)
        else {
            continue;
        };

        if is_init_file(&parent_package.file_path) && graph.has_file(&parent_package.file_path) {
            graph.add_import(from, &parent_package.file_path);
        }
    }
}

fn add_extra_dependency_edges(
    graph: &mut FileImportGraph,
    project_root: &Path,
    file_walker: &fs::FSWalker,
    extra_dependencies: &std::collections::HashMap<String, Vec<String>>,
) {
    for (source_pattern, dependency_patterns) in extra_dependencies {
        let source_files = file_walker
            .walk_globbed_files(
                project_root.to_str().unwrap_or("."),
                std::iter::once(source_pattern),
            )
            .collect::<Vec<_>>();

        for source_file in source_files {
            if !graph.has_file(&source_file) {
                continue;
            }

            for dependency_pattern in dependency_patterns {
                for dependency_file in file_walker.walk_globbed_files(
                    project_root.to_str().unwrap_or("."),
                    std::iter::once(dependency_pattern),
                ) {
                    if graph.has_file(&dependency_file) {
                        graph.add_import(&source_file, &dependency_file);
                    }
                }
            }
        }
    }
}

fn emit_dead_file_diagnostics(
    diagnostics: &mut Vec<Diagnostic>,
    project_config: &ProjectConfig,
    project_root: &Path,
    source_roots: &[PathBuf],
    graph: &FileImportGraph,
    entry_file_roots: &[PathBuf],
    parse_failures: &BTreeSet<PathBuf>,
) {
    let Ok(severity) = (&project_config.deadcode.severity).try_into() else {
        return;
    };

    let reachable = graph.reachable_from(entry_file_roots);
    if parse_failures
        .iter()
        .any(|failed_file| reachable.contains(failed_file))
    {
        return;
    }

    let unreachable_files = graph
        .files()
        .filter(|file| !reachable.contains(*file))
        .cloned()
        .collect::<Vec<_>>();

    for file in unreachable_files {
        if parse_failures.contains(&file) {
            continue;
        }
        if project_config.deadcode.protect_init_files && is_init_file(&file) {
            continue;
        }

        let Some(module_path) = graph.module_path(&file) else {
            continue;
        };

        if should_ignore_file(
            &project_config.deadcode.ignore,
            project_root,
            source_roots,
            &file,
            Some(module_path),
        ) {
            continue;
        }
        if is_unchecked_module(project_config, module_path) {
            continue;
        }

        diagnostics.push(Diagnostic::new_located(
            severity,
            DiagnosticDetails::Code(CodeDiagnostic::DeadFile {
                module_path: module_path.to_string(),
            }),
            file,
            1,
            None,
        ));
    }
}

struct DeadSymbolDiagnosticContext<'a> {
    project_config: &'a ProjectConfig,
    project_root: &'a Path,
    source_roots: &'a [PathBuf],
    file_walker: &'a fs::FSWalker,
    graph: &'a FileImportGraph,
    entry_file_roots: &'a [PathBuf],
    parse_failures: &'a BTreeSet<PathBuf>,
    entry_points: &'a [String],
}

fn emit_dead_symbol_diagnostics(
    diagnostics: &mut Vec<Diagnostic>,
    context: DeadSymbolDiagnosticContext<'_>,
) {
    let Ok(severity) = (&context.project_config.deadcode.severity).try_into() else {
        return;
    };

    for symbol in find_dead_symbols(DeadSymbolAnalysisInput {
        project_root: context.project_root,
        project_config: context.project_config,
        source_roots: context.source_roots,
        file_walker: context.file_walker,
        graph: context.graph,
        entry_file_roots: context.entry_file_roots,
        parse_failures: context.parse_failures,
        entry_points: context.entry_points,
    }) {
        if should_ignore_symbol(
            &context.project_config.deadcode.ignore,
            context.project_root,
            context.source_roots,
            &symbol.file_path,
            &symbol.module_path,
            &symbol.symbol_name,
        ) {
            continue;
        }
        if is_unchecked_module(context.project_config, &symbol.module_path) {
            continue;
        }

        diagnostics.push(Diagnostic::new_located(
            severity,
            DiagnosticDetails::Code(CodeDiagnostic::DeadSymbol {
                module_path: symbol.module_path,
                symbol_name: symbol.symbol_name,
                symbol_kind: symbol.symbol_kind.as_str().to_string(),
            }),
            symbol.file_path,
            symbol.line_number,
            None,
        ));
    }
}

fn should_ignore_symbol(
    ignore_rules: &[String],
    project_root: &Path,
    source_roots: &[PathBuf],
    file_path: &Path,
    module_path: &str,
    symbol_name: &str,
) -> bool {
    if should_ignore_file(
        ignore_rules,
        project_root,
        source_roots,
        file_path,
        Some(module_path),
    ) {
        return true;
    }

    for ignore in ignore_rules {
        let Some((rule_module, rule_symbol)) = parse_symbol_ignore_rule(ignore) else {
            continue;
        };
        if rule_module == module_path && rule_symbol == symbol_name {
            return true;
        }
    }

    false
}

fn parse_symbol_ignore_rule(rule: &str) -> Option<(&str, &str)> {
    if let Some((module_path, symbol_name)) = rule.split_once(':') {
        return (!module_path.is_empty() && !symbol_name.is_empty())
            .then_some((module_path, symbol_name));
    }

    rule.rsplit_once('.')
        .and_then(|(module_path, symbol_name)| {
            (!module_path.is_empty() && !symbol_name.is_empty())
                .then_some((module_path, symbol_name))
        })
}

fn should_ignore_file(
    ignore_rules: &[String],
    project_root: &Path,
    source_roots: &[PathBuf],
    file_path: &Path,
    module_path: Option<&str>,
) -> bool {
    let relative_file_path = fs::relative_to(file_path, project_root)
        .ok()
        .map(|path| normalize_path(&path.to_string_lossy()));
    let source_relative_file_paths = source_roots
        .iter()
        .filter_map(|source_root| fs::relative_to(file_path, source_root).ok())
        .map(|path| normalize_path(&path.to_string_lossy()))
        .collect::<Vec<_>>();
    let absolute_file_path = normalize_path(&file_path.to_string_lossy());
    let file_name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();

    for ignore in ignore_rules {
        let has_symbol_separator = ignore.contains(':');
        let rule = ignore.split(':').next().unwrap_or(ignore).trim();
        if rule.is_empty() {
            continue;
        }

        let normalized_rule = normalize_path(rule);

        if file_name == rule {
            return true;
        }

        if has_symbol_separator {
            continue;
        }

        if module_path.is_some_and(|module| module == rule) {
            return true;
        }

        if normalized_rule == file_name
            || relative_file_path
                .as_ref()
                .is_some_and(|path| path == &normalized_rule)
            || source_relative_file_paths
                .iter()
                .any(|path| path == &normalized_rule)
            || absolute_file_path == normalized_rule
        {
            return true;
        }
    }

    false
}

fn is_unchecked_module(project_config: &ProjectConfig, module_path: &str) -> bool {
    project_config
        .all_modules()
        .filter(|module| module_matches(&module.path, module_path))
        .max_by_key(|module| {
            if module.path == ROOT_MODULE_SENTINEL_TAG {
                0
            } else {
                module.path.len()
            }
        })
        .is_some_and(|module| module.unchecked)
}

fn module_matches(configured_module_path: &str, module_path: &str) -> bool {
    if configured_module_path == ROOT_MODULE_SENTINEL_TAG {
        return true;
    }

    module_path == configured_module_path
        || module_path
            .strip_prefix(configured_module_path)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn sort_diagnostics(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by(|left, right| match (left.file_path(), right.file_path()) {
        (None, None) => left.message().cmp(&right.message()),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(left_file), Some(right_file)) => left_file
            .cmp(right_file)
            .then_with(|| left.message().cmp(&right.message())),
    });

    diagnostics
}
