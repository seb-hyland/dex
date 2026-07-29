use std::any::{Any, type_name};

use utils::AsAny;

use crate::{pool::NodeUid, region::ScreenRegion};

/**
    A type-erased request addressed to a node.
    Call sites should use [`TypedRequest`], which remembers the response type.
*/
pub struct Request {
    pub dest: NodeUid,
    pub body: Box<dyn RequestBody>,
}

pub trait RequestBody: AsAny {}

/**
    A typed request addressed to a node.
*/
pub struct TypedRequest<Resp> {
    pub dest: NodeUid,
    pub body: Box<dyn TypedRequestBody<Response = Resp>>,
}

pub trait TypedRequestBody: RequestBody {
    type Response: Any;
}

impl<T> RequestBody for T where T: TypedRequestBody {}

impl<Resp> From<TypedRequest<Resp>> for Request {
    fn from(q: TypedRequest<Resp>) -> Self {
        Self {
            dest: q.dest,
            body: q.body,
        }
    }
}

pub trait Requestable {
    fn request(&self, body: Box<dyn RequestBody>) -> Option<Box<dyn Any>>;
}

pub trait TypedRequestable: Requestable {
    fn request_typed<Resp: Any>(
        &self,
        body: Box<dyn TypedRequestBody<Response = Resp>>,
    ) -> Option<Resp>;
}

impl<T: Requestable + ?Sized> TypedRequestable for T {
    fn request_typed<Resp: Any>(
        &self,
        body: Box<dyn TypedRequestBody<Response = Resp>>,
    ) -> Option<Resp> {
        self.request(body).map(downcast_resp)
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

/// Match on an incoming request and respond
#[macro_export]
macro_rules! respond_requests {
    (
        $matched_item:expr, $($binding:ident : $match_type:ty => $body:expr,)*
    ) => {
        match $matched_item {
            $($binding if $binding.as_any_ref().is::<$match_type>() => {
                let $binding = *$binding.as_any().downcast::<$match_type>().unwrap();
                // Ensure the response matches the expected type
                let __resp: <$match_type as $crate::messages::request::TypedRequestBody>::Response = $body;
                ::std::option::Option::Some(::utils::boxed_any!(__resp))
            })*
            _ => None,
        }
    };
}

#[macro_export]
macro_rules! defrequest {
    // Unit struct
    ($type_name:ident -> requests $req_purpose:literal and returns $return_type:ty) => {
        defrequest!(@build $type_name $req_purpose $return_type | ;);
    };

    // With fields
    ($type_name:ident { $($f_name:ident: $f_type:ty),* $(,)? } requests $req_purpose:literal returns $return_type:ty) => {
        defrequest!(@build $type_name $req_purpose $return_type | { $($f_name: $f_type),* });
    };

    // Shared expansion
    (@build $type_name:ident $req_purpose:literal $return_type:ty | $body:tt) => {
        #[doc = concat!("A query that requests ", $req_purpose,
            ".\n\n## Returns\n[`", stringify!($return_type), "`] if the query succeeds.")]
        #[derive(Clone, ::serde::Serialize, ::serde::Deserialize)]
        pub struct $type_name $body

        impl $crate::messages::request::TypedRequestBody for $type_name {
            type Response = $return_type;
        }
    };
}

defrequest! { Region -> requests "the last known draw region of a node" and returns Option<ScreenRegion> }
