use petgraph::{Directed, graph::NodeIndex, prelude::StableGraph};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/**
   A graph representing the history of a data structure.

   The associated type `T` should be cheaply clonable, since each epoch stores a snapshot of state.
   For large data structures, consider wrapping it in [`super::Shared`] for structural sharing between shapshots.
   Mutation is then possible through [`super::Shared::make_mut`].
*/
#[derive(Clone, Serialize, Deserialize)]
pub struct HistoryGraph<T: Clone, Edge> {
    graph: StableGraph<Epoch<T>, Edge, Directed, u32>,
    active_epoch: NodeIndex<u32>,
}

impl<T: Clone, Edge> HistoryGraph<T, Edge> {
    pub fn new(root: T) -> Self {
        let mut graph = StableGraph::new();
        let root_time = Timestamp::now();
        let root_idx = graph.add_node(Epoch {
            time: root_time,
            data: root,
        });

        Self {
            graph,
            active_epoch: root_idx,
        }
    }

    pub fn current_epoch(&self) -> &Epoch<T> {
        self.graph
            .node_weight(self.active_epoch)
            .expect("active_node should point to a valid node")
    }

    pub fn current_epoch_mut(&mut self) -> &mut T {
        &mut self
            .graph
            .node_weight_mut(self.active_epoch)
            .expect("active_node should point to a valid node")
            .data
    }

    pub fn start_epoch(&mut self, edge: Edge, time: Timestamp) {
        let active_node = self.current_epoch();

        let new_epoch = active_node.data.clone();
        let new_epoch_idx = self.graph.add_node(Epoch {
            time,
            data: new_epoch,
        });

        self.graph.add_edge(self.active_epoch, new_epoch_idx, edge);

        self.active_epoch = new_epoch_idx;
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Epoch<T> {
    time: Timestamp,
    pub data: T,
}

impl<T> Epoch<T> {
    pub const fn time(&self) -> Timestamp {
        self.time
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp {
    id: Uuid,
}

impl Timestamp {
    pub fn now() -> Self {
        Self { id: Uuid::now_v7() }
    }
}
