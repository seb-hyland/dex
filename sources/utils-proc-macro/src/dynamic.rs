use proc_macro::TokenStream;
use proc_macro2::TokenStream as TS2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DeriveInput, Fields, FnArg, ImplItem, ItemImpl, ReturnType, Type, Visibility,
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

/// The doc comment on an item, joined into one string for the stub.
fn doc_of(attrs: &[Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
        {
            lines.push(s.value().trim().to_owned());
        }
    }
    lines.join("\n")
}

/// The final path-segment ident of a type, e.g. `Vector` for `crate::Vector`.
fn type_ident(ty: &Type) -> Option<syn::Ident> {
    match ty {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.clone()),
        _ => None,
    }
}

/**
    Whether a type can cross the Python boundary.

    Only owned path types are considered: a reference, slice, `impl Trait` or
    tuple in a signature means the method is skipped. That is also what keeps
    raw egui out of the bindings for free — `&mut Ui`, `&Painter` and
    `impl FnOnce(&mut Ui)` all fail here without the macro naming egui at all.

    Whether a path type *actually* converts is then decided by the compiler,
    through [`FromDynamic`] / [`IntoDynamic`].
*/
fn is_bindable(ty: &Type) -> bool {
    matches!(ty, Type::Path(_))
}

/// Whether a return type can be handed back: either `()` or a bindable type.
fn is_bindable_return(ret: &ReturnType) -> bool {
    match ret {
        ReturnType::Default => true,
        ReturnType::Type(_, ty) => is_bindable(ty),
    }
}

