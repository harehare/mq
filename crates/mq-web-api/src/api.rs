use miette::miette;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, Serialize, ToSchema, Clone, Debug)]
pub struct ApiRequest {
    #[schema(example = ".h")]
    pub query: String,
    #[schema(example = "## Markdown Content\n\nThis is an example markdown string.")]
    pub input: Option<String>,
    pub input_format: Option<InputFormat>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QueryApiResponse {
    pub results: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DiagnosticsApiResponse {
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum InputFormat {
    #[serde(rename = "markdown")]
    Markdown,
    #[serde(rename = "mdx")]
    Mdx,
    #[serde(rename = "text")]
    Text,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    message: String,
}

pub fn query(request: ApiRequest) -> miette::Result<QueryApiResponse> {
    let results = execute_query(request);
    match results {
        Ok(values) => {
            let response = values
                .into_iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>();

            Ok(QueryApiResponse { results: response })
        }
        Err(e) => Err(miette!(format!("Execution error: {}", e))),
    }
}

pub fn diagnostics(request: ApiRequest) -> DiagnosticsApiResponse {
    let (_, errors) = mq_lang::parse_recovery(&request.query);
    DiagnosticsApiResponse {
        diagnostics: errors
            .error_ranges(&request.query)
            .iter()
            .map(|(message, range)| Diagnostic {
                start_line: range.start.line,
                start_column: range.start.column as u32,
                end_line: range.end.line,
                end_column: range.end.column as u32,
                message: message.to_owned(),
            })
            .collect::<Vec<_>>(),
    }
}

fn execute_query(request: ApiRequest) -> miette::Result<mq_lang::Values> {
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();

    let input = match request.input_format.unwrap_or(InputFormat::Markdown) {
        format @ (InputFormat::Markdown | InputFormat::Mdx) => {
            let md = if matches!(format, InputFormat::Mdx) {
                mq_markdown::Markdown::from_mdx_str(&request.input.unwrap_or_default())
            } else {
                request
                    .input
                    .unwrap_or_default()
                    .parse::<mq_markdown::Markdown>()
            }?;

            md.nodes
                .into_iter()
                .map(mq_lang::Value::from)
                .collect::<Vec<_>>()
        }
        InputFormat::Text => request
            .input
            .unwrap_or_default()
            .lines()
            .map(mq_lang::Value::from)
            .collect::<Vec<_>>(),
    };

    engine
        .eval(&request.query, input.into_iter())
        .map_err(|e| miette::miette!("Error executing query: {}", e))
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
        };
        let result = query(req);
        assert!(result.is_err());
    }
}
