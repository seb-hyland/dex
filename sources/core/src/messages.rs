use std::any::{Any, type_name};
use std::borrow::Cow;

use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};
use utils::AsAny;

use crate::{Node, NodeContext, pool::NodeUid};

// ======================================================================
// Actions
// ======================================================================

#[derive(Serialize, Deserialize)]
pub struct Action {
    /// The addressed node, or [`NodeUid::nil`] for a workspace-level action.
    pub dest: NodeUid,
    pub description: ActionDescription,
    pub body: Box<dyn ActionBody>,
}

impl Clone for Action {
    fn clone(&self) -> Self {
        Self {
            dest: self.dest,
            description: self.description.clone(),
            body: dyn_clone::clone_box(&*self.body),
        }
    }
}

type ActionDescription = Cow<'static, str>;

#[typetag::serde]
pub trait ActionBody: AsAny + DynClone + Send {}

/**
    A special marker type that represents a series of actions.
*/
#[derive(Clone, Serialize, Deserialize)]
pub struct ActionGroup {
    pub actions: Vec<Action>,
}

#[typetag::serde]
impl ActionBody for ActionGroup {}

/// Erased action dispatch.
pub trait ActionHandler {
    /// Returns [`Some`] with the action if it was not understood, so it can be forwarded.
    fn handle_action(
        &mut self,
        body: Box<dyn ActionBody>,
        ctx: NodeContext,
    ) -> Option<Box<dyn ActionBody>>;
}

/// Compile-time proof that an action type is accepted by node type `N`.
pub trait ActionFor<N: Node + ?Sized>: ActionBody {}

/// An erased node accepts any action.
impl<A: ActionBody> ActionFor<dyn Node> for A {}

// ======================================================================
// Requests
// ======================================================================

/**
    A type-erased request addressed to a node.
*/
pub struct Request {
    pub dest: NodeUid,
    pub body: Box<dyn RequestBody>,
}

pub trait RequestBody: AsAny {}

pub trait TypedRequestBody: RequestBody {
    type Response: Any;
}

impl<T> RequestBody for T where T: TypedRequestBody {}

/// Erased request dispatch.
pub trait RequestableDyn {
    /// Returns [`Err`] with the request if it was not answered, so it can be forwarded.
    fn request_dyn(
        &self,
        body: Box<dyn RequestBody>,
        ctx: NodeContext,
    ) -> Result<Box<dyn Any>, Box<dyn RequestBody>>;
}

/// Query a node instance directly.
pub trait Requestable: Node {
    fn request<R>(&self, body: R, ctx: NodeContext) -> Option<R::Response>
    where
        R: RequestFor<Self> + 'static;
}

impl<T: Node + ?Sized> Requestable for T {
    fn request<R>(&self, body: R, ctx: NodeContext) -> Option<R::Response>
    where
        R: RequestFor<Self> + 'static,
    {
        self.request_dyn(Box::new(body), ctx)
            .ok()
            .map(downcast_resp)
    }
}

pub fn downcast_resp<Resp: Any>(response: Box<dyn Any>) -> Resp {
    *response.downcast::<Resp>().unwrap_or_else(|_| {
        panic!(
            "Request with response type {} returned a different type.",
            type_name::<Resp>()
        )
    })
}

/// Compile-time proof that a request type is answered by node type `N`.
pub trait RequestFor<N: Node + ?Sized>: TypedRequestBody {}

/// An erased node answers any request.
impl<R: TypedRequestBody> RequestFor<dyn Node> for R {}

// ======================================================================
// The dynamic message registry
// ======================================================================

