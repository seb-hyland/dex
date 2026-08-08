use std::{
    cell::RefCell,
    fmt,
    hash::{DefaultHasher, Hash, Hasher},
    marker::PhantomData,
};

use rpds::HashTrieMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use slotmap::{SlotMap, new_key_type};
use utils::{HistoryGraph, Reset, Timestamp, impl_Reset_noop};
use uuid::Uuid;

use crate::{Node, messages::Action, region::ScreenRegion, workspace::PushWorkspaceNode};

/**
    A unique identifier for a node.
    Used for registry lookup and messaging.

    The type parameter records the node type as a compile-time only tag.
*/
pub struct NodeUid<T: ?Sized = dyn Node> {
    id: Uuid,
    _marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized> NodeUid<T> {
    /// A fresh, random (v4) id for a workspace (registry-resident) node.
    pub(crate) fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            _marker: PhantomData,
        }
    }

    /// The nil id, used as an action's destination when there is no target node (the workspace itself is the target).
    pub fn nil() -> Self {
        Self {
            id: Uuid::nil(),
            _marker: PhantomData,
        }
    }

    /// A stable, deterministic id for a parent-owned node, derived from the `parent` id and a stable ident.
    pub fn new_local(parent: NodeUid, ident: impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        ident.hash(&mut hasher);
        let name = hasher.finish().to_le_bytes();
        Self {
            id: Uuid::new_v5(&parent.id, &name),
            _marker: PhantomData,
        }
    }

    pub fn erase(self) -> NodeUid {
        NodeUid {
            id: self.id,
            _marker: PhantomData,
        }
    }

    pub fn cast<U: ?Sized>(self) -> NodeUid<U> {
        NodeUid {
            id: self.id,
            _marker: PhantomData,
        }
    }

    /**
       ## Returns
       - `true` for a workspace-owned node
       - `false` for a local node
    */
    pub fn is_workspace(self) -> bool {
        self.id.get_version_num() == 4
    }
}

/*
    Trait impls ----------------------------------------
    (written manually to ignore the tag field)
*/

impl<T: ?Sized> Clone for NodeUid<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for NodeUid<T> {}

impl<T: ?Sized> PartialEq for NodeUid<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: ?Sized> Eq for NodeUid<T> {}

impl<T: ?Sized> Hash for NodeUid<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T: ?Sized> fmt::Debug for NodeUid<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeUid({})", self.id)
    }
}

impl<T: ?Sized> Serialize for NodeUid<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.id.serialize(serializer)
    }
}

impl<'de, T: ?Sized> Deserialize<'de> for NodeUid<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self {
            id: Uuid::deserialize(deserializer)?,
            _marker: PhantomData,
        })
    }
}

impl<T: ?Sized> Reset for NodeUid<T> {
    #[inline(always)]
    fn reset(&self) {}
}

/**
   A historical registry of all nodes and their changes over time.
*/
#[derive(Serialize, Deserialize, Reset)]
pub struct Registry {
    /// An owned pool of all nodes that exist, at any time, in any place.
    pool: NodePool,

    /// A history of nodes and requests
    history: HistoryGraph<WorldSnapshot, Action>,

    /// An ID for the current frame
    current_frame: u64,
}

impl Registry {
    pub(crate) fn new(root_node: Box<dyn Node>) -> (Self, NodeUid) {
        let mut pool = NodePool::default();
        let mut snapshot = WorldSnapshot::default();

        let root_id = NodeUid::new();
        snapshot.push(root_node, root_id, &mut pool);

        (
            Self {
                pool,
                history: HistoryGraph::new(snapshot),
                current_frame: 0,
            },
            root_id,
        )
    }

    pub(crate) fn tick_frame(&mut self) {
        self.current_frame += 1;
    }

    pub(crate) fn get(&self, id: NodeUid) -> Option<(&dyn Node, Option<ScreenRegion>)> {
        let maybe_nobj = self.history.current_epoch().data.map.get(&id);
        maybe_nobj.map(|nobj| (nobj.current(&self.pool), *nobj.last_known_region.borrow()))
    }

