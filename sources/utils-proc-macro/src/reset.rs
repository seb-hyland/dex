use quote::quote;
use synstructure::{AddBounds, BindStyle, Structure};

pub fn reset_derive(mut s: Structure) -> proc_macro2::TokenStream {
    s.bind_with(|_| BindStyle::Ref);
    s.add_bounds(AddBounds::Fields);

    // Call `reset` on each field
    let body = s.each(|bi| quote!(utils::Reset::reset(#bi);));

    s.gen_impl(quote! {
        gen impl utils::Reset for @Self {
            fn reset(&self) {
                match *self { #body }
            }
        }
    })
}