pub fn dynamic_type_impl(attr: TokenStream, body: TokenStream) -> TokenStream {
    // `#[dynamic_type(name = "...")]` overrides the script-facing name.
    let mut name_override: Option<String> = None;
    let mut no_reduce = false;
    let mut no_copy = false;
    let mut constructor = false;
    let name_parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("name") {
            name_override = Some(meta.value()?.parse::<syn::LitStr>()?.value());
            Ok(())
        } else if meta.path.is_ident("new") {
            // Opt-in: only valid when every field is public and bindable.
            constructor = true;
            Ok(())
        } else if meta.path.is_ident("no_reduce") {
            // For types holding something unserialisable — a live channel, say.
            no_reduce = true;
            Ok(())
        } else if meta.path.is_ident("no_copy") {
            // For a type whose copy semantics are not plain `Clone`, so it can
            // write its own `__copy__` / `__deepcopy__`.
            no_copy = true;
            Ok(())
        } else {
            Err(meta.error("unsupported #[dynamic_type] option"))
        }
    });
    parse_macro_input!(attr with name_parser);

    let mut input = parse_macro_input!(body as DeriveInput);
    let name = input.ident.clone();
    let script_name = name_override.unwrap_or_else(|| name.to_string());

    // Annotate exposed (`pub`, non-skip) named fields with `#[pyo3(get, set)]`,
    // collecting them in case a constructor was asked for.
    let mut ctor_fields: Vec<(syn::Ident, Type)> = Vec::new();
    let mut stub_fields: Vec<TS2> = Vec::new();
    if let Data::Struct(data) = &mut input.data
        && let Fields::Named(named) = &mut data.fields
    {
        for field in named.named.iter_mut() {
            let is_pub = matches!(field.vis, Visibility::Public(_));
            let skip = take_dynamic_skip(&mut field.attrs);
            if field.ident.is_none() || !is_pub || skip || !is_bindable(&field.ty) {
                continue;
            }

            field.attrs.push(syn::parse_quote!(#[pyo3(get, set)]));
            let fname = field.ident.clone().unwrap();
            let fname_str = fname.to_string();
            let fty = field.ty.clone();
            stub_fields.push(quote! {
                ::dex_core::stubs::StubField {
                    name: #fname_str,
                    ty: ::core::stringify!(#fty),
                }
            });
            ctor_fields.push((fname, fty));
        }
    }

    /*
        `#[dynamic_type(new)]` gives a value type the struct-literal equivalent a
        script otherwise lacks. Each field arrives opaque and converts through
        `FromDynamic`, so fields the pyo3 derive cannot express still work.
    */
    let ctor = constructor.then(|| {
        let names: Vec<_> = ctor_fields.iter().map(|(n, _)| n.clone()).collect();
        let tys: Vec<_> = ctor_fields.iter().map(|(_, t)| t.clone()).collect();
        quote! {
            #[::pyo3::pymethods]
            impl #name {
                #[new]
                fn __dyn_new(
                    #( #names: ::pyo3::Bound<'_, ::pyo3::PyAny> ),*
                ) -> ::pyo3::PyResult<Self> {
                    ::core::result::Result::Ok(#name {
                        #( #names: <#tys as ::dex_core::scripting::FromDynamic>::from_dynamic(
                            &#names,
                        )? ),*
                    })
                }
            }
        }
    });

    // Copying a bound value is cloning it, unless the type opts out to define
    // its own — a node handle rewrites itself when copied into a clone.
    let copy_methods = (!no_copy).then(|| {
        quote! {
            #[::pyo3::pymethods]
            impl #name {
                fn __copy__(&self) -> Self {
                    ::core::clone::Clone::clone(self)
                }

                #[pyo3(signature = (_memo=None))]
                fn __deepcopy__(
                    &self,
                    _memo: ::core::option::Option<::pyo3::Bound<'_, ::pyo3::PyAny>>,
                ) -> Self {
                    ::core::clone::Clone::clone(self)
                }
            }
        }
    });

    /*
        A pyclass keeps its data in Rust, not a Python `__dict__`, so the default
        pickle protocol cannot describe how to rebuild one — `copy.deepcopy` and
        `cloudpickle` both raise `cannot pickle` without this. Reducing through
        the type's own serde representation makes bound values persist inside a
        script-defined node.
    */
    let reduce = (!no_reduce).then(|| {
        quote! {
            #[::pyo3::pymethods]
            impl #name {
                fn __reduce__(
                    &self,
                    py: ::pyo3::Python<'_>,
                ) -> ::pyo3::PyResult<(
                    ::pyo3::Py<::pyo3::PyAny>,
                    (::std::string::String, ::std::vec::Vec<u8>),
                )> {
                    use ::pyo3::prelude::*;
                    let __rebuild = py.import("dex")?.getattr("_rebuild")?;
                    let __bytes = ::dex_core::scripting::reduce_to_bytes(self)?;
                    ::core::result::Result::Ok((
                        __rebuild.unbind(),
                        (::std::string::String::from(#script_name), __bytes),
                    ))
                }
            }

            ::dex_dynamic::__rt::inventory::submit! {
                ::dex_core::scripting::DynamicRebuild {
                    name: #script_name,
                    from_bytes: |__data, __py| {
                        use ::pyo3::prelude::*;
                        let __v: #name = ::dex_core::scripting::reduce_from_bytes(__data)?;
                        ::core::result::Result::Ok(::pyo3::Py::new(__py, __v)?.into_any())
                    },
                }
            }
        }
    });

    // Enum variants are part of the surface: `dex.AxisConstraint.Exactly(w)`.
    let mut stub_variants: Vec<TS2> = Vec::new();
    if let Data::Enum(data) = &input.data {
        for variant in &data.variants {
            let vname = variant.ident.to_string();
            let mut vfields: Vec<TS2> = Vec::new();
            for (i, field) in variant.fields.iter().enumerate() {
                // pyo3 names a tuple variant's fields `_0`, `_1`, ...
                let fname = field
                    .ident
                    .as_ref()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| format!("_{i}"));
                let fty = &field.ty;
                vfields.push(quote! {
                    ::dex_core::stubs::StubField {
                        name: #fname,
                        ty: ::core::stringify!(#fty),
                    }
                });
            }
            stub_variants.push(quote! {
                ::dex_core::stubs::StubVariant {
                    name: #vname,
                    fields: &[#(#vfields),*],
                }
            });
        }
    }

    let class_doc = doc_of(&input.attrs);
    let stub = quote! {
        ::dex_dynamic::__rt::inventory::submit! {
            ::dex_core::stubs::StubClass {
                name: #script_name,
                doc: #class_doc,
                fields: &[#(#stub_fields),*],
                constructible: #constructor,
                variants: &[#(#stub_variants),*],
            }
        }
    };

    quote! {
        #[::pyo3::pyclass(from_py_object, module = "dex", name = #script_name)]
        #input

        #ctor
        #reduce
        #stub

        /*
            Bound types are values, so a script may hold one in a field and have
            `copy.deepcopy` reach it. Without these, deepcopy falls back to
            pickling and raises for every pyclass — which would silently defeat
            the copy-on-write that `DynamicNode`'s action handling relies on.
            A Rust `Clone` is the deep copy: these types own their data, and a
            shared `Arc<dyn Node>` is an immutable value.
        */
        #copy_methods

        // Every bound type is usable on both sides of any bound signature.
        impl ::dex_core::scripting::FromDynamic for #name {
            fn from_dynamic(
                obj: &::pyo3::Bound<'_, ::pyo3::PyAny>,
            ) -> ::pyo3::PyResult<Self> {
                use ::pyo3::prelude::*;
                // `?` bridges the pyclass guard error into `PyErr`.
                ::core::result::Result::Ok(obj.extract::<#name>()?)
            }
        }

        impl ::dex_core::scripting::IntoDynamic for #name {
            fn into_dynamic(
                self,
                py: ::pyo3::Python<'_>,
            ) -> ::pyo3::PyResult<::pyo3::Py<::pyo3::PyAny>> {
                ::core::result::Result::Ok(::pyo3::Py::new(py, self)?.into_any())
            }
        }

        ::dex_dynamic::__rt::inventory::submit! {
            ::dex_dynamic::DynamicBinding {
                name: #script_name,
                register_python: |m| {
                    use ::dex_dynamic::__rt::pyo3::types::PyModuleMethods;
                    m.add_class::<#name>()
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
                ::dex_core::stubs::StubNodeImpl {
                    name: ::core::stringify!(#self_ty),
                }
            }

            ::dex_dynamic::__rt::inventory::submit! {
                ::dex_core::scripting::NodeExtractor {
                    from_python: |obj| {
                        use ::dex_dynamic::__rt::pyo3::prelude::*;
                        obj.extract::<#self_ty>()
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

/**
    Build the Python wrapper for one method.

    Every argument arrives as an opaque `PyAny` and is reconstructed through
    [`FromDynamic`]; the result leaves through [`IntoDynamic`]. The macro
    therefore never names a boundary type — teaching it about a new one is an
    impl, not an edit here.
*/
/// Record one bound signature for stub generation.
fn stub_for(owner: &str, f: &syn::ImplItemFn, param_names: &[String], is_static: bool) -> TS2 {
    let name = f.sig.ident.to_string();
    let doc = doc_of(&f.attrs);
    let mut params: Vec<TS2> = Vec::new();
    let mut idx = 0usize;
    for arg in f.sig.inputs.iter() {
        let FnArg::Typed(pt) = arg else { continue };
        let ty = &pt.ty;
        // Prefer the real Rust parameter name; fall back to a positional one.
        let pname = param_names
            .get(idx)
            .cloned()
            .unwrap_or_else(|| format!("arg{idx}"));
        idx += 1;
        params.push(quote! {
            ::dex_core::stubs::StubField {
                name: #pname,
                ty: ::core::stringify!(#ty),
            }
        });
    }
    let returns = match &f.sig.output {
        ReturnType::Default => quote!(""),
        ReturnType::Type(_, ty) => quote!(::core::stringify!(#ty)),
    };
    quote! {
        ::dex_dynamic::__rt::inventory::submit! {
            ::dex_core::stubs::StubMethod {
                owner: #owner,
                name: #name,
                doc: #doc,
                params: &[#(#params),*],
                returns: #returns,
                is_static: #is_static,
            }
        }
    }
}

/// The declared names of a method's non-receiver parameters.
fn param_names(f: &syn::ImplItemFn) -> Vec<String> {
    f.sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pt) => Some(match &*pt.pat {
                syn::Pat::Ident(id) => id.ident.to_string(),
                _ => "value".to_owned(),
            }),
            _ => None,
        })
        .collect()
}

fn bind_method(self_ty: &Type, f: &mut syn::ImplItemFn) -> Option<(TS2, TS2)> {
    let skip = take_dynamic_skip(&mut f.attrs);
    let is_pub = matches!(f.vis, Visibility::Public(_));
    if skip || !is_pub || !f.sig.generics.params.is_empty() {
        return None;
    }
    if !is_bindable_return(&f.sig.output) {
        return None;
    }

    // Classify the receiver.
    let recv = match f.sig.receiver() {
        None => Recv::None,
        Some(r) if r.reference.is_none() => Recv::Value,
        Some(r) if r.mutability.is_some() => Recv::RefMut,
        Some(_) => Recv::Ref,
    };

    let mut py_inputs: Vec<TS2> = Vec::new();
    let mut conversions: Vec<TS2> = Vec::new();
    let mut call_args: Vec<TS2> = Vec::new();

    let names = param_names(f);
    let mut visible = 0usize;
    for arg in f.sig.inputs.iter() {
        let FnArg::Typed(pt) = arg else { continue };
        if !is_bindable(&pt.ty) {
            return None;
        }
        // The declared name, so keyword arguments work and match the stub.
        let id = format_ident!("{}", names[visible]);
        visible += 1;
        let ty = &pt.ty;
        py_inputs.push(quote!(#id: ::pyo3::Bound<'_, ::pyo3::PyAny>));
        conversions.push(quote! {
            let #id = <#ty as ::dex_core::scripting::FromDynamic>::from_dynamic(&#id)?;
        });
        call_args.push(quote!(#id));
    }

    let mname = f.sig.ident.clone();
    let mname_str = mname.to_string();
    let wrapper = format_ident!("__dyn_{mname}");

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
    wrapper_inputs.push(quote!(__py: ::pyo3::Python<'_>));
    wrapper_inputs.extend(py_inputs);

    let mut full_call: Vec<TS2> = Vec::new();
    if let Some(cs) = call_self {
        full_call.push(cs);
    }
    full_call.extend(call_args);

    let owner = type_ident(self_ty)
        .map(|i| i.to_string())
        .unwrap_or_default();
    let stub = stub_for(&owner, f, &names, matches!(recv, Recv::None));

    let wrapper_fn = quote! {
        #static_attr
        #[pyo3(name = #mname_str)]
        fn #wrapper(
            #(#wrapper_inputs),*
        ) -> ::pyo3::PyResult<::pyo3::Py<::pyo3::PyAny>> {
            #(#conversions)*
            let __ret = #self_ty::#mname(#(#full_call),*);
            ::dex_core::scripting::IntoDynamic::into_dynamic(__ret, __py)
        }
    };
    Some((wrapper_fn, stub))
}

/**
    Bind an inherent `impl` on a *borrowed* type onto its scoped Python handle.

    `#[dynamic_scoped(PyDrawContext)]` emits a `#[pymethods] impl PyDrawContext`
    block whose wrappers run against the live borrow via `try_with`, raising if
    the handle has outlived the call it belongs to. Several `impl` blocks may
    target the same handle — pyo3's `multiple-pymethods` merges them.

    Only methods with a receiver are bound: an associated function has no
    context to run against.
*/
pub fn dynamic_scoped_impl(attr: TokenStream, body: TokenStream) -> TokenStream {
    let handle = parse_macro_input!(attr as syn::Ident);
    let mut input = parse_macro_input!(body as ItemImpl);

    let self_ty = (*input.self_ty).clone();
    if input.trait_.is_some() || type_ident(&self_ty).is_none() {
        return quote!(#input).into();
    }

    let handle_name = handle.to_string();
    // The class is declared by hand; the stub takes the script-facing name.
    let script_name = handle_name
        .strip_prefix("Py")
        .unwrap_or(&handle_name)
        .to_owned();

    let mut py_methods = TS2::new();
    let mut stubs = TS2::new();
    for item in input.items.iter_mut() {
        let ImplItem::Fn(f) = item else { continue };
        if let Some((wrapper, stub)) = bind_scoped_method(&script_name, f) {
            py_methods.extend(wrapper);
            stubs.extend(stub);
        }
    }

    quote! {
        #input

        #[::pyo3::pymethods]
        impl #handle {
            #py_methods
        }

        #stubs
    }
    .into()
}

/// Build the wrapper for one method reached through a scoped handle.
fn bind_scoped_method(owner: &str, f: &mut syn::ImplItemFn) -> Option<(TS2, TS2)> {
    let skip = take_dynamic_skip(&mut f.attrs);
    let is_pub = matches!(f.vis, Visibility::Public(_));
    if skip || !is_pub || !f.sig.generics.params.is_empty() {
        return None;
    }
    if !is_bindable_return(&f.sig.output) {
        return None;
    }
    // No receiver means nothing to borrow; such a function is not a context method.
    f.sig.receiver()?;

    let mut py_inputs: Vec<TS2> = Vec::new();
    let mut conversions: Vec<TS2> = Vec::new();
    let mut call_args: Vec<TS2> = Vec::new();

    let names = param_names(f);
    let mut visible = 0usize;
    for arg in f.sig.inputs.iter() {
        let FnArg::Typed(pt) = arg else { continue };
        if !is_bindable(&pt.ty) {
            return None;
        }
        let id = format_ident!("{}", names[visible]);
        visible += 1;
        let ty = &pt.ty;
        py_inputs.push(quote!(#id: ::pyo3::Bound<'_, ::pyo3::PyAny>));
        conversions.push(quote! {
            let #id = <#ty as ::dex_core::scripting::FromDynamic>::from_dynamic(&#id)?;
        });
        call_args.push(quote!(#id));
    }

    let mname = f.sig.ident.clone();
    let mname_str = mname.to_string();
    let wrapper = format_ident!("__dyn_{mname}");
    let stub = stub_for(owner, f, &names, false);

    let wrapper_fn = quote! {
        #[pyo3(name = #mname_str)]
        fn #wrapper(
            &self,
            __py: ::pyo3::Python<'_>,
            #(#py_inputs),*
        ) -> ::pyo3::PyResult<::pyo3::Py<::pyo3::PyAny>> {
            #(#conversions)*
            let __ret = self.try_with(|__target| __target.#mname(#(#call_args),*))?;
            ::dex_core::scripting::IntoDynamic::into_dynamic(__ret, __py)
        }
    };
    Some((wrapper_fn, stub))
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

    let mut py_methods = TS2::new();
    let mut stubs = TS2::new();
    for item in input.items.iter_mut() {
        let ImplItem::Fn(f) = item else { continue };
        if let Some((wrapper, stub)) = bind_method(&self_ty, f) {
            py_methods.extend(wrapper);
            stubs.extend(stub);
        }
    }

    quote! {
        #input

        #[::pyo3::pymethods]
        impl #self_ty {
            #py_methods
        }

        #stubs
    }
    .into()
}
