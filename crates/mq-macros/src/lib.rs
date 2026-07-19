use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    Attribute, Expr, Ident, LitStr, Meta, Token,
    parse::{Parse, ParseStream, Parser},
    parse_macro_input,
    punctuated::Punctuated,
};

/// Defines a builtin function and creates the corresponding `LazyLock<BuiltinFunction>` static.
///
/// The static name is derived by uppercasing the `name` attribute value.
///
/// # Parameters
/// - `name`: The string name of the builtin (e.g., `"sort_desc"`) — also used as the static name
///   (`SORT_DESC`).
/// - `params`: The `ParamNum` variant without the `ParamNum::` prefix (e.g., `None`, `Fixed(1)`,
///   `Range(0, 255)`).
/// - `capability` (optional): one of `"read"`, `"write"`, `"net"`. When given, a guard is
///   inserted as the first statement of the function body that returns
///   `Err(Error::Runtime(...))` unless the matching `capability::is_{read,write,net}_allowed()`
///   check passes — the same gate every capability-restricted builtin needs, generated instead
///   of hand-written per function.
///
/// # Example
/// ```ignore
/// #[mq_fn(name = "sort_desc", params = Fixed(1))]
/// fn sort_desc_impl(
///     ident: &Ident,
///     _: &RuntimeValue,
///     mut args: Args,
///     _: &SharedEnv,
/// ) -> Result<RuntimeValue, Error> {
///     // implementation
/// }
/// // Generates: static SORT_DESC: LazyLock<BuiltinFunction> = LazyLock::new(|| ...);
/// // Register SORT_DESC in `builtin_dispatch!` to make it callable.
///
/// #[mq_fn(name = "read_file", params = Fixed(1), capability = "read")]
/// fn read_file_impl(...) -> Result<RuntimeValue, Error> {
///     // capability::is_read_allowed() is checked before this body runs.
/// }
/// ```
#[proc_macro_attribute]
pub fn mq_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut item_fn = parse_macro_input!(item as syn::ItemFn);

    let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
    let metas = match parser.parse(attr) {
        Ok(m) => m,
        Err(e) => return e.to_compile_error().into(),
    };

    let mut name_lit: Option<LitStr> = None;
    let mut params_expr: Option<Expr> = None;
    let mut capability_lit: Option<LitStr> = None;

    for meta in &metas {
        match meta {
            Meta::NameValue(nv) if nv.path.is_ident("name") => {
                if let Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s), ..
                }) = &nv.value
                {
                    name_lit = Some(s.clone());
                }
            }
            Meta::NameValue(nv) if nv.path.is_ident("params") => {
                params_expr = Some(nv.value.clone());
            }
            Meta::NameValue(nv) if nv.path.is_ident("capability") => {
                if let Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s), ..
                }) = &nv.value
                {
                    capability_lit = Some(s.clone());
                }
            }
            _ => {
                return syn::Error::new_spanned(meta, "unknown mq_fn attribute key")
                    .to_compile_error()
                    .into();
            }
        }
    }

    let name = match name_lit {
        Some(n) => n,
        None => {
            return syn::Error::new(Span::call_site(), "mq_fn requires `name = \"...\"`")
                .to_compile_error()
                .into();
        }
    };

    let params = match params_expr {
        Some(p) => p,
        None => {
            return syn::Error::new(Span::call_site(), "mq_fn requires `params = ...`")
                .to_compile_error()
                .into();
        }
    };

    if let Some(capability) = &capability_lit {
        let (getter, phrase, verb, flag) = match capability.value().as_str() {
            "read" => ("is_read_allowed", "filesystem reads", "are", "--allow-read"),
            "write" => ("is_write_allowed", "filesystem writes", "are", "--allow-write"),
            "net" => ("is_net_allowed", "network access", "is", "--allow-net"),
            other => {
                return syn::Error::new_spanned(
                    capability,
                    format!("unknown capability {other:?}, expected \"read\", \"write\", or \"net\""),
                )
                .to_compile_error()
                .into();
            }
        };
        let getter_ident = Ident::new(getter, Span::call_site());
        let message = format!("{{name}}: {phrase} {verb} disabled; re-run mq with {flag} to enable {{name}}");
        let check: syn::Stmt = syn::parse_quote! {
            if !capability::#getter_ident() {
                return Err(Error::Runtime(format!(#message, name = #name)));
            }
        };
        item_fn.block.stmts.insert(0, check);
    }

    let fn_ident = &item_fn.sig.ident;
    let static_name = name.value().to_uppercase();
    let static_ident = Ident::new(&static_name, Span::call_site());

    let cfg_attrs: Vec<&Attribute> = item_fn.attrs.iter().filter(|a| a.path().is_ident("cfg")).collect();

    quote! {
        #item_fn

        #(#cfg_attrs)*
        #[allow(non_upper_case_globals)]
        static #static_ident: ::std::sync::LazyLock<BuiltinFunction> =
            ::std::sync::LazyLock::new(|| BuiltinFunction::new(#name, ParamNum::#params, #fn_ident));
    }
    .into()
}

struct BuiltinEntry {
    attrs: Vec<Attribute>,
    ident: Ident,
}

impl Parse for BuiltinEntry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(BuiltinEntry {
            attrs: input.call(Attribute::parse_outer)?,
            ident: input.parse()?,
        })
    }
}

struct BuiltinDispatchInput {
    entries: Punctuated<BuiltinEntry, Token![,]>,
}

impl Parse for BuiltinDispatchInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(BuiltinDispatchInput {
            entries: Punctuated::parse_terminated(input)?,
        })
    }
}

