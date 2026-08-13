pub mod canvas;
pub mod desktops;
pub mod horizontal;
pub mod horizontal_dnd;
pub mod vertical;

use dex_core::prelude::*;
use serde::{Deserialize, Serialize};
use utils::Reset;

/**
   A single child of a layout with the polymorphic ability to draw itself.
*/
#[derive(Clone, Reset, Serialize, Deserialize)]
pub enum LayoutChild {
    /// A registry node, drawn via [`DrawContext::draw_workspace_node`].
    Workspace(NodeUid),
    /// An inline, parent-owned node, drawn via [`DrawContext::draw_node`].
    Local(Box<dyn Node>),
}

impl LayoutChild {
    pub fn local(node: impl Node) -> Self {
        LayoutChild::Local(Box::new(node))
    }

    /**
       Draw this child within a layout.

       `local_id` gives inline ([`LayoutChild::Local`]) nodes a stable identity, and is ignored for [`LayoutChild::Workspace`]

       ## Returns
       - `None` if the workspace child no longer exists
    */
    pub fn draw(
        &self,
        ctx: &mut DrawContext,
        local_id: NodeUid,
        constraints: DrawConstraints,
    ) -> Option<DrawResult> {
        match self {
            LayoutChild::Workspace(uid) => ctx.draw_workspace_node(*uid, constraints),
            LayoutChild::Local(node) => Some(ctx.draw_node(&**node, local_id, constraints)),
        }
    }
}

impl From<NodeUid> for LayoutChild {
    fn from(uid: NodeUid) -> Self {
        LayoutChild::Workspace(uid)
    }
}
