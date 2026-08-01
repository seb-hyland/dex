use std::hash::{DefaultHasher, Hash, Hasher};

use dyn_clone::DynClone;
use egui::Ui;

mod compute;
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
            request::{
                Region, Request, RequestBody, Requestable, TypedRequest, TypedRequestBody,
                TypedRequestable,
            },
        },
        pool::NodeUid,
        region::{ScreenPos, ScreenRegion, Vector},
        workspace::Workspace,
        *,
    };
}
pub use prelude::*;
use serde::{Deserialize, Serialize};
use utils::Reset;

#[typetag::serde]
pub trait Node: Requestable + Reset + 'static + DynClone + Send {
    fn type_name(&self) -> String;

    /// Given some context, draw the node on screen
    #[deprecated = "This should never be called directly. Use `DrawContext::draw_node` or `DrawContext::draw_workspace_node` instead."]
    // This deprecation attribute prevents direct `<instance>.draw(ctx)` calls
    fn draw(&self, ctx: DrawContext) -> DrawResult;

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

#[non_exhaustive]
pub struct DrawContext<'ctx> {
    /// The unique identifier of the node being drawn
    pub id: Id,

    /// A set of constraints to determine draw sizing
    pub constraints: DrawConstraints,

    /// A UI surface to draw on
    pub ui: &'ctx mut Ui,

    /// A handle to the workspace in which this node is being drawn
    pub workspace: &'ctx Workspace,
}

impl<'ctx> DrawContext<'ctx> {
    pub fn last_frame_location(&self) -> Option<ScreenRegion> {
        let workspace_id = self.id.into_workspace_id()?;
        self.workspace
            .send_request(TypedRequest {
                dest: workspace_id,
                body: Box::new(Region),
            })
            .flatten()
    }

    pub fn request_skip_frame(&self) {
        self.ui.request_discard("need_skip");
    }

    pub(crate) fn reborrow<'rb>(&'rb mut self) -> DrawContext<'rb> {
        DrawContext {
            id: self.id,
            constraints: self.constraints,
            workspace: self.workspace,
            ui: &mut *self.ui,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Id {
    Workspace(NodeUid),
    Local(LocalId),
}

impl Id {
    pub fn into_workspace_id(self) -> Option<NodeUid> {
        match self {
            Self::Workspace(id) => Some(id),
            _ => None,
        }
    }
}

/**
   A stable, local ID for parent-owned nodes.
   Typically produced from a hash of the parent ID and some stable identifier (e.g., field name)
*/
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct LocalId(u64);

impl LocalId {
    pub fn from_cons(car: impl Hash, cdr: impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        car.hash(&mut hasher);
        cdr.hash(&mut hasher);
        Self(hasher.finish())
    }
}
