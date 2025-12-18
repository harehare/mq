use crate::value::MQValue;
use magnus::{DataTypeFunctions, Error, RModule, Ruby, TypedData, Value, method, prelude::*};

/// Result of an mq query execution
#[derive(Debug, Clone, TypedData)]
#[magnus(class = "MQ::Result", free_immediately, mark)]
pub struct MQResult {
    pub values: Vec<MQValue>,
}

impl DataTypeFunctions for MQResult {}

impl MQResult {
    /// Define the MQResult class in Ruby
    pub fn define_class(ruby: &Ruby, mq_module: RModule) -> Result<(), Error> {
        let class = mq_module.define_class("Result", ruby.class_object())?;
        class.define_method("text", method!(MQResult::text, 0))?;
        class.define_method("values", method!(MQResult::values_as_strings, 0))?;
        class.define_method("length", method!(MQResult::len, 0))?;
        class.define_method("[]", method!(MQResult::get_at, 1))?;
        class.define_method("each", method!(MQResult::each, 0))?;
        Ok(())
    }

    /// Get the text representation of all values joined by newlines
    pub fn text(&self) -> String {
        self.values
            .iter()
            .filter_map(|value| if value.is_empty() { None } else { Some(value.text()) })
            .collect::<Vec<String>>()
            .join("\n")
    }

    /// Get an array of text values
    pub fn values_as_strings(&self) -> Vec<String> {
        self.values
            .iter()
            .filter_map(|value| if value.is_empty() { None } else { Some(value.text()) })
            .collect()
    }

    /// Get the number of values
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if the result is empty
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Access a value by index (for Ruby)
    fn get_at(&self, idx: usize) -> Result<String, Error> {
        if idx < self.values.len() {
            Ok(self.values[idx].text())
        } else {
            let ruby = Ruby::get().unwrap();
            Err(Error::new(
                ruby.exception_runtime_error(),
                format!("Index {} out of range for MQResult with length {}", idx, self.len()),
            ))
        }
    }

    /// Iterator for Ruby each method
    fn each(&self) -> Result<Value, Error> {
        let ruby = Ruby::get().unwrap();
        let block = ruby.block_proc()?;

        for value in &self.values {
            let text = value.text();
            block.call::<(String,), Value>((text,))?;
        }

        Ok(ruby.qnil().as_value())
    }
}

impl From<Vec<MQValue>> for MQResult {
    fn from(values: Vec<MQValue>) -> Self {
        Self { values }
    }
}
