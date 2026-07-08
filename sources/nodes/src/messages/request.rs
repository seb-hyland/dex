use std::borrow::Cow;

use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};
use utils::AsAny;
use uuid::Uuid;

use crate::pool::NodeUid;

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub dest: NodeUid,
    pub brand: Uuid,
    pub description: RequestDescription,
    pub body: Box<dyn RequestType>,
}

impl Clone for Request {
    fn clone(&self) -> Self {
        Self {
            dest: self.dest,
            brand: self.brand,
            description: self.description.clone(),
            body: dyn_clone::clone_box(&*self.body),
        }
    }
}

type RequestDescription = Cow<'static, str>;

#[typetag::serde]
pub trait RequestType: AsAny + DynClone {
    fn is_history_defining(&self) -> bool {
        true
    }
}

/**
    A special marker type that represents a series of requests.
    This request group represents a single operation.
*/
#[derive(Clone, Serialize, Deserialize)]
pub struct RequestGroup {
    pub requests: Vec<Request>,
}

#[typetag::serde]
impl RequestType for RequestGroup {
    fn is_history_defining(&self) -> bool {
        self.requests.iter().any(|r| r.body.is_history_defining())
    }
}
