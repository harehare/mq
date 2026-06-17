use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap, rc::Rc, str::FromStr};
use wasm_bindgen::prelude::*;

#[cfg(feature = "opfs")]
use opfs::DirectoryHandle;

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

export interface InlayHint {
  line: number,
  column: number,
  label: string,
}

export interface HoverResult {
  content: string;
}

export interface Options {
    isUpdate: boolean,
    inputFormat: 'markdown' | 'text' | 'mdx' | 'html' | 'null' | 'raw' | null,
    listStyle: 'dash' | 'plus' | 'star' | null,
    linkTitleStyle: 'double' | 'single' | 'paren' | null,
    linkUrlStyle: 'angle' | 'none' | null,
    /** Domains permitted for HTTP module imports in addition to github.com/harehare (always allowed). */
    allowedDomains?: string[],
}

export function definedValues(code: string, module?: string): Promise<ReadonlyArray<DefinedValue>>;
export function diagnostics(code: string, enableTypeCheck?: boolean): Promise<ReadonlyArray<Diagnostic>>;
export function hover(code: string, line: number, column: number): Promise<HoverResult | null>;
export function inlayHints(code: string): Promise<ReadonlyArray<InlayHint>>;
export function run(code: string, content: string, options: Options): Promise<string>;
/** Clears mutable HTTP module cache (HEAD/branch imports). Versioned (tagged) cache is preserved. */
export function clearHttpCache(): Promise<void>;
/** Clears all HTTP module cache including versioned (tagged) imports. */
export function clearAllHttpCache(): Promise<void>;
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
pub struct InlayHint {
    line: u32,
    column: u32,
    label: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverResult {
    content: String,
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
    allowed_domains: Option<Vec<String>>,
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

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[wasm_bindgen]
#[serde(rename_all = "camelCase")]
pub struct ConversionOptions {
    pub extract_scripts_as_code_blocks: bool,
    pub generate_front_matter: bool,
    pub use_title_as_h1: bool,
}

impl From<ConversionOptions> for mq_markdown::ConversionOptions {
    fn from(options: ConversionOptions) -> Self {
        Self {
            extract_scripts_as_code_blocks: options.extract_scripts_as_code_blocks,
            generate_front_matter: options.generate_front_matter,
            use_title_as_h1: options.use_title_as_h1,
        }
    }
}

/// Sync HTTP fetcher for WASM that reads from a pre-populated in-memory cache.
///
/// Content is inserted by [`WasmFetcher::preload_url`] (async, keyed by raw HTTPS URL)
/// and then read synchronously by [`mq_lang::ModuleResolver::resolve`].
///
/// When the `opfs` feature is enabled and an OPFS root handle is set, `preload_url` also
/// maintains a persistent on-disk cache with SHA-256 sidecar files for tamper detection.
/// HTTP imports are blocked entirely when OPFS is compiled in but unavailable at runtime.
#[derive(Debug, Clone, Default)]
struct WasmFetcher {
    /// Keyed by the normalized raw HTTPS URL (e.g. `https://raw.githubusercontent.com/...`).
    cache: Rc<RefCell<HashMap<String, String>>>,
    #[cfg(feature = "opfs")]
    /// OPFS root handle shared with `WasmModuleResolver`. `None` means OPFS is unavailable.
    root_dir: Rc<RefCell<Option<opfs::persistent::DirectoryHandle>>>,
}

impl WasmFetcher {
    #[cfg(feature = "opfs")]
    fn is_opfs_available(&self) -> bool {
        self.root_dir.borrow().is_some()
    }

    /// Ensures `fetch_url` is in the in-memory cache, using OPFS as a persistent backing store.
    ///
    /// - If the URL is already in the memory cache, returns immediately.
    /// - Otherwise checks the OPFS `http_cache/{subdir}` directory for a cached copy with a valid
    ///   SHA-256 sidecar.  On a hit the content is loaded into memory.
    /// - On an OPFS miss the URL is fetched from the network, written to OPFS, then cached in memory.
    #[cfg(feature = "opfs")]
    async fn preload_url(&self, fetch_url: &str) {
        if self.cache.borrow().contains_key(fetch_url) {
            return;
        }

        let root = self.root_dir.borrow().clone();
        let Some(ref r) = root else { return };

        let subdir = if mq_lang::http_import::is_versioned_url(fetch_url) {
            "versioned"
        } else {
            "mutable"
        };
        let stem = cache_file_stem(fetch_url);

        if let Some(content) = try_read_opfs_http_cache(r, subdir, &stem).await {
            self.cache.borrow_mut().insert(fetch_url.to_string(), content);
            return;
        }

        if let Ok(content) = fetch_text(fetch_url).await {
            write_opfs_http_cache(r, subdir, &stem, &content).await;
            self.cache.borrow_mut().insert(fetch_url.to_string(), content);
        }
    }
}

impl mq_lang::HttpFetcher for WasmFetcher {
    fn fetch(&self, url: &str) -> Result<String, mq_lang::ModuleError> {
        self.cache
            .borrow()
            .get(url)
            .cloned()
            .ok_or_else(|| mq_lang::ModuleError::NotFound(std::borrow::Cow::Owned(url.to_string())))
    }
}

#[derive(Debug, Clone)]
pub struct WasmModuleResolver {
    /// HTTP resolver: handles URL normalization, domain allow-list, and delegates fetch to WasmFetcher.
    http_resolver: Rc<RefCell<mq_lang::HttpModuleResolver<WasmFetcher>>>,
    /// Direct handle to the WasmFetcher; shares the same Rc data as the clone held by `http_resolver`.
    fetcher: WasmFetcher,
    #[cfg(feature = "opfs")]
    /// Cache of preloaded local `.mq` module contents (from OPFS), keyed by module name.
    cache: Rc<RefCell<HashMap<String, String>>>,
    #[cfg(feature = "opfs")]
    /// OPFS root handle shared with `fetcher.root_dir`.
    root_dir: Rc<RefCell<Option<opfs::persistent::DirectoryHandle>>>,
    #[cfg(feature = "opfs")]
    /// Whether OPFS was successfully initialized.
    is_available: Rc<RefCell<bool>>,
}

impl Default for WasmModuleResolver {
    fn default() -> Self {
        #[cfg(feature = "opfs")]
        let root_dir: Rc<RefCell<Option<opfs::persistent::DirectoryHandle>>> = Rc::new(RefCell::new(None));

        let fetcher = WasmFetcher {
            cache: Rc::new(RefCell::new(HashMap::new())),
            #[cfg(feature = "opfs")]
            root_dir: Rc::clone(&root_dir),
        };
        let http_resolver = mq_lang::HttpModuleResolver::new(vec![], fetcher.clone());

        Self {
            http_resolver: Rc::new(RefCell::new(http_resolver)),
            fetcher,
            #[cfg(feature = "opfs")]
            cache: Rc::new(RefCell::new(HashMap::new())),
            #[cfg(feature = "opfs")]
            root_dir,
            #[cfg(feature = "opfs")]
            is_available: Rc::new(RefCell::new(false)),
        }
    }
}

impl WasmModuleResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the list of additional allowed domains for HTTP imports.
    ///
    /// `github.com/{path}` entries are automatically expanded to `raw.githubusercontent.com/{path}`.
    /// `DEFAULT_ALLOWED_DOMAIN` (`raw.githubusercontent.com/harehare`) is always permitted
    /// regardless of this list.
    pub fn set_allowed_domains(&self, domains: Vec<String>) {
        self.http_resolver.borrow_mut().set_allowed_domains(domains);
    }

    /// Initializes the OPFS root directory handle
    ///
    /// If OPFS is not available, this method will silently fail and the resolver
    /// will operate as a NoOp resolver (only using manually added modules via `add_module`).
    pub async fn initialize(&self) {
        #[cfg(feature = "opfs")]
        match opfs::persistent::app_specific_dir().await {
            Ok(root) => {
                *self.root_dir.borrow_mut() = Some(root);
                *self.is_available.borrow_mut() = true;
            }
            Err(_) => {
                // OPFS is not available, resolver will work as NoOp
                *self.is_available.borrow_mut() = false;
            }
        }
    }

    /// Loads and caches only the `.mq` modules that `code` actually imports (directly or
    /// transitively through other local modules).
    ///
    /// Only modules reachable from the imports in `code` are read from OPFS, so queries that
    /// use a small subset of the available modules pay only for what they need. Cycles
    /// (e.g. A imports B, B imports A) are handled safely via a visited set.
    ///
    /// If OPFS is not available, this method returns immediately without error.
    pub async fn preload_modules(&self, code: &str) {
        #[cfg(feature = "opfs")]
        {
            if !*self.is_available.borrow() {
                return;
            }

            let root = match self.root_dir.borrow().clone() {
                Some(r) => r,
                None => return,
            };

            let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut queue: std::collections::VecDeque<String> = extract_local_import_names(code).into_iter().collect();

            while let Some(name) = queue.pop_front() {
                if visited.contains(&name) {
                    continue;
                }
                visited.insert(name.clone());

                let content = if let Some(cached) = self.cache.borrow().get(&name).cloned() {
                    cached
                } else if let Some(c) = self.load_module_from_opfs(&name, &root).await {
                    self.cache.borrow_mut().insert(name.clone(), c.clone());
                    c
                } else {
                    continue;
                };

                for dep in extract_local_import_names(&content) {
                    if !visited.contains(&dep) {
                        queue.push_back(dep);
                    }
                }
            }
        }
    }

    /// Reads `{name}.mq` from the OPFS root and returns its content, or `None` on any error.
    #[cfg(feature = "opfs")]
    async fn load_module_from_opfs(&self, name: &str, root: &opfs::persistent::DirectoryHandle) -> Option<String> {
        use opfs::{DirectoryHandle as _, FileHandle as _};
        let file_handle = root
            .get_file_handle_with_options(&format!("{}.mq", name), &opfs::GetFileHandleOptions { create: false })
            .await
            .ok()?;
        let data = file_handle.read().await.ok()?;
        String::from_utf8(data).ok()
    }

    /// Manually adds a module to the cache
    ///
    /// This is useful for injecting module contents without using OPFS
    pub fn add_module(&self, _module_name: &str, _content: String) {
        #[cfg(feature = "opfs")]
        self.cache.borrow_mut().insert(_module_name.to_string(), _content);
    }

    /// Clears the module cache
    pub fn clear_cache(&self) {
        #[cfg(feature = "opfs")]
        self.cache.borrow_mut().clear();
    }

    /// Pre-fetches HTTP/GitHub import URLs found directly in `code` (top-level only).
    ///
    /// Only imports written in the user's own code are resolved; HTTP imports inside
    /// fetched modules are intentionally ignored.
    ///
    /// When the `opfs` feature is enabled, HTTP imports require OPFS to be available —
    /// this method returns immediately (without fetching) if OPFS is unavailable.
    /// Versioned URLs (`@v1.0`) are persisted in `http_cache/versioned/` and mutable URLs
    /// in `http_cache/mutable/`, each with a SHA-256 sidecar for tamper detection.
    pub async fn preload_http_modules(&self, code: &str) {
        // HTTP import requires OPFS for caching; skip if OPFS is unavailable.
        #[cfg(feature = "opfs")]
        if !self.fetcher.is_opfs_available() {
            return;
        }

        for module_path in extract_http_import_urls(code) {
            let fetch_url = if mq_lang::http_import::is_github_url(&module_path) {
                match mq_lang::http_import::github_to_raw_url(&module_path) {
                    Some(u) => u,
                    None => continue,
                }
            } else if mq_lang::http_import::is_remote_url(&module_path) {
                module_path.clone()
            } else {
                continue;
            };

            if !self.http_resolver.borrow().is_allowed_domain(&fetch_url) {
                continue;
            }

            #[cfg(feature = "opfs")]
            self.fetcher.preload_url(&fetch_url).await;

            #[cfg(not(feature = "opfs"))]
            {
                if self.fetcher.cache.borrow().contains_key(&fetch_url) {
                    continue;
                }
                if let Ok(content) = fetch_text(&fetch_url).await {
                    self.fetcher.cache.borrow_mut().insert(fetch_url, content);
                }
            }
        }
    }
}

impl mq_lang::ModuleResolver for WasmModuleResolver {
    fn canonical_name<'a>(&self, module_path: &'a str) -> &'a str {
        if mq_lang::http_import::is_github_url(module_path) || mq_lang::http_import::is_remote_url(module_path) {
            mq_lang::http_import::extract_module_name(module_path)
        } else {
            module_path
        }
    }

    fn resolve(&self, module_name: &str) -> Result<String, mq_lang::ModuleError> {
        if let Some(content_fn) = mq_lang::STANDARD_MODULES.get(module_name) {
            return Ok(content_fn().to_string());
        }

        #[cfg(feature = "opfs")]
        if let Some(content) = self.cache.borrow().get(module_name) {
            return Ok(content.clone());
        }

        let is_http =
            mq_lang::http_import::is_remote_url(module_name) || mq_lang::http_import::is_github_url(module_name);

        if is_http {
            #[cfg(feature = "opfs")]
            if !self.fetcher.is_opfs_available() {
                return Err(mq_lang::ModuleError::IOError(std::borrow::Cow::Owned(format!(
                    "HTTP import of '{}' is not available: OPFS is not supported in this environment.",
                    module_name
                ))));
            }
            return self.http_resolver.borrow().resolve(module_name);
        }

        #[cfg(feature = "opfs")]
        return Err(mq_lang::ModuleError::NotFound(std::borrow::Cow::Owned(format!(
            "Module '{}' not found in cache. Use preload_modules() to load it first.",
            module_name
        ))));
        #[cfg(not(feature = "opfs"))]
        Err(mq_lang::ModuleError::NotFound(std::borrow::Cow::Owned(format!(
            "Module '{}' not found. Module resolution is not supported in this environment.",
            module_name
        ))))
    }

    fn get_path(&self, module_name: &str) -> Result<String, mq_lang::ModuleError> {
        match self.http_resolver.borrow().get_path(module_name) {
            Ok(path) => Ok(path),
            Err(_) => Ok(module_name.to_string()),
        }
    }

    fn search_paths(&self) -> Vec<std::path::PathBuf> {
        vec![]
    }

    fn set_search_paths(&mut self, _: Vec<std::path::PathBuf>) {
        // OPFS doesn't use search paths
    }
}

