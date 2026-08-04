use std::borrow::Cow;

use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};
use utils::AsAny;

use crate::pool::NodeUid;

#[derive(Serialize, Deserialize)]
pub struct Action {
    pub dest: Option<NodeUid>,
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
