//! Ruby bindings for the mq markdown processing library.
//!
//! This crate provides Ruby bindings for mq, allowing Ruby applications to
//! process markdown, MDX, and HTML using the mq query language.

pub mod result;
pub mod value;

use magnus::{Error, RHash, Ruby, TryConvert, function, prelude::*};
use result::MQResult;
use value::InputFormat;

/// Main entry point for the Ruby extension
#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let mq_module = ruby.define_module("MQ")?;

    // Define input format constants
    InputFormat::define_constants(ruby, mq_module)?;

    // Define result class
    MQResult::define_class(ruby, mq_module)?;

    // Define module functions
    mq_module.define_singleton_method("_run", function!(run, 3))?;
    mq_module.define_singleton_method("_html_to_markdown", function!(html_to_markdown, 2))?;

    Ok(())
}

/// Run an mq query on the provided content
fn run(code: String, content: String, options_hash: Option<RHash>) -> Result<MQResult, Error> {
    let ruby = Ruby::get().unwrap();
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();

    // Parse options from hash
    let input_format = if let Some(opts) = options_hash {
        if let Some(val) = opts.get(ruby.to_symbol("input_format")) {
            let format_val: i32 = TryConvert::try_convert(val)?;
            InputFormat::from_i32(format_val)
        } else {
            InputFormat::Markdown
        }
    } else {
        InputFormat::Markdown
    };

    let input = match input_format {
        InputFormat::Markdown => mq_lang::parse_markdown_input(&content),
        InputFormat::Mdx => mq_lang::parse_mdx_input(&content),
        InputFormat::Text => mq_lang::parse_text_input(&content),
        InputFormat::Html => mq_lang::parse_html_input(&content),
        InputFormat::Raw => Ok(mq_lang::raw_input(&content)),
        InputFormat::Null => Ok(mq_lang::null_input()),
    }
    .map_err(|e| Error::new(ruby.exception_runtime_error(), format!("Error parsing input: {}", e)))?;

    engine
        .eval(&code, input.into_iter())
        .map(|values| MQResult::from(values.into_iter().map(Into::into).collect::<Vec<_>>()))
        .map_err(|e| Error::new(ruby.exception_runtime_error(), format!("Error evaluating query: {}", e)))
}

/// Convert HTML to Markdown
fn html_to_markdown(content: String, options_hash: Option<RHash>) -> Result<String, Error> {
    let ruby = Ruby::get().unwrap();
    let opts = if let Some(opts) = options_hash {
        let extract_scripts = get_bool_option(&ruby, &opts, "extract_scripts_as_code_blocks")?;
        let generate_front = get_bool_option(&ruby, &opts, "generate_front_matter")?;
        let use_title = get_bool_option(&ruby, &opts, "use_title_as_h1")?;

        mq_markdown::ConversionOptions {
            extract_scripts_as_code_blocks: extract_scripts,
            generate_front_matter: generate_front,
            use_title_as_h1: use_title,
        }
    } else {
        mq_markdown::ConversionOptions::default()
    };

    mq_markdown::convert_html_to_markdown(&content, opts).map_err(|e| {
        Error::new(
            ruby.exception_runtime_error(),
            format!("Error converting HTML to Markdown: {}", e),
        )
    })
}

fn get_bool_option(ruby: &Ruby, hash: &RHash, key: &str) -> Result<bool, Error> {
    if let Some(val) = hash.get(ruby.to_symbol(key)) {
        TryConvert::try_convert(val)
    } else {
        Ok(false)
    }
}