/// Removes mutable HTTP module cache (HEAD/branch imports).
/// Versioned (tagged) cached modules are preserved, matching `--refresh-modules` CLI behaviour.
#[wasm_bindgen(js_name=clearHttpCache)]
pub async fn clear_http_cache() -> Result<(), JsValue> {
    #[cfg(feature = "opfs")]
    {
        use opfs::DirectoryHandle as _;

        let root = opfs::persistent::app_specific_dir()
            .await
            .map_err(|e| JsValue::from_str(&format!("OPFS unavailable: {:?}", e)))?;

        if let Ok(mut cache_dir) = root
            .get_directory_handle_with_options(HTTP_CACHE_DIR, &opfs::GetDirectoryHandleOptions { create: false })
            .await
        {
            let _ = cache_dir
                .remove_entry_with_options("mutable", &opfs::FileSystemRemoveOptions { recursive: true })
                .await;
        }
    }
    Ok(())
}

/// Removes all HTTP module cache including versioned (tagged) imports, matching `--clear-cache` CLI behaviour.
#[wasm_bindgen(js_name=clearAllHttpCache)]
pub async fn clear_all_http_cache() -> Result<(), JsValue> {
    #[cfg(feature = "opfs")]
    {
        use opfs::DirectoryHandle as _;

        let mut root = opfs::persistent::app_specific_dir()
            .await
            .map_err(|e| JsValue::from_str(&format!("OPFS unavailable: {:?}", e)))?;

        let _ = root
            .remove_entry_with_options(HTTP_CACHE_DIR, &opfs::FileSystemRemoveOptions { recursive: true })
            .await;
    }
    Ok(())
}

