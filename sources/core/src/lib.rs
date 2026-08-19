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

    fn deref_target(&self) -> Option<NodeUid> {
        None
    }

    fn on_delete(&self, _ctx: NodeContext) {}
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

/// A node's context for its place in the world.
#[derive(Clone, Copy)]
pub struct NodeContext<'ctx> {
    pub id: NodeUid,
    pub workspace: &'ctx Workspace,
}

#[non_exhaustive]
pub struct DrawContext<'ctx> {
    /// The identity and workspace handle of the node being drawn.
    pub node: NodeContext<'ctx>,

    /// A set of constraints to determine draw sizing.
    pub constraints: DrawConstraints,

    /// A surface to draw on.
    pub ui: &'ctx mut Ui,
}

impl<'ctx> DrawContext<'ctx> {
    pub fn submit_action_for_self<N, A>(&self, body: A, description: impl Into<Cow<'static, str>>)
    where
        N: Node + ?Sized,
        A: ActionFor<N> + 'static,
    {
        if self.node.id.is_workspace() {
            self.node
                .workspace
                .submit_action(self.node.id.cast::<N>(), description, body);
        }
    }

    pub fn request_skip_frame(&self) {
        self.ui.request_discard("need_skip");
    }

    pub(crate) fn reborrow<'rb>(&'rb mut self) -> DrawContext<'rb> {
        DrawContext {
            node: self.node,
            constraints: self.constraints,
            ui: &mut *self.ui,
        }
    }
}
