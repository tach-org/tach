use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use ruff_linter::Locator;
use ruff_python_ast::visitor::{Visitor, walk_expr};
use ruff_python_ast::{Alias, AnyParameterRef, Expr, ExprContext, Mod, Stmt};
use ruff_source_file::LineIndex;
use ruff_text_size::TextRange;

use crate::config::ProjectConfig;
use crate::deadcode::{FileImportGraph, resolve_entry_points};
use crate::filesystem as fs;
use crate::python::parsing::parse_python_source;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SymbolKey {
    module_path: String,
    symbol_name: String,
}

impl SymbolKey {
    fn new(module_path: impl Into<String>, symbol_name: impl Into<String>) -> Self {
        Self {
            module_path: module_path.into(),
            symbol_name: symbol_name.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Class,
    Variable,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Class => "class",
            Self::Variable => "variable",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeadSymbolFinding {
    pub module_path: String,
    pub symbol_name: String,
    pub symbol_kind: SymbolKind,
    pub file_path: PathBuf,
    pub line_number: usize,
}

pub struct DeadSymbolAnalysisInput<'a> {
    pub project_root: &'a Path,
    pub project_config: &'a ProjectConfig,
    pub source_roots: &'a [PathBuf],
    pub file_walker: &'a fs::FSWalker,
    pub graph: &'a FileImportGraph,
    pub entry_file_roots: &'a [PathBuf],
    pub parse_failures: &'a BTreeSet<PathBuf>,
    pub entry_points: &'a [String],
}

#[derive(Debug, Clone)]
struct SymbolDefinition {
    kind: SymbolKind,
    file_path: PathBuf,
    line_number: usize,
    references: BTreeSet<SymbolKey>,
    has_public_decorator: bool,
}

#[derive(Debug, Clone)]
struct ModuleSymbols {
    symbols: BTreeMap<String, SymbolDefinition>,
    module_references: BTreeSet<SymbolKey>,
    exports: BTreeSet<String>,
    star_imports: Vec<String>,
    is_dynamic: bool,
}

#[derive(Debug, Clone)]
enum ImportTarget {
    Module(String),
    Symbol(SymbolKey),
}

#[derive(Debug)]
struct ModuleCollector<'a> {
    source_roots: &'a [PathBuf],
    module_path: String,
    file_path: PathBuf,
    line_index: LineIndex,
    imports: BTreeMap<String, ImportTarget>,
    local_symbols: BTreeSet<String>,
    public_decorators: BTreeSet<String>,
    symbols: BTreeMap<String, SymbolDefinition>,
    module_references: BTreeSet<SymbolKey>,
    exports: BTreeSet<String>,
    star_imports: Vec<String>,
    is_package: bool,
    is_dynamic: bool,
}

impl<'a> ModuleCollector<'a> {
    fn new(
        source_roots: &'a [PathBuf],
        module_path: String,
        file_path: PathBuf,
        contents: &str,
        public_decorators: BTreeSet<String>,
    ) -> Self {
        Self {
            source_roots,
            module_path,
            file_path: file_path.clone(),
            line_index: Locator::new(contents).to_index().clone(),
            imports: BTreeMap::new(),
            local_symbols: BTreeSet::new(),
            public_decorators,
            symbols: BTreeMap::new(),
            module_references: BTreeSet::new(),
            exports: BTreeSet::new(),
            star_imports: Vec::new(),
            is_package: file_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "__init__.py"),
            is_dynamic: false,
        }
    }

    fn finish(self) -> ModuleSymbols {
        ModuleSymbols {
            symbols: self.symbols,
            module_references: self.module_references,
            exports: self.exports,
            star_imports: self.star_imports,
            is_dynamic: self.is_dynamic,
        }
    }

    fn collect(mut self, body: &[Stmt]) -> ModuleSymbols {
        self.collect_imports_and_definitions(body);
        self.collect_references(body);
        self.finish()
    }

    fn collect_imports_and_definitions(&mut self, body: &[Stmt]) {
        for stmt in body {
            self.collect_statement_imports_and_definitions(stmt);
        }
    }

