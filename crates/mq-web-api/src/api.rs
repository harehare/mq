use std::collections::HashMap;

use miette::miette;
use mq_formatter::{Formatter, FormatterConfig};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Maximum number of documents allowed in a single `POST /api/v1/batch` request.
pub const MAX_BATCH_SIZE: usize = 100;

#[derive(Deserialize, Serialize, ToSchema, Clone, Debug)]
pub struct ApiRequest {
    #[schema(example = ".h")]
    pub query: String,
    #[schema(example = "## Markdown Content\n\nThis is an example markdown string.")]
    pub input: Option<String>,
    pub input_format: Option<InputFormat>,
    /// Names of builtin modules to load (e.g. "json", "csv", "table").
    pub modules: Option<Vec<String>>,
    /// String variable definitions passed to the engine (name → value).
    pub args: Option<HashMap<String, String>>,
    /// Output format for query results. Defaults to `markdown`.
    pub output_format: Option<OutputFormat>,
    /// When true, aggregates all input nodes before applying the query
    /// (equivalent to the CLI `-A` flag).
    pub aggregate: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct QueryApiResponse {
    pub results: Vec<String>,
}

/// Request body for `POST /api/v1/batch`. Runs `query` against each entry
/// of `inputs`, avoiding one HTTP round trip per document.
#[derive(Deserialize, Serialize, ToSchema, Clone, Debug)]
pub struct BatchApiRequest {
    #[schema(example = ".h")]
    pub query: String,
    /// At most [`MAX_BATCH_SIZE`] entries are accepted.
    pub inputs: Vec<String>,
    pub input_format: Option<InputFormat>,
    pub modules: Option<Vec<String>>,
    pub args: Option<HashMap<String, String>>,
    pub output_format: Option<OutputFormat>,
    pub aggregate: Option<bool>,
}

/// Result of running the batch query against a single input document.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchItemResult {
    /// Empty when `error` is set.
    pub results: Vec<String>,
    /// A failure here doesn't affect other documents in the batch.
    pub error: Option<String>,
}

/// Response body for `POST /api/v1/batch`. `items` is ordered like `inputs`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BatchApiResponse {
    pub items: Vec<BatchItemResult>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputFormat {
    #[default]
    #[serde(rename = "markdown")]
    Markdown,
    #[serde(rename = "mdx")]
    Mdx,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "html")]
    Html,
    #[serde(rename = "raw")]
    Raw,
    #[serde(rename = "null")]
    Null,
    /// Module-backed formats: fed as raw text and parsed by the corresponding
    /// builtin mq module (same mechanism as the CLI's `-I` flag).
    #[serde(rename = "csv")]
    Csv,
    #[serde(rename = "tsv")]
    Tsv,
    #[serde(rename = "psv")]
    Psv,
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "yaml")]
    Yaml,
    #[serde(rename = "toml")]
    Toml,
    #[serde(rename = "xml")]
    Xml,
    #[serde(rename = "toon")]
    Toon,
}

