use std::cell::RefCell;

use crate::{pool::NodeUid, region::ScreenPos, region::ScreenRegion};

/// How far outside a node still counts as pointing at it.
const HOVER_MARGIN: f32 = 30.0;

/// The addressable node under the pointer.
#[derive(Clone)]
pub struct InspectTarget {
    pub node: NodeUid,
    /// The on-screen part of the node.
    pub region: ScreenRegion,
}

/// Per-frame record of every addressable node drawn.
#[derive(Default)]
pub struct InspectProbe {
    inner: RefCell<ProbeState>,
}

#[derive(Default)]
struct ProbeState {
    pointer: Option<ScreenPos>,
    /// This frame's nodes, in draw order, as each finishes drawing.
    drawn: Vec<Drawn>,
    /// The last frame that finished, as a fallback.
    settled: Vec<Drawn>,
}

/// Where an addressable node drew, and how deep it sat.
#[derive(Clone, Copy)]
struct Drawn {
    node: NodeUid,
    /// Deeper wins a hit test.
    depth: u32,
    /// Everything the node drew, on screen or not, for anchoring to it.
    region: ScreenRegion,
    /// The part of `region` actually on screen.
    visible: Option<ScreenRegion>,
}

impl InspectProbe {
    /// Settle last frame's record and begin this one.
    pub fn begin_frame(&self, pointer: Option<ScreenPos>) {
        let mut state = self.inner.borrow_mut();
        state.pointer = pointer;
        state.settled = std::mem::take(&mut state.drawn);
    }

    /// Record that an addressable `node` drew into `region`, of which `visible` is on screen.
    pub(crate) fn record(
        &self,
        node: NodeUid,
        depth: u32,
        region: Option<ScreenRegion>,
        visible: Option<ScreenRegion>,
    ) {
        let Some(region) = region else {
            return;
        };
        self.inner.borrow_mut().drawn.push(Drawn {
            node,
            depth,
            region,
            visible,
        });
    }

    /// The addressable node under the pointer, if found.
    pub fn target(&self) -> Option<InspectTarget> {
        let state = self.inner.borrow();
        let hit = deepest_over(&state.drawn, state.pointer?, HOVER_MARGIN)?;
        Some(InspectTarget {
            node: hit.node,
            region: hit.visible.unwrap_or(hit.region),
        })
    }

    /// The innermost addressable node drawn over `pos`, as of the last finished frame.
    pub fn at(&self, pos: ScreenPos) -> Option<NodeUid> {
        deepest_over(&self.inner.borrow().settled, pos, 0.0).map(|hit| hit.node)
    }

    /// Everything an addressable `node` drew, as of the last finished frame.
    pub fn region_of(&self, node: NodeUid) -> Option<ScreenRegion> {
        self.inner
            .borrow()
            .settled
            .iter()
            .rev()
            .find(|drawn| drawn.node == node)
            .map(|drawn| drawn.region)
    }
}

/// The innermost node whose on-screen part, grown by `margin`, covers `pos`.
fn deepest_over(drawn: &[Drawn], pos: ScreenPos, margin: f32) -> Option<&Drawn> {
    drawn
        .iter()
        .filter(|entry| {
            entry
                .visible
                .is_some_and(|region| region.expand(margin).contains(pos))
        })
        // `max_by_key` keeps the last of equals, so at equal depth the later draw (on top) wins.
        .max_by_key(|entry| entry.depth)
}