    pub(crate) fn push(&mut self, action: PushWorkspaceNode) {
        self.history
            .current_epoch_mut()
            .push(action.node, action.uid, &mut self.pool);
    }

    pub(crate) fn start_epoch(&mut self, edge: Action) {
        let ts = Timestamp::now();
        self.history.start_epoch(edge, ts);
    }

    pub(crate) fn current_epoch_time(&self) -> Timestamp {
        self.history.current_epoch().time()
    }

    pub(crate) fn apply_action(&mut self, mut req: Action) {
        loop {
            let cur_epoch_time = self.current_epoch_time();

            let dest = req.dest;
            let Some(nobj) = self.history.current_epoch_mut().map.get_mut(&dest) else {
                // Unknown destination; discard
                return;
            };

            let node_mut = nobj.make_mut(req.clone(), cur_epoch_time, &mut self.pool);
            let Some(unhandled) = node_mut.handle_action(req.body) else {
                // The action was understood and handled
                return;
            };

            // Not understood here; dereference to the child, if any, and try again
            let Some((node, _)) = self.get(dest) else {
                return;
            };
            let Some(target) = node.deref_target() else {
                return;
            };
            req = Action {
                dest: target,
                description: req.description,
                body: unhandled,
            };
        }
    }

    pub(crate) fn update_node_region(&self, id: NodeUid, new_region: Option<ScreenRegion>) {
        let Some(nobj) = self.history.current_epoch().data.map.get(&id) else {
            // Node does not exist
            return;
        };

        let Some(region) = new_region else {
            // No information available to update with
            return;
        };

        if *nobj.last_known_frame.borrow() != self.current_frame {
            // Stale data; clear it
            *nobj.last_known_region.borrow_mut() = None;
            *nobj.last_known_frame.borrow_mut() = self.current_frame;
        }

        let last_known_region = *nobj.last_known_region.borrow();
        *nobj.last_known_region.borrow_mut() = match last_known_region {
            None => Some(region),
            Some(existing_region) => Some(existing_region.union(region)),
        };
    }
}

/**
   An instance of [`WorldSnapshot`] represents a snapshot of the workspace at a point in time.
   It is structurally shared and CoW, so cloning it is cheap.
*/
#[derive(Clone, Default, Reset, Serialize, Deserialize)]
struct WorldSnapshot {
    map: HashTrieMap<NodeUid, NodeObject>,
}

impl WorldSnapshot {
    fn push(&mut self, new_node: Box<dyn Node>, uid: NodeUid, pool: &mut NodePool) {
        let new_node_object = NodeObject::new(new_node, pool);
        self.map.insert_mut(uid, new_node_object);
    }
}

#[derive(Clone, Serialize, Deserialize, Reset)]
struct NodeObject {
    /// A history of the node's previous states for fine-grained rollbacks
    history: HistoryGraph<NodeRef, Action>,

    /// The regions at which this node was drawn either last frame or this frame
    pub(crate) last_known_region: RefCell<Option<ScreenRegion>>,

    /// The frame at which [`Self::last_known_regions`] was updated
    pub(crate) last_known_frame: RefCell<u64>,
}

impl NodeObject {
    fn new(node: Box<dyn Node>, pool: &mut NodePool) -> Self {
        let new_ref = pool.insert(node);
        Self {
            history: HistoryGraph::new(new_ref),
            last_known_region: RefCell::new(None),
            last_known_frame: RefCell::new(0),
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
#[derive(Default, Serialize, Deserialize, Reset)]
pub(crate) struct NodePool {
    inner: SlotMap<NodeRef, Box<dyn Node>>,
}

new_key_type! { struct NodeRef; }
impl_Reset_noop!(NodeRef);

impl NodePool {
    fn get(&self, nref: NodeRef) -> Option<&dyn Node> {
        self.inner.get(nref).map(|n| &**n)
    }

    #[must_use]
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
