use std::borrow::Cow;

use dyn_clone::DynClone;
use egui::Ui;

// Allows macro-generated `::dex_core::...` paths to resolve within this crate itself.
extern crate self as dex_core;

mod compute;
mod constraints;
pub mod inspect;
pub mod messages;
mod pool;
pub mod pycontext;
pub mod refs;
mod region;
pub mod scripting;
pub mod snapshot;
pub mod stubs;
pub mod stubs_gen;
mod style;
mod theme;
mod workspace;

pub mod prelude {
    pub use crate::{
        compute::ComputeTask,
        constraints::*,
        inspect::{InspectProbe, InspectTarget, Inspectable},
        messages::*,
        pool::NodeUid,
        pycontext::{PyDrawContext, PyNodeContext, PyWorkspace},
        refs::NodeRefs,
        region::{ScreenPos, ScreenRegion, Vector},
        scripting::{NodeExtractor, NodeHandle},
        snapshot::GraphSnapshot,
        style::{BOLD_FAMILY, BOLD_ITALIC_FAMILY, Color, Font, ITALIC_FAMILY, Stroke, StrokeKind},
        workspace::{Workspace, WorkspaceActionHandle},
        *,
    };
    pub use std::sync::Arc;
    pub use utils::AsAny;
}
pub use prelude::*;
use utils::Reset;

#[typetag::serde]
pub trait Node:
    RequestableDyn + ActionHandler + Reset + NodeRefs + 'static + DynClone + Send + Sync + utils::AsAny
{
    /// The node's name, as shown to the user.
    fn type_name(&self, ctx: NodeContext) -> String;

    /// A token for version identity. No invariants (e.g., ordering) must be upheld, except version uniqueness.
    fn version(&self, ctx: NodeContext) -> u64 {
        ctx.workspace.subtree_version(ctx.id)
    }

    /// Given some context, draw the node on screen
    #[deprecated = "This should never be called directly. Use `DrawContext::draw_workspace_node` instead."]
    // This deprecation attribute prevents direct `<instance>.draw(ctx)` calls
    fn draw(&self, ctx: DrawContext) -> DrawResult;

    fn deref_target(&self) -> Option<NodeUid> {
        None
    }

    /// Build this node's inspector into the workspace, returning its root.
    fn build_inspector(&self, _ctx: NodeContext) -> Option<NodeUid> {
        None
    }

    /// A tick run for every node regardless of whether it is drawn. Allows nodes to compute and sync offscreen.
    fn tick(&self, _ctx: NodeContext) {}

    fn on_delete(&self, _ctx: NodeContext) {}
}

dyn_clone::clone_trait_object!(Node);

#[utils::dynamic_type]
#[utils::portable]
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

#[utils::dynamic_methods]
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

    /// How deep in the draw tree this node sits.
    pub depth: u32,
}

#[utils::dynamic_scoped(PyDrawContext)]
impl<'ctx> DrawContext<'ctx> {
    #[dynamic(skip)] // generic over the message type; scripts use `send_action`
    pub fn submit_action_for_self<N, A>(&self, body: A, description: impl Into<Cow<'static, str>>)
    where
        N: Node + ?Sized,
        A: ActionFor<N> + 'static,
    {
        self.node
            .workspace
            .submit_action(self.node.id.cast::<N>(), description, body);
    }

    pub fn request_skip_frame(&self) {
        self.ui.request_discard("need_skip");
    }

    pub(crate) fn reborrow<'rb>(&'rb mut self) -> DrawContext<'rb> {
        DrawContext {
            node: self.node,
            constraints: self.constraints,
            depth: self.depth,
            ui: &mut *self.ui,
        }
    }
}
