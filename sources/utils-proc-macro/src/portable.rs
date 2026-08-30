use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{DeriveInput, Path, Token, parse_macro_input, punctuated::Punctuated};

pub fn portable_impl(attr_tokens: TokenStream, body_tokens: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr_tokens with Punctuated::<Path, Token![,]>::parse_terminated);

    // `noop_reset` is a behaviour flag; everything else is an extra derive.
    let mut noop_reset = false;
    let mut extra_derives: Vec<&Path> = Vec::new();
    for path in &args {
        if path.is_ident("noop_reset") {
            noop_reset = true;
        } else {
            extra_derives.push(path);
        }
    }
    let extra = if extra_derives.is_empty() {
        quote!()
    } else {
        quote!(, #(#extra_derives),*)
    };

    // Re-emit the item verbatim; parse to reach the ident + generics (for no-op `Reset`).
    let body: TokenStream2 = body_tokens.into();
    let input = match syn::parse2::<DeriveInput>(body.clone()) {
        Ok(input) => input,
        Err(e) => return e.to_compile_error().into(),
    };

    if noop_reset {
        let ident = &input.ident;
        let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
        quote! {
            #[derive(Clone, ::serde::Serialize, ::serde::Deserialize, ::utils::NodeRefs #extra)]
            #body

            impl #impl_generics ::utils::Reset for #ident #ty_generics #where_clause {
                #[inline(always)]
                fn reset(&self) {}
            }
        }
        .into()
    } else {
        quote! {
            #[derive(Clone, ::serde::Serialize, ::serde::Deserialize, ::utils::Reset, ::utils::NodeRefs #extra)]
            #body
        }
        .into()
    }
}
