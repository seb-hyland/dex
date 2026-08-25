use proc_macro::TokenStream;
use proc_macro2::TokenStream as TS2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DeriveInput, Fields, FnArg, ImplItem, ItemImpl, Type, Visibility,
    parse_macro_input,
};

/// Strip every `#[dynamic(...)]` helper attribute from `attrs`, returning whether any of them requested `skip`.
fn take_dynamic_skip(attrs: &mut Vec<Attribute>) -> bool {
    let mut skip = false;
    let mut kept = Vec::with_capacity(attrs.len());
    for attr in attrs.drain(..) {
        if attr.path().is_ident("dynamic") {
            // Best-effort parse of the nested flags; unknown shapes are ignored.
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("skip") {
                    skip = true;
                }
                Ok(())
            });
            // Drop the helper attribute so it does not reach the compiler.
        } else {
            kept.push(attr);
        }
    }
    *attrs = kept;
    skip
}

/// The final path-segment ident of a type, e.g. `Vector` for `crate::Vector`.
fn type_ident(ty: &Type) -> Option<syn::Ident> {
    match ty {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.clone()),
        _ => None,
    }
}

/// Only owned path types are bindable across the FFI; references, slices, `impl Trait`, etc. are skipped.
fn is_bindable(ty: &Type) -> bool {
    matches!(ty, Type::Path(_))
}

/// A `NodeUid<T>` type.
/// Should be surfaced to scripts as the dynamic opaque `NodeHandle`.
fn is_node_uid(ty: &Type) -> bool {
    if let Type::Path(p) = ty {
        return p.path.segments.last().is_some_and(|s| s.ident == "NodeUid");
    }
    false
}

/// A `Vec<NodeUid<T>>` type.
/// Should be surfaced to scripts as `Vec<NodeHandle>`.
fn is_node_uid_vec(ty: &Type) -> bool {
    let Type::Path(p) = ty else { return false };
    let Some(seg) = p.path.segments.last() else {
        return false;
    };
    if seg.ident != "Vec" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return false;
    };
    args.args
        .iter()
        .any(|a| matches!(a, syn::GenericArgument::Type(t) if is_node_uid(t)))
}

/// The single type argument of a `Vec<...>`, if `ty` is one.
fn vec_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != "Vec" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    })
}

/// A `Vec<T>` whose element matches `pred`.
fn is_vec_of(ty: &Type, pred: fn(&Type) -> bool) -> bool {
    vec_inner(ty).is_some_and(pred)
}

/// An `Arc<dyn Node>` type: surfaced to scripts as any dynamic value and mapped back to a node.
fn is_arc_dyn_node(ty: &Type) -> bool {
    let Type::Path(p) = ty else { return false };
    let Some(seg) = p.path.segments.last() else {
        return false;
    };
    if seg.ident != "Arc" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return false;
    };
    args.args.iter().any(|a| match a {
        syn::GenericArgument::Type(Type::TraitObject(t)) => t.bounds.iter().any(|b| {
            matches!(b, syn::TypeParamBound::Trait(tr)
                if tr.path.segments.last().is_some_and(|s| s.ident == "Node"))
        }),
        _ => false,
    })
}

/// A `LayoutChild` type: surfaced to scripts as any dynamic value or node handle.
fn is_layout_child(ty: &Type) -> bool {
    matches!(ty, Type::Path(p)
        if p.path.segments.last().is_some_and(|s| s.ident == "LayoutChild"))
}

