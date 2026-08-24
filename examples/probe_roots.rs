use std::path::PathBuf;
use tach::filesystem::FSWalker;
use tach::resolvers::SourceRootResolver;

fn main() {
    let root = PathBuf::from("/private/tmp/dc_probe11");
    let walker = FSWalker::empty(&root);
    let resolver = SourceRootResolver::new(&root, &walker);
    let specs: Vec<Vec<&str>> = vec![
        vec!["a", "b"],
        vec!["b", "a"],
        vec!["src", "stubs"],
        vec!["stubs", "src"],
        vec!["one", "two", "three", "four", "five"],
    ];
    for spec in specs {
        let roots: Vec<PathBuf> = spec.iter().map(|s| PathBuf::from(*s)).collect();
        let resolved = resolver.resolve(&roots).unwrap();
        let names: Vec<String> = resolved
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        print!("{spec:?}->{names:?}  ");
    }
    println!();
}
