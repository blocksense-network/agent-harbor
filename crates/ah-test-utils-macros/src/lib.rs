// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Procedural macros enabling unified test logging infrastructure.
//!
//! These macros wrap test functions with [`ah_test_utils::TestLoggerGuard`]
//! ensuring that every test automatically creates a unique log file, logs
//! success/failure metadata, and adheres to the project guidelines from
//! `AGENTS.md`.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{ItemFn, ReturnType, Type, parse_macro_input, spanned::Spanned};

/// Attribute macro for synchronous tests.
///
/// Apply to a function in place of `#[ah_test_utils::logged_test]`:
///
/// ```rust
/// use ah_test_utils::logged_test;
///
/// #[logged_test]
/// fn my_test() {
///     logger.log("running").unwrap();
/// }
/// ```
#[proc_macro_attribute]
pub fn logged_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            Span::call_site(),
            "#[logged_test] does not accept arguments",
        )
        .to_compile_error()
        .into();
    }

    let input = parse_macro_input!(item as ItemFn);

    if let Some(async_token) = &input.sig.asyncness {
        return syn::Error::new(
            async_token.span(),
            "#[logged_test] cannot be applied to async functions; use #[logged_tokio_test] instead",
        )
        .to_compile_error()
        .into();
    }

    generate_logged_test(input, quote! { #[::core::prelude::v1::test] })
}

/// Attribute macro for asynchronous Tokio tests.
///
/// Usage mirrors `#[ah_test_utils::logged_tokio_test]` and supports the same optional arguments.
#[proc_macro_attribute]
pub fn logged_tokio_test(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_tokens = TokenStream2::from(attr);
    let input = parse_macro_input!(item as ItemFn);

    if input.sig.asyncness.is_none() {
        return syn::Error::new(
            input.sig.ident.span(),
            "#[logged_tokio_test] requires an async function",
        )
        .to_compile_error()
        .into();
    }

    let tokio_attr = build_tokio_attribute(&attr_tokens);
    generate_logged_async_test(input, tokio_attr)
}

fn build_tokio_attribute(args: &TokenStream2) -> proc_macro2::TokenStream {
    if args.is_empty() {
        quote! { #[::tokio::test] }
    } else {
        quote! { #[::tokio::test( #args )] }
    }
}

fn generate_logged_test(mut input: ItemFn, harness_attr: proc_macro2::TokenStream) -> TokenStream {
    input.attrs.retain(|attr| !is_logged_attr(attr));

    let fn_ident = &input.sig.ident;
    let fn_name = fn_ident.to_string();
    let visibility = &input.vis;
    let generics = &input.sig.generics;
    let inputs = &input.sig.inputs;
    let output = &input.sig.output;

    if !inputs.is_empty() {
        return syn::Error::new(
            fn_ident.span(),
            "#[logged_test] can only be applied to functions without parameters",
        )
        .to_compile_error()
        .into();
    }

    let (return_kind, return_tokens) = classify_return(output);
    let success_body = build_success_body(&return_kind);
    let block = &input.block;
    let other_attrs = &input.attrs;

    let expanded = quote! {
        #harness_attr
        #(#other_attrs)*
        #visibility fn #fn_ident #generics () #return_tokens {
            let mut __guard = ::ah_test_utils::TestLoggerGuard::new(#fn_name)
                .expect("failed to create TestLogger");
            let mut logger = __guard.logger();
            let _ = &mut logger;

            let inner_result = { #block };
            drop(logger);
            #success_body
        }
    };

    expanded.into()
}

fn generate_logged_async_test(
    mut input: ItemFn,
    harness_attr: proc_macro2::TokenStream,
) -> TokenStream {
    input.attrs.retain(|attr| !is_logged_attr(attr));

    let fn_ident = &input.sig.ident;
    let fn_name = fn_ident.to_string();
    let visibility = &input.vis;
    let generics = &input.sig.generics;
    let output = &input.sig.output;

    if !input.sig.inputs.is_empty() {
        return syn::Error::new(
            fn_ident.span(),
            "#[logged_tokio_test] can only be applied to functions without parameters",
        )
        .to_compile_error()
        .into();
    }

    let (return_kind, return_tokens) = classify_return(output);
    let success_body = build_success_body(&return_kind);
    let block = &input.block;
    let other_attrs = &input.attrs;

    let expanded = quote! {
        #harness_attr
        #(#other_attrs)*
        #visibility async fn #fn_ident #generics () #return_tokens {
            let mut __guard = ::ah_test_utils::TestLoggerGuard::new(#fn_name)
                .expect("failed to create TestLogger");
            let mut logger = __guard.logger();
            let _ = &mut logger;

            let inner_result = { #block };
            drop(logger);
            #success_body
        }
    };

    expanded.into()
}

#[derive(Clone, Copy)]
enum ReturnKind {
    Unit,
    Result,
    Other,
}

fn classify_return(output: &ReturnType) -> (ReturnKind, proc_macro2::TokenStream) {
    match output {
        ReturnType::Default => (ReturnKind::Unit, quote! {}),
        ReturnType::Type(arrow, ty) => {
            if is_result_type(ty) {
                (ReturnKind::Result, quote! { #arrow #ty })
            } else {
                (ReturnKind::Other, quote! { #arrow #ty })
            }
        }
    }
}

fn is_result_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Result";
        }
    }
    false
}

fn build_success_body(return_kind: &ReturnKind) -> proc_macro2::TokenStream {
    match return_kind {
        ReturnKind::Unit => {
            quote! {
                let _ = inner_result;
                if let Err(e) = __guard.finish_success() {
                    panic!("failed to finalize TestLogger: {}", e);
                }
            }
        }
        ReturnKind::Result => {
            quote! {
                match inner_result {
                    ::std::result::Result::Ok(value) => {
                        if let Err(e) = __guard.finish_success() {
                            panic!("failed to finalize TestLogger: {}", e);
                        }
                        ::std::result::Result::Ok(value)
                    }
                    ::std::result::Result::Err(err) => {
                        let __err_msg = format!("{}", err);
                        if let Err(e) = __guard.finish_failure(&__err_msg) {
                            eprintln!("failed to finalize TestLogger after error: {}", e);
                        }
                        ::std::result::Result::Err(err)
                    }
                }
            }
        }
        ReturnKind::Other => {
            quote! {
                let value = inner_result;
                if let Err(e) = __guard.finish_success() {
                    panic!("failed to finalize TestLogger: {}", e);
                }
                value
            }
        }
    }
}

fn is_logged_attr(attr: &syn::Attribute) -> bool {
    if let Some(ident) = attr.path().get_ident() {
        let name = ident.to_string();
        name == "logged_test" || name == "logged_tokio_test"
    } else {
        false
    }
}
