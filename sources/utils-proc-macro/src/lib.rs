use proc_macro::TokenStream;
use synstructure::decl_derive;

mod dynamic;
mod portable;
mod reset;

decl_derive!([Reset] => reset::reset_derive);

/// Derive standard required trait implementations for usage within dex.
#[proc_macro_attribute]
pub fn portable(attr_tokens: TokenStream, body_tokens: TokenStream) -> TokenStream {
    portable::portable_impl(attr_tokens, body_tokens)
}

/// Bind a type to Python + Steel.
#[proc_macro_attribute]
pub fn dynamic_type(attr_tokens: TokenStream, body_tokens: TokenStream) -> TokenStream {
    dynamic::dynamic_type_impl(attr_tokens, body_tokens)
}

/// Bind an impl block's methods to Python + Steel.
#[proc_macro_attribute]
pub fn dynamic_methods(attr_tokens: TokenStream, body_tokens: TokenStream) -> TokenStream {
    dynamic::dynamic_methods_impl(attr_tokens, body_tokens)
}

/// Mark an `impl Node for T` block to implement `#[typetag::serde]` and allow it to be returned by value.
#[proc_macro_attribute]
pub fn dynamic_node(attr_tokens: TokenStream, body_tokens: TokenStream) -> TokenStream {
    dynamic::dynamic_node_impl(attr_tokens, body_tokens)
}
