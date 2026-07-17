use dyn_clone::DynClone;
use egui::Ui;

mod messages;
mod pool;
mod region;
mod theme;
mod workspace;

pub mod prelude {
    pub use crate::{
        messages::{
            action::{Action, ActionBody},
            request::{Request, RequestBody, Requestable},
        },
        pool::NodeUid,
        region::{ScreenPos, ScreenRegion, Vector},
        workspace::Workspace,
        *,
    };
}
pub use prelude::*;

#[typetag::serde]
pub trait Node: 'static + Requestable + DynClone {
    /// Given some context, draw the node on screen
    fn draw(&self, ctx: &mut DrawContext) -> DrawResult;

    /// Resolve an action
    fn handle_action(&mut self, r: Box<dyn ActionBody>);
}

dyn_clone::clone_trait_object!(Node);

pub enum DrawResult {
    /// Drawing succeeded.
    /// Here is the region on screen that was occupied.
    Complete { region: ScreenRegion },

    /// Not enough space is available; here is what was occupied.
    /// Please wrap and call [`Node::draw`] again with this continuation.
    Wrap {
        region: ScreenRegion,
        continuation: u64,
    },
}

pub struct DrawContext<'ctx> {
    /// The unique identifier of the node being drawn
    pub id: NodeUid,

    /// A handle to the workspace in which this node is being drawn
    pub workspace: &'ctx mut Workspace,

    /// A set of constraints to determine draw sizing
    pub constraints: DrawConstraints,

    /// A UI surface to draw on
    pub ui: &'ctx mut Ui,
}

#[derive(Clone, Copy)]
pub struct DrawConstraints {
    pub pos: PositionConstraint,
    pub x: Option<AxisConstraint>,
    pub y: Option<AxisConstraint>,
    pub can_request_wrap: bool,
    pub continuation: Option<u64>,
}

#[derive(Clone, Copy)]
pub enum PositionConstraint {
    Center(ScreenPos),
    TopLeft(ScreenPos),
}

#[derive(Clone, Copy)]
pub enum AxisConstraint {
    Exactly(f32),
    AtMost(f32),
}

impl AxisConstraint {
    pub fn provided_value(&self) -> f32 {
        match self {
            Self::Exactly(v) => *v,
            Self::AtMost(v) => *v,
        }
    }
}
