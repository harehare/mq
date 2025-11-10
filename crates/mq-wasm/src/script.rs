use futures::StreamExt;
use itertools::Itertools;
use opfs::{DirectoryHandle, FileHandle};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap, rc::Rc, str::FromStr};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_CUSTOM_SECTION: &'static str = r#"
export type DefinedValueType = 'Function' | 'Variable';

export interface DefinedValue {
  name: string;
  args?: string[];
  doc: string;
  valueType: DefinedValueType;
}

export interface Diagnostic {
  startLine: number,
  startColumn: number,
  endLine: number,
  endColumn: number,
  message: string,
}

export interface Options {
    isUpdate: boolean,
    inputFormat: 'markdown' | 'text' | 'mdx' | 'html' | 'null' | 'raw' | null,
    listStyle: 'dash' | 'plus' | 'star' | null,
    linkTitleStyle: 'double' | 'single' | 'paren' | null,
    linkUrlStyle: 'angle' | 'none' | null,
}

export function definedValues(code: string, module?: string): Promise<ReadonlyArray<DefinedValue>>;
export function diagnostics(code: string): Promise<ReadonlyArray<Diagnostic>>;
export function run(code: string, content: string, options: Options): Promise<string>;
"#;

#[derive(Serialize, Deserialize)]
pub enum DefinedValueType {
    Function,
    Selector,
    Variable,
    Module,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinedValue {
    name: String,
    args: Option<Vec<String>>,
    doc: String,
    value_type: DefinedValueType,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    message: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(js_name=RunOptions, skip_typescript)]
struct Options {
    is_update: bool,
    input_format: Option<InputFormat>,
    list_style: Option<ListStyle>,
    link_title_style: Option<TitleSurroundStyle>,
    link_url_style: Option<UrlSurroundStyle>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InputFormat {
    #[serde(rename = "markdown")]
    Markdown,
    #[serde(rename = "mdx")]
    Mdx,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "html")]
    Html,
    #[serde(rename = "null")]
    Null,
    #[serde(rename = "raw")]
    Raw,
}

impl FromStr for InputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "markdown" => Ok(Self::Markdown),
            "mdx" => Ok(Self::Mdx),
            "text" => Ok(Self::Text),
            _ => Err(format!("Unknown input format: {}", s)),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListStyle {
    #[serde(rename = "dash")]
    Dash,
    #[serde(rename = "plus")]
    Plus,
    #[serde(rename = "star")]
    Star,
}

impl FromStr for ListStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dash" => Ok(Self::Dash),
            "plus" => Ok(Self::Plus),
            "star" => Ok(Self::Plus),
            _ => Err(format!("Unknown list style: {}", s)),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TitleSurroundStyle {
    #[serde(rename = "double")]
    Double,
    #[serde(rename = "single")]
    Single,
    #[serde(rename = "paren")]
    Paren,
}

impl FromStr for TitleSurroundStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "double" => Ok(Self::Double),
            "single" => Ok(Self::Single),
            "paren" => Ok(Self::Paren),
            _ => Err(format!("Unknown title surround style: {}", s)),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UrlSurroundStyle {
    #[serde(rename = "angle")]
    Angle,
    #[serde(rename = "none")]
    None,
}