    fn collect_statement_imports_and_definitions(&mut self, stmt: &Stmt) {
        if statement_contains_dynamic_call(stmt) {
            self.is_dynamic = true;
        }

        match stmt {
            Stmt::Import(import) => {
                for alias in &import.names {
                    self.register_import(alias);
                }
            }
            Stmt::ImportFrom(import) => {
                let Some(base_module) = self.resolve_import_from_base(
                    import.module.as_ref().map(|module| module.as_str()),
                    import.level,
                ) else {
                    return;
                };

                for alias in &import.names {
                    if alias.name.as_str() == "*" {
                        self.is_dynamic = true;
                        self.star_imports.push(base_module.clone());
                        continue;
                    }

                    self.register_import_from(alias, &base_module);
                }
            }
            Stmt::FunctionDef(function) => {
                if function.name.as_str() == "__getattr__" {
                    self.is_dynamic = true;
                }
                self.register_symbol(
                    function.name.to_string(),
                    SymbolKind::Function,
                    function.range,
                    false,
                );
            }
            Stmt::ClassDef(class) => {
                self.register_symbol(
                    class.name.to_string(),
                    SymbolKind::Class,
                    class.range,
                    false,
                );
            }
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    if let Some(name) = simple_name(target) {
                        if name == "__all__" {
                            self.collect_exports(&assign.value);
                        } else {
                            self.register_symbol(
                                name.to_string(),
                                SymbolKind::Variable,
                                assign.range,
                                false,
                            );
                        }
                    }
                }
            }
            Stmt::AnnAssign(assign) => {
                if assign.simple
                    && let Some(name) = simple_name(&assign.target)
                {
                    if name == "__all__" {
                        if let Some(value) = &assign.value {
                            self.collect_exports(value);
                        }
                    } else {
                        self.register_symbol(
                            name.to_string(),
                            SymbolKind::Variable,
                            assign.range,
                            false,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_references(&mut self, body: &[Stmt]) {
        let function_like_symbols = self.symbols.keys().cloned().collect::<BTreeSet<_>>();
        for stmt in body {
            self.collect_statement_references(stmt, &function_like_symbols);
        }
    }

    fn collect_statement_references(&mut self, stmt: &Stmt, module_symbols: &BTreeSet<String>) {
        match stmt {
            Stmt::Import(_) | Stmt::ImportFrom(_) => {}
            Stmt::FunctionDef(function) => {
                let symbol_name = function.name.to_string();
                if !module_symbols.contains(&symbol_name) {
                    return;
                }

                let has_public_decorator = function.decorator_list.iter().any(|decorator| {
                    self.decorator_paths(&decorator.expression)
                        .iter()
                        .any(|path| self.public_decorators.contains(path))
                });
                if has_public_decorator && let Some(symbol) = self.symbols.get_mut(&symbol_name) {
                    symbol.has_public_decorator = true;
                }

                let shadowed =
                    collect_function_shadowed_names(&function.parameters, &function.body);
                let mut collector = ReferenceCollector::new(
                    &self.module_path,
                    &self.local_symbols,
                    &self.imports,
                    shadowed,
                );
                for decorator in &function.decorator_list {
                    collector.visit_expr(&decorator.expression);
                }
                for stmt in &function.body {
                    collector.visit_stmt(stmt);
                }
                if let Some(returns) = &function.returns {
                    collector.visit_expr(returns);
                }
                if let Some(symbol) = self.symbols.get_mut(&symbol_name) {
                    symbol.references.extend(collector.references);
                }
            }
            Stmt::ClassDef(class) => {
                let symbol_name = class.name.to_string();
                if !module_symbols.contains(&symbol_name) {
                    return;
                }

                let has_public_decorator = class.decorator_list.iter().any(|decorator| {
                    self.decorator_paths(&decorator.expression)
                        .iter()
                        .any(|path| self.public_decorators.contains(path))
                });
                if has_public_decorator && let Some(symbol) = self.symbols.get_mut(&symbol_name) {
                    symbol.has_public_decorator = true;
                }

                let shadowed = collect_class_shadowed_names(&class.body);
                let mut collector = ReferenceCollector::new(
                    &self.module_path,
                    &self.local_symbols,
                    &self.imports,
                    shadowed,
                );
                for decorator in &class.decorator_list {
                    collector.visit_expr(&decorator.expression);
                }
                if let Some(arguments) = &class.arguments {
                    collector.visit_arguments(arguments);
                }
                for stmt in &class.body {
                    collector.visit_stmt(stmt);
                }
                if let Some(symbol) = self.symbols.get_mut(&symbol_name) {
                    symbol.references.extend(collector.references);
                }
            }
            _ => {
                let mut collector = ReferenceCollector::new(
                    &self.module_path,
                    &self.local_symbols,
                    &self.imports,
                    BTreeSet::new(),
                );
                collector.visit_stmt(stmt);
                self.module_references.extend(collector.references);
            }
        }
    }

    fn register_symbol(
        &mut self,
        name: String,
        kind: SymbolKind,
        range: TextRange,
        has_public_decorator: bool,
    ) {
        self.local_symbols.insert(name.clone());
        if should_skip_symbol_candidate(&name) || self.symbols.contains_key(&name) {
            return;
        }

        self.symbols.insert(
            name,
            SymbolDefinition {
                kind,
                file_path: self.file_path.clone(),
                line_number: self.line_index.line_index(range.start()).get(),
                references: BTreeSet::new(),
                has_public_decorator,
            },
        );
    }

    fn register_import(&mut self, alias: &Alias) {
        let imported_module = alias.name.to_string();
        let binding = alias.asname.as_ref().map_or_else(
            || {
                imported_module
                    .split('.')
                    .next()
                    .unwrap_or_default()
                    .to_string()
            },
            std::string::ToString::to_string,
        );
        if binding.is_empty() {
            return;
        }

        let target_module = if alias.asname.is_some() {
            imported_module
        } else {
            binding.clone()
        };
        self.imports
            .insert(binding, ImportTarget::Module(target_module));
    }

    fn register_import_from(&mut self, alias: &Alias, base_module: &str) {
        let binding = alias
            .asname
            .as_ref()
            .map_or_else(|| alias.name.to_string(), std::string::ToString::to_string);
        if binding.is_empty() {
            return;
        }

        let full_import = if base_module.is_empty() {
            alias.name.to_string()
        } else {
            format!("{}.{}", base_module, alias.name)
        };
        let target = import_target_from_path(self.source_roots, &full_import);
        self.imports.insert(binding, target);
    }

    fn resolve_import_from_base(&self, module: Option<&str>, level: u32) -> Option<String> {
        let import_depth = usize::try_from(level).ok()?;
        let num_paths_to_strip = if self.is_package {
            import_depth.saturating_sub(1)
        } else {
            import_depth
        };

        let mut base_parts = if self.module_path.is_empty() {
            Vec::new()
        } else {
            self.module_path.split('.').collect::<Vec<_>>()
        };
        if num_paths_to_strip > base_parts.len() {
            return None;
        }
        if num_paths_to_strip > 0 {
            base_parts.truncate(base_parts.len() - num_paths_to_strip);
        }

        match module {
            Some(module) if level > 0 && !base_parts.is_empty() => {
                Some(format!("{}.{}", base_parts.join("."), module))
            }
            Some(module) => Some(module.to_string()),
            None if !base_parts.is_empty() => Some(base_parts.join(".")),
            None => None,
        }
    }

    fn collect_exports(&mut self, value: &Expr) {
        match value {
            Expr::List(list) => {
                for value in &list.elts {
                    self.collect_export_value(value);
                }
            }
            Expr::Tuple(tuple) => {
                for value in &tuple.elts {
                    self.collect_export_value(value);
                }
            }
            _ => {}
        }
    }

    fn collect_export_value(&mut self, value: &Expr) {
        if let Expr::StringLiteral(string) = value {
            self.exports.insert(string.value.to_string());
        }
    }

    fn decorator_paths(&self, expr: &Expr) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        match expr {
            Expr::Call(call) => {
                paths.extend(self.decorator_paths(&call.func));
            }
            Expr::Name(name) => {
                paths.insert(name.id.to_string());
                if self.local_symbols.contains(name.id.as_str()) {
                    paths.insert(format!("{}.{}", self.module_path, name.id));
                }
                if let Some(ImportTarget::Symbol(symbol)) = self.imports.get(name.id.as_str()) {
                    paths.insert(format!("{}.{}", symbol.module_path, symbol.symbol_name));
                }
            }
            Expr::Attribute(attribute) => {
                if let Some(module_path) =
                    resolve_module_expression(&attribute.value, &self.imports)
                {
                    paths.insert(format!("{}.{}", module_path, attribute.attr));
                }
                if let Some(raw_path) = raw_expression_path(expr) {
                    paths.insert(raw_path);
                }
            }
            _ => {}
        }
        paths
    }
}

struct ReferenceCollector<'a> {
    module_path: &'a str,
    local_symbols: &'a BTreeSet<String>,
    imports: &'a BTreeMap<String, ImportTarget>,
    shadowed: BTreeSet<String>,
    references: BTreeSet<SymbolKey>,
}

impl<'a> ReferenceCollector<'a> {
    fn new(
        module_path: &'a str,
        local_symbols: &'a BTreeSet<String>,
        imports: &'a BTreeMap<String, ImportTarget>,
        shadowed: BTreeSet<String>,
    ) -> Self {
        Self {
            module_path,
            local_symbols,
            imports,
            shadowed,
            references: BTreeSet::new(),
        }
    }

    fn resolve_name(&self, name: &str) -> Option<SymbolKey> {
        if self.shadowed.contains(name) {
            return None;
        }
        if self.local_symbols.contains(name) {
            return Some(SymbolKey::new(self.module_path, name));
        }
        match self.imports.get(name)? {
            ImportTarget::Symbol(symbol) => Some(symbol.clone()),
            ImportTarget::Module(_) => None,
        }
    }
}

impl Visitor<'_> for ReferenceCollector<'_> {
    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Name(name) => {
                if matches!(name.ctx, ExprContext::Load)
                    && let Some(symbol) = self.resolve_name(name.id.as_str())
                {
                    self.references.insert(symbol);
                }
                walk_expr(self, expr);
            }
            Expr::Attribute(attribute) => {
                if matches!(attribute.ctx, ExprContext::Load)
                    && let Some(module_path) =
                        resolve_module_expression(&attribute.value, self.imports)
                {
                    self.references
                        .insert(SymbolKey::new(module_path, attribute.attr.to_string()));
                }
                walk_expr(self, expr);
            }
            _ => walk_expr(self, expr),
        }
    }
}