impl InputFormat {
    /// Returns the mq query prefix that parses raw text of this format into
    /// structured data, for module-backed formats. `None` for formats that
    /// are ingested natively (markdown, html, ...) without an `import`.
    pub fn module_query_prefix(&self) -> Option<&'static str> {
        match self {
            Self::Csv => Some(r#"import "csv" | csv::csv_parse(true)"#),
            Self::Tsv => Some(r#"import "csv" | csv::tsv_parse(true)"#),
            Self::Psv => Some(r#"import "csv" | csv::psv_parse(true)"#),
            Self::Json => Some(r#"import "json" | json::json_parse()"#),
            Self::Yaml => Some(r#"import "yaml" | yaml::yaml_parse()"#),
            Self::Toml => Some(r#"import "toml" | toml::toml_parse()"#),
            Self::Xml => Some(r#"import "xml" | xml::xml_parse()"#),
            Self::Toon => Some(r#"import "toon" | toon::toon_parse()"#),
            Self::Markdown | Self::Mdx | Self::Text | Self::Html | Self::Raw | Self::Null => None,
        }
    }
}

/// Output format for query results.
#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Render as Markdown (default).
    #[default]
    Markdown,
    /// Render as HTML.
    Html,
    /// Render as plain text.
    Text,
    /// Render as JSON.
    Json,
    /// Suppress output.
    None,
}

/// Request body for `POST /api/check`.
#[derive(Deserialize, Serialize, ToSchema, Clone, Debug)]
pub struct CheckApiRequest {
    #[schema(example = "upcase() | downcase()")]
    pub query: String,
}

/// A single type-check or syntax error.
#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
pub struct CheckError {
    pub message: String,
    pub start_line: Option<u32>,
    pub start_column: Option<u32>,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
    /// Error kind: `"syntax_error"`, `"type_mismatch"`, `"undefined_symbol"`,
    /// `"unification_error"`, `"wrong_arity"`, `"undefined_field"`,
    /// `"heterogeneous_array"`, `"nullable_propagation"`, `"unreachable_code"`,
    /// `"occurs_check"`, or `"internal_error"`.
    pub kind: String,
}

/// Response body for `POST /api/check`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CheckApiResponse {
    /// Type/syntax errors found. Empty means no errors.
    pub errors: Vec<CheckError>,
}

/// Request body for `POST /api/format`.
#[derive(Deserialize, Serialize, ToSchema, Clone, Debug)]
pub struct FormatApiRequest {
    #[schema(example = "if(a):1 elif(b):2 else:3")]
    pub query: String,
    pub indent_width: Option<usize>,
    pub sort_imports: Option<bool>,
    pub sort_functions: Option<bool>,
    pub sort_fields: Option<bool>,
    pub max_width: Option<usize>,
}

/// Response body for `POST /api/format`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FormatApiResponse {
    pub formatted: String,
}

/// Documentation for a single builtin function.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FunctionDoc {
    pub name: String,
    pub description: String,
    pub params: Vec<String>,
}

/// Response body for `GET /api/functions`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FunctionsApiResponse {
    pub functions: Vec<FunctionDoc>,
}

/// Documentation for a single builtin selector.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SelectorDoc {
    pub name: String,
    pub description: String,
    pub params: Vec<String>,
}

/// Response body for `GET /api/selectors`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SelectorsApiResponse {
    pub selectors: Vec<SelectorDoc>,
}

/// Request body for `POST /api/lint`.
#[derive(Deserialize, Serialize, ToSchema, Clone, Debug)]
pub struct LintApiRequest {
    #[schema(example = "def f(x): let y = 1; x;")]
    pub query: String,
}

/// A single lint diagnostic.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LintDiagnostic {
    /// Identifier of the lint rule that produced this diagnostic (e.g. `"unused_variable"`).
    pub rule_id: String,
    pub message: String,
    /// Severity: `"style"`, `"perf"`, `"warn"`, or `"error"`.
    pub severity: String,
    pub help: Option<String>,
    pub start_line: Option<u32>,
    pub start_column: Option<u32>,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
}

/// Response body for `POST /api/lint`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LintApiResponse {
    pub diagnostics: Vec<LintDiagnostic>,
}

pub fn query(request: ApiRequest, timeout: std::time::Duration) -> miette::Result<QueryApiResponse> {
    execute_query(request, timeout)
}

/// Runs `request.query` against every document in `request.inputs` in
/// parallel, each with its own engine instance.
pub fn batch_query(request: BatchApiRequest, timeout: std::time::Duration) -> miette::Result<BatchApiResponse> {
    if request.inputs.len() > MAX_BATCH_SIZE {
        return Err(miette!(
            "Batch request exceeds maximum of {} documents (got {})",
            MAX_BATCH_SIZE,
            request.inputs.len()
        ));
    }

    let items = request
        .inputs
        .par_iter()
        .map(|input| {
            let item_request = ApiRequest {
                query: request.query.clone(),
                input: Some(input.clone()),
                input_format: request.input_format.clone(),
                modules: request.modules.clone(),
                args: request.args.clone(),
                output_format: request.output_format.clone(),
                aggregate: request.aggregate,
            };
            match execute_query(item_request, timeout) {
                Ok(response) => BatchItemResult {
                    results: response.results,
                    error: None,
                },
                Err(e) => BatchItemResult {
                    results: vec![],
                    error: Some(e.to_string()),
                },
            }
        })
        .collect();

    Ok(BatchApiResponse { items })
}

