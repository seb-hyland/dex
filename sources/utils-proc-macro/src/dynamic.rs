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

        // Classify each non-receiver arg: a `NodeUid`/`Vec<NodeUid>`, a bindable arg, or an unsupported type (skip the method).
        let mut visible_inputs: Vec<TS2> = Vec::new(); // `argN: Ty` for script args
        let mut full_call: Vec<TS2> = Vec::new(); // original arg order
        let mut has_node_uid_arg = false;
        let mut bindable = true;
        let mut visible_idx = 0usize;
        for arg in f.sig.inputs.iter() {
            let FnArg::Typed(pt) = arg else { continue };
            let id = format_ident!("arg{visible_idx}");
            visible_idx += 1;
            if is_node_uid(&pt.ty) {
                has_node_uid_arg = true;
                visible_inputs.push(quote!(#id: ::dex_core::NodeHandle));
                // `.cast()` re-tags the erased uid to the method's expected `T`.
                full_call.push(quote!(#id.0.cast()));
                continue;
            }
            if is_node_uid_vec(&pt.ty) {
                has_node_uid_arg = true;
                visible_inputs.push(quote!(#id: ::std::vec::Vec<::dex_core::NodeHandle>));
                full_call.push(quote!(#id.into_iter().map(|__h| __h.0.cast()).collect()));
                continue;
            }
            if !is_bindable(&pt.ty) {
                bindable = false;
                break;
            }
            let ty = &pt.ty;
            visible_inputs.push(quote!(#id: #ty));
            full_call.push(quote!(#id));
        }
        if !bindable {
            continue;
        }

        // A `NodeUid` return is erased to the `NodeHandle` on the way out.
        let returns_node_uid = matches!(
            &f.sig.output,
            syn::ReturnType::Type(_, ty) if is_node_uid(ty)
        );

        // Steel needs a wrapping closure (not the bare fn) whenever we unwrap a
        // `NodeHandle` arg or erase the result. These only apply to constructors
        // (no receiver); skip the rare `self` combo.
        let needs_closure = has_node_uid_arg || returns_node_uid;
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
        wrapper_inputs.extend(visible_inputs.iter().cloned());
        let mut wrapper_call: Vec<TS2> = Vec::new();
        if let Some(cs) = call_self {
            wrapper_call.push(cs);
        }
        wrapper_call.extend(full_call.iter().cloned());
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
            let steel_body = wrap_result(quote!(#self_ty::#mname(#(#full_call),*)));
            steel_regs.extend(quote! {
                engine.register_fn(#steel_name, |#(#visible_inputs),*| {
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
