use std::borrow::Cow;

use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};
use utils::AsAny;
use uuid::Uuid;

use crate::pool::NodeUid;

#[derive(Serialize, Deserialize)]
pub struct Action {
    pub dest: NodeUid,
    pub brand: Uuid,
    pub description: ActionDescription,
    pub body: Box<dyn ActionType>,
}

impl Clone for Action {
    fn clone(&self) -> Self {
        Self {
            dest: self.dest,
            brand: self.brand,
            description: self.description.clone(),
            body: dyn_clone::clone_box(&*self.body),
        }
    }
}

type ActionDescription = Cow<'static, str>;

#[typetag::serde]
pub trait ActionType: AsAny + DynClone {
    fn is_history_defining(&self) -> bool {
        true
    }
}

/**
    A special marker type that represents a series of requests.
    This request group represents a single operation.
*/
#[derive(Clone, Serialize, Deserialize)]
pub struct ActionGroup {
    pub actions: Vec<Action>,
}

#[typetag::serde]
impl ActionType for ActionGroup {
    fn is_history_defining(&self) -> bool {
        self.actions.iter().any(|r| r.body.is_history_defining())
    }
}