pub fn find_dead_symbols(input: DeadSymbolAnalysisInput<'_>) -> Vec<DeadSymbolFinding> {
    let reachable = input.graph.reachable_from(input.entry_file_roots);
    if input
        .parse_failures
        .iter()
        .any(|failed_file| reachable.contains(failed_file))
    {
        return Vec::new();
    }

    let public_decorators = input
        .project_config
        .deadcode
        .public_decorators
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut modules = BTreeMap::<String, ModuleSymbols>::new();
    for file_path in &reachable {
        let Some(module_path) = input.graph.module_path(file_path) else {
            continue;
        };
        if let Some(module_symbols) = collect_module_symbols(
            input.source_roots,
            file_path,
            module_path,
            public_decorators.clone(),
        ) {
            modules.insert(module_path.to_string(), module_symbols);
        }
    }

    add_star_import_references(&mut modules, input.project_config.deadcode.respect_all);

    let mut live = seed_live_symbols(
        input.project_root,
        input.project_config,
        input.source_roots,
        input.file_walker,
        input.graph,
        input.entry_points,
        &modules,
    );
    propagate_liveness(&modules, &mut live);

    let mut findings = Vec::new();
    for (module_path, module_symbols) in modules {
        if input.project_config.deadcode.ignore_dynamic_modules && module_symbols.is_dynamic {
            continue;
        }

        for (symbol_name, symbol) in module_symbols.symbols {
            let symbol_key = SymbolKey::new(module_path.clone(), symbol_name.clone());
            if live.contains(&symbol_key) {
                continue;
            }
            findings.push(DeadSymbolFinding {
                module_path: module_path.clone(),
                symbol_name,
                symbol_kind: symbol.kind,
                file_path: symbol.file_path,
                line_number: symbol.line_number,
            });
        }
    }

    findings.sort_by(|left, right| {
        left.file_path
            .cmp(&right.file_path)
            .then_with(|| left.line_number.cmp(&right.line_number))
            .then_with(|| left.module_path.cmp(&right.module_path))
            .then_with(|| left.symbol_name.cmp(&right.symbol_name))
    });
    findings
}

