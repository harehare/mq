use mq_lang::Engine;
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Result,
    parse::{Parse, ParseStream},
};

struct MqArgs {
    code: syn::LitStr,
    input: syn::LitStr,
}
impl Parse for MqArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        // Parse the first string literal
        let code: syn::LitStr = input.parse()?;

        // Parse the comma separator
        input.parse::<syn::Token![,]>()?;

        // Parse the second string literal
        let content: syn::LitStr = input.parse()?;

        // Check if there are any more tokens (which would be an error)
        if !input.is_empty() {
            return Err(syn::Error::new(input.span(), "Expected exactly 2 arguments"));
        }

        Ok(MqArgs { code, input: content })
    }
}

#[proc_macro]
pub fn mq_eval(input: TokenStream) -> TokenStream {
    let mq_args = syn::parse_macro_input!(input as MqArgs);
    let mut engine = Engine::default();
    engine.load_builtin_module();

    if let Err(e) = engine.eval(&mq_args.code.value(), vec!["".into()].into_iter()) {
        return syn::Error::new_spanned(mq_args.code, e.cause.to_string())
            .to_compile_error()
            .into();
    }

    if let Err(e) = mq_markdown::Markdown::from_markdown_str(&mq_args.input.value()) {
        return syn::Error::new_spanned(mq_args.input, e).to_compile_error().into();
    }

    let code_lit = mq_args.code;
    let input_lit = mq_args.input;
    let generate = {
        quote! {
            {
                let mut engine = ::mq_lang::Engine::default();
                engine.load_builtin_module();

                let code = #code_lit;
                let input = #input_lit;
                let input = mq_markdown::Markdown::from_markdown_str(&input)
                    .unwrap()
                    .nodes
                    .into_iter()
                    .map(mq_lang::RuntimeValue::from)
                    .collect::<Vec<_>>();

                engine.eval(code, input.into_iter())
            }
        }
    };
    generate.into()
}
