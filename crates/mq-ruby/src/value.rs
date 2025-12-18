use magnus::{Error, Module, RModule, Ruby};
use std::{collections::HashMap, fmt};

// ============================================================================
// InputFormat enum
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum InputFormat {
    #[default]
    Markdown,
    Mdx,
    Text,
    Html,
    Raw,
    Null,
}

impl InputFormat {
    pub fn define_constants(ruby: &Ruby, mq_module: RModule) -> Result<(), Error> {
        let class = ruby.define_class("InputFormat", ruby.class_object())?;
        mq_module.const_set("InputFormat", class)?;
        class.const_set("MARKDOWN", 0)?;
        class.const_set("MDX", 1)?;
        class.const_set("TEXT", 2)?;
        class.const_set("HTML", 3)?;
        class.const_set("RAW", 4)?;
        class.const_set("NULL", 5)?;
        Ok(())
    }

    pub fn from_i32(val: i32) -> Self {
        match val {
            0 => InputFormat::Markdown,
            1 => InputFormat::Mdx,
            2 => InputFormat::Text,
            3 => InputFormat::Html,
            4 => InputFormat::Raw,
            5 => InputFormat::Null,
            _ => InputFormat::Markdown, // Default
        }
    }
}

// ============================================================================
// MQValue - internal representation
// ============================================================================

#[derive(Debug, Clone)]
pub enum MQValue {
    Array { value: Vec<MQValue> },
    Dict { value: HashMap<String, MQValue> },
    Markdown { value: String },
}

impl fmt::Display for MQValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MQValue::Array { value } => write!(
                f,
                "{}",
                value.iter().map(|val| val.text()).collect::<Vec<String>>().join("\n")
            ),
            MQValue::Dict { value } => write!(
                f,
                "{}",
                value
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.text()))
                    .collect::<Vec<String>>()
                    .join("\n")
            ),
            MQValue::Markdown { value } => write!(f, "{}", value),
        }
    }
}

impl MQValue {
    pub fn text(&self) -> String {
        self.to_string()
    }

    pub fn is_empty(&self) -> bool {
        match self {
            MQValue::Array { value } => value.is_empty(),
            MQValue::Dict { value } => value.is_empty(),
            MQValue::Markdown { value } => value.is_empty(),
        }
    }
}

impl From<mq_lang::RuntimeValue> for MQValue {
    fn from(value: mq_lang::RuntimeValue) -> Self {
        match value {
            mq_lang::RuntimeValue::Array(arr) => MQValue::Array {
                value: arr.into_iter().map(|v| v.into()).collect(),
            },
            mq_lang::RuntimeValue::Dict(map) => MQValue::Dict {
                value: map.into_iter().map(|(k, v)| (k.as_str(), v.into())).collect(),
            },
            mq_lang::RuntimeValue::Markdown(node, _) => MQValue::Markdown {
                value: node.to_string(),
            },
            mq_lang::RuntimeValue::String(s) => MQValue::Markdown { value: s },
            mq_lang::RuntimeValue::Symbol(i) => MQValue::Markdown { value: i.as_str() },
            mq_lang::RuntimeValue::Number(n) => MQValue::Markdown { value: n.to_string() },
            mq_lang::RuntimeValue::Boolean(b) => MQValue::Markdown { value: b.to_string() },
            mq_lang::RuntimeValue::Function(..)
            | mq_lang::RuntimeValue::NativeFunction(..)
            | mq_lang::RuntimeValue::Module(..) => MQValue::Markdown { value: "".to_string() },
            mq_lang::RuntimeValue::None => MQValue::Markdown { value: "".to_string() },
        }
    }
}
