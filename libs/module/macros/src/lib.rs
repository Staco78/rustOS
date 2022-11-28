use proc_macro::TokenStream;
use quote::quote;

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
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let name = if attr.is_empty() {
        input.sig.ident.to_string()
    } else {
        attr.to_string()
    };
    quote! {
        #[no_mangle]
        #[export_name = #name]
        #input
    }
    .into()
}
