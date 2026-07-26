use proc_macro::TokenStream;
use synstructure::decl_derive;

mod dynamic;
mod reset;

decl_derive!([Reset] => reset::reset_derive);

#[proc_macro_attribute]
pub fn dynamic(attr_tokens: TokenStream, body_tokens: TokenStream) -> TokenStream {
    dynamic::dynamic_impl(attr_tokens, body_tokens)
}
