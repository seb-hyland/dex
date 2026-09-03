use dex_core::prelude::*;
use serde::{Deserialize, Serialize};
use utils::Reset;

/// How much of a layout's space a child asks for.
#[derive(Copy, Default, PartialEq, Eq)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub enum Sizing {
    /// As much as the child needs, and no more.
    #[default]
    Fixed,
    /// An equal share of whatever the fixed children leave.
    Fill,
}

#[utils::dynamic_methods]
impl Sizing {
    /// Sizing for `count` children where only the last one fills.
    pub fn fill_last(count: usize) -> Vec<Sizing> {
        let mut all = vec![Sizing::Fixed; count];
        if let Some(last) = all.last_mut() {
            *last = Sizing::Fill;
        }
        all
    }

    /// Sizing for `count` children where only the one at `index` fills.
    pub fn fill_at(count: usize, index: usize) -> Vec<Sizing> {
        let mut all = vec![Sizing::Fixed; count];
        if let Some(slot) = all.get_mut(index) {
            *slot = Sizing::Fill;
        }
        all
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum LayoutChild {
    Id(NodeUid),
    /// Like [`LayoutChild::Id`], but content the user can point at.
    Inspectable(NodeUid),
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

    /// Lay this child out under `constraints` without showing it, to find out how big it comes out.
    pub(crate) fn measure(
        &self,
        ctx: &mut DrawContext,
        constraints: DrawConstraints,
    ) -> DrawResult {
        match self {
            LayoutChild::Id(uid) | LayoutChild::Inspectable(uid) => ctx
                .measure_workspace_node(*uid, constraints)
                .unwrap_or(DrawResult::Complete { region: None }),
            LayoutChild::Node(node) => ctx.measure_node(&**node, constraints),
        }
    }

    /// Draw this child under `constraints`.
    pub(crate) fn draw(&self, ctx: &mut DrawContext, constraints: DrawConstraints) -> DrawResult {
        match self {
            LayoutChild::Id(uid) => ctx
                .draw_workspace_node(*uid, constraints)
                .unwrap_or(DrawResult::Complete { region: None }),
            LayoutChild::Inspectable(uid) => ctx
                .draw_inspectable_node(*uid, constraints)
                .unwrap_or(DrawResult::Complete { region: None }),
            LayoutChild::Node(node) => ctx.draw_node(&**node, constraints),
        }
    }
}

/// A layout child accepts a node handle (kept live) or any value (coerced).
impl dex_core::scripting::FromDynamic for LayoutChild {
    fn from_dynamic(obj: &pyo3::Bound<'_, pyo3::PyAny>) -> pyo3::PyResult<Self> {
        Ok(Self::from_dynamic_py(obj))
    }
}

impl dex_core::scripting::IntoDynamic for LayoutChild {
    fn into_dynamic(self, py: pyo3::Python<'_>) -> pyo3::PyResult<pyo3::Py<pyo3::PyAny>> {
        match self {
            LayoutChild::Id(uid) | LayoutChild::Inspectable(uid) => uid.into_dynamic(py),
            LayoutChild::Node(node) => node.into_dynamic(py),
        }
    }
}

impl Reset for LayoutChild {
    fn reset(&self) {
        match self {
            LayoutChild::Id(uid) | LayoutChild::Inspectable(uid) => uid.reset(),
            LayoutChild::Node(node) => node.reset(),
        }
    }
}

impl dex_core::refs::NodeRefs for LayoutChild {
    fn owned_refs(&self, f: &mut dyn FnMut(NodeUid)) {
        match self {
            LayoutChild::Id(uid) | LayoutChild::Inspectable(uid) => f(*uid),
            LayoutChild::Node(node) => node.owned_refs(f),
        }
    }

    fn remap_refs(&mut self, map: &std::collections::HashMap<NodeUid, NodeUid>) {
        match self {
            LayoutChild::Id(uid) | LayoutChild::Inspectable(uid) => {
                if let Some(replacement) = map.get(uid) {
                    *uid = *replacement;
                }
            }
            LayoutChild::Node(node) => {
                *node = dex_core::refs::remapped(&**node, map);
            }
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
