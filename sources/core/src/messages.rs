use std::any::{Any, type_name};
use std::borrow::Cow;

use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};
use utils::AsAny;

use crate::{Node, pool::NodeUid, region::ScreenRegion};

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
    fn handle_action(&mut self, body: Box<dyn ActionBody>) -> Option<Box<dyn ActionBody>>;
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
    fn request_dyn(&self, body: Box<dyn RequestBody>)
    -> Result<Box<dyn Any>, Box<dyn RequestBody>>;
}

/// Query a node instance directly.
pub trait Requestable: Node {
    fn request<R>(&self, body: R) -> Option<R::Response>
    where
        R: RequestFor<Self> + 'static;
}

impl<T: Node + ?Sized> Requestable for T {
    fn request<R>(&self, body: R) -> Option<R::Response>
    where
        R: RequestFor<Self> + 'static,
    {
        self.request_dyn(Box::new(body)).ok().map(downcast_resp)
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
// Macros
// ======================================================================

/**
    Declare a node's typed action and request handlers in one place. Each entry
    defines its message struct, wires up the routing trait ([`ActionFor`] /
    [`RequestFor`]), and supplies the handler body, whose first binding names the
    node and second the decoded message. Use `extern_actions` / `extern_requests`
    for a message type defined elsewhere (e.g. shared between nodes).

    ```ignore
    defhandlers! { MyNode {
        actions: [ SetSize { size: Vector } => (this, s) { this.size = s.size } ],
        requests: [ GetSize => (this, _q): Vector { this.size } ],
    }}
    ```
*/
#[macro_export]
macro_rules! defhandlers {
    (
        $node:ty {
            // Defines an action struct and trait impl
            $( actions: [ $(
                $a_name:ident $({ $($af:ident : $aty:ty),* $(,)? })? => ( $aself:ident, $ab:pat_param ) $abody:block
            ),* $(,)? ] $(,)? )?
            // Trait impls an action for an extern struct
            $( extern_actions: [ $(
                $ea_name:ty => ( $eaself:ident, $eab:pat_param ) $eabody:block
            ),* $(,)? ] $(,)? )?
            // Defines a request struct and trait impl
            $( requests: [ $(
                $r_name:ident $({ $($rf:ident : $rty:ty),* $(,)? })? => ( $rself:ident, $rb:pat_param ) : $rret:ty $rbody:block
            ),* $(,)? ] $(,)? )?
            // Trait impls a request for an extern struct
            $( extern_requests: [ $(
                $er_name:ty => ( $erself:ident, $erb:pat_param ) : $erret:ty $erbody:block
            ),* $(,)? ] $(,)? )?
        }
    ) => {
        // New action types ----------------------------------------
        $($(
            $crate::defhandlers!(@def_struct $a_name $({ $($af : $aty),* })?);
            #[typetag::serde]
            impl $crate::messages::ActionBody for $a_name {}
            impl $crate::messages::ActionFor<$node> for $a_name {}
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
            ) -> ::std::option::Option<::std::boxed::Box<dyn $crate::messages::ActionBody>> {
                $($(
                    if r.as_any_ref().is::<$a_name>() {
                        let $ab = *r.as_any().downcast::<$a_name>().unwrap();
                        let $aself = &mut *self;
                        $abody
                        return ::std::option::Option::None;
                    }
                )*)?
                $($(
                    if r.as_any_ref().is::<$ea_name>() {
                        let $eab = *r.as_any().downcast::<$ea_name>().unwrap();
                        let $eaself = &mut *self;
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
            ) -> ::std::result::Result<
                ::std::boxed::Box<dyn ::std::any::Any>,
                ::std::boxed::Box<dyn $crate::messages::RequestBody>,
            > {
                $($(
                    if body.as_any_ref().is::<$r_name>() {
                        let $rb = *body.as_any().downcast::<$r_name>().unwrap();
                        let $rself = &*self;
                        let __resp: $rret = $rbody;
                        return ::std::result::Result::Ok(::utils::boxed_any!(__resp));
                    }
                )*)?
                $($(
                    if body.as_any_ref().is::<$er_name>() {
                        let $erb = *body.as_any().downcast::<$er_name>().unwrap();
                        let $erself = &*self;
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
        #[derive(Clone, ::serde::Serialize, ::serde::Deserialize)]
        pub struct $name { $(pub $f : $ty),* }
    };
    (@def_struct $name:ident) => {
        #[derive(Clone, ::serde::Serialize, ::serde::Deserialize)]
        pub struct $name;
    };
}

// ======================================================================
// Builtins
// ======================================================================

#[derive(Clone, Serialize, Deserialize)]
pub struct Region;

impl TypedRequestBody for Region {
    type Response = Option<ScreenRegion>;
}
