use dex_core::prelude::*;
use serde::{Deserialize, Serialize};
use utils::Reset;

#[derive(Clone, Serialize, Deserialize)]
pub enum LayoutChild {
    Id(NodeUid),
    Node(Arc<dyn Node>),
}

impl LayoutChild {
    /// Build a child from a Python value.
    pub fn from_dynamic_py(obj: &pyo3::Bound<'_, pyo3::PyAny>) -> Self {
        use pyo3::prelude::*;
        if let Ok(handle) = obj.extract::<dex_core::NodeHandle>() {
            return LayoutChild::Id(handle.0);
        }
        LayoutChild::Node(crate::scripting::to_dyn_node_py(obj))
    }

    /// Build a child from a Steel value.
    pub fn from_dynamic_steel(val: &dex_dynamic::__rt::steel::rvals::SteelVal) -> Self {
        use dex_dynamic::__rt::steel::rvals::FromSteelVal;
        if let Ok(handle) = dex_core::NodeHandle::from_steelval(val) {
            return LayoutChild::Id(handle.0);
        }
        LayoutChild::Node(crate::scripting::to_dyn_node_steel(val))
    }

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
