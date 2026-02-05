use std::{borrow::Cow, fs, path::PathBuf};

use crate::module::error::ModuleError;

const DEFAULT_PATHS: [&str; 4] = ["$HOME/.mq", "$ORIGIN/../lib/mq", "$ORIGIN/../lib", "$ORIGIN"];

pub trait ModuleResolver: Clone + Default {
    fn resolve(&self, module_name: &str) -> Result<String, ModuleError>;
    fn get_path(&self, module_name: &str) -> Result<String, ModuleError>;
    fn search_paths(&self) -> Vec<PathBuf>;
    fn set_search_paths(&mut self, paths: Vec<PathBuf>);
}

pub fn module_name(name: &str) -> Cow<'static, str> {
    // For common module names, use static strings to avoid allocation
    match name {
        "csv" => Cow::Borrowed("csv.mq"),
        "json" => Cow::Borrowed("json.mq"),
        "yaml" => Cow::Borrowed("yaml.mq"),
        "xml" => Cow::Borrowed("xml.mq"),
        "toml" => Cow::Borrowed("toml.mq"),
        "test" => Cow::Borrowed("test.mq"),
        "fuzzy" => Cow::Borrowed("fuzzy.mq"),
        _ => Cow::Owned(format!("{}.mq", name)),
    }
}

#[derive(Debug, Clone, Default)]
pub struct LocalFsModuleResolver {
    pub(crate) paths: Option<Vec<PathBuf>>,
}

impl ModuleResolver for LocalFsModuleResolver {
    fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        let file_path =
            search(module_name, &self.paths).map_err(|e| ModuleError::IOError(Cow::Owned(e.to_string())))?;
        fs::read_to_string(&file_path).map_err(|e| ModuleError::IOError(Cow::Owned(e.to_string())))
    }

    fn get_path(&self, module_name: &str) -> Result<String, ModuleError> {
        let file_path = search(module_name, &self.paths)?;
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
    pub fn new(paths: Option<Vec<PathBuf>>) -> Self {
        Self { paths }
    }
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

            PathBuf::from(path).join(module_name(name).as_ref())
        })
        .find(|p| p.is_file())
        .ok_or_else(|| ModuleError::NotFound(Cow::Owned(module_name(name).to_string())))
}