#[wasm_bindgen(js_name=run, skip_typescript)]
pub async fn run(code: &str, content: &str, options: JsValue) -> Result<String, JsValue> {
    let options: Options = serde_wasm_bindgen::from_value(options)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse options: {}", e)))?;

    let resolver = WasmModuleResolver::new();
    resolver.initialize().await;
    if let Some(ref domains) = options.allowed_domains {
        resolver.set_allowed_domains(domains.clone());
    }
    resolver.preload_modules(code).await;
    resolver.preload_http_modules(code).await;

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
        .map_err(|e| JsValue::from_str(&format!("{}", &e)))
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
                        mq_lang::RuntimeValue::Markdown(node, _) => *node,
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

#[wasm_bindgen(js_name=htmlToMarkdown)]
pub async fn html_to_markdown(html_input: &str, options: Option<ConversionOptions>) -> Result<String, JsValue> {
    mq_markdown::convert_html_to_markdown(html_input, options.map(Into::into).unwrap_or_default())
        .map_err(|e| JsValue::from_str(&format!("Failed to convert HTML to Markdown: {}", e)))
}

#[wasm_bindgen(js_name=toHtml)]
pub async fn to_html(markdown_input: &str) -> String {
    mq_markdown::to_html(markdown_input)
}

#[wasm_bindgen(js_name=diagnostics, skip_typescript)]
pub async fn diagnostics(code: &str, enable_type_check: Option<bool>) -> JsValue {
    let (_, errors) = mq_lang::parse_recovery(code);
    let mut errors = errors
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

    if enable_type_check.unwrap_or(false) && errors.is_empty() {
        let mut hir = mq_hir::Hir::default();
        hir.add_code(None, code);

        let mut checker = mq_check::TypeChecker::default();
        let type_errors = checker.check(&hir);

        for error in type_errors {
            if let Some(range) = error.location() {
                errors.push(Diagnostic {
                    start_line: range.start.line,
                    start_column: range.start.column as u32,
                    end_line: range.end.line,
                    end_column: range.end.column as u32,
                    message: error.to_string(),
                });
            }
        }
    }

    serde_wasm_bindgen::to_value(&errors).unwrap()
}

#[wasm_bindgen(js_name=inlayHints, skip_typescript)]
pub async fn inlay_hints(code: &str) -> JsValue {
    let mut hir = mq_hir::Hir::default();
    hir.add_code(None, code);

    let mut checker = mq_check::TypeChecker::default();
    let _ = checker.check(&hir);

    let symbol_types = checker.symbol_types();

    let hints: Vec<InlayHint> = hir
        .symbols()
        .filter_map(|(symbol_id, symbol)| {
            if hir.is_builtin_symbol(symbol) {
                return None;
            }

            let show = matches!(
                symbol.kind,
                mq_hir::SymbolKind::Variable
                    | mq_hir::SymbolKind::Function(_)
                    | mq_hir::SymbolKind::DestructuringBinding
                    | mq_hir::SymbolKind::PatternVariable { .. }
            );
            if !show {
                return None;
            }

            let range = symbol.source.text_range.as_ref()?;
            let type_scheme = symbol_types.get(&symbol_id)?;

            if !type_scheme.ty.is_concrete() {
                return None;
            }

            Some(InlayHint {
                line: range.end.line,
                column: range.end.column as u32,
                label: format!(": {}", type_scheme),
            })
        })
        .collect();

    serde_wasm_bindgen::to_value(&hints).unwrap_or_else(|_| JsValue::from(js_sys::Array::new()))
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
                    kind: mq_hir::SymbolKind::Selector(_),
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

/// Returns `true` if a doc line is a deprecation marker.
fn is_deprecated_marker(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    lower == "deprecated" || lower.starts_with("deprecated:") || lower.starts_with("deprecated ")
}

/// Extracts the human-readable message from a deprecation marker line, if any.
fn extract_deprecated_message(text: &str) -> Option<String> {
    let after_colon = text.trim().split_once(':')?.1.trim();
    if after_colon.is_empty() {
        None
    } else {
        Some(after_colon.to_string())
    }
}

/// Builds a Markdown hover string from a pre-formatted signature, doc comments, and
/// deprecation status.
fn format_hover_content(signature: &str, docs: &[mq_hir::Doc], deprecated: bool) -> String {
    let mut sections: Vec<String> = Vec::new();

    sections.push(format!("```mq\n{}\n```", signature));

    if deprecated {
        let dep_msg = docs
            .iter()
            .find(|(_, text)| is_deprecated_marker(text))
            .and_then(|(_, text)| extract_deprecated_message(text));

        match dep_msg {
            Some(msg) => sections.push(format!("> ⚠️ **Deprecated**: {}", msg)),
            None => sections.push("> ⚠️ **Deprecated**".to_string()),
        }
    }

    let doc_text = docs
        .iter()
        .filter(|(_, text)| !is_deprecated_marker(text))
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    if !doc_text.trim().is_empty() {
        sections.push("---".to_string());
        sections.push(doc_text);
    }

    sections.join("\n\n")
}

/// Returns hover information (Markdown content) for the symbol at `line`/`column` (1-based).
///
/// Returns `null` if no symbol is found at the given position.
#[wasm_bindgen(js_name=hover, skip_typescript)]
pub async fn hover(code: &str, line: u32, column: u32) -> JsValue {
    let mut hir = mq_hir::Hir::default();
    let (source_id, _) = hir.add_code(None, code);

    let mut checker = mq_check::TypeChecker::default();
    let _ = checker.check(&hir);
    let type_env = checker.symbol_types();

    let pos = mq_lang::Position::new(line, column as usize);
    let Some((symbol_id, symbol)) = hir.find_symbol_in_position(source_id, pos) else {
        return JsValue::NULL;
    };

    let deprecated = symbol.is_deprecated();
    let type_scheme = type_env.get(&symbol_id);

    let signature = match &symbol.kind {
        mq_hir::SymbolKind::Function(args) | mq_hir::SymbolKind::Macro(args) => {
            let type_annotation = type_scheme.map(|s| format!(": {}", s.ty)).unwrap_or_default();
            format!(
                "{}({}){}",
                symbol.value.as_deref().unwrap_or_default(),
                args.iter().map(|p| p.to_string()).join(", "),
                type_annotation
            )
        }
        mq_hir::SymbolKind::Variable
        | mq_hir::SymbolKind::DestructuringBinding
        | mq_hir::SymbolKind::PatternVariable { .. } => {
            let type_annotation = type_scheme.map(|s| format!(": {}", s.ty)).unwrap_or_default();
            format!("{}{}", symbol.value.as_deref().unwrap_or_default(), type_annotation)
        }
        _ => return JsValue::NULL,
    };

    let result = HoverResult {
        content: format_hover_content(&signature, &symbol.doc, deprecated),
    };
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}

/// Name of the OPFS subdirectory used to store cached HTTP modules.
const HTTP_CACHE_DIR: &str = "http_cache";

/// Returns the MD5 hex string of `url`, used as the cache file stem.
fn cache_file_stem(url: &str) -> String {
    format!("{:x}", md5::compute(url))
}

/// Computes SHA-256 of `content` as a lowercase hex string.
fn compute_content_hash(content: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(content.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// Tries to read a cached module from OPFS. Returns `None` on any error or hash mismatch.
#[cfg(feature = "opfs")]
async fn try_read_opfs_http_cache(
    root: &opfs::persistent::DirectoryHandle,
    subdir: &str,
    stem: &str,
) -> Option<String> {
    use opfs::{DirectoryHandle as _, FileHandle as _};

    let cache_dir = root
        .get_directory_handle_with_options(HTTP_CACHE_DIR, &opfs::GetDirectoryHandleOptions { create: false })
        .await
        .ok()?;
    let sub = cache_dir
        .get_directory_handle_with_options(subdir, &opfs::GetDirectoryHandleOptions { create: false })
        .await
        .ok()?;

    let content_fh = sub
        .get_file_handle_with_options(&format!("{}.mq", stem), &opfs::GetFileHandleOptions { create: false })
        .await
        .ok()?;
    let hash_fh = sub
        .get_file_handle_with_options(
            &format!("{}.mq.sha256", stem),
            &opfs::GetFileHandleOptions { create: false },
        )
        .await
        .ok()?;

    let content = String::from_utf8(content_fh.read().await.ok()?).ok()?;
    let stored = String::from_utf8(hash_fh.read().await.ok()?).ok()?;

    if stored.trim() == compute_content_hash(&content) {
        Some(content)
    } else {
        None
    }
}

/// Writes `content` and its SHA-256 sidecar to the OPFS HTTP cache. Silently ignores errors.
#[cfg(feature = "opfs")]
async fn write_opfs_http_cache(root: &opfs::persistent::DirectoryHandle, subdir: &str, stem: &str, content: &str) {
    async fn write_file(dir: &opfs::persistent::DirectoryHandle, name: &str, data: &[u8]) -> Option<()> {
        use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _};
        let mut fh = dir
            .get_file_handle_with_options(name, &opfs::GetFileHandleOptions { create: true })
            .await
            .ok()?;
        let mut w = fh
            .create_writable_with_options(&opfs::CreateWritableOptions {
                keep_existing_data: false,
            })
            .await
            .ok()?;
        w.write_at_cursor_pos(data).await.ok()?;
        w.close().await.ok()
    }

    let Ok(cache_dir) = root
        .get_directory_handle_with_options(HTTP_CACHE_DIR, &opfs::GetDirectoryHandleOptions { create: true })
        .await
    else {
        return;
    };
    let Ok(sub) = cache_dir
        .get_directory_handle_with_options(subdir, &opfs::GetDirectoryHandleOptions { create: true })
        .await
    else {
        return;
    };

    let _ = write_file(&sub, &format!("{}.mq", stem), content.as_bytes()).await;
    let _ = write_file(
        &sub,
        &format!("{}.mq.sha256", stem),
        compute_content_hash(content).as_bytes(),
    )
    .await;
}

/// Parses `code` and returns all import/include paths that are local module names (not URLs).
fn extract_local_import_names(code: &str) -> Vec<String> {
    let token_arena = mq_lang::Shared::new(mq_lang::SharedCell::new(mq_lang::Arena::new(1024)));
    let Ok(program) = mq_lang::parse(code, token_arena) else {
        return vec![];
    };

    program
        .iter()
        .filter_map(|node| {
            let path = match &*node.expr {
                mq_lang::AstExpr::Import(mq_lang::AstLiteral::String(p)) => p,
                mq_lang::AstExpr::Include(mq_lang::AstLiteral::String(p)) => p,
                _ => return None,
            };
            if !mq_lang::http_import::is_remote_url(path) && !mq_lang::http_import::is_github_url(path) {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Parses `code` and returns all import/include paths that look like HTTP or GitHub URLs.
fn extract_http_import_urls(code: &str) -> Vec<String> {
    let token_arena = mq_lang::Shared::new(mq_lang::SharedCell::new(mq_lang::Arena::new(1024)));
    let Ok(program) = mq_lang::parse(code, token_arena) else {
        return vec![];
    };

    program
        .iter()
        .filter_map(|node| {
            let url = match &*node.expr {
                mq_lang::AstExpr::Import(mq_lang::AstLiteral::String(url)) => url,
                mq_lang::AstExpr::Include(mq_lang::AstLiteral::String(url)) => url,
                _ => return None,
            };
            if mq_lang::http_import::is_remote_url(url) || mq_lang::http_import::is_github_url(url) {
                Some(url.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Fetches the text content of a HTTPS URL.
///
/// Uses the global `fetch` function, which is available in browsers (`window.fetch`),
/// Node.js 18+, and Deno — so the same implementation works across all WASM hosts.
async fn fetch_text(url: &str) -> Result<String, String> {
    if !url.starts_with("https://") {
        return Err(format!("only HTTPS URLs are supported: {}", url));
    }

    let global = js_sys::global();
    let fetch_val = js_sys::Reflect::get(&global, &JsValue::from_str("fetch"))
        .map_err(|_| "global fetch is not available".to_string())?;

    if !fetch_val.is_function() {
        return Err("fetch is not available in this environment".to_string());
    }

    let fetch_fn: js_sys::Function = fetch_val.unchecked_into();
    let fetch_promise: js_sys::Promise = fetch_fn
        .call1(&JsValue::UNDEFINED, &JsValue::from_str(url))
        .map_err(|e| format!("fetch() call failed: {:?}", e))?
        .unchecked_into();

    let response_val = wasm_bindgen_futures::JsFuture::from(fetch_promise)
        .await
        .map_err(|e| format!("fetch request failed: {:?}", e))?;

    let response: web_sys::Response = response_val
        .dyn_into()
        .map_err(|_| "failed to cast fetch result to Response".to_string())?;

    if !response.ok() {
        return Err(format!("HTTP {} fetching {}", response.status(), url));
    }

    let text_promise = response.text().map_err(|e| format!("{:?}", e))?;
    let text_val = wasm_bindgen_futures::JsFuture::from(text_promise)
        .await
        .map_err(|e| format!("failed to read response body: {:?}", e))?;

    text_val
        .as_string()
        .ok_or_else(|| "response body is not a string".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
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
                allowed_domains: None,
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
                allowed_domains: None,
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
                allowed_domains: None,
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
                    allowed_domains: None,
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
            | let $var = 42;
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
                | let $module_var = 100;
            end

            def top_level_func(z): z;
            | let $top_level_var = 42;
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
        resolver.add_module("my_module", "def foo(x): x | upcase();".to_string());

        // Should be able to resolve it
        let result = mq_lang::ModuleResolver::resolve(&resolver, "my_module");
        #[cfg(feature = "opfs")]
        {
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "def foo(x): x | upcase();");
        }
        #[cfg(not(feature = "opfs"))]
        assert!(result.is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_wasm_module_resolver_not_found() {
        let resolver = WasmModuleResolver::new();

        // Should fail when module is not in cache
        let result = mq_lang::ModuleResolver::resolve(&resolver, "nonexistent");
        assert!(result.is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_wasm_module_resolver_clear_cache() {
        let resolver = WasmModuleResolver::new();

        // Add a module
        resolver.add_module("my_module", "content".to_string());
        #[cfg(feature = "opfs")]
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "my_module").is_ok());

        // Clear cache
        resolver.clear_cache();

        // Should no longer be resolvable
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "my_module").is_err());
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_opfs_create_and_import_module() {
        use opfs::{FileHandle as _, WritableFileStream as _};

        // Initialize OPFS
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;

        // Skip test if OPFS is not available
        if !*resolver.is_available.borrow() {
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
                .write_at_cursor_pos(module_content.as_bytes())
                .await
                .expect("Failed to write to file");

            writer.close().await.expect("Failed to close writer");
        }

        let code = r#"
            import "test_module"
            | test_module::upcase_exclaim()
        "#;

        resolver.preload_modules(code).await;

        let resolved_content =
            mq_lang::ModuleResolver::resolve(&resolver, "test_module").expect("Module should be found in cache");
        assert_eq!(resolved_content, module_content);

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

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_opfs_multiple_modules() {
        use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _};

        // Initialize OPFS
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;

        // Skip test if OPFS is not available
        if !*resolver.is_available.borrow() {
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
                .unwrap_or_else(|_| panic!("Failed to get file handle for {}", file_name));

            let mut writer = file_handle
                .create_writable_with_options(&opfs::CreateWritableOptions {
                    keep_existing_data: false,
                })
                .await
                .unwrap_or_else(|_| panic!("Failed to create writable for {}", file_name));

            writer
                .write_at_cursor_pos(content.as_bytes())
                .await
                .unwrap_or_else(|_| panic!("Failed to write to {}", file_name));

            writer
                .close()
                .await
                .unwrap_or_else(|_| panic!("Failed to close writer for {}", file_name));
        }

        let code = r#"
            import "math"
            import "string"
            | string::greet("World")
        "#;

        resolver.preload_modules(code).await;

        assert!(mq_lang::ModuleResolver::resolve(&resolver, "math").is_ok());
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "string").is_ok());

        let mut engine = mq_lang::Engine::new(resolver.clone());
        engine.load_builtin_module();

        let input = mq_lang::null_input();
        let result = engine.eval(code, input.into_iter()).expect("Failed to evaluate code");

        let output: Vec<String> = result.into_iter().map(|v| v.to_string()).collect();

        assert_eq!(output.join(""), "Hello, World!");

        // Note: File cleanup is skipped as OPFS persistent storage is isolated per origin
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_http_import_urls_import_https() {
        let code = r#"import "https://example.com/foo.mq""#;
        let urls = extract_http_import_urls(code);
        assert_eq!(urls, vec!["https://example.com/foo.mq"]);
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_http_import_urls_include_https() {
        let code = r#"include "https://example.com/foo.mq""#;
        let urls = extract_http_import_urls(code);
        assert_eq!(urls, vec!["https://example.com/foo.mq"]);
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_http_import_urls_import_github() {
        let code = r#"import "github.com/alice/mymod""#;
        let urls = extract_http_import_urls(code);
        assert_eq!(urls, vec!["github.com/alice/mymod"]);
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_http_import_urls_include_github() {
        let code = r#"include "github.com/alice/mymod""#;
        let urls = extract_http_import_urls(code);
        assert_eq!(urls, vec!["github.com/alice/mymod"]);
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_http_import_urls_local_import_excluded() {
        let code = r#"import "local_module""#;
        let urls = extract_http_import_urls(code);
        assert!(urls.is_empty());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_http_import_urls_local_include_excluded() {
        let code = r#"include "local_module""#;
        let urls = extract_http_import_urls(code);
        assert!(urls.is_empty());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_http_import_urls_multiple_mixed() {
        let code = r#"
            import "https://example.com/a.mq"
            include "github.com/alice/b"
            import "local_mod"
            include "https://example.com/c.mq"
        "#;
        let urls = extract_http_import_urls(code);
        assert!(urls.contains(&"https://example.com/a.mq".to_string()));
        assert!(urls.contains(&"github.com/alice/b".to_string()));
        assert!(urls.contains(&"https://example.com/c.mq".to_string()));
        assert!(!urls.contains(&"local_mod".to_string()));
        assert_eq!(urls.len(), 3);
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_http_import_urls_invalid_syntax_returns_empty() {
        let urls = extract_http_import_urls("import =>");
        assert!(urls.is_empty());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_http_import_urls_empty_code() {
        let urls = extract_http_import_urls("");
        assert!(urls.is_empty());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_local_import_names_import() {
        let code = r#"import "mymod""#;
        let names = extract_local_import_names(code);
        assert_eq!(names, vec!["mymod"]);
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_local_import_names_include() {
        let code = r#"include "utils""#;
        let names = extract_local_import_names(code);
        assert_eq!(names, vec!["utils"]);
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_local_import_names_excludes_urls() {
        let code = r#"
            import "local_mod"
            import "https://example.com/foo.mq"
            include "github.com/alice/mymod"
        "#;
        let names = extract_local_import_names(code);
        assert_eq!(names, vec!["local_mod"]);
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_local_import_names_empty_code() {
        assert!(extract_local_import_names("").is_empty());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_local_import_names_invalid_syntax_returns_empty() {
        assert!(extract_local_import_names("import =>").is_empty());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_local_import_names_multiple_local() {
        let code = r#"
            import "modA"
            include "modB"
            import "modC"
        "#;
        let names = extract_local_import_names(code);
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"modA".to_string()));
        assert!(names.contains(&"modB".to_string()));
        assert!(names.contains(&"modC".to_string()));
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_local_import_names_only_urls_returns_empty() {
        let code = r#"
            import "https://example.com/foo.mq"
            include "github.com/alice/mymod"
        "#;
        assert!(extract_local_import_names(code).is_empty());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_local_import_names_no_imports_in_expression_code() {
        // expressions without any import/include
        assert!(extract_local_import_names("upcase() | trim()").is_empty());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_extract_local_import_names_standard_module_name_treated_as_local() {
        // standard module names (csv, json) are syntactically local; resolve() handles them
        let names = extract_local_import_names(r#"import "csv""#);
        assert_eq!(names, vec!["csv"]);
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_without_opfs_available() {
        // do NOT call initialize() → OPFS stays unavailable
        let resolver = WasmModuleResolver::new();
        resolver.preload_modules(r#"import "mymod""#).await;
        // OPFS unavailable, so nothing should be cached
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "mymod").is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_empty_code_loads_nothing() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        resolver.preload_modules("").await;
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "anything").is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_no_imports_in_code_loads_nothing() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        resolver.preload_modules("upcase() | trim()").await;
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "anything").is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_uses_already_cached_module() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        // Pre-populate cache directly — should survive without touching OPFS
        resolver.add_module("pre_cached", "def pre(): 1;".to_string());
        resolver.preload_modules(r#"import "pre_cached""#).await;
        let content = mq_lang::ModuleResolver::resolve(&resolver, "pre_cached").unwrap();
        assert_eq!(content, "def pre(): 1;");
    }

    /// Writes `content` to `{name}` under the OPFS root. Panics on any failure.
    #[cfg(feature = "opfs")]
    async fn write_opfs_file(root: &opfs::persistent::DirectoryHandle, name: &str, content: &str) {
        use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _};
        let mut fh = root
            .get_file_handle_with_options(name, &opfs::GetFileHandleOptions { create: true })
            .await
            .expect("get_file_handle");
        let mut w = fh
            .create_writable_with_options(&opfs::CreateWritableOptions {
                keep_existing_data: false,
            })
            .await
            .expect("create_writable");
        w.write_at_cursor_pos(content.as_bytes()).await.expect("write");
        w.close().await.expect("close");
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_loads_imported_module_from_opfs() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        let root = opfs::persistent::app_specific_dir().await.unwrap();
        write_opfs_file(&root, "pll_single.mq", "def hello(): \"hello\";").await;

        resolver.preload_modules(r#"import "pll_single""#).await;

        let content = mq_lang::ModuleResolver::resolve(&resolver, "pll_single").unwrap();
        assert_eq!(content, "def hello(): \"hello\";");
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_does_not_load_nonimported_module() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        let root = opfs::persistent::app_specific_dir().await.unwrap();
        write_opfs_file(&root, "pll_wanted.mq", "def wanted(): 1;").await;
        write_opfs_file(&root, "pll_unwanted.mq", "def unwanted(): 2;").await;

        // only pll_wanted is in the import list
        resolver.preload_modules(r#"import "pll_wanted""#).await;

        assert!(mq_lang::ModuleResolver::resolve(&resolver, "pll_wanted").is_ok());
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "pll_unwanted").is_err());
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_loads_multiple_direct_imports() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        let root = opfs::persistent::app_specific_dir().await.unwrap();
        write_opfs_file(&root, "pll_multi_a.mq", "def fa(x): x;").await;
        write_opfs_file(&root, "pll_multi_b.mq", "def fb(x): x;").await;

        let code = r#"import "pll_multi_a" import "pll_multi_b""#;
        resolver.preload_modules(code).await;

        assert!(mq_lang::ModuleResolver::resolve(&resolver, "pll_multi_a").is_ok());
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "pll_multi_b").is_ok());
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_resolves_transitive_dependencies() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        let root = opfs::persistent::app_specific_dir().await.unwrap();
        // pll_trans_a imports pll_trans_b
        write_opfs_file(&root, "pll_trans_a.mq", "import \"pll_trans_b\"\ndef fa(x): x;").await;
        write_opfs_file(&root, "pll_trans_b.mq", "def fb(x): x;").await;

        // user code only imports pll_trans_a
        resolver.preload_modules(r#"import "pll_trans_a""#).await;

        // pll_trans_b should be loaded transitively
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "pll_trans_a").is_ok());
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "pll_trans_b").is_ok());
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_handles_circular_dependencies() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        let root = opfs::persistent::app_specific_dir().await.unwrap();
        // pll_circ_a → pll_circ_b → pll_circ_a (cycle)
        write_opfs_file(&root, "pll_circ_a.mq", "import \"pll_circ_b\"\ndef fca(x): x;").await;
        write_opfs_file(&root, "pll_circ_b.mq", "import \"pll_circ_a\"\ndef fcb(x): x;").await;

        // must terminate without infinite loop
        resolver.preload_modules(r#"import "pll_circ_a""#).await;

        assert!(mq_lang::ModuleResolver::resolve(&resolver, "pll_circ_a").is_ok());
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "pll_circ_b").is_ok());
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_missing_module_skipped_gracefully() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        // "pll_ghost" does not exist in OPFS
        resolver.preload_modules(r#"import "pll_ghost""#).await;
        // should not panic; module stays unresolvable
        assert!(mq_lang::ModuleResolver::resolve(&resolver, "pll_ghost").is_err());
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_duplicate_import_loaded_once() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        let root = opfs::persistent::app_specific_dir().await.unwrap();
        write_opfs_file(&root, "pll_dup.mq", "def fdup(x): x;").await;

        // same module listed twice in imports
        let code = r#"import "pll_dup" import "pll_dup""#;
        resolver.preload_modules(code).await;

        // resolve must succeed and content is correct
        let content = mq_lang::ModuleResolver::resolve(&resolver, "pll_dup").unwrap();
        assert_eq!(content, "def fdup(x): x;");
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_modules_cached_content_not_overwritten_from_opfs() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        let root = opfs::persistent::app_specific_dir().await.unwrap();
        // write one version to OPFS
        write_opfs_file(&root, "pll_override.mq", "def opfs_version(): 2;").await;
        // pre-populate cache with a different version
        resolver.add_module("pll_override", "def cache_version(): 1;".to_string());

        resolver.preload_modules(r#"import "pll_override""#).await;

        // cache takes precedence; OPFS file should not overwrite it
        let content = mq_lang::ModuleResolver::resolve(&resolver, "pll_override").unwrap();
        assert_eq!(content, "def cache_version(): 1;");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_canonical_name_https_url_returns_module_name() {
        let resolver = WasmModuleResolver::new();
        assert_eq!(
            mq_lang::ModuleResolver::canonical_name(&resolver, "https://example.com/mymod.mq"),
            "mymod"
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_canonical_name_github_shorthand_returns_module_name() {
        let resolver = WasmModuleResolver::new();
        assert_eq!(
            mq_lang::ModuleResolver::canonical_name(&resolver, "github.com/alice/mymod"),
            "mymod"
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_canonical_name_github_versioned_strips_version_and_extension() {
        let resolver = WasmModuleResolver::new();
        assert_eq!(
            mq_lang::ModuleResolver::canonical_name(&resolver, "github.com/alice/mymod.mq@v1.0"),
            "mymod"
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_canonical_name_local_module_unchanged() {
        let resolver = WasmModuleResolver::new();
        assert_eq!(
            mq_lang::ModuleResolver::canonical_name(&resolver, "local_mod"),
            "local_mod"
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_get_path_harehare_github_url_expands_to_raw_url() {
        let resolver = WasmModuleResolver::new();
        let path = mq_lang::ModuleResolver::get_path(&resolver, "github.com/harehare/mymod").unwrap();
        assert!(path.starts_with("https://raw.githubusercontent.com/harehare/mymod/"));
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_get_path_non_allowed_github_url_falls_back_to_module_name() {
        // github.com/other-user/... is not in the default allowlist, so to_fetch_url returns
        // IOError; WasmModuleResolver::get_path falls back to the module name itself.
        let resolver = WasmModuleResolver::new();
        let path = mq_lang::ModuleResolver::get_path(&resolver, "github.com/other/mymod").unwrap();
        assert_eq!(path, "github.com/other/mymod");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_get_path_local_module_returns_name() {
        let resolver = WasmModuleResolver::new();
        let path = mq_lang::ModuleResolver::get_path(&resolver, "my_local_mod").unwrap();
        assert_eq!(path, "my_local_mod");
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_resolve_https_url_without_opfs_returns_io_error() {
        let resolver = WasmModuleResolver::new();
        // initialize() not called → OPFS unavailable
        let result =
            mq_lang::ModuleResolver::resolve(&resolver, "https://raw.githubusercontent.com/harehare/mod/HEAD/mod.mq");
        assert!(
            matches!(result, Err(mq_lang::ModuleError::IOError(_))),
            "expected IOError when OPFS is unavailable, got: {:?}",
            result
        );
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_resolve_github_url_without_opfs_returns_io_error() {
        let resolver = WasmModuleResolver::new();
        let result = mq_lang::ModuleResolver::resolve(&resolver, "github.com/harehare/mymod");
        assert!(matches!(result, Err(mq_lang::ModuleError::IOError(_))));
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_resolve_non_allowlisted_domain_returns_io_error() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        // example.com is not in the default allowlist
        let result = mq_lang::ModuleResolver::resolve(&resolver, "https://example.com/mod.mq");
        assert!(
            matches!(result, Err(mq_lang::ModuleError::IOError(_))),
            "expected IOError for disallowed domain, got: {:?}",
            result
        );
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_resolve_default_allowed_domain_not_in_cache_returns_not_found() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        // raw.githubusercontent.com/harehare is always allowed; fetcher cache is empty
        let result = mq_lang::ModuleResolver::resolve(
            &resolver,
            "https://raw.githubusercontent.com/harehare/test/HEAD/test.mq",
        );
        assert!(
            matches!(result, Err(mq_lang::ModuleError::NotFound(_))),
            "expected NotFound when URL is allowed but not cached, got: {:?}",
            result
        );
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_resolve_non_harehare_github_url_blocked_by_empty_allowlist() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        // raw.githubusercontent.com/other-user does not match the default allowed domain prefix
        let result = mq_lang::ModuleResolver::resolve(
            &resolver,
            "https://raw.githubusercontent.com/other-user/mod/HEAD/mod.mq",
        );
        assert!(matches!(result, Err(mq_lang::ModuleError::IOError(_))));
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_set_allowed_domains_changes_domain_error_to_not_found() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }

        let url = "https://example.com/mod.mq";
        // before: domain blocked → IOError
        let before = mq_lang::ModuleResolver::resolve(&resolver, url);
        assert!(
            matches!(before, Err(mq_lang::ModuleError::IOError(_))),
            "expected IOError before setting domain, got: {:?}",
            before
        );

        // after: domain allowed → domain check passes, fetcher cache empty → NotFound
        resolver.set_allowed_domains(vec!["example.com".to_string()]);
        let after = mq_lang::ModuleResolver::resolve(&resolver, url);
        assert!(
            matches!(after, Err(mq_lang::ModuleError::NotFound(_))),
            "expected NotFound after setting domain, got: {:?}",
            after
        );
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_set_allowed_domains_github_shorthand_expands_correctly() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        // github.com/alice/repo shorthand should expand to raw.githubusercontent.com/alice/repo
        resolver.set_allowed_domains(vec!["github.com/alice/myrepo".to_string()]);
        // Now alice/myrepo is allowed; fetcher cache empty → NotFound (not IOError)
        let result = mq_lang::ModuleResolver::resolve(
            &resolver,
            "https://raw.githubusercontent.com/alice/myrepo/HEAD/myrepo.mq",
        );
        assert!(
            matches!(result, Err(mq_lang::ModuleError::NotFound(_))),
            "expected NotFound after allowing github.com/alice/myrepo, got: {:?}",
            result
        );
        // alice/other is still blocked
        let blocked =
            mq_lang::ModuleResolver::resolve(&resolver, "https://raw.githubusercontent.com/alice/other/HEAD/other.mq");
        assert!(matches!(blocked, Err(mq_lang::ModuleError::IOError(_))));
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_http_modules_empty_code_is_noop() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        resolver.preload_http_modules("").await;
        // nothing cached; http resolve of any URL still fails
        #[cfg(feature = "opfs")]
        assert!(
            mq_lang::ModuleResolver::resolve(
                &resolver,
                "https://raw.githubusercontent.com/harehare/test/HEAD/test.mq"
            )
            .is_err()
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_http_modules_local_only_imports_do_not_affect_http_cache() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        resolver.preload_http_modules(r#"import "local_mod""#).await;
        // local_mod is not an HTTP URL → skipped; HTTP resolve still fails
        #[cfg(feature = "opfs")]
        assert!(
            mq_lang::ModuleResolver::resolve(
                &resolver,
                "https://raw.githubusercontent.com/harehare/test/HEAD/test.mq"
            )
            .is_err()
        );
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_http_modules_without_opfs_is_noop() {
        let resolver = WasmModuleResolver::new();
        // initialize() not called → OPFS unavailable → preload_http_modules returns early
        resolver
            .preload_http_modules(r#"import "https://raw.githubusercontent.com/harehare/test/HEAD/test.mq""#)
            .await;
        // resolve should still fail with OPFS-unavailable error, not a domain error
        let result = mq_lang::ModuleResolver::resolve(
            &resolver,
            "https://raw.githubusercontent.com/harehare/test/HEAD/test.mq",
        );
        assert!(matches!(result, Err(mq_lang::ModuleError::IOError(_))));
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_preload_http_modules_disallowed_domain_not_cached() {
        let resolver = WasmModuleResolver::new();
        resolver.initialize().await;
        if !*resolver.is_available.borrow() {
            return;
        }
        // example.com is not in the allowlist → preload_http_modules skips it
        resolver
            .preload_http_modules(r#"import "https://example.com/mod.mq""#)
            .await;
        let result = mq_lang::ModuleResolver::resolve(&resolver, "https://example.com/mod.mq");
        // still IOError (domain not allowed), not NotFound — URL was not cached
        assert!(matches!(result, Err(mq_lang::ModuleError::IOError(_))));
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_clear_http_cache_succeeds_when_no_cache_exists() {
        // Calling clear when there is nothing to clear should succeed silently.
        let result = clear_http_cache().await;
        assert!(result.is_ok());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_clear_all_http_cache_succeeds_when_no_cache_exists() {
        let result = clear_all_http_cache().await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_clear_http_cache_removes_mutable_subdir() {
        use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _};

        let root = match opfs::persistent::app_specific_dir().await {
            Ok(r) => r,
            Err(_) => return, // OPFS unavailable in this environment
        };

        // Seed a mutable cache file
        let cache_dir = root
            .get_directory_handle_with_options(HTTP_CACHE_DIR, &opfs::GetDirectoryHandleOptions { create: true })
            .await
            .unwrap();
        let mutable_dir = cache_dir
            .get_directory_handle_with_options("mutable", &opfs::GetDirectoryHandleOptions { create: true })
            .await
            .unwrap();
        write_opfs_file(&mutable_dir, "sentinel.mq", "test").await;

        // Clear only mutable cache
        clear_http_cache().await.expect("clear_http_cache failed");

        // mutable/ should no longer exist (or sentinel is gone)
        let still_exists = cache_dir
            .get_directory_handle_with_options("mutable", &opfs::GetDirectoryHandleOptions { create: false })
            .await
            .is_ok();
        assert!(!still_exists, "mutable/ cache dir should have been removed");
    }

    #[cfg(feature = "opfs")]
    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_clear_all_http_cache_removes_entire_cache_dir() {
        use opfs::DirectoryHandle as _;

        let mut root = match opfs::persistent::app_specific_dir().await {
            Ok(r) => r,
            Err(_) => return,
        };

        // Ensure the http_cache dir exists with something in it
        let cache_dir = root
            .get_directory_handle_with_options(HTTP_CACHE_DIR, &opfs::GetDirectoryHandleOptions { create: true })
            .await
            .unwrap();
        let _ = cache_dir
            .get_directory_handle_with_options("versioned", &opfs::GetDirectoryHandleOptions { create: true })
            .await;

        clear_all_http_cache().await.expect("clear_all_http_cache failed");

        let still_exists = root
            .get_directory_handle_with_options(HTTP_CACHE_DIR, &opfs::GetDirectoryHandleOptions { create: false })
            .await
            .is_ok();
        assert!(!still_exists, "http_cache/ dir should have been fully removed");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_html_to_markdown() {
        let html = r#"<h1>Hello World</h1><p>This is a <strong>test</strong>.</p>"#;
        let options = ConversionOptions::default();

        let result = html_to_markdown(html, Some(options)).await;
        assert!(result.is_ok());
        let markdown = result.unwrap();
        assert!(markdown.contains("# Hello World"));
        assert!(markdown.contains("**test**"));
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_to_html() {
        let markdown = "# Hello World\n\nThis is a **test**.";
        let result = to_html(markdown).await;

        assert!(result.contains("<h1>Hello World</h1>"));
        assert!(result.contains("<strong>test</strong>"));
    }
}