fn collect_module_symbols(
    source_roots: &[PathBuf],
    file_path: &Path,
    module_path: &str,
    public_decorators: BTreeSet<String>,
) -> Option<ModuleSymbols> {
    let contents = fs::read_file_content(file_path).ok()?;
    let ast = parse_python_source(&contents).ok()?;
    let Mod::Module(module) = ast else {
        return None;
    };

    Some(
        ModuleCollector::new(
            source_roots,
            module_path.to_string(),
            file_path.to_path_buf(),
            &contents,
            public_decorators,
        )
        .collect(&module.body),
    )
}

fn add_star_import_references(modules: &mut BTreeMap<String, ModuleSymbols>, respect_all: bool) {
    let imported_symbols = modules
        .iter()
        .map(|(module_path, module)| {
            let mut symbols = BTreeSet::new();
            for (name, symbol) in &module.symbols {
                if respect_all && !module.exports.is_empty() {
                    if module.exports.contains(name) {
                        symbols.insert(SymbolKey::new(module_path.clone(), name.clone()));
                    }
                } else if !name.starts_with('_') || symbol.has_public_decorator {
                    symbols.insert(SymbolKey::new(module_path.clone(), name.clone()));
                }
            }
            (module_path.clone(), symbols)
        })
        .collect::<BTreeMap<_, _>>();

    for module in modules.values_mut() {
        for star_module in &module.star_imports {
            if let Some(symbols) = imported_symbols.get(star_module) {
                module.module_references.extend(symbols.iter().cloned());
            }
        }
    }
}

