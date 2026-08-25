use std::sync::Arc;

use crate::{Node, NodeUid};

/// A fully type-erased id, as seen by scripts. Replaces [`NodeUid`]
#[utils::dynamic_type(name = "NodeUid")]
#[derive(Clone, Copy)]
pub struct NodeHandle(pub NodeUid);

/// Coerces a dynamic value into some type of node (Rust or dynamic-defined).
pub struct NodeExtractor {
    pub from_python: fn(&pyo3::Bound<'_, pyo3::PyAny>) -> Option<Arc<dyn Node>>,
    pub from_steel: fn(&steel::rvals::SteelVal) -> Option<Arc<dyn Node>>,
}

dex_dynamic::__rt::inventory::collect!(NodeExtractor);
