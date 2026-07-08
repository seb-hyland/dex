use std::any::Any;

use dyn_clone::DynClone;
use egui::{Pos2, Ui};

use crate::{
    messages::{QueryType, RequestType},
    pool::NodeUid,
    region::DrawRegion,
    workspace::Workspace,
};

pub mod messages;
mod pool;
mod region;
mod workspace;

#[typetag::serde]
pub trait Node: 'static + DynClone {
    /// Draw the node. Called every frame; should not be blocking.
    fn draw(&self, ctx: &mut DrawContext) -> Option<DrawRegion>;

    /// Resolve a query message
    fn query(&self, q: Box<dyn QueryType>) -> Option<Box<dyn Any>>;

    /// Resolve an action
    fn handle_request(&mut self, r: Box<dyn RequestType>);
}

dyn_clone::clone_trait_object!(Node);

pub struct DrawContext<'ctx> {
    /// The unique identifier of the node being drawn
    id: NodeUid,

    /// A UI surface to draw on
    ui: &'ctx mut Ui,

    /// The top-left corner of the draw location
    /// May be ignored by nodes that position relative to other nodes
    pos: Pos2,

    /// The width that this node should occupy
    width: Option<f32>,

    /// The height that this node should occupy
    height: Option<f32>,

    /// A handle to the workspace in which this node is being drawn
    workspace: &'ctx mut Workspace,
}