fn seed_live_symbols(
    project_root: &Path,
    project_config: &ProjectConfig,
    source_roots: &[PathBuf],
    file_walker: &fs::FSWalker,
    graph: &FileImportGraph,
    entry_points: &[String],
    modules: &BTreeMap<String, ModuleSymbols>,
) -> BTreeSet<SymbolKey> {
    let mut live = BTreeSet::new();

    for module in modules.values() {
        for reference in &module.module_references {
            if symbol_exists(modules, reference) {
                live.insert(reference.clone());
            }
        }
    }

    if project_config.deadcode.respect_all {
        for (module_path, module) in modules {
            for symbol_name in &module.exports {
                let key = SymbolKey::new(module_path.clone(), symbol_name.clone());
                if symbol_exists(modules, &key) {
                    live.insert(key);
                }
            }
        }
    }

    for public_symbol in &project_config.deadcode.public_symbols {
        if let Some(symbol) = parse_symbol_rule(public_symbol)
            && symbol_exists(modules, &symbol)
        {
            live.insert(symbol);
        }
    }

    for public_module in &project_config.deadcode.public_modules {
        if let Some(module) = modules.get(public_module) {
            for symbol_name in module.symbols.keys() {
                if !symbol_name.starts_with('_') {
                    live.insert(SymbolKey::new(public_module.clone(), symbol_name.clone()));
                }
            }
        }
    }

    for (module_path, module) in modules {
        for (symbol_name, symbol) in &module.symbols {
            if symbol.has_public_decorator {
                live.insert(SymbolKey::new(module_path.clone(), symbol_name.clone()));
            }
        }
    }

    for entry_symbol in
        entry_point_symbol_seeds(project_root, source_roots, file_walker, graph, entry_points)
    {
        if symbol_exists(modules, &entry_symbol) {
            live.insert(entry_symbol);
        }
    }

    live
}

fn propagate_liveness(modules: &BTreeMap<String, ModuleSymbols>, live: &mut BTreeSet<SymbolKey>) {
    let mut worklist = live.iter().cloned().collect::<VecDeque<_>>();
    while let Some(symbol_key) = worklist.pop_front() {
        let Some(module) = modules.get(&symbol_key.module_path) else {
            continue;
        };
        let Some(symbol) = module.symbols.get(&symbol_key.symbol_name) else {
            continue;
        };
        for reference in &symbol.references {
            if symbol_exists(modules, reference) && live.insert(reference.clone()) {
                worklist.push_back(reference.clone());
            }
        }
    }
}