/// Generates FNV-1a hash constants and the `get_builtin_functions_by_str` dispatch function
/// from a compact list of builtin static names.
///
/// The string name used for hashing and lookup is derived by lowercasing the static identifier.
/// The hash constant name strips any leading underscores from the static name (e.g., `_DIFF`
/// becomes `HASH_DIFF`).
///
/// Supports `#[cfg(...)]` attributes on individual entries.
///
/// # Example
/// ```ignore
/// builtin_dispatch! {
///     ABS,
///     ADD,
///     SORT_DESC,
///     #[cfg(feature = "file-io")]
///     READ_FILE,
/// }
/// ```
/// Generates `const HASH_ABS`, `const HASH_ADD`, `const HASH_SORT_DESC`, and
/// `pub fn get_builtin_functions_by_str` with a `match fnv1a_hash_64(name_str)` body.
#[proc_macro]
pub fn builtin_dispatch(input: TokenStream) -> TokenStream {
    let BuiltinDispatchInput { entries } = parse_macro_input!(input as BuiltinDispatchInput);

    let mut hash_consts: Vec<TokenStream2> = Vec::with_capacity(entries.len());
    let mut match_arms: Vec<TokenStream2> = Vec::with_capacity(entries.len());

    for entry in &entries {
        let ident = &entry.ident;
        let name_str = ident.to_string().to_lowercase();
        let hash_name = format!("HASH_{}", ident.to_string().trim_start_matches('_'));
        let hash_ident = Ident::new(&hash_name, ident.span());
        let attrs = &entry.attrs;

        hash_consts.push(quote! {
            #(#attrs)*
            const #hash_ident: u64 = fnv1a_hash_64(#name_str);
        });

        match_arms.push(quote! {
            #(#attrs)*
            #hash_ident => Some(&#ident),
        });
    }

    quote! {
        #(#hash_consts)*

        pub fn get_builtin_functions_by_str(name_str: &str) -> Option<&'static BuiltinFunction> {
            match fnv1a_hash_64(name_str) {
                #(#match_arms)*
                _ => None,
            }
            .filter(|func| func.name == name_str)
            .map(|v| &**v)
        }
    }
    .into()
}
