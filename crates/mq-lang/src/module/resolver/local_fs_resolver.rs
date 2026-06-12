use crate::{ModuleError, ModuleResolver};
use std::{borrow::Cow, fs, path::PathBuf};

pub(crate) const DEFAULT_PATHS: [&str; 5] = [
    "$HOME/.mq",
    "$HOME/.config/mq",
    "$ORIGIN/../lib/mq",
    "$ORIGIN/../lib",
    "$ORIGIN",
];

/// Resolves mq modules from the local filesystem, searching a configurable list of directories.
#[derive(Debug, Clone, Default)]
pub struct LocalFsModuleResolver {
    pub(crate) paths: Option<Vec<PathBuf>>,
}

impl ModuleResolver for LocalFsModuleResolver {
    fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        let file_path = Self::search(module_name, &self.paths)?;
        fs::read_to_string(&file_path).map_err(|e| ModuleError::IOError(Cow::Owned(e.to_string())))
    }

    fn get_path(&self, module_name: &str) -> Result<String, ModuleError> {
        let file_path = Self::search(module_name, &self.paths)?;
        Ok(file_path.to_str().unwrap_or_default().to_string())
    }

    fn search_paths(&self) -> Vec<PathBuf> {
        self.paths
            .clone()
            .unwrap_or_else(|| DEFAULT_PATHS.iter().map(PathBuf::from).collect())
    }

    fn set_search_paths(&mut self, paths: Vec<PathBuf>) {
        self.paths = if paths.is_empty() { None } else { Some(paths) };
    }
}

impl LocalFsModuleResolver {
    /// Creates a new resolver.  `None` falls back to the default search paths.
    pub fn new(paths: Option<Vec<PathBuf>>) -> Self {
        Self { paths }
    }

    fn module_file_name(name: &str) -> String {
        format!("{}.mq", name)
    }

    fn search(name: &str, search_paths: &Option<Vec<PathBuf>>) -> Result<PathBuf, ModuleError> {
        let home = dirs::home_dir()
            .map(|p| p.to_str().unwrap_or("").to_string())
            .ok_or(ModuleError::IOError(Cow::Borrowed(
                "Could not determine home directory",
            )))?;
        let origin = std::env::current_dir().ok();

        search_paths
            .as_ref()
            .map(|p| {
                p.iter()
                    .map(|p| p.to_str().map(|p| p.to_string()).unwrap_or_default())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| DEFAULT_PATHS.iter().map(|p| p.to_string()).collect())
            .iter()
            .map(|path| {
                let path = origin
                    .clone()
                    .map(|p| {
                        path.replace("$ORIGIN", p.to_str().unwrap_or(""))
                            .replace("$HOME", home.as_str())
                    })
                    .unwrap_or_else(|| home.clone());

                PathBuf::from(path).join(Self::module_file_name(name))
            })
            .find(|p| p.is_file())
            .ok_or_else(|| ModuleError::NotFound(Cow::Owned(Self::module_file_name(name))))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tempfile::TempDir;

    use super::*;

    fn write_module(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(format!("{}.mq", name));
        std::fs::write(&path, content).unwrap();
        path
    }

    #[rstest]
    #[case("def foo(): 1;")]
    #[case("let x = 42;")]
    fn test_resolve_existing_module(#[case] content: &str) {
        let dir = TempDir::new().unwrap();
        write_module(&dir, "mymod", content);

        let resolver = LocalFsModuleResolver::new(Some(vec![dir.path().to_path_buf()]));
        let result = resolver.resolve("mymod");
        assert_eq!(result.unwrap(), content);
    }

    #[rstest]
    #[case("missing")]
    #[case("nonexistent_module")]
    fn test_resolve_missing_module_returns_not_found(#[case] name: &str) {
        let dir = TempDir::new().unwrap();
        let resolver = LocalFsModuleResolver::new(Some(vec![dir.path().to_path_buf()]));
        assert!(matches!(resolver.resolve(name), Err(ModuleError::NotFound(_))));
    }

    #[rstest]
    #[case("mymod", "def foo(): 1;")]
    fn test_get_path_existing_module(#[case] name: &str, #[case] content: &str) {
        let dir = TempDir::new().unwrap();
        let path = write_module(&dir, name, content);

        let resolver = LocalFsModuleResolver::new(Some(vec![dir.path().to_path_buf()]));
        let result = resolver.get_path(name).unwrap();
        assert_eq!(std::path::PathBuf::from(&result), path);
    }

    #[rstest]
    #[case("ghost")]
    fn test_get_path_missing_returns_not_found(#[case] name: &str) {
        let dir = TempDir::new().unwrap();
        let resolver = LocalFsModuleResolver::new(Some(vec![dir.path().to_path_buf()]));
        assert!(matches!(resolver.get_path(name), Err(ModuleError::NotFound(_))));
    }

    #[test]
    fn test_search_paths_custom() {
        let paths = vec![PathBuf::from("/custom/path")];
        let resolver = LocalFsModuleResolver::new(Some(paths.clone()));
        assert_eq!(resolver.search_paths(), paths);
    }

    #[test]
    fn test_search_paths_default_when_none() {
        let resolver = LocalFsModuleResolver::new(None);
        let paths = resolver.search_paths();
        assert_eq!(paths.len(), DEFAULT_PATHS.len());
    }

    #[test]
    fn test_set_search_paths_empty_resets_to_default() {
        let mut resolver = LocalFsModuleResolver::new(Some(vec![PathBuf::from("/x")]));
        resolver.set_search_paths(vec![]);
        assert!(resolver.paths.is_none());
    }

    #[test]
    fn test_set_search_paths_non_empty() {
        let mut resolver = LocalFsModuleResolver::new(None);
        let paths = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        resolver.set_search_paths(paths.clone());
        assert_eq!(resolver.paths, Some(paths));
    }

    #[rstest]
    #[case("first", "second")]
    fn test_first_matching_path_wins(#[case] name: &str, #[case] other: &str) {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        write_module(&dir1, name, "content from dir1");
        write_module(&dir2, name, "content from dir2");
        write_module(&dir2, other, "content from dir2 only");

        let resolver = LocalFsModuleResolver::new(Some(vec![dir1.path().to_path_buf(), dir2.path().to_path_buf()]));
        assert_eq!(resolver.resolve(name).unwrap(), "content from dir1");
        assert_eq!(resolver.resolve(other).unwrap(), "content from dir2 only");
    }
}