fn entry_point_symbol_seeds(
    project_root: &Path,
    source_roots: &[PathBuf],
    file_walker: &fs::FSWalker,
    graph: &FileImportGraph,
    entry_points: &[String],
) -> BTreeSet<SymbolKey> {
    let mut seeds = BTreeSet::new();
    for entry_point in entry_points {
        let Some((module_part, symbol_name)) = entry_point.split_once(':') else {
            continue;
        };
        let symbol_name = symbol_name.trim();
        if symbol_name.is_empty() {
            continue;
        }
        let resolved = resolve_entry_points(
            project_root,
            source_roots,
            file_walker,
            &[module_part.trim().to_string()],
        );
        for file_path in resolved.files {
            if let Some(module_path) = graph.module_path(&file_path) {
                seeds.insert(SymbolKey::new(module_path, symbol_name));
            }
        }
    }
    seeds
}

fn symbol_exists(modules: &BTreeMap<String, ModuleSymbols>, symbol: &SymbolKey) -> bool {
    modules
        .get(&symbol.module_path)
        .is_some_and(|module| module.symbols.contains_key(&symbol.symbol_name))
}

fn import_target_from_path(source_roots: &[PathBuf], full_import: &str) -> ImportTarget {
    if fs::module_to_file_path(source_roots, full_import, false).is_some() {
        return ImportTarget::Module(full_import.to_string());
    }

    if let Some(resolved) = fs::module_to_file_path(source_roots, full_import, true) {
        if let Some(member_name) = resolved.member_name
            && let Ok(module_path) = fs::file_to_module_path(source_roots, &resolved.file_path)
        {
            return ImportTarget::Symbol(SymbolKey::new(module_path, member_name));
        }
        return ImportTarget::Module(full_import.to_string());
    }

    full_import.rsplit_once('.').map_or_else(
        || ImportTarget::Module(full_import.to_string()),
        |(module_path, symbol_name)| {
            ImportTarget::Symbol(SymbolKey::new(
                module_path.to_string(),
                symbol_name.to_string(),
            ))
        },
    )
}

fn resolve_module_expression(
    expr: &Expr,
    imports: &BTreeMap<String, ImportTarget>,
) -> Option<String> {
    match expr {
        Expr::Name(name) => imports
            .get(name.id.as_str())
            .and_then(|target| match target {
                ImportTarget::Module(module_path) => Some(module_path.clone()),
                ImportTarget::Symbol(_) => None,
            }),
        Expr::Attribute(attribute) => resolve_module_expression(&attribute.value, imports)
            .map(|prefix| format!("{}.{}", prefix, attribute.attr)),
        _ => None,
    }
}

fn raw_expression_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.id.to_string()),
        Expr::Attribute(attribute) => raw_expression_path(&attribute.value)
            .map(|prefix| format!("{}.{}", prefix, attribute.attr)),
        _ => None,
    }
}

fn simple_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(name) => Some(name.id.as_str()),
        _ => None,
    }
}

fn should_skip_symbol_candidate(name: &str) -> bool {
    name.starts_with("__") && name.ends_with("__")
}

fn parse_symbol_rule(rule: &str) -> Option<SymbolKey> {
    parse_symbol_rule_parts(rule).map(|(module_path, symbol_name)| {
        SymbolKey::new(module_path.to_string(), symbol_name.to_string())
    })
}

pub fn parse_symbol_rule_parts(rule: &str) -> Option<(&str, &str)> {
    if let Some((module_path, symbol_name)) = rule.split_once(':') {
        return valid_symbol_rule_parts(module_path, symbol_name);
    }

    rule.rsplit_once('.')
        .and_then(|(module_path, symbol_name)| valid_symbol_rule_parts(module_path, symbol_name))
}

fn valid_symbol_rule_parts<'a>(
    module_path: &'a str,
    symbol_name: &'a str,
) -> Option<(&'a str, &'a str)> {
    (!module_path.is_empty() && !symbol_name.is_empty()).then_some((module_path, symbol_name))
}

fn collect_function_shadowed_names(
    parameters: &ruff_python_ast::Parameters,
    body: &[Stmt],
) -> BTreeSet<String> {
    collect_shadowed_names(body, Some(parameters))
}

fn collect_class_shadowed_names(body: &[Stmt]) -> BTreeSet<String> {
    collect_shadowed_names(body, None)
}

