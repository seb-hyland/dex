use dyn_clone::DynClone;
use egui::Ui;

mod constraints;
pub mod messages;
mod pool;
mod region;
mod theme;
mod workspace;

pub mod prelude {
    pub use crate::{
        constraints::*,
        messages::{
            action::*,
            request::{Region, Request, RequestBody, Requestable, TypedRequest, TypedRequestBody},
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
    /// Drawing completed.
    /// Here is the region on screen that was occupied.
    Complete { region: Option<ScreenRegion> },

    /// Not enough space is available; here is what was occupied.
    /// Please wrap and call [`Node::draw`] again with this continuation.
    Wrap {
        region: Option<ScreenRegion>,
        continuation: u64,
    },
}

impl DrawResult {
    pub fn region(&self) -> Option<ScreenRegion> {
        match self {
            Self::Complete { region } => *region,
            Self::Wrap { region, .. } => *region,
        }
    }
}

pub struct DrawContext<'ctx> {
    /// The unique identifier of the node being drawn
    pub id: NodeUid,

    /// A set of constraints to determine draw sizing
    pub constraints: DrawConstraints,

    /// A UI surface to draw on
    pub ui: &'ctx mut Ui,

    /// A handle to the workspace in which this node is being drawn
    pub workspace: &'ctx mut Workspace,
}

impl<'ctx> DrawContext<'ctx> {
    pub fn last_frame_location(&self) -> Option<ScreenRegion> {
        self.workspace
            .send_request(TypedRequest {
                dest: self.id,
                body: Box::new(Region),
            })
            .flatten()
    }

    pub fn request_skip_frame(&self) {
        self.ui.request_discard("need_skip");
    }

    pub fn reborrow<'rb>(&'rb mut self) -> DrawContext<'rb> {
        DrawContext {
            id: self.id,
            constraints: self.constraints,
            workspace: &mut *self.workspace,
            ui: &mut *self.ui,
        }
    }
}
