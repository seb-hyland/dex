use std::cell::RefCell;

use crate::{pool::NodeUid, region::ScreenPos, region::ScreenRegion};

/// How far outside a node still counts as pointing at it.
const HOVER_MARGIN: f32 = 30.0;

/// The addressable node under the pointer, and the draw path that reached it.
#[derive(Clone)]
pub struct InspectTarget {
    pub node: NodeUid,
    /// The region the node drew into, for handle placement.
    pub region: ScreenRegion,
    /// Addressable ancestors, outermost first.
    pub path: Vec<NodeUid>,
}

/// Per-frame record of the addressable node under the pointer.
#[derive(Default)]
pub struct InspectProbe {
    inner: RefCell<ProbeState>,
}

#[derive(Default)]
struct ProbeState {
    pointer: Option<ScreenPos>,
    /// Addressable nodes currently being drawn into, outermost first.
    path: Vec<NodeUid>,
    /// The deepest hit so far this frame, with its depth.
    best: Option<(u32, InspectTarget)>,
}

impl InspectProbe {
    /// Drop the previous winner and take the pointer position.
    pub fn begin_frame(&self, pointer: Option<ScreenPos>) {
        let mut state = self.inner.borrow_mut();
        state.pointer = pointer;
        state.path.clear();
        state.best = None;
    }

    /// Note that an addressable `node` is about to be drawn.
    pub(crate) fn enter(&self, node: NodeUid) {
        self.inner.borrow_mut().path.push(node);
    }

    /// Finish `node`, recording it if it is the best hit so far.
    pub(crate) fn leave(&self, node: NodeUid, depth: u32, region: Option<ScreenRegion>) {
        let mut state = self.inner.borrow_mut();
        state.path.pop();

        let (Some(pointer), Some(region)) = (state.pointer, region) else {
            return;
        };
        let hover_area = ScreenRegion::from_min_max(
            ScreenPos {
                x: region.min.x - HOVER_MARGIN,
                y: region.min.y - HOVER_MARGIN,
            },
            ScreenPos {
                x: region.max.x + HOVER_MARGIN,
                y: region.max.y + HOVER_MARGIN,
            },
        );
        if !hover_area.contains(pointer) {
            return;
        }

        // Deeper wins, so the innermost element is the target. At equal depth the later draw wins.
        if state.best.as_ref().is_some_and(|(best, _)| *best > depth) {
            return;
        }

        let mut path = state.path.clone();
        path.push(node);
        state.best = Some((depth, InspectTarget { node, region, path }));
    }

    /// The addressable node under the pointer, if found.
    pub fn target(&self) -> Option<InspectTarget> {
        self.inner.borrow().best.as_ref().map(|(_, t)| t.clone())
    }
}
