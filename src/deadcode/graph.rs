use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct FileImportGraph {
    file_modules: BTreeMap<PathBuf, String>,
    imports: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
}

impl FileImportGraph {
    pub fn new() -> Self {
        Self {
            file_modules: BTreeMap::new(),
            imports: BTreeMap::new(),
        }
    }

    pub fn add_file(&mut self, file_path: PathBuf, module_path: String) {
        self.file_modules
            .entry(file_path.clone())
            .or_insert(module_path);
        self.imports.entry(file_path).or_default();
    }

    pub fn add_import(&mut self, from: &Path, to: &Path) {
        self.imports
            .entry(from.to_path_buf())
            .or_default()
            .insert(to.to_path_buf());
    }

    pub fn files(&self) -> impl Iterator<Item = &PathBuf> {
        self.file_modules.keys()
    }

    pub fn has_file(&self, file_path: &Path) -> bool {
        self.file_modules.contains_key(file_path)
    }

    pub fn module_path(&self, file_path: &Path) -> Option<&str> {
        self.file_modules
            .get(file_path)
            .map(std::string::String::as_str)
    }

    pub fn reachable_from(&self, roots: &[PathBuf]) -> BTreeSet<PathBuf> {
        let mut reachable = BTreeSet::new();
        let mut queue = VecDeque::new();

        for root in roots {
            if self.has_file(root) {
                reachable.insert(root.to_path_buf());
                queue.push_back(root.to_path_buf());
            }
        }

        while let Some(current) = queue.pop_front() {
            let Some(imported_modules) = self.imports.get(&current) else {
                continue;
            };

            for imported in imported_modules {
                if reachable.insert(imported.clone()) {
                    queue.push_back(imported.clone());
                }
            }
        }

        reachable
    }

    pub fn unreachable_from(&self, roots: &[PathBuf]) -> BTreeSet<PathBuf> {
        let reachable = self.reachable_from(roots);
        self.file_modules
            .keys()
            .filter(|file| !reachable.contains(*file))
            .cloned()
            .collect()
    }
}
