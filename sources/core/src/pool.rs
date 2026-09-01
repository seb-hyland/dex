use std::{
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    sync::Arc,
};

use rpds::HashTrieMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use slotmap::{Key, SlotMap, new_key_type};
use utils::{HistoryGraph, Reset, Timestamp, impl_Reset_noop};
use uuid::Uuid;

use crate::{Node, messages::Action, workspace::PushWorkspaceNode};

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
    /// A fresh id for a workspace node.
    pub fn mint() -> Self {
        Self {
            id: Uuid::new_v4(),
            _marker: PhantomData,
        }
    }

    /// The nil id, used as an action's destination when there is no target node.
    pub fn nil() -> Self {
        Self {
            id: Uuid::nil(),
            _marker: PhantomData,
        }
    }

    /// A stable, filesystem-safe key for this id.
    pub fn key(self) -> String {
        self.id.to_string()
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
}

impl Registry {
    pub(crate) fn new(root_node: impl Node) -> (Self, NodeUid) {
        let mut pool = NodePool::default();
        let mut snapshot = WorldSnapshot::default();

        let root_id = NodeUid::mint();
        snapshot.push(Arc::new(root_node), root_id, &mut pool);

        (
            Self {
                pool,
                history: HistoryGraph::new(snapshot),
            },
            root_id,
        )
    }

    /// An empty registry, holding no nodes. Nodes are added synchronously with
    /// [`Registry::push`] during workspace construction.
    pub(crate) fn empty() -> Self {
        Self {
            pool: NodePool::default(),
            history: HistoryGraph::new(WorldSnapshot::default()),
        }
    }

    /// An version token for the current content referenced by a [`NodeUid`]. No ordering guarantees are provided.
    pub(crate) fn version(&self, uid: NodeUid) -> u64 {
        self.history
            .current_epoch()
            .data
            .map
            .get(&uid)
            .map(|nobj| nobj.current_ref().data().as_ffi())
            .unwrap_or(0)
    }

    pub(crate) fn get(&self, id: NodeUid) -> Option<Arc<dyn Node>> {
        let maybe_nobj = self.history.current_epoch().data.map.get(&id);
        maybe_nobj.map(|nobj| nobj.current(&self.pool))
    }

    /// The ids of every node currently in the workspace.
    pub(crate) fn live_ids(&self) -> Vec<NodeUid> {
        self.history
            .current_epoch()
            .data
            .map
            .keys()
            .copied()
            .collect()
    }

    pub(crate) fn push(&mut self, action: PushWorkspaceNode) {
        self.history
            .current_epoch_mut()
            .push(action.node, action.uid, &mut self.pool);
    }

    pub(crate) fn remove(&mut self, uid: NodeUid) {
        self.history.current_epoch_mut().map.remove_mut(&uid);
    }

    pub(crate) fn start_epoch(&mut self, edge: Action) {
        let ts = Timestamp::now();
        self.history.start_epoch(edge, ts);
    }

    pub(crate) fn current_epoch_time(&self) -> Timestamp {
        self.history.current_epoch().time()
    }

    /// Commit `node` as the new current state of `dest`, recording `edge` in the node's history.
    pub(crate) fn commit_node(&mut self, dest: NodeUid, edge: Action, node: Arc<dyn Node>) {
        let cur_epoch_time = self.current_epoch_time();

        if let Some(nobj) = self.history.current_epoch_mut().map.get_mut(&dest) {
            nobj.commit(node, edge, cur_epoch_time, &mut self.pool);
        }
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
    fn push(&mut self, new_node: Arc<dyn Node>, uid: NodeUid, pool: &mut NodePool) {
        let new_node_object = NodeObject::new(new_node, pool);
        self.map.insert_mut(uid, new_node_object);
    }
}

#[derive(Clone, Serialize, Deserialize, Reset)]
struct NodeObject {
    /// A history of the node's previous states for fine-grained rollbacks
    history: HistoryGraph<NodeRef, Action>,
}

impl NodeObject {
    fn new(node: Arc<dyn Node>, pool: &mut NodePool) -> Self {
        let new_ref = pool.insert(node);
        Self {
            history: HistoryGraph::new(new_ref),
        }
    }

    pub(crate) fn current(&self, pool: &NodePool) -> Arc<dyn Node> {
        let cur_epoch = self.history.current_epoch();
        pool.get(cur_epoch.data)
            .expect("Reference in use should not be freed")
    }

    fn current_ref(&self) -> NodeRef {
        self.history.current_epoch().data
    }

    /// Record `node` as this object's current state, starting a new history epoch labelled with `edge`
    pub(crate) fn commit(
        &mut self,
        node: Arc<dyn Node>,
        edge: Action,
        epoch_ts: Timestamp,
        pool: &mut NodePool,
    ) {
        self.history.start_epoch(edge, epoch_ts);
        let cur_epoch_mut = self.history.current_epoch_mut();

        // Point this object's reference within the current epoch to the new node
        *cur_epoch_mut = pool.insert(node);
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
    inner: SlotMap<NodeRef, Arc<dyn Node>>,
}

new_key_type! { struct NodeRef; }
impl_Reset_noop!(NodeRef);

impl NodePool {
    fn get(&self, nref: NodeRef) -> Option<Arc<dyn Node>> {
        self.inner.get(nref).cloned()
    }

    #[must_use]
    fn insert(&mut self, node: Arc<dyn Node>) -> NodeRef {
        self.inner.insert(node)
    }
}