/// Type-checks the given query and returns any errors found.
///
/// Always returns HTTP 200 — errors are data, not exceptional failures.
pub fn check(request: CheckApiRequest) -> CheckApiResponse {
    let (_, parse_errors) = mq_lang::parse_recovery(&request.query);
    let syntax_errors: Vec<_> = parse_errors
        .error_ranges(&request.query)
        .into_iter()
        .map(|(message, range)| CheckError {
            message: message.clone(),
            start_line: Some(range.start.line),
            start_column: Some(range.start.column as u32),
            end_line: Some(range.end.line),
            end_column: Some(range.end.column as u32),
            kind: "syntax_error".to_string(),
        })
        .collect();

    if !syntax_errors.is_empty() {
        return CheckApiResponse { errors: syntax_errors };
    }

    let mut hir = mq_hir::Hir::default();
    hir.add_code(None, &request.query);

    let type_errors = mq_check::TypeChecker::new()
        .check(&hir)
        .into_iter()
        .map(|e| {
            let location = e.location();
            CheckError {
                message: e.to_string(),
                start_line: location.map(|r| r.start.line),
                start_column: location.map(|r| r.start.column as u32),
                end_line: location.map(|r| r.end.line),
                end_column: location.map(|r| r.end.column as u32),
                kind: type_error_kind(&e),
            }
        })
        .collect();

    CheckApiResponse { errors: type_errors }
}

/// Formats mq query code using the given formatting options.
pub fn format_query(request: FormatApiRequest) -> miette::Result<FormatApiResponse> {
    let config = FormatterConfig {
        indent_width: request.indent_width.unwrap_or(2),
        sort_imports: request.sort_imports.unwrap_or(false),
        sort_functions: request.sort_functions.unwrap_or(false),
        sort_fields: request.sort_fields.unwrap_or(false),
        max_width: request.max_width,
    };
    let formatted = Formatter::new(Some(config))
        .format(&request.query)
        .map_err(|e| miette!("Format error: {}", e))?;
    Ok(FormatApiResponse { formatted })
}

/// Lists all builtin mq functions with their documentation.
pub fn list_functions() -> FunctionsApiResponse {
    let mut functions: Vec<FunctionDoc> = mq_lang::BUILTIN_FUNCTION_DOC
        .iter()
        .map(|(name, doc)| FunctionDoc {
            name: name.to_string(),
            description: doc.description.to_string(),
            params: doc.params.iter().map(|p| p.to_string()).collect(),
        })
        .collect();
    functions.sort_by(|a, b| a.name.cmp(&b.name));
    FunctionsApiResponse { functions }
}

/// Lists all builtin mq selectors with their documentation.
pub fn list_selectors() -> SelectorsApiResponse {
    let mut selectors: Vec<SelectorDoc> = mq_lang::BUILTIN_SELECTOR_DOC
        .iter()
        .map(|(name, doc)| SelectorDoc {
            name: name.to_string(),
            description: doc.description.to_string(),
            params: doc.params.iter().map(|p| p.to_string()).collect(),
        })
        .collect();
    selectors.sort_by(|a, b| a.name.cmp(&b.name));
    SelectorsApiResponse { selectors }
}

/// Lints the given query and returns any diagnostics found.
pub fn lint(request: LintApiRequest) -> LintApiResponse {
    let mut hir = mq_hir::Hir::default();
    let (source_id, _) = hir.add_code(None, &request.query);

    let config = mq_lint::LintConfig::default();
    let ctx = mq_lint::LintContext::new(&hir, source_id, &config);
    let diagnostics = mq_lint::Linter::with_default_rules()
        .run(&ctx)
        .into_iter()
        .map(|d| {
            let range = d.range;
            LintDiagnostic {
                rule_id: d.rule_id().as_str().to_string(),
                message: d.message(),
                severity: d.severity.to_string(),
                help: d.help(),
                start_line: range.as_ref().map(|r| r.start.line),
                start_column: range.as_ref().map(|r| r.start.column as u32),
                end_line: range.as_ref().map(|r| r.end.line),
                end_column: range.as_ref().map(|r| r.end.column as u32),
            }
        })
        .collect();

    LintApiResponse { diagnostics }
}

