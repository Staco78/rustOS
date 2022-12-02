#![feature(proc_macro_diagnostic)]
#![feature(proc_macro_def_site)]

use proc_macro::{Diagnostic, Level, Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{ForeignItemFn, ForeignItemStatic, Ident, Token, VisPublic, Visibility};

extern crate proc_macro;

#[proc_macro_attribute]
pub fn module(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemStatic);
    let name = &input.ident;
    let result = quote! {
        #input

        #[no_mangle]
        pub static MODULE_NAME: &str = env!("CARGO_PKG_NAME");

        #[no_mangle]
        pub static MODULE: &dyn Module = &#name;
    };
    result.into()
}

#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn: Result<syn::ItemFn, _> = syn::parse(Clone::clone(&item));
    let input_static: Result<syn::ItemStatic, _> = syn::parse(Clone::clone(&item));
    let (name, input, foreign_input) = if let Ok(input_fn) = input_fn {
        let name = if attr.is_empty() {
            input_fn.sig.ident.to_string()
        } else {
            attr.to_string()
        };
        let stream = input_fn.to_token_stream();
        let mut sig = input_fn.sig;
        sig.ident = Ident::new(&name, Span::call_site().into());
        let stream_foreign = ForeignItemFn {
            attrs: Vec::new(),
            vis: Visibility::Public(VisPublic {
                pub_token: Token![pub]([Span::def_site().into()]),
            }),
            sig,
            semi_token: Token![;]([Span::def_site().into()]),
        }
        .into_token_stream();

        (name, stream, stream_foreign)
    } else if let Ok(input_static) = input_static {
        let name = if attr.is_empty() {
            input_static.ident.to_string()
        } else {
            attr.to_string()
        };
        let stream = input_static.to_token_stream();
        let stream_foreign = ForeignItemStatic {
            attrs: Vec::new(),
            vis: Visibility::Public(VisPublic {
                pub_token: Token![pub]([Span::def_site().into()]),
            }),
            static_token: input_static.static_token,
            mutability: input_static.mutability,
            ident: Ident::new(&name, Span::call_site().into()),
            colon_token: input_static.colon_token,
            ty: input_static.ty,
            semi_token: Token![;]([Span::def_site().into()]),
        }
        .into_token_stream();
        (name, stream, stream_foreign)
    } else {
        let span = Span::call_site();
        Diagnostic::spanned(
            span,
            Level::Error,
            "This macro should only be applied on fn or static.",
        )
        .emit();
        return item;
    };
    let export_name = format_ident!("__{}__", name);
    let export_data = foreign_input.to_string();
    let export_data = export_data.as_bytes();
    let export_data_len = export_data.len();

    let export_name2 = format_ident!("__{}___", name);
    let export_data2 = name.as_bytes();
    let export_data_len2 = export_data2.len();
    quote! {
        #[no_mangle]
        #[export_name = #name]
        #input

        #[used]
        #[link_section = ".defs_exports"]
        #[allow(non_snake_case)]
        static #export_name: [u8; #export_data_len + 1] = [#(#export_data),*, b' '];

        #[used]
        #[link_section = ".sym_exports"]
        static #export_name2: [u8; #export_data_len2 + 1] = [#(#export_data2),*, b'\n'];
    }
    .into()
}