/// A message type that can be named and constructed in Python.
pub struct DynamicRequest {
    pub name: &'static str,
    /// The module that defined it, shown when reporting an unrecognised value.
    pub path: &'static str,
    /// Whether this Python value is an instance of this request class.
    pub matches: fn(&pyo3::Bound<'_, pyo3::PyAny>) -> bool,
    /// Build the erased body from such an instance.
    pub build: fn(&pyo3::Bound<'_, pyo3::PyAny>) -> pyo3::PyResult<Box<dyn RequestBody>>,
    /// Convert this request's response type back to a Python value.
    pub respond: fn(Box<dyn Any>, pyo3::Python<'_>) -> pyo3::PyResult<pyo3::Py<pyo3::PyAny>>,
    /// Hand an incoming Rust request to Python, if it is this type. The other
    /// direction: what a script-defined node receives in its `request` handler.
    #[allow(clippy::type_complexity)]
    pub to_python:
        fn(&dyn RequestBody, pyo3::Python<'_>) -> Option<pyo3::PyResult<pyo3::Py<pyo3::PyAny>>>,
    /// Box a script's return value as this request's declared response type.
    pub response_from_python: fn(&pyo3::Bound<'_, pyo3::PyAny>) -> pyo3::PyResult<Box<dyn Any>>,
}

dex_dynamic::__rt::inventory::collect!(DynamicRequest);

/// The action counterpart of [`DynamicRequest`].
pub struct DynamicAction {
    pub name: &'static str,
    /// The module that defined it, shown when reporting an unrecognised value.
    pub path: &'static str,
    /// Whether this Python value is an instance of this action class.
    pub matches: fn(&pyo3::Bound<'_, pyo3::PyAny>) -> bool,
    pub build: fn(&pyo3::Bound<'_, pyo3::PyAny>) -> pyo3::PyResult<Box<dyn ActionBody>>,
    /// Hand an incoming Rust action to Python, if it is this type.
    #[allow(clippy::type_complexity)]
    pub to_python:
        fn(&dyn ActionBody, pyo3::Python<'_>) -> Option<pyo3::PyResult<pyo3::Py<pyo3::PyAny>>>,
}

dex_dynamic::__rt::inventory::collect!(DynamicAction);

/// The registered request matching this Python value. Dispatch is by class identity.
pub fn request_for(obj: &pyo3::Bound<'_, pyo3::PyAny>) -> Option<&'static DynamicRequest> {
    dex_dynamic::__rt::inventory::iter::<DynamicRequest>
        .into_iter()
        .find(|r| (r.matches)(obj))
}

/// The registered action matching this Python value. See [`request_for`].
pub fn action_for(obj: &pyo3::Bound<'_, pyo3::PyAny>) -> Option<&'static DynamicAction> {
    dex_dynamic::__rt::inventory::iter::<DynamicAction>
        .into_iter()
        .find(|a| (a.matches)(obj))
}

/// Present an incoming Rust request to Python. Returns the registry entry alongside the Python object.
pub fn request_to_python<'py>(
    body: &dyn RequestBody,
    py: pyo3::Python<'py>,
) -> Option<(
    &'static DynamicRequest,
    pyo3::PyResult<pyo3::Py<pyo3::PyAny>>,
)> {
    dex_dynamic::__rt::inventory::iter::<DynamicRequest>
        .into_iter()
        .find_map(|entry| (entry.to_python)(body, py).map(|obj| (entry, obj)))
}

/// Present an incoming Rust action to Python. See [`request_to_python`].
pub fn action_to_python(
    body: &dyn ActionBody,
    py: pyo3::Python<'_>,
) -> Option<pyo3::PyResult<pyo3::Py<pyo3::PyAny>>> {
    dex_dynamic::__rt::inventory::iter::<DynamicAction>
        .into_iter()
        .find_map(|entry| (entry.to_python)(body, py))
}

/// Every registered message name, for discovery from a script.
pub fn registered_messages() -> (Vec<&'static str>, Vec<&'static str>) {
    let requests = dex_dynamic::__rt::inventory::iter::<DynamicRequest>
        .into_iter()
        .map(|r| r.name)
        .collect();
    let actions = dex_dynamic::__rt::inventory::iter::<DynamicAction>
        .into_iter()
        .map(|a| a.name)
        .collect();
    (requests, actions)
}

// ======================================================================
// Macros
// ======================================================================

/**
    Declare a node's action and request handlers.

    ```ignore
    defhandlers! {
        MyNode {
            actions: [
                SetSize {
                    size: Vector
                } => (this, s) {
                    this.size = s.size
                }
            ],
            requests: [
                GetSize => (this, _q): Vector {
                    this.size
                }
            ],
        }
    }
    ```
*/
#[macro_export]
macro_rules! defhandlers {
    (
        $node:ty {
            // Defines an action struct and trait impl
            $( actions: [ $(
                $a_name:ident $({ $($af:ident : $aty:ty),* $(,)? })? => ( $aself:ident, $ab:pat_param $(, $actx:ident )? ) $abody:block
            ),* $(,)? ] $(,)? )?
            // Trait impls an action for an extern struct
            $( extern_actions: [ $(
                $ea_name:ty => ( $eaself:ident, $eab:pat_param $(, $eactx:ident )? ) $eabody:block
            ),* $(,)? ] $(,)? )?
            // Defines a request struct and trait impl
            $( requests: [ $(
                $r_name:ident $({ $($rf:ident : $rty:ty),* $(,)? })? => ( $rself:ident, $rb:pat_param $(, $rctx:ident )? ) : $rret:ty $rbody:block
            ),* $(,)? ] $(,)? )?
            // Trait impls a request for an extern struct
            $( extern_requests: [ $(
                $er_name:ty => ( $erself:ident, $erb:pat_param $(, $erctx:ident )? ) : $erret:ty $erbody:block
            ),* $(,)? ] $(,)? )?
        }
    ) => {
        // New action types ----------------------------------------
        $($(
            $crate::defhandlers!(@def_struct $a_name $({ $($af : $aty),* })?);
            #[typetag::serde]
            impl $crate::messages::ActionBody for $a_name {}
            impl $crate::messages::ActionFor<$node> for $a_name {}

            ::dex_dynamic::__rt::inventory::submit! {
                $crate::messages::DynamicAction {
                    name: ::core::stringify!($a_name),
                    path: ::core::module_path!(),
                    matches: |__obj| {
                        use ::pyo3::prelude::*;
                        __obj.is_instance_of::<$a_name>()
                    },
                    build: |__obj| {
                        use ::pyo3::prelude::*;
                        ::core::result::Result::Ok(
                            ::std::boxed::Box::new(__obj.extract::<$a_name>()?),
                        )
                    },
                    to_python: |__body, __py| {
                        use ::pyo3::prelude::*;
                        ::utils::AsAny::as_any_ref(__body)
                            .downcast_ref::<$a_name>()
                            .map(|__v| {
                                ::pyo3::Py::new(__py, ::core::clone::Clone::clone(__v))
                                    .map(|__o| __o.into_any())
                            })
                    },
                }
            }
        )*)?
        $($(
            impl $crate::messages::ActionFor<$node> for $ea_name {}
        )*)?

        // Action dispatch ----------------------------------------
        impl $crate::messages::ActionHandler for $node {
            #[allow(unused_variables, unused_mut)]
            fn handle_action(
                &mut self,
                r: ::std::boxed::Box<dyn $crate::messages::ActionBody>,
                ctx: $crate::NodeContext,
            ) -> ::std::option::Option<::std::boxed::Box<dyn $crate::messages::ActionBody>> {
                $($(
                    if (*r).as_any_ref().is::<$a_name>() {
                        let $ab = *r.as_any().downcast::<$a_name>().unwrap();
                        let $aself = &mut *self;
                        $( let $actx = ctx; )?
                        $abody
                        return ::std::option::Option::None;
                    }
                )*)?
                $($(
                    if (*r).as_any_ref().is::<$ea_name>() {
                        let $eab = *r.as_any().downcast::<$ea_name>().unwrap();
                        let $eaself = &mut *self;
                        $( let $eactx = ctx; )?
                        $eabody
                        return ::std::option::Option::None;
                    }
                )*)?
                ::std::option::Option::Some(r)
            }
        }

        // New request types ----------------------------------------
        $($(
            $crate::defhandlers!(@def_struct $r_name $({ $($rf : $rty),* })?);
            impl $crate::messages::TypedRequestBody for $r_name {
                type Response = $rret;
            }
            impl $crate::messages::RequestFor<$node> for $r_name {}

            ::dex_dynamic::__rt::inventory::submit! {
                $crate::messages::DynamicRequest {
                    name: ::core::stringify!($r_name),
                    path: ::core::module_path!(),
                    matches: |__obj| {
                        use ::pyo3::prelude::*;
                        __obj.is_instance_of::<$r_name>()
                    },
                    build: |__obj| {
                        use ::pyo3::prelude::*;
                        ::core::result::Result::Ok(
                            ::std::boxed::Box::new(__obj.extract::<$r_name>()?),
                        )
                    },
                    to_python: |__body, __py| {
                        use ::pyo3::prelude::*;
                        ::utils::AsAny::as_any_ref(__body)
                            .downcast_ref::<$r_name>()
                            .map(|__v| {
                                ::pyo3::Py::new(__py, ::core::clone::Clone::clone(__v))
                                    .map(|__o| __o.into_any())
                            })
                    },
                    response_from_python: |__obj| {
                        let __v = <$rret as $crate::scripting::FromDynamic>::from_dynamic(__obj)?;
                        ::core::result::Result::Ok(::std::boxed::Box::new(__v) as ::std::boxed::Box<dyn ::std::any::Any>)
                    },
                    respond: |__any, __py| {
                        use $crate::scripting::IntoDynamic;
                        $crate::messages::downcast_resp::<$rret>(__any).into_dynamic(__py)
                    },
                }
            }
        )*)?
        $($(
            impl $crate::messages::RequestFor<$node> for $er_name {}
        )*)?

        // Request dispatch ----------------------------------------
        impl $crate::messages::RequestableDyn for $node {
            #[allow(unused_variables, unused_mut)]
            fn request_dyn(
                &self,
                body: ::std::boxed::Box<dyn $crate::messages::RequestBody>,
                ctx: $crate::NodeContext,
            ) -> ::std::result::Result<
                ::std::boxed::Box<dyn ::std::any::Any>,
                ::std::boxed::Box<dyn $crate::messages::RequestBody>,
            > {
                $($(
                    if (*body).as_any_ref().is::<$r_name>() {
                        let $rb = *body.as_any().downcast::<$r_name>().unwrap();
                        let $rself = &*self;
                        $( let $rctx = ctx; )?
                        let __resp: $rret = $rbody;
                        return ::std::result::Result::Ok(::utils::boxed_any!(__resp));
                    }
                )*)?
                $($(
                    if (*body).as_any_ref().is::<$er_name>() {
                        let $erb = *body.as_any().downcast::<$er_name>().unwrap();
                        let $erself = &*self;
                        $( let $erctx = ctx; )?
                        let __resp: $erret = $erbody;
                        return ::std::result::Result::Ok(::utils::boxed_any!(__resp));
                    }
                )*)?
                ::std::result::Result::Err(body)
            }
        }
    };

    // Define a message struct, with named fields or as a unit struct.
    (@def_struct $name:ident { $($f:ident : $ty:ty),* $(,)? }) => {
        #[::pyo3::pyclass(from_py_object, module = "dex")]
        #[derive(Clone, ::serde::Serialize, ::serde::Deserialize)]
        pub struct $name { $(pub $f : $ty),* }

        #[::pyo3::pymethods]
        impl $name {
            #[new]
            fn __dyn_new(
                $( $f: ::pyo3::Bound<'_, ::pyo3::PyAny> ),*
            ) -> ::pyo3::PyResult<Self> {
                ::core::result::Result::Ok($name {
                    $( $f: <$ty as $crate::scripting::FromDynamic>::from_dynamic(&$f)? ),*
                })
            }

            $(
                #[getter]
                fn $f(
                    &self,
                    py: ::pyo3::Python<'_>,
                ) -> ::pyo3::PyResult<::pyo3::Py<::pyo3::PyAny>> {
                    $crate::scripting::IntoDynamic::into_dynamic(
                        ::core::clone::Clone::clone(&self.$f),
                        py,
                    )
                }
            )*

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

            fn __reduce__(
                &self,
                py: ::pyo3::Python<'_>,
            ) -> ::pyo3::PyResult<(
                ::pyo3::Py<::pyo3::PyAny>,
                (::std::string::String, ::std::vec::Vec<u8>),
            )> {
                use ::pyo3::prelude::*;
                let __rebuild = py.import("dex")?.getattr("_rebuild")?;
                let __bytes = $crate::scripting::reduce_to_bytes(self)?;
                ::core::result::Result::Ok((
                    __rebuild.unbind(),
                    (::std::string::String::from(::core::stringify!($name)), __bytes),
                ))
            }
        }

        $crate::defhandlers!(@register_class $name);
    };
    (@def_struct $name:ident) => {
        #[::pyo3::pyclass(from_py_object, module = "dex")]
        #[derive(Clone, ::serde::Serialize, ::serde::Deserialize)]
        pub struct $name;

        #[::pyo3::pymethods]
        impl $name {
            #[new]
            fn __dyn_new() -> Self {
                $name
            }

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

            fn __reduce__(
                &self,
                py: ::pyo3::Python<'_>,
            ) -> ::pyo3::PyResult<(
                ::pyo3::Py<::pyo3::PyAny>,
                (::std::string::String, ::std::vec::Vec<u8>),
            )> {
                use ::pyo3::prelude::*;
                let __rebuild = py.import("dex")?.getattr("_rebuild")?;
                let __bytes = $crate::scripting::reduce_to_bytes(self)?;
                ::core::result::Result::Ok((
                    __rebuild.unbind(),
                    (::std::string::String::from(::core::stringify!($name)), __bytes),
                ))
            }
        }

        $crate::defhandlers!(@register_class $name);
    };

    // Publish a message class into the `dex` module, and teach `dex._rebuild`
    // how to restore one from its captured bytes.
    (@register_class $name:ident) => {
        ::dex_dynamic::__rt::inventory::submit! {
            $crate::scripting::DynamicRebuild {
                name: ::core::stringify!($name),
                from_bytes: |__data, __py| {
                    use ::pyo3::prelude::*;
                    let __v: $name = $crate::scripting::reduce_from_bytes(__data)?;
                    ::core::result::Result::Ok(::pyo3::Py::new(__py, __v)?.into_any())
                },
            }
        }

        ::dex_dynamic::__rt::inventory::submit! {
            ::dex_dynamic::DynamicBinding {
                name: ::core::stringify!($name),
                register_python: |m| {
                    use ::dex_dynamic::__rt::pyo3::types::PyModuleMethods;
                    m.add_class::<$name>()
                },
            }
        }
    };
}
