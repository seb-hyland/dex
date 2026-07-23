use rpds::HashTrieMap;
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};
use utils::{HistoryGraph, Timestamp};

use crate::{Node, messages::action::Action, region::ScreenRegion};

/**
    A unique identifier for a node.
    Used for registry lookup and messaging.
*/
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct NodeUid(u64);

/**
   A historical registry of all nodes and their changes over time.
*/
#[derive(Serialize, Deserialize)]
pub struct Registry {
    /// An owned pool of all nodes that exist, at any time, in any place.
    pool: NodePool,

    /// A history of nodes and requests
    history: HistoryGraph<WorldSnapshot, Action>,

    /// An ID for the current frame
    current_frame: u64,
}

impl Registry {
    pub fn get(&self, id: NodeUid) -> Option<(&dyn Node, Option<ScreenRegion>)> {
        let maybe_nobj = self.history.current_epoch().data.map.get(&id);
        maybe_nobj.map(|nobj| (nobj.current(&self.pool), nobj.last_known_region.clone()))
    }

    pub fn start_epoch(&mut self, edge: Action) {
        let ts = Timestamp::now();
        self.history.start_epoch(edge, ts);
    }

    pub fn current_epoch_time(&self) -> Timestamp {
        self.history.current_epoch().time()
    }

    pub fn apply_request(&mut self, req: Action) {
        let cur_epoch_time = self.current_epoch_time();

        if let Some(nobj) = self.history.current_epoch_mut().map.get_mut(&req.dest) {
            let node_mut = nobj.make_mut(req.clone(), cur_epoch_time, &mut self.pool);
            node_mut.handle_action(req.body);
        }
    }

    pub fn update_node_region(&mut self, id: NodeUid, new_region: Option<ScreenRegion>) {
        let Some(nobj) = self.history.current_epoch_mut().map.get_mut(&id) else {
            // Node does not exist
            return;
        };

        let Some(region) = new_region else {
            // No information available to update with
            return;
        };

        if nobj.last_known_frame != self.current_frame {
            // Stale data; clear it
            nobj.last_known_region = None;
            nobj.last_known_frame = self.current_frame;
        }

        nobj.last_known_region = match nobj.last_known_region {
            None => Some(region),
            Some(existing_region) => Some(existing_region.union(region)),
        };
    }
}

/**
   A registry of nodes for messaging.

   An instance of [`NodeRegistry`] represents a snapshot of the workspace at a point in time.
   A registry is structurally shared and CoW; cloning it is cheap.
*/
#[derive(Clone, Serialize, Deserialize)]
struct WorldSnapshot {
    map: HashTrieMap<NodeUid, NodeObject>,
    next_count: u64,
}

impl WorldSnapshot {
    fn push(&mut self, new_node: Box<dyn Node>, pool: &mut NodePool) -> NodeUid {
        let uid = NodeUid(self.next_count);
        let new_node_object = NodeObject::new(new_node, pool);
        self.map.insert_mut(uid, new_node_object);

        self.next_count += 1;
        uid
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct NodeObject {
    /// A history of the node's previous states for fine-grained rollbacks
    history: HistoryGraph<NodeRef, Action>,

    /// The regions at which this node was drawn either last frame or this frame
    pub(crate) last_known_region: Option<ScreenRegion>,

    /// The frame at which [`Self::last_known_regions`] was updated
    pub(crate) last_known_frame: u64,
}

impl NodeObject {
    fn new(node: Box<dyn Node>, pool: &mut NodePool) -> Self {
        let new_ref = pool.insert(node);
        Self {
            history: HistoryGraph::new(new_ref),
            last_known_region: None,
            last_known_frame: 0,
        }
    }

    pub(crate) fn current<'pool>(&self, pool: &'pool NodePool) -> &'pool dyn Node {
        let cur_epoch = self.history.current_epoch();
        pool.get(cur_epoch.data)
            .expect("Reference in use should not be freed")
    }

    pub(crate) fn make_mut<'pool>(
        &mut self,
        req: Action,
        epoch_ts: Timestamp,
        pool: &'pool mut NodePool,
    ) -> &'pool mut dyn Node {
        self.history.start_epoch(req, epoch_ts);
        let cur_epoch_mut = self.history.current_epoch_mut();
        let (new_node_ref, node_mut) = pool
            .make_mut(*cur_epoch_mut)
            .expect("Reference in use should not be freed");

        // Point this object's reference within the current epoch to the new node
        *cur_epoch_mut = new_node_ref;
        node_mut
    }
}

/**
   A pool of persistent, immutable nodes.

   This is used internally for flat (de)serialization.
   [`NodeRef`]s should never be used directly; the public API is [`NodeUid`]s.
   A [`NodeUid`] may point to different [`NodeRef`]s at different points in time (i.e., if it is replaced).
*/
#[derive(Serialize, Deserialize)]
pub(crate) struct NodePool {
    inner: SlotMap<NodeRef, Box<dyn Node>>,
}

new_key_type! { struct NodeRef; }

impl NodePool {
    fn get(&self, nref: NodeRef) -> Option<&dyn Node> {
        self.inner.get(nref).map(|n| &**n)
    }

    fn insert(&mut self, node: Box<dyn Node>) -> NodeRef {
        self.inner.insert(node)
    }

    fn make_mut(&mut self, cur_ref: NodeRef) -> Option<(NodeRef, &mut dyn Node)> {
        if let Some(cur_node) = self.get(cur_ref) {
            let new_node = dyn_clone::clone_box(cur_node);
            let new_ref = self.inner.insert(new_node);
            let new_node_mut = &mut **self.inner.get_mut(new_ref).unwrap();
            Some((new_ref, new_node_mut))
        } else {
            None
        }
    }
}
