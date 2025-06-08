use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, Serialize, ToSchema, Clone, Debug)]
pub struct ApiRequest {
    #[schema(example = ".h")]
    pub query: String,
    #[schema(example = "## Markdown Content\n\nThis is an example markdown string.")]
    pub input: String,
    pub input_format: Option<InputFormat>,
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

/// Execute an mq query against the given request.
pub fn execute(request: ApiRequest) -> miette::Result<mq_lang::Values> {
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();

    let input = match request.input_format.unwrap_or(InputFormat::Markdown) {
        format @ (InputFormat::Markdown | InputFormat::Mdx) => {
            let md = if matches!(format, InputFormat::Mdx) {
                mq_markdown::Markdown::from_mdx_str(&request.input)
            } else {
                request.input.parse::<mq_markdown::Markdown>()
            }?;

            md.nodes
                .into_iter()
                .map(mq_lang::Value::from)
                .collect::<Vec<_>>()
        }
        InputFormat::Text => request
            .input
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
            input: "# Title\n\nContent".to_string(),
            input_format: Some(InputFormat::Markdown),
        };
        let result = execute(req);
        assert!(result.is_ok());
        let values = result.unwrap();
        assert!(!values.is_empty());
    }

    #[test]
    fn test_execute_text() {
        let req = ApiRequest {
            query: ".h".to_string(),
            input: "line1\nline2".to_string(),
            input_format: Some(InputFormat::Text),
        };
        let result = execute(req);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_invalid_query() {
        let req = ApiRequest {
            query: "invalid query".to_string(),
            input: "# Title".to_string(),
            input_format: Some(InputFormat::Markdown),
        };
        let result = execute(req);
        assert!(result.is_err());
    }
}