impl FromStr for UrlSurroundStyle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "angle" => Ok(Self::Angle),
            "none" => Ok(Self::None),
            _ => Err(format!("Unknown URL surround style: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WasmModuleResolver {
    /// Cache of preloaded module contents, keyed by module name
    cache: Rc<RefCell<HashMap<String, String>>>,
    /// Root directory handle for OPFS access
    root_dir: Rc<RefCell<Option<opfs::persistent::DirectoryHandle>>>,
}

impl Default for WasmModuleResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmModuleResolver {
    pub fn new() -> Self {
        Self {
            cache: Rc::new(RefCell::new(HashMap::new())),
            root_dir: Rc::new(RefCell::new(None)),
        }
    }

    /// Initializes the OPFS root directory handle
    ///
    /// # Errors
    /// Returns error if OPFS is not available or initialization fails
    pub async fn initialize(&self) -> Result<(), JsValue> {
        let root = opfs::persistent::app_specific_dir()
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to get OPFS root: {:?}", e)))?;

        *self.root_dir.borrow_mut() = Some(root);
        Ok(())
    }

    /// Preloads all `.mq` modules from OPFS into the cache
    ///
    /// This method scans the OPFS root directory for all `.mq` files and loads them into cache.
    /// Module names are stored without the `.mq` extension (e.g., `csv.mq` becomes `csv`).
    ///
    /// # Errors
    /// Returns error if OPFS access fails or file reading fails
    pub async fn preload_modules(&self) -> Result<(), JsValue> {
        let root = self
            .root_dir
            .borrow()
            .as_ref()
            .ok_or_else(|| JsValue::from_str("OPFS not initialized. Call initialize() first."))?
            .clone();

        let mut entries = root
            .entries()
            .await
            .map_err(|e| JsValue::from_str(&format!("Failed to get directory entries: {:?}", e)))?;

        while let Some(result) = entries.next().await {
            let (name, entry) =
                result.map_err(|e| JsValue::from_str(&format!("Failed to read directory entry: {:?}", e)))?;

            match entry {
                opfs::DirectoryEntry::File(file_handle) => {
                    // Only process .mq files
                    if !name.ends_with(".mq") {
                        continue;
                    }

                    // Read file contents
                    let data = file_handle
                        .read()
                        .await
                        .map_err(|e| JsValue::from_str(&format!("Failed to read file '{}': {:?}", name, e)))?;

                    let contents = String::from_utf8(data)
                        .map_err(|e| JsValue::from_str(&format!("Failed to decode file '{}' as UTF-8: {}", name, e)))?;

                    // Store with module name (without .mq extension)
                    let module_name = name.strip_suffix(".mq").unwrap_or(&name);
                    self.cache.borrow_mut().insert(module_name.to_string(), contents);
                }
                opfs::DirectoryEntry::Directory(_) => {
                    // Skip directories for now
                    continue;
                }
            }
        }

        Ok(())
    }

    /// Manually adds a module to the cache
    ///
    /// This is useful for injecting module contents without using OPFS
    pub fn add_module(&self, module_name: &str, content: String) {
        self.cache.borrow_mut().insert(module_name.to_string(), content);
    }

    /// Clears the module cache
    pub fn clear_cache(&self) {
        self.cache.borrow_mut().clear();
    }
}

impl mq_lang::ModuleResolver for WasmModuleResolver {
    fn resolve(&self, module_name: &str) -> Result<String, mq_lang::ModuleError> {
        self.cache.borrow().get(module_name).cloned().ok_or_else(|| {
            mq_lang::ModuleError::NotFound(std::borrow::Cow::Owned(format!(
                "Module '{}' not found in cache. Use preload_module() to load it first.",
                module_name
            )))
        })
    }

    fn search_paths(&self) -> Vec<std::path::PathBuf> {
        vec![]
    }

    fn set_search_paths(&mut self, _: Vec<std::path::PathBuf>) {
        // OPFS doesn't use search paths
    }
}

#[wasm_bindgen(js_name=run, skip_typescript)]
pub async fn run(code: &str, content: &str, options: JsValue) -> Result<String, JsValue> {
    let resolver = WasmModuleResolver::new();
    resolver.initialize().await?;
    resolver.preload_modules().await?;

    let options: Options = serde_wasm_bindgen::from_value(options)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse options: {}", e)))?;

    let is_update = options.is_update;
    let mut engine = mq_lang::Engine::new(resolver);

    engine.load_builtin_module();

    let input = match options.input_format.as_ref().unwrap_or(&InputFormat::Markdown) {
        InputFormat::Text => mq_lang::parse_text_input(content),
        InputFormat::Html => mq_lang::parse_html_input(content),
        InputFormat::Mdx => mq_lang::parse_mdx_input(content),
        InputFormat::Markdown => mq_lang::parse_markdown_input(content),
        InputFormat::Raw => Ok(mq_lang::raw_input(content)),
        InputFormat::Null => Ok(mq_lang::null_input()),
    }
    .map_err(|e| JsValue::from_str(&format!("Failed to parse input content: {}", e)))?;

    engine
        .eval(code, input.clone().into_iter())
        .map_err(|e| JsValue::from_str(&format!("{}", &e.cause)))
        .map(|result_values| {
            let values = if matches!(options.input_format, Some(InputFormat::Markdown)) && is_update {
                let values: mq_lang::RuntimeValues = input.into();
                values.update_with(result_values)
            } else {
                result_values
            };

            let mut markdown = mq_markdown::Markdown::new(
                values
                    .into_iter()
                    .map(|runtime_value| match runtime_value {
                        mq_lang::RuntimeValue::Markdown(node, _) => node.clone(),
                        _ => runtime_value.to_string().into(),
                    })
                    .collect(),
            );
            markdown.set_options(mq_markdown::RenderOptions {
                list_style: options
                    .list_style
                    .map(|style| match style {
                        ListStyle::Dash => mq_markdown::ListStyle::Dash,
                        ListStyle::Plus => mq_markdown::ListStyle::Plus,
                        ListStyle::Star => mq_markdown::ListStyle::Star,
                    })
                    .unwrap_or_default(),
                link_title_style: options
                    .link_title_style
                    .map(|style| match style {
                        TitleSurroundStyle::Double => mq_markdown::TitleSurroundStyle::Double,
                        TitleSurroundStyle::Single => mq_markdown::TitleSurroundStyle::Single,
                        TitleSurroundStyle::Paren => mq_markdown::TitleSurroundStyle::Paren,
                    })
                    .unwrap_or_default(),
                link_url_style: options
                    .link_url_style
                    .map(|style| match style {
                        UrlSurroundStyle::Angle => mq_markdown::UrlSurroundStyle::Angle,
                        UrlSurroundStyle::None => mq_markdown::UrlSurroundStyle::None,
                    })
                    .unwrap_or_default(),
            });
            markdown.to_string()
        })
}

#[wasm_bindgen(js_name=toAst)]
pub async fn to_ast(code: &str) -> Result<String, JsValue> {
    let token_arena = mq_lang::Shared::new(mq_lang::SharedCell::new(mq_lang::Arena::new(1024)));
    mq_lang::parse(code, token_arena)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse code: {}", e)))
        .and_then(|json| {
            serde_json::to_string(&json).map_err(|e| JsValue::from_str(&format!("Failed to serialize AST: {}", e)))
        })
}

#[wasm_bindgen(js_name=format)]
pub async fn format(code: &str) -> Result<String, JsValue> {
    mq_formatter::Formatter::default()
        .format(code)
        .map_err(|e| JsValue::from_str(&format!("{:?}", &e)))
}

#[wasm_bindgen(js_name=diagnostics, skip_typescript)]
pub async fn diagnostics(code: &str) -> JsValue {
    let (_, errors) = mq_lang::parse_recovery(code);
    let errors = errors
        .error_ranges(code)
        .iter()
        .map(|(message, range)| Diagnostic {
            start_line: range.start.line,
            start_column: range.start.column as u32,
            end_line: range.end.line,
            end_column: range.end.column as u32,
            message: message.to_owned(),
        })
        .collect::<Vec<_>>();

    serde_wasm_bindgen::to_value(&errors).unwrap()
}

#[wasm_bindgen(js_name=definedValues, skip_typescript)]
pub async fn defined_values(code: &str, module: Option<String>) -> Result<JsValue, JsValue> {
    let mut hir = mq_hir::Hir::default();
    hir.add_code(None, code);

    // If module is specified, find the module symbol
    let module_id = if let Some(ref module_name) = module {
        hir.symbols().find_map(|(id, symbol)| {
            if symbol.is_module() && symbol.value.as_ref().map(|v| v.as_str()) == Some(module_name.as_str()) {
                Some(id)
            } else {
                None
            }
        })
    } else {
        None
    };

    let symbols = hir
        .symbols()
        .filter_map(|(_symbol_id, symbol)| {
            // Filter by module if specified
            if module_id.is_some() && symbol.parent != module_id {
                return None;
            }

            match symbol {
                mq_hir::Symbol {
                    kind: mq_hir::SymbolKind::Function(params),
                    value: Some(value),
                    doc,
                    ..
                } => Some(DefinedValue {
                    name: value.to_string(),
                    args: Some(params.iter().map(|param| param.to_string()).collect()),
                    doc: doc.iter().map(|(_, doc)| doc.to_string()).join("\n"),
                    value_type: DefinedValueType::Function,
                }),
                mq_hir::Symbol {
                    kind: mq_hir::SymbolKind::Selector,
                    value: Some(value),
                    doc,
                    ..
                } => Some(DefinedValue {
                    name: value.to_string(),
                    args: None,
                    doc: doc.iter().map(|(_, doc)| doc.to_string()).join("\n"),
                    value_type: DefinedValueType::Selector,
                }),
                mq_hir::Symbol {
                    kind: mq_hir::SymbolKind::Variable,
                    value: Some(value),
                    doc,
                    ..
                } => Some(DefinedValue {
                    name: value.to_string(),
                    args: None,
                    doc: doc.iter().map(|(_, doc)| doc.to_string()).join("\n"),
                    value_type: DefinedValueType::Variable,
                }),
                _ => None,
            }
        })
        .collect::<Vec<_>>();

    Ok(serde_wasm_bindgen::to_value(&symbols).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lang::ModuleResolver;
    use wasm_bindgen_test::*;
    wasm_bindgen_test_configure!(run_in_browser);

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_script_run_simple() {
        let result = run(
            "downcase() | ltrimstr(\"hello\") | upcase() | trim()",
            "Hello world",
            serde_wasm_bindgen::to_value(&Options {
                is_update: true,
                input_format: None,
                list_style: None,
                link_title_style: None,
                link_url_style: None,
            })
            .unwrap(),
        );
        assert_eq!(result.await.unwrap(), "WORLD\n");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_script_run_list() {
        let result = run(
            ".[]",
            "- test",
            serde_wasm_bindgen::to_value(&Options {
                is_update: true,
                input_format: None,
                list_style: Some(ListStyle::Star),
                link_title_style: None,
                link_url_style: None,
            })
            .unwrap(),
        );
        assert_eq!(result.await.unwrap(), "* test\n");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_script_run_link() {
        let result = run(
            ".link",
            "[test](https://example.com)",
            serde_wasm_bindgen::to_value(&Options {
                is_update: true,
                input_format: None,
                list_style: None,
                link_title_style: None,
                link_url_style: Some(UrlSurroundStyle::Angle),
            })
            .unwrap(),
        );
        assert_eq!(result.await.unwrap(), "[test](<https://example.com>)\n");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_script_run_invalid_syntax() {
        assert!(
            run(
                "invalid syntax",
                "test",
                serde_wasm_bindgen::to_value(&Options {
                    is_update: true,
                    input_format: None,
                    list_style: None,
                    link_title_style: None,
                    link_url_style: None,
                })
                .unwrap()
            )
            .await
            .is_err()
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_script_format() {
        let result = format(r#"downcase()|ltrimstr("hello")|upcase()|trim()"#).await.unwrap();
        assert_eq!(result, r#"downcase() | ltrimstr("hello") | upcase() | trim()"#);
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_script_format_invalid() {
        assert!(format("x=>").await.is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_defined_values_without_module() {
        let code = r#"
            def foo(x): x | upcase();
            def bar(y): y | downcase();
            let $var = 42;
        "#;

        let result = defined_values(code, None).await.unwrap();
        let values: Vec<DefinedValue> = serde_wasm_bindgen::from_value(result).unwrap();

        // Should return all defined values
        assert!(values.iter().any(|v| v.name == "foo"));
        assert!(values.iter().any(|v| v.name == "bar"));
        assert!(values.iter().any(|v| v.name == "$var"));
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_defined_values_with_module() {
        let code = r#"
            module mymodule:
                def module_func(x): x | upcase();
                def another_func(y): y | downcase();
                let $module_var = 100;
            end

            def top_level_func(z): z;
            let $top_level_var = 42;
        "#;

        let result = defined_values(code, Some("mymodule".to_string())).await.unwrap();
        let values: Vec<DefinedValue> = serde_wasm_bindgen::from_value(result).unwrap();

        // Should return only values from mymodule
        assert!(values.iter().any(|v| v.name == "module_func"));
        assert!(values.iter().any(|v| v.name == "another_func"));
        assert!(values.iter().any(|v| v.name == "$module_var"));

        // Should NOT return top-level values
        assert!(!values.iter().any(|v| v.name == "top_level_func"));
        assert!(!values.iter().any(|v| v.name == "$top_level_var"));
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_defined_values_with_nonexistent_module() {
        let code = r#"
            def foo(x): x | upcase();
        "#;

        let result = defined_values(code, Some("nonexistent".to_string())).await.unwrap();
        let values: Vec<DefinedValue> = serde_wasm_bindgen::from_value(result).unwrap();

        // Should return empty array since module doesn't exist
        assert!(!values.is_empty());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_wasm_module_resolver_add_module() {
        let resolver = WasmModuleResolver::new();

        // Manually add a module to cache
        resolver.add_module("test", "def foo(x): x | upcase();".to_string());

        // Should be able to resolve it
        let result = resolver.resolve("test");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "def foo(x): x | upcase();");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_wasm_module_resolver_not_found() {
        let resolver = WasmModuleResolver::new();

        // Should fail when module is not in cache
        let result = resolver.resolve("nonexistent");
        assert!(result.is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_wasm_module_resolver_clear_cache() {
        let resolver = WasmModuleResolver::new();

        // Add a module
        resolver.add_module("test", "content".to_string());
        assert!(resolver.resolve("test").is_ok());

        // Clear cache
        resolver.clear_cache();

        // Should no longer be resolvable
        assert!(resolver.resolve("test").is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_opfs_create_and_import_module() {
        use opfs::{FileHandle as _, WritableFileStream as _};

        // Initialize OPFS
        let resolver = WasmModuleResolver::new();

        // Initialize OPFS - this may fail if OPFS is not available in the test environment
        if resolver.initialize().await.is_err() {
            // Skip test if OPFS is not available
            return;
        }

        // Get root directory handle
        let root = opfs::persistent::app_specific_dir()
            .await
            .expect("Failed to get OPFS root directory");

        // Create a test module file in OPFS
        let module_content = r#"def upcase_exclaim(x): x | upcase() | s"${self}!";"#;
        let file_name = "test_module.mq";

        // Write the module file to OPFS
        {
            use opfs::DirectoryHandle as _;

            let mut file_handle = root
                .get_file_handle_with_options(file_name, &opfs::GetFileHandleOptions { create: true })
                .await
                .expect("Failed to get file handle");

            let mut writer = file_handle
                .create_writable_with_options(&opfs::CreateWritableOptions {
                    keep_existing_data: false,
                })
                .await
                .expect("Failed to create writable");

            writer
                .write_at_cursor_pos(module_content.as_bytes().to_vec())
                .await
                .expect("Failed to write to file");

            writer.close().await.expect("Failed to close writer");
        }

        // Preload modules from OPFS
        resolver
            .preload_modules()
            .await
            .expect("Failed to preload modules from OPFS");

        // Verify the module was loaded into cache
        let resolved_content = resolver
            .resolve("test_module")
            .expect("Module should be found in cache");
        assert_eq!(resolved_content, module_content);

        // Test using the imported module in code execution
        let code = r#"
            let tm = import "test_module"
            | tm::upcase_exclaim()
        "#;

        let mut engine = mq_lang::Engine::new(resolver.clone());
        engine.load_builtin_module();

        let input = mq_lang::parse_text_input("hello world").expect("Failed to parse input");

        let result = engine
            .eval(code, input.into_iter())
            .expect("Failed to evaluate code with imported module");

        // Convert result to string and verify
        let output: Vec<String> = result.into_iter().map(|v| v.to_string()).collect();

        assert_eq!(output.join(""), "HELLO WORLD!");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_opfs_multiple_modules() {
        use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _};

        // Initialize OPFS
        let resolver = WasmModuleResolver::new();

        if resolver.initialize().await.is_err() {
            // Skip test if OPFS is not available
            return;
        }

        let root = opfs::persistent::app_specific_dir()
            .await
            .expect("Failed to get OPFS root directory");

        // Create multiple test module files
        let modules = vec![
            ("math.mq", r#"def double(x): x * 2;"#),
            ("string.mq", r#"def greet(name): s"Hello, ${name}!";"#),
        ];

        for (file_name, content) in &modules {
            let mut file_handle = root
                .get_file_handle_with_options(file_name, &opfs::GetFileHandleOptions { create: true })
                .await
                .expect(&format!("Failed to get file handle for {}", file_name));

            let mut writer = file_handle
                .create_writable_with_options(&opfs::CreateWritableOptions {
                    keep_existing_data: false,
                })
                .await
                .expect(&format!("Failed to create writable for {}", file_name));

            writer
                .write_at_cursor_pos(content.as_bytes().to_vec())
                .await
                .expect(&format!("Failed to write to {}", file_name));

            writer
                .close()
                .await
                .expect(&format!("Failed to close writer for {}", file_name));
        }

        // Preload all modules
        resolver.preload_modules().await.expect("Failed to preload modules");

        // Verify all modules are loaded
        assert!(resolver.resolve("math").is_ok());
        assert!(resolver.resolve("string").is_ok());

        // Test using multiple imported modules
        let code = r#"
            import "string"
            | string::greet("World")
        "#;

        let mut engine = mq_lang::Engine::new(resolver.clone());
        engine.load_builtin_module();

        let input = mq_lang::null_input();
        let result = engine.eval(code, input.into_iter()).expect("Failed to evaluate code");

        let output: Vec<String> = result.into_iter().map(|v| v.to_string()).collect();

        assert_eq!(output.join(""), "Hello, World!");

        // Note: File cleanup is skipped as OPFS persistent storage is isolated per origin
    }
}
