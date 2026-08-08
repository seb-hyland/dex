use std::borrow::Cow;

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
        messages::*,
        pool::NodeUid,
        region::{ScreenPos, ScreenRegion, Vector},
        workspace::Workspace,
        *,
    };
    pub use utils::AsAny;
}
pub use prelude::*;
use utils::Reset;

#[typetag::serde]
pub trait Node: RequestableDyn + ActionHandler + Reset + 'static + DynClone + Send {
    fn type_name(&self) -> String;

    /// Given some context, draw the node on screen
    #[deprecated = "This should never be called directly. Use `DrawContext::draw_node` or `DrawContext::draw_workspace_node` instead."]
    // This deprecation attribute prevents direct `<instance>.draw(ctx)` calls
    fn draw(&self, ctx: DrawContext) -> DrawResult;

    fn menu_sidebar(&self) -> Option<Box<dyn Node>> {
        None
    }

    fn deref_target(&self) -> Option<NodeUid> {
        None
    }
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
    pub id: NodeUid,

    /// A set of constraints to determine draw sizing
    pub constraints: DrawConstraints,

    /// A UI surface to draw on
    pub ui: &'ctx mut Ui,

    /// A handle to the workspace in which this node is being drawn
    pub workspace: &'ctx Workspace,
}

impl<'ctx> DrawContext<'ctx> {
    pub fn this_node_last_frame_location(&self) -> Option<ScreenRegion> {
        self.last_frame_location_of(self.id)
    }

    pub fn submit_action_for_self<N, A>(&self, body: A, description: impl Into<Cow<'static, str>>)
    where
        N: Node + ?Sized,
        A: ActionFor<N> + 'static,
    {
        if self.id.is_workspace() {
            self.workspace
                .submit_action(self.id.cast::<N>(), description, body);
        }
    }

    pub fn last_frame_location_of(&self, id: NodeUid) -> Option<ScreenRegion> {
        self.workspace.send_request(id, Region).flatten()
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
