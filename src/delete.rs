use std::path::Path;

pub fn delete_path(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|e| e.to_string())
}

pub fn delete_paths(paths: &[impl AsRef<Path>]) -> Result<(), String> {
    let paths: Vec<_> = paths.iter().map(|p| p.as_ref()).collect();
    trash::delete_all(paths).map_err(|e| e.to_string())
}