fn type_error_kind(e: &mq_check::TypeError) -> String {
    match e {
        mq_check::TypeError::Mismatch { .. } => "type_mismatch",
        mq_check::TypeError::UnificationError { .. } => "unification_error",
        mq_check::TypeError::OccursCheck { .. } => "occurs_check",
        mq_check::TypeError::UndefinedSymbol { .. } => "undefined_symbol",
        mq_check::TypeError::WrongArity { .. } => "wrong_arity",
        mq_check::TypeError::UndefinedField { .. } => "undefined_field",
        mq_check::TypeError::HeterogeneousArray { .. } => "heterogeneous_array",
        mq_check::TypeError::TypeVarNotFound(_) => "type_var_not_found",
        mq_check::TypeError::Internal(_) => "internal_error",
        mq_check::TypeError::NullablePropagation { .. } => "nullable_propagation",
        mq_check::TypeError::UnreachableCode { .. } => "unreachable_code",
        mq_check::TypeError::NonExhaustiveMatch { .. } => "non_exhaustive_match",
    }
    .to_string()
}

fn execute_query(request: ApiRequest, timeout: std::time::Duration) -> miette::Result<QueryApiResponse> {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.set_timeout(timeout);

    if let Some(modules) = &request.modules {
        for module_name in modules {
            engine
                .load_module(module_name)
                .map_err(|e| miette!("Failed to load module '{}': {}", module_name, e))?;
        }
    }

    if let Some(args) = &request.args {
        for (name, value) in args {
            engine.define_string_value(name, value);
        }
    }

    let input_format = request.input_format.clone().unwrap_or(InputFormat::Markdown);

    let query = {
        let base = if request.aggregate.unwrap_or(false) {
            format!(r#"nodes | import "section" | {}"#, request.query)
        } else {
            request.query.clone()
        };
        match input_format.module_query_prefix() {
            Some(prefix) => format!("{} | {}", prefix, base),
            None => base,
        }
    };

    let input = match input_format {
        InputFormat::Markdown => mq_lang::parse_markdown_input(&request.input.unwrap_or_default())?,
        InputFormat::Mdx => mq_lang::parse_mdx_input(&request.input.unwrap_or_default())?,
        InputFormat::Text => mq_lang::parse_text_input(&request.input.unwrap_or_default())?,
        InputFormat::Html => mq_lang::parse_html_input(&request.input.unwrap_or_default())?,
        InputFormat::Raw => mq_lang::raw_input(&request.input.unwrap_or_default()),
        InputFormat::Null => mq_lang::null_input(),
        // Module-backed formats: pass raw text through; the `import`ed module parses it.
        InputFormat::Csv
        | InputFormat::Tsv
        | InputFormat::Psv
        | InputFormat::Json
        | InputFormat::Yaml
        | InputFormat::Toml
        | InputFormat::Xml
        | InputFormat::Toon => mq_lang::raw_input(&request.input.unwrap_or_default()),
    };

    let runtime_values = engine
        .eval(&query, input.into_iter())
        .map_err(|e| miette!("Error executing query: {}", e))?;

    let nodes: Vec<mq_markdown::Node> = runtime_values
        .values()
        .iter()
        .flat_map(runtime_value_to_nodes)
        .collect();

    let markdown = mq_markdown::Markdown::new(nodes);

    let results = match request.output_format.unwrap_or_default() {
        OutputFormat::Html => vec![markdown.to_html()],
        OutputFormat::Text => vec![markdown.to_text()],
        OutputFormat::Json => vec![
            markdown
                .to_json()
                .map_err(|e| miette!("JSON serialization error: {}", e))?,
        ],
        OutputFormat::None => vec![],
        OutputFormat::Markdown => vec![markdown.to_string()],
    }
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect();

    Ok(QueryApiResponse { results })
}

fn collect_markdown_nodes(value: &mq_lang::RuntimeValue, nodes: &mut Vec<mq_markdown::Node>) {
    match value {
        mq_lang::RuntimeValue::Markdown(node, _) => nodes.push((**node).clone()),
        mq_lang::RuntimeValue::Array(items) => {
            for item in items {
                collect_markdown_nodes(item, nodes);
            }
        }
        _ => {}
    }
}

fn is_typed_dict(map: &std::collections::BTreeMap<mq_lang::Ident, mq_lang::RuntimeValue>) -> bool {
    let type_key = mq_lang::Ident::new("type");
    matches!(
        map.get(&type_key),
        Some(mq_lang::RuntimeValue::Symbol(s)) if matches!(s.as_str().as_str(), "section" | "table")
    )
}

fn expand_typed_dict(
    map: &std::collections::BTreeMap<mq_lang::Ident, mq_lang::RuntimeValue>,
) -> Option<Vec<mq_markdown::Node>> {
    let type_key = mq_lang::Ident::new("type");
    match map.get(&type_key) {
        Some(mq_lang::RuntimeValue::Symbol(s)) => match s.as_str().as_str() {
            "section" => {
                let mut nodes = Vec::new();
                if let Some(header) = map.get(&mq_lang::Ident::new("header")) {
                    collect_markdown_nodes(header, &mut nodes);
                }
                if let Some(children) = map.get(&mq_lang::Ident::new("children")) {
                    collect_markdown_nodes(children, &mut nodes);
                }
                Some(nodes)
            }
            "table" => {
                let mut nodes = Vec::new();
                if let Some(header) = map.get(&mq_lang::Ident::new("header")) {
                    collect_markdown_nodes(header, &mut nodes);
                }
                if let Some(align) = map.get(&mq_lang::Ident::new("align")) {
                    collect_markdown_nodes(align, &mut nodes);
                }
                if let Some(rows) = map.get(&mq_lang::Ident::new("rows")) {
                    collect_markdown_nodes(rows, &mut nodes);
                }
                Some(nodes)
            }
            _ => None,
        },
        _ => None,
    }
}

fn runtime_value_to_nodes(runtime_value: &mq_lang::RuntimeValue) -> Vec<mq_markdown::Node> {
    match runtime_value {
        mq_lang::RuntimeValue::Markdown(node, _) => vec![(**node).clone()],
        mq_lang::RuntimeValue::Dict(map) => {
            expand_typed_dict(map).unwrap_or_else(|| vec![runtime_value.to_string().into()])
        }
        mq_lang::RuntimeValue::Array(items) => {
            let has_expandable = items.iter().any(|v| match v {
                mq_lang::RuntimeValue::Markdown(_, _) => true,
                mq_lang::RuntimeValue::Dict(m) => is_typed_dict(m),
                _ => false,
            });
            if has_expandable {
                items.iter().flat_map(runtime_value_to_nodes).collect()
            } else if items.is_empty() {
                vec![]
            } else {
                vec![runtime_value.to_string().into()]
            }
        }
        _ => vec![runtime_value.to_string().into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(InputFormat::Csv, "name,age\nAlice,30\nBob,25")]
    #[case(InputFormat::Tsv, "name\tage\nAlice\t30")]
    #[case(InputFormat::Psv, "name|age\nAlice|30")]
    #[case(InputFormat::Json, r#"{"name": "Alice", "age": 30}"#)]
    #[case(InputFormat::Yaml, "name: Alice\nage: 30")]
    #[case(InputFormat::Toml, "name = \"Alice\"\nage = 30")]
    #[case(InputFormat::Xml, "<person><name>Alice</name></person>")]
    #[case(InputFormat::Toon, "name: Alice\nage: 30")]
    fn test_execute_module_backed_formats(#[case] input_format: InputFormat, #[case] input: &str) {
        let req = ApiRequest {
            query: "identity()".to_string(),
            input: Some(input.to_string()),
            input_format: Some(input_format),
            modules: None,
            args: None,
            output_format: None,
            aggregate: None,
        };
        let result = query(req, std::time::Duration::from_secs(10));
        assert!(result.is_ok(), "{:?}", result.err());
        assert!(!result.unwrap().results.is_empty());
    }

    #[test]
    fn test_module_query_prefix_native_formats_are_none() {
        for fmt in [
            InputFormat::Markdown,
            InputFormat::Mdx,
            InputFormat::Text,
            InputFormat::Html,
            InputFormat::Raw,
            InputFormat::Null,
        ] {
            assert_eq!(fmt.module_query_prefix(), None);
        }
    }

    #[test]
    fn test_execute_markdown() {
        let req = ApiRequest {
            query: ".h".to_string(),
            input: Some("# Title\n\nContent".to_string()),
            input_format: Some(InputFormat::Markdown),
            modules: None,
            args: None,
            output_format: None,
            aggregate: None,
        };
        let result = query(req, std::time::Duration::from_secs(10));
        assert!(result.is_ok());
        assert!(!result.unwrap().results.is_empty());
    }

    #[test]
    fn test_execute_text() {
        let req = ApiRequest {
            query: ".h".to_string(),
            input: Some("line1\nline2".to_string()),
            input_format: Some(InputFormat::Text),
            modules: None,
            args: None,
            output_format: None,
            aggregate: None,
        };
        let result = query(req, std::time::Duration::from_secs(10));
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_invalid_query() {
        let req = ApiRequest {
            query: "invalid query".to_string(),
            input: Some("# Title".to_string()),
            input_format: Some(InputFormat::Markdown),
            modules: None,
            args: None,
            output_format: None,
            aggregate: None,
        };
        let result = query(req, std::time::Duration::from_secs(10));
        assert!(result.is_err());
    }

    #[test]
    fn test_modules_empty_no_error() {
        // Providing an empty modules list should not affect normal query execution.
        let req = ApiRequest {
            query: ".h".to_string(),
            input: Some("# Title".to_string()),
            input_format: Some(InputFormat::Markdown),
            modules: Some(vec![]),
            args: None,
            output_format: None,
            aggregate: None,
        };
        let result = query(req, std::time::Duration::from_secs(10));
        assert!(result.is_ok());
    }

    #[test]
    fn test_modules_invalid_returns_error() {
        // Providing a non-existent module name should return an error.
        let req = ApiRequest {
            query: ".h".to_string(),
            input: Some("# Title".to_string()),
            input_format: Some(InputFormat::Markdown),
            modules: Some(vec!["nonexistent_module_xyz".to_string()]),
            args: None,
            output_format: None,
            aggregate: None,
        };
        let result = query(req, std::time::Duration::from_secs(10));
        assert!(result.is_err());
    }

    #[test]
    fn test_args_variable() {
        // Variables defined via `args` are accessed without `$` prefix in mq queries.
        let mut args = HashMap::new();
        args.insert("myval".to_string(), "test".to_string());
        let req = ApiRequest {
            query: "select(contains(myval))".to_string(),
            input: Some("# test\n\n- test1\n- test2".to_string()),
            input_format: Some(InputFormat::Markdown),
            modules: None,
            args: Some(args),
            output_format: None,
            aggregate: None,
        };
        let result = query(req, std::time::Duration::from_secs(10));
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(!resp.results.is_empty());
    }

    #[test]
    fn test_output_format_html() {
        let req = ApiRequest {
            query: ".h".to_string(),
            input: Some("# Title".to_string()),
            input_format: Some(InputFormat::Markdown),
            modules: None,
            args: None,
            output_format: Some(OutputFormat::Html),
            aggregate: None,
        };
        let result = query(req, std::time::Duration::from_secs(10));
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(!resp.results.is_empty());
        assert!(resp.results[0].contains('<'));
    }

    #[test]
    fn test_output_format_none() {
        let req = ApiRequest {
            query: ".h".to_string(),
            input: Some("# Title".to_string()),
            input_format: Some(InputFormat::Markdown),
            modules: None,
            args: None,
            output_format: Some(OutputFormat::None),
            aggregate: None,
        };
        let result = query(req, std::time::Duration::from_secs(10));
        assert!(result.is_ok());
        assert!(result.unwrap().results.is_empty());
    }

    #[test]
    fn test_batch_query_multiple_documents() {
        let req = BatchApiRequest {
            query: ".h1".to_string(),
            inputs: vec!["# Title One".to_string(), "# Title Two".to_string()],
            input_format: Some(InputFormat::Markdown),
            modules: None,
            args: None,
            output_format: None,
            aggregate: None,
        };
        let result = batch_query(req, std::time::Duration::from_secs(10));
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.items.len(), 2);
        assert!(resp.items[0].error.is_none());
        assert_eq!(resp.items[0].results, vec!["# Title One\n"]);
        assert_eq!(resp.items[1].results, vec!["# Title Two\n"]);
    }

    #[test]
    fn test_batch_query_preserves_order_and_isolates_errors() {
        let req = BatchApiRequest {
            query: "invalid query".to_string(),
            inputs: vec!["# Title One".to_string()],
            input_format: Some(InputFormat::Markdown),
            modules: None,
            args: None,
            output_format: None,
            aggregate: None,
        };
        let result = batch_query(req, std::time::Duration::from_secs(10));
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.items.len(), 1);
        assert!(resp.items[0].error.is_some());
        assert!(resp.items[0].results.is_empty());
    }

    #[test]
    fn test_batch_query_empty_inputs() {
        let req = BatchApiRequest {
            query: ".h1".to_string(),
            inputs: vec![],
            input_format: Some(InputFormat::Markdown),
            modules: None,
            args: None,
            output_format: None,
            aggregate: None,
        };
        let result = batch_query(req, std::time::Duration::from_secs(10));
        assert!(result.is_ok());
        assert!(result.unwrap().items.is_empty());
    }

    #[test]
    fn test_batch_query_exceeds_max_size() {
        let req = BatchApiRequest {
            query: ".h1".to_string(),
            inputs: vec!["# Title".to_string(); MAX_BATCH_SIZE + 1],
            input_format: Some(InputFormat::Markdown),
            modules: None,
            args: None,
            output_format: None,
            aggregate: None,
        };
        let result = batch_query(req, std::time::Duration::from_secs(10));
        assert!(result.is_err());
    }

    #[test]
    fn test_check_valid_query() {
        let req = CheckApiRequest {
            query: "upcase".to_string(),
        };
        let resp = check(req);
        assert!(resp.errors.is_empty());
    }

    #[test]
    fn test_check_syntax_error() {
        let req = CheckApiRequest {
            query: "def f(: 1;".to_string(),
        };
        let resp = check(req);
        assert!(!resp.errors.is_empty());
        assert!(resp.errors.iter().any(|e| e.kind == "syntax_error"));
    }

    #[test]
    fn test_format_valid() {
        let req = FormatApiRequest {
            query: "if(a):1 elif(b):2 else:3".to_string(),
            indent_width: None,
            sort_imports: None,
            sort_functions: None,
            sort_fields: None,
            max_width: None,
        };
        let result = format_query(req);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().formatted, "if (a): 1 elif (b): 2 else: 3");
    }

    #[test]
    fn test_format_with_options() {
        let req = FormatApiRequest {
            query: "if(a):1 elif(b):2 else:3".to_string(),
            indent_width: Some(4),
            sort_imports: Some(false),
            sort_functions: Some(false),
            sort_fields: Some(false),
            max_width: None,
        };
        let result = format_query(req);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_functions() {
        let resp = list_functions();
        assert!(!resp.functions.is_empty());
        assert!(resp.functions.iter().any(|f| f.name == "halt"));
    }

    #[test]
    fn test_list_selectors() {
        let resp = list_selectors();
        assert!(!resp.selectors.is_empty());
        assert!(resp.selectors.iter().any(|s| s.name == ".h"));
    }

    #[test]
    fn test_lint_unused_variable() {
        let req = LintApiRequest {
            query: "let x = .h1 | .text".to_string(),
        };
        let resp = lint(req);
        assert!(resp.diagnostics.iter().any(|d| d.rule_id == "unused_variable"));
    }

    #[test]
    fn test_lint_clean_query_has_no_diagnostics() {
        let req = LintApiRequest {
            query: ".h1".to_string(),
        };
        let resp = lint(req);
        assert!(resp.diagnostics.is_empty());
    }
}
