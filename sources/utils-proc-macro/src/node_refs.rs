use quote::quote;
use syn::Attribute;
use synstructure::{AddBounds, BindStyle, Structure};

/// Whether a field is marked `#[uid_ref]`: a pointer to a node owned elsewhere,
/// which a clone rewrites but must not follow.
fn is_reference(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("uid_ref"))
}

pub fn node_refs_derive(s: Structure) -> proc_macro2::TokenStream {
    let mut visit = s.clone();
    visit.bind_with(|_| BindStyle::Ref);
    visit.add_bounds(AddBounds::Fields);
    let visit_body = visit.each(|bi| {
        if is_reference(&bi.ast().attrs) {
            // A reference is not a child; a deep clone must not follow it.
            quote!()
        } else {
            quote!(::dex_core::refs::NodeRefs::owned_refs(#bi, __f);)
        }
    });

    let mut remap = s.clone();
    remap.bind_with(|_| BindStyle::RefMut);
    remap.add_bounds(AddBounds::Fields);
    // Both kinds are rewritten: ownership decides traversal, not identity. That
    // is what lets a back- or self-reference follow the copy when its target
    // came along, while a pointer out of the copied set stays put.
    let remap_body = remap.each(|bi| quote!(::dex_core::refs::NodeRefs::remap_refs(#bi, __map);));

    s.gen_impl(quote! {
        gen impl ::dex_core::refs::NodeRefs for @Self {
            fn owned_refs(&self, __f: &mut dyn FnMut(::dex_core::NodeUid)) {
                match *self { #visit_body }
            }

            fn remap_refs(
                &mut self,
                __map: &::std::collections::HashMap<::dex_core::NodeUid, ::dex_core::NodeUid>,
            ) {
                match *self { #remap_body }
            }
        }
    })
}
