from enum import Enum
from pathlib import Path
from typing import Literal

class PythonImport:
    module_path: str
    line_number: int

def get_project_imports(
    project_root: Path,
    source_roots: list[Path],
    file_path: Path,
    project_config: ProjectConfig,
) -> list[PythonImport]: ...
def get_external_imports(
    project_root: Path,
    source_roots: list[Path],
    file_path: Path,
    project_config: ProjectConfig,
) -> list[PythonImport]: ...
def create_dependency_report(
    project_root: Path,
    project_config: ProjectConfig,
    path: Path,
    include_dependency_modules: list[str] | None,
    include_usage_modules: list[str] | None,
    skip_dependencies: bool,
    skip_usages: bool,
    raw: bool,
) -> str: ...
def create_computation_cache_key(
    project_root: Path,
    source_roots: list[Path],
    action: str,
    py_interpreter_version: str,
    file_dependencies: list[str],
    env_dependencies: list[str],
    backend: str,
    respect_gitignore: bool,
) -> str: ...
def check_computation_cache(
    project_root: Path, cache_key: str
) -> tuple[list[tuple[int, str]], int] | None: ...
def update_computation_cache(
    project_root: Path, cache_key: str, value: tuple[list[tuple[int, str]], int]
) -> None: ...
def parse_project_config(filepath: Path) -> tuple[ProjectConfig, bool]: ...
def parse_project_config_from_pyproject(filepath: Path) -> ProjectConfig: ...
def dump_project_config_to_toml(project_config: ProjectConfig) -> str: ...
def check(
    project_root: Path,
    project_config: ProjectConfig,
    dependencies: bool,
    interfaces: bool,
) -> list[Diagnostic]: ...
def check_external_dependencies(
    project_root: Path,
    project_config: ProjectConfig,
) -> list[Diagnostic]: ...
def format_diagnostics(diagnostics: list[Diagnostic]) -> str: ...
def detect_unused_dependencies(
    project_root: Path,
    project_config: ProjectConfig,
) -> list[UnusedDependencies]: ...
def sync_project(
    project_root: Path,
    project_config: ProjectConfig,
    add: bool = False,
) -> None: ...
def run_server(project_root: Path, project_config: ProjectConfig) -> None: ...
def serialize_modules_json(modules: list[ModuleConfig]) -> str: ...

class Diagnostic:
    def is_code(self) -> bool: ...
    def is_configuration(self) -> bool: ...
    def is_dependency_error(self) -> bool: ...
    def is_interface_error(self) -> bool: ...
    def is_warning(self) -> bool: ...
    def is_error(self) -> bool: ...
    def is_deprecated(self) -> bool: ...
    def usage_module(self) -> str | None: ...
    def definition_module(self) -> str | None: ...
    def to_string(self) -> str: ...
    def pyfile_path(self) -> str | None: ...
    def pyline_number(self) -> int | None: ...

def serialize_diagnostics_json(
    diagnostics: list[Diagnostic], pretty_print: bool
) -> str: ...

class DependencyConfig:
    path: str
    deprecated: bool

class ModuleConfig:
    path: str
    depends_on: list[DependencyConfig] | None
    cannot_depend_on: list[DependencyConfig] | None
    depends_on_external: list[str] | None
    cannot_depend_on_external: list[str] | None
    visibility: list[str]
    strict: bool
    unchecked: bool
    layer: str | None

    def __new__(cls, path: str, strict: bool) -> ModuleConfig: ...
    def mod_path(self) -> str: ...

InterfaceDataTypes = Literal["all", "primitive"]

class InterfaceConfig:
    expose: list[str]
    # 'from' in tach.toml
    from_modules: list[str]
    visibility: list[str] | None
    data_types: InterfaceDataTypes

CacheBackend = Literal["disk"]

class CacheConfig:
    backend: CacheBackend
    file_dependencies: list[str]
    env_dependencies: list[str]

class ExternalDependencyConfig:
    exclude: list[str]
    rename: list[str]

class UnusedDependencies:
    path: str
    dependencies: list[DependencyConfig]

RuleSetting = Literal["error", "warn", "off"]

RootModuleTreatment = Literal["allow", "ignore", "dependenciesonly", "forbid"]

class RulesConfig:
    unused_ignore_directives: RuleSetting
    require_ignore_directive_reasons: RuleSetting

class ProjectConfig:
    cache: CacheConfig
    external: ExternalDependencyConfig
    exclude: list[str]
    source_roots: list[Path]
    exact: bool
    disable_logging: bool
    ignore_type_checking_imports: bool
    include_string_imports: bool
    forbid_circular_dependencies: bool
    respect_gitignore: bool
    # [DEPRECATED] Just reading this to print a warning
    use_regex_matching: bool
    rules: RulesConfig
    root_module: RootModuleTreatment

    def __new__(cls) -> ProjectConfig: ...
    def serialize_json(self) -> str: ...
    def exists(self) -> bool: ...
    def set_location(self, location: Path) -> None: ...
    def has_no_modules(self) -> bool: ...
    def has_no_dependencies(self) -> bool: ...
    def has_root_module_reference(self) -> bool: ...
    def module_paths(self) -> list[str]: ...
    def utility_paths(self) -> list[str]: ...
    def create_module(self, path: str) -> None: ...
    def delete_module(self, path: str) -> None: ...
    def mark_module_as_utility(self, path: str) -> None: ...
    def unmark_module_as_utility(self, path: str) -> None: ...
    def add_dependency(self, path: str, dependency: str) -> None: ...
    def remove_dependency(self, path: str, dependency: str) -> None: ...
    def add_source_root(self, path: Path) -> None: ...
    def remove_source_root(self, path: Path) -> None: ...
    def save_edits(self) -> None: ...
    def all_modules(self) -> list[ModuleConfig]: ...
    def all_interfaces(self) -> list[InterfaceConfig]: ...
    def filtered_modules(self, included_paths: list[Path]) -> list[ModuleConfig]: ...

class TachPytestPluginHandler:
    removed_test_paths: set[str]
    all_affected_modules: set[str]
    num_removed_items: int
    tests_ran_to_completion: bool
    def __new__(
        cls,
        project_root: Path,
        project_config: ProjectConfig,
        changed_files: list[Path],
        all_affected_modules: set[Path],
    ) -> TachPytestPluginHandler: ...
    def remove_test_path(self, path: Path) -> None: ...
    def should_remove_items(self, file_path: Path) -> bool: ...

class Direction(Enum):
    Dependencies = 0
    Dependents = 1

class DependentMap:
    def __new__(
        cls,
        project_root: Path,
        project_config: ProjectConfig,
        direction: Direction | None = None,
    ) -> DependentMap: ...
    def rebuild(self) -> None: ...
    def get_closure(self, paths: list[Path]) -> set[str]: ...
    def update_files(self, changed_files: list[Path]) -> None: ...
    def write_to_file(self, path: Path) -> None: ...
    def write_to_stdout(self) -> None: ...
