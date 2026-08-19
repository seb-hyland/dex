use dex_core::prelude::*;
use serde::{Deserialize, Serialize};
use utils::Reset;

#[derive(Clone, Serialize, Deserialize)]
pub enum LayoutChild {
    Id(NodeUid),
    Node(Arc<dyn Node>),
}

impl LayoutChild {
    /// Draw this child under `constraints`.
    pub(crate) fn draw(&self, ctx: &mut DrawContext, constraints: DrawConstraints) -> DrawResult {
        match self {
            LayoutChild::Id(uid) => ctx
                .draw_workspace_node(*uid, constraints)
                .unwrap_or(DrawResult::Complete { region: None }),
            LayoutChild::Node(node) => ctx.draw_node(&**node, constraints),
        }
    }
}

impl Reset for LayoutChild {
    fn reset(&self) {
        match self {
            LayoutChild::Id(uid) => uid.reset(),
            LayoutChild::Node(node) => node.reset(),
        }
    }
}

impl<T: ?Sized> From<NodeUid<T>> for LayoutChild {
    fn from(uid: NodeUid<T>) -> Self {
        Self::Id(uid.erase())
    }
}

impl From<Arc<dyn Node>> for LayoutChild {
    fn from(node: Arc<dyn Node>) -> Self {
        Self::Node(node)
    }
}
