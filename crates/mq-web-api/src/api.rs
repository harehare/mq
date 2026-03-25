use std::collections::HashMap;

use miette::miette;
use mq_formatter::{Formatter, FormatterConfig};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QueryApiResponse {
    pub results: Vec<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone, Default)]
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
}

/// Response body for `POST /api/format`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FormatApiResponse {
    pub formatted: String,
}

pub fn query(request: ApiRequest) -> miette::Result<QueryApiResponse> {
    execute_query(request)
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
    };
    let formatted = Formatter::new(Some(config))
        .format(&request.query)
        .map_err(|e| miette!("Format error: {}", e))?;
    Ok(FormatApiResponse { formatted })
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

fn execute_query(request: ApiRequest) -> miette::Result<QueryApiResponse> {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();

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

    let query = if request.aggregate.unwrap_or(false) {
        format!(r#"nodes | import "section" | {}"#, request.query)
    } else {
        request.query.clone()
    };

    let input = match request.input_format.unwrap_or(InputFormat::Markdown) {
        InputFormat::Markdown => mq_lang::parse_markdown_input(&request.input.unwrap_or_default())?,
        InputFormat::Mdx => mq_lang::parse_mdx_input(&request.input.unwrap_or_default())?,
        InputFormat::Text => mq_lang::parse_text_input(&request.input.unwrap_or_default())?,
        InputFormat::Html => mq_lang::parse_html_input(&request.input.unwrap_or_default())?,
        InputFormat::Raw => mq_lang::raw_input(&request.input.unwrap_or_default()),
        InputFormat::Null => mq_lang::null_input(),
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
    };

    Ok(QueryApiResponse { results })
}

fn collect_markdown_nodes(value: &mq_lang::RuntimeValue, nodes: &mut Vec<mq_markdown::Node>) {
    match value {
        mq_lang::RuntimeValue::Markdown(node, _) => nodes.push(node.clone()),
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
        mq_lang::RuntimeValue::Markdown(node, _) => vec![node.clone()],
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
        let result = query(req);
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
        let result = query(req);
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
        let result = query(req);
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
        let result = query(req);
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
        let result = query(req);
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
        let result = query(req);
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
        let result = query(req);
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
        let result = query(req);
        assert!(result.is_ok());
        assert!(result.unwrap().results.is_empty());
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
        };
        let result = format_query(req);
        assert!(result.is_ok());
    }
}