fn collect_shadowed_names(
    body: &[Stmt],
    parameters: Option<&ruff_python_ast::Parameters>,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(parameters) = parameters {
        for parameter in parameters.iter() {
            names.insert(parameter_name(parameter).to_string());
        }
    }
    collect_statement_bound_names(body, &mut names);
    names
}

fn parameter_name(parameter: AnyParameterRef<'_>) -> &str {
    match parameter {
        AnyParameterRef::NonVariadic(parameter) => parameter.name().as_str(),
        AnyParameterRef::Variadic(parameter) => parameter.name().as_str(),
    }
}

fn collect_statement_bound_names(body: &[Stmt], names: &mut BTreeSet<String>) {
    for stmt in body {
        match stmt {
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    collect_expr_bound_names(target, names);
                }
            }
            Stmt::AnnAssign(assign) => collect_expr_bound_names(&assign.target, names),
            Stmt::AugAssign(assign) => collect_expr_bound_names(&assign.target, names),
            Stmt::For(stmt) => {
                collect_expr_bound_names(&stmt.target, names);
                collect_statement_bound_names(&stmt.body, names);
                collect_statement_bound_names(&stmt.orelse, names);
            }
            Stmt::With(stmt) => {
                for item in &stmt.items {
                    if let Some(optional_vars) = &item.optional_vars {
                        collect_expr_bound_names(optional_vars, names);
                    }
                }
                collect_statement_bound_names(&stmt.body, names);
            }
            Stmt::FunctionDef(function) => {
                names.insert(function.name.to_string());
            }
            Stmt::ClassDef(class) => {
                names.insert(class.name.to_string());
            }
            Stmt::Import(import) => {
                for alias in &import.names {
                    let imported_module = alias.name.to_string();
                    names.insert(alias.asname.as_ref().map_or_else(
                        || {
                            imported_module
                                .split('.')
                                .next()
                                .unwrap_or_default()
                                .to_string()
                        },
                        std::string::ToString::to_string,
                    ));
                }
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    if alias.name.as_str() != "*" {
                        names.insert(alias.asname.as_ref().map_or_else(
                            || alias.name.to_string(),
                            std::string::ToString::to_string,
                        ));
                    }
                }
            }
            Stmt::If(stmt) => {
                collect_statement_bound_names(&stmt.body, names);
                for clause in &stmt.elif_else_clauses {
                    collect_statement_bound_names(&clause.body, names);
                }
            }
            Stmt::Try(stmt) => {
                collect_statement_bound_names(&stmt.body, names);
                collect_statement_bound_names(&stmt.orelse, names);
                collect_statement_bound_names(&stmt.finalbody, names);
            }
            _ => {}
        }
    }
}

fn collect_expr_bound_names(expr: &Expr, names: &mut BTreeSet<String>) {
    match expr {
        Expr::Name(name) => {
            names.insert(name.id.to_string());
        }
        Expr::Tuple(tuple) => {
            for element in &tuple.elts {
                collect_expr_bound_names(element, names);
            }
        }
        Expr::List(list) => {
            for element in &list.elts {
                collect_expr_bound_names(element, names);
            }
        }
        _ => {}
    }
}

fn statement_contains_dynamic_call(stmt: &Stmt) -> bool {
    let mut visitor = DynamicCallVisitor { found: false };
    visitor.visit_stmt(stmt);
    visitor.found
}

struct DynamicCallVisitor {
    found: bool,
}

impl Visitor<'_> for DynamicCallVisitor {
    fn visit_expr(&mut self, expr: &Expr) {
        if self.found {
            return;
        }
        if let Expr::Call(call) = expr
            && expression_is_dynamic_call(&call.func)
        {
            self.found = true;
            return;
        }
        walk_expr(self, expr);
    }
}

fn expression_is_dynamic_call(expr: &Expr) -> bool {
    match expr {
        Expr::Name(name) => matches!(
            name.id.as_str(),
            "globals" | "locals" | "getattr" | "setattr" | "__import__"
        ),
        Expr::Attribute(attribute) => {
            attribute.attr.as_str() == "import_module"
                && raw_expression_path(&attribute.value).is_some_and(|path| path == "importlib")
        }
        _ => false,
    }
}
