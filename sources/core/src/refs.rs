//! Ownership traversal over the [`NodeUid`]s a node holds for deep copying.

use std::{collections::HashMap, sync::Arc};

use crate::{Node, pool::NodeUid};

/// The uid edges a value holds.
pub trait NodeRefs {
    /// Visit every uid this value *owns*. Not recursive across the registry: it
    /// yields direct children, and the caller walks outward from there.
    fn owned_refs(&self, f: &mut dyn FnMut(NodeUid));

    /// Rewrite every uid this value holds — owned or referenced — through
    /// `map`. A uid absent from `map` is left as it is.
    fn remap_refs(&mut self, map: &HashMap<NodeUid, NodeUid>);
}

/// Implement [`NodeRefs`] as a no-op, for types that hold no uids.
#[macro_export]
macro_rules! impl_NodeRefs_noop {
    ($($impl_type:ty),* $(,)?) => {
        $(
            impl $crate::refs::NodeRefs for $impl_type {
                #[inline(always)]
                fn owned_refs(&self, _f: &mut dyn FnMut($crate::NodeUid)) {}
                #[inline(always)]
                fn remap_refs(
                    &mut self,
                    _map: &::std::collections::HashMap<$crate::NodeUid, $crate::NodeUid>,
                ) {}
            }
        )*
    };
}

impl_NodeRefs_noop!(
    (),
    bool,
    char,
    String,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
    std::path::PathBuf,
    egui::Color32,
    egui::Pos2,
    egui::Vec2,
    egui::Rect,
    egui::Stroke,
    egui::StrokeKind,
    egui::FontId,
);

impl<T: ?Sized> NodeRefs for NodeUid<T> {
    fn owned_refs(&self, f: &mut dyn FnMut(NodeUid)) {
        f(self.erase());
    }

    fn remap_refs(&mut self, map: &HashMap<NodeUid, NodeUid>) {
        if let Some(replacement) = map.get(&self.erase()) {
            *self = replacement.cast();
        }
    }
}

impl<T: NodeRefs> NodeRefs for Option<T> {
    fn owned_refs(&self, f: &mut dyn FnMut(NodeUid)) {
        if let Some(inner) = self {
            inner.owned_refs(f);
        }
    }

    fn remap_refs(&mut self, map: &HashMap<NodeUid, NodeUid>) {
        if let Some(inner) = self {
            inner.remap_refs(map);
        }
    }
}

impl<T: NodeRefs> NodeRefs for Vec<T> {
    fn owned_refs(&self, f: &mut dyn FnMut(NodeUid)) {
        for item in self {
            item.owned_refs(f);
        }
    }

    fn remap_refs(&mut self, map: &HashMap<NodeUid, NodeUid>) {
        for item in self {
            item.remap_refs(map);
        }
    }
}

impl<T: NodeRefs, const N: usize> NodeRefs for [T; N] {
    fn owned_refs(&self, f: &mut dyn FnMut(NodeUid)) {
        for item in self {
            item.owned_refs(f);
        }
    }

    fn remap_refs(&mut self, map: &HashMap<NodeUid, NodeUid>) {
        for item in self {
            item.remap_refs(map);
        }
    }
}

/// An inline node is a value carried by its holder, not a registry entry, so
/// its uids are rewritten in place rather than walked as children.
impl NodeRefs for Arc<dyn Node> {
    fn owned_refs(&self, f: &mut dyn FnMut(NodeUid)) {
        (**self).owned_refs(f);
    }

    fn remap_refs(&mut self, map: &HashMap<NodeUid, NodeUid>) {
        *self = remapped(&**self, map);
    }
}

/// A cached value is rebuilt rather than carried, so it holds no edges worth
/// following: a copy resets it.
impl<T: Clone> NodeRefs for utils::Transient<T> {
    #[inline(always)]
    fn owned_refs(&self, _f: &mut dyn FnMut(NodeUid)) {}
    #[inline(always)]
    fn remap_refs(&mut self, _map: &HashMap<NodeUid, NodeUid>) {}
}

/// A copy of `node` with every uid it holds rewritten through `map`.
///
/// The node is treated as a value: this does not register anything or walk
/// owned children.
pub fn remapped(node: &dyn Node, map: &HashMap<NodeUid, NodeUid>) -> Arc<dyn Node> {
    let mut owned = dyn_clone::clone_box(node);
    // A node whose `Clone` shares state duplicates it here, in its own
    // `remap_refs`: this is only ever reached while building a clone.
    owned.remap_refs(map);
    Arc::from(owned)
}