pub fn dynamic_type_impl(attr: TokenStream, body: TokenStream) -> TokenStream {
    // `#[dynamic_type(name = "...")]` overrides the script-facing name.
    let mut name_override: Option<String> = None;
    let name_parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("name") {
            name_override = Some(meta.value()?.parse::<syn::LitStr>()?.value());
            Ok(())
        } else {
            Err(meta.error("unsupported #[dynamic_type] option"))
        }
    });
    parse_macro_input!(attr with name_parser);

    let mut input = parse_macro_input!(body as DeriveInput);
    let name = input.ident.clone();
    let script_name = name_override.unwrap_or_else(|| name.to_string());

    // Gather Steel field accessors for exposed (`pub`, non-skip) named fields,
    // and annotate those fields with `#[pyo3(get, set)]` for Python.
    let mut steel_accessors = TS2::new();
    if let Data::Struct(data) = &mut input.data
        && let Fields::Named(named) = &mut data.fields
    {
        for field in named.named.iter_mut() {
            let is_pub = matches!(field.vis, Visibility::Public(_));
            let skip = take_dynamic_skip(&mut field.attrs);
            let Some(fname) = field.ident.clone() else {
                continue;
            };
            if !is_pub || skip || !is_bindable(&field.ty) {
                continue;
            }

            field.attrs.push(syn::parse_quote!(#[pyo3(get, set)]));

            let fty = &field.ty;
            let getter = format!("{script_name}-{fname}");
            let setter = format!("set-{script_name}-{fname}!");
            steel_accessors.extend(quote! {
                engine.register_fn(#getter, |v: &#name| ::core::clone::Clone::clone(&v.#fname));
                engine.register_fn(#setter, |v: &mut #name, nv: #fty| { v.#fname = nv; });
            });
        }
    }

    quote! {
        #[::pyo3::pyclass(from_py_object, name = #script_name)]
        #input

        impl ::dex_dynamic::__rt::steel::rvals::Custom for #name {}

        ::dex_dynamic::__rt::inventory::submit! {
            ::dex_dynamic::DynamicBinding {
                name: #script_name,
                register_python: |m| {
                    use ::dex_dynamic::__rt::pyo3::types::PyModuleMethods;
                    m.add_class::<#name>()
                },
                register_steel: |engine| {
                    use ::dex_dynamic::__rt::steel::steel_vm::register_fn::RegisterFn;
                    #steel_accessors
                },
            }
        }
    }
    .into()
}

/// Emit `#[typetag::serde]` for an `impl Node for T`, plus (unless `skip`) an
/// extractor letting a script return a `T` as its output.
pub fn dynamic_node_impl(attr: TokenStream, body: TokenStream) -> TokenStream {
    let mut skip = false;
    let skip_parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("skip") {
            skip = true;
            Ok(())
        } else {
            Err(meta.error("unsupported #[dynamic_node] option"))
        }
    });
    parse_macro_input!(attr with skip_parser);

    let input = parse_macro_input!(body as ItemImpl);
    let self_ty = (*input.self_ty).clone();

    // Only a `#[dynamic_type]` node can be pulled back out of a script value, so
    // `skip` nodes (internal, non-bound) register no extractor.
    let node_extractor = (!skip).then(|| {
        quote! {
            ::dex_dynamic::__rt::inventory::submit! {
                ::dex_core::scripting::NodeExtractor {
                    from_python: |obj| {
                        use ::dex_dynamic::__rt::pyo3::prelude::*;
                        obj.extract::<#self_ty>()
                            .ok()
                            .map(|v| ::std::sync::Arc::new(v) as ::std::sync::Arc<dyn ::dex_core::Node>)
                    },
                    from_steel: |val| {
                        use ::dex_dynamic::__rt::steel::rvals::FromSteelVal;
                        <#self_ty as FromSteelVal>::from_steelval(val)
                            .ok()
                            .map(|v| ::std::sync::Arc::new(v) as ::std::sync::Arc<dyn ::dex_core::Node>)
                    },
                }
            }
        }
    });

    quote! {
        #[::typetag::serde]
        #input

        #node_extractor
    }
    .into()
}

/// The receiver shape of a bound method.
enum Recv {
    /// No receiver: a constructor / associated function.
    None,
    Ref,
    RefMut,
    /// `self` by value.
    Value,
}

pub fn dynamic_methods_impl(_attr: TokenStream, body: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(body as ItemImpl);

    // Only plain inherent impls on a named type get bindings; anything unsupported is re-emitted untouched.
    let self_ty = (*input.self_ty).clone();
    let bindings_supported = input.trait_.is_none()
        && input.generics.params.is_empty()
        && type_ident(&self_ty).is_some();

    if !bindings_supported {
        return quote!(#input).into();
    }
    let type_name = type_ident(&self_ty).unwrap().to_string();

    let mut py_methods = TS2::new();
    let mut steel_regs = TS2::new();

    for item in input.items.iter_mut() {
        let ImplItem::Fn(f) = item else { continue };

        let skip = take_dynamic_skip(&mut f.attrs);
        let is_pub = matches!(f.vis, Visibility::Public(_));
        if skip || !is_pub || !f.sig.generics.params.is_empty() {
            continue;
        }

        // Classify the receiver.
        let recv = match f.sig.receiver() {
            None => Recv::None,
            Some(r) if r.reference.is_none() => Recv::Value,
            Some(r) if r.mutability.is_some() => Recv::RefMut,
            Some(_) => Recv::Ref,
        };

        // Classify each non-receiver arg. Most kinds bind identically for both
        // backends, but node-shaped dynamic args differ (a Python object vs a
        // `SteelVal`), so python and steel keep separate inputs / call exprs.
        let mut py_inputs: Vec<TS2> = Vec::new();
        let mut py_call: Vec<TS2> = Vec::new();
        let mut st_inputs: Vec<TS2> = Vec::new();
        let mut st_call: Vec<TS2> = Vec::new();
        let mut needs_closure = false; // steel must wrap, not bind the bare fn
        let mut bindable = true;
        let mut visible_idx = 0usize;
        for arg in f.sig.inputs.iter() {
            let FnArg::Typed(pt) = arg else { continue };
            let id = format_ident!("arg{visible_idx}");
            visible_idx += 1;

            if is_node_uid(&pt.ty) {
                needs_closure = true;
                py_inputs.push(quote!(#id: ::dex_core::NodeHandle));
                st_inputs.push(quote!(#id: ::dex_core::NodeHandle));
                // `.cast()` re-tags the erased uid to the method's expected `T`.
                py_call.push(quote!(#id.0.cast()));
                st_call.push(quote!(#id.0.cast()));
                continue;
            }
            if is_node_uid_vec(&pt.ty) {
                needs_closure = true;
                py_inputs.push(quote!(#id: ::std::vec::Vec<::dex_core::NodeHandle>));
                st_inputs.push(quote!(#id: ::std::vec::Vec<::dex_core::NodeHandle>));
                py_call.push(quote!(#id.into_iter().map(|__h| __h.0.cast()).collect()));
                st_call.push(quote!(#id.into_iter().map(|__h| __h.0.cast()).collect()));
                continue;
            }
            // `Arc<dyn Node>` accepts any dynamic value, mapped back to a node.
            if is_arc_dyn_node(&pt.ty) {
                needs_closure = true;
                py_inputs.push(quote!(#id: ::pyo3::Bound<'_, ::pyo3::PyAny>));
                st_inputs.push(quote!(#id: ::dex_dynamic::__rt::steel::rvals::SteelVal));
                py_call.push(quote!(::dex_nodes::scripting::to_dyn_node_py(&#id)));
                st_call.push(quote!(::dex_nodes::scripting::to_dyn_node_steel(&#id)));
                continue;
            }
            if is_vec_of(&pt.ty, is_arc_dyn_node) {
                needs_closure = true;
                py_inputs.push(quote!(#id: ::std::vec::Vec<::pyo3::Bound<'_, ::pyo3::PyAny>>));
                st_inputs.push(
                    quote!(#id: ::std::vec::Vec<::dex_dynamic::__rt::steel::rvals::SteelVal>),
                );
                py_call.push(quote!(#id.iter().map(|__v| ::dex_nodes::scripting::to_dyn_node_py(__v)).collect()));
                st_call.push(quote!(#id.iter().map(|__v| ::dex_nodes::scripting::to_dyn_node_steel(__v)).collect()));
                continue;
            }
            // `LayoutChild` also accepts a node handle (kept live) besides a value.
            if is_layout_child(&pt.ty) {
                needs_closure = true;
                py_inputs.push(quote!(#id: ::pyo3::Bound<'_, ::pyo3::PyAny>));
                st_inputs.push(quote!(#id: ::dex_dynamic::__rt::steel::rvals::SteelVal));
                py_call.push(quote!(::dex_nodes::layouts::LayoutChild::from_dynamic_py(&#id)));
                st_call.push(quote!(::dex_nodes::layouts::LayoutChild::from_dynamic_steel(&#id)));
                continue;
            }
            if is_vec_of(&pt.ty, is_layout_child) {
                needs_closure = true;
                py_inputs.push(quote!(#id: ::std::vec::Vec<::pyo3::Bound<'_, ::pyo3::PyAny>>));
                st_inputs.push(
                    quote!(#id: ::std::vec::Vec<::dex_dynamic::__rt::steel::rvals::SteelVal>),
                );
                py_call.push(quote!(#id.iter().map(|__v| ::dex_nodes::layouts::LayoutChild::from_dynamic_py(__v)).collect()));
                st_call.push(quote!(#id.iter().map(|__v| ::dex_nodes::layouts::LayoutChild::from_dynamic_steel(__v)).collect()));
                continue;
            }
            if !is_bindable(&pt.ty) {
                bindable = false;
                break;
            }
            let ty = &pt.ty;
            py_inputs.push(quote!(#id: #ty));
            st_inputs.push(quote!(#id: #ty));
            py_call.push(quote!(#id));
            st_call.push(quote!(#id));
        }
        if !bindable {
            continue;
        }

        // A `NodeUid` return is erased to the `NodeHandle` on the way out.
        let returns_node_uid = matches!(
            &f.sig.output,
            syn::ReturnType::Type(_, ty) if is_node_uid(ty)
        );
        needs_closure |= returns_node_uid;

        // A steel wrapping closure can't carry a receiver, so any method that
        // needs one must be a constructor (no receiver).
        if needs_closure && !matches!(recv, Recv::None) {
            continue;
        }

        let mname = f.sig.ident.clone();
        let mname_str = mname.to_string();
        let wrapper = format_ident!("__dyn_{mname}");
        let steel_name = format!("{type_name}/{mname_str}");
        let wrapper_ret: TS2 = if returns_node_uid {
            quote!(-> ::dex_core::NodeHandle)
        } else {
            let r = &f.sig.output;
            quote!(#r)
        };
        let wrap_result = |call: TS2| {
            if returns_node_uid {
                quote!(::dex_core::NodeHandle(#call.erase()))
            } else {
                call
            }
        };

        // Python wrapper: fixed receiver forms, delegating to the real method.
        let (recv_tok, call_self, static_attr): (Option<TS2>, Option<TS2>, TS2) = match recv {
            Recv::None => (None, None, quote!(#[staticmethod])),
            Recv::Ref => (Some(quote!(&self)), Some(quote!(self)), quote!()),
            Recv::RefMut => (Some(quote!(&mut self)), Some(quote!(self)), quote!()),
            Recv::Value => (
                Some(quote!(&self)),
                Some(quote!(::core::clone::Clone::clone(self))),
                quote!(),
            ),
        };
        let mut wrapper_inputs: Vec<TS2> = Vec::new();
        if let Some(r) = recv_tok {
            wrapper_inputs.push(r);
        }
        wrapper_inputs.extend(py_inputs.iter().cloned());
        let mut wrapper_call: Vec<TS2> = Vec::new();
        if let Some(cs) = call_self {
            wrapper_call.push(cs);
        }
        wrapper_call.extend(py_call.iter().cloned());
        let py_body = wrap_result(quote!(#self_ty::#mname(#(#wrapper_call),*)));

        py_methods.extend(quote! {
            #static_attr
            #[pyo3(name = #mname_str)]
            fn #wrapper(#(#wrapper_inputs),*) #wrapper_ret {
                #py_body
            }
        });

        // Steel: bind the bare fn when possible, else a closure over the visible
        // args (no receiver in this branch) that injects / unwraps / erases.
        if needs_closure {
            let steel_body = wrap_result(quote!(#self_ty::#mname(#(#st_call),*)));
            steel_regs.extend(quote! {
                engine.register_fn(#steel_name, |#(#st_inputs),*| {
                    #steel_body
                });
            });
        } else {
            steel_regs.extend(quote! {
                engine.register_fn(#steel_name, #self_ty::#mname);
            });
        }
    }

    quote! {
        #input

        #[::pyo3::pymethods]
        impl #self_ty {
            #py_methods
        }

        ::dex_dynamic::__rt::inventory::submit! {
            ::dex_dynamic::DynamicBinding {
                name: ::core::concat!(#type_name, " methods"),
                register_python: |_m| ::core::result::Result::Ok(()),
                register_steel: |engine| {
                    use ::dex_dynamic::__rt::steel::steel_vm::register_fn::RegisterFn;
                    #steel_regs
                },
            }
        }
    }
    .into()
}
