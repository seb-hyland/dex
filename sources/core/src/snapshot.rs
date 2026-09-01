//! A read-only view of the node graph that can be carried onto a worker thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;

use crate::{
    Node, NodeContext, NodeHandle, NodeUid, Workspace,
    pycontext::send_request_py,
    refs::NodeRefs,
    scripting::DynNode,
    stubs::{StubClass, StubField, StubMethod},
};

/// The node graph as it stood at one moment, detached from the workspace it came from.
pub struct GraphSnapshot {
    root: NodeUid,
    nodes: Vec<(NodeUid, Arc<dyn Node>)>,
}

impl GraphSnapshot {
    /// Take the graph as it stands. `O(n)` in live nodes.
    pub fn capture(ws: &Workspace) -> Self {
        Self {
            root: ws.root(),
            nodes: ws.live_nodes(),
        }
    }

    /// A workspace over these nodes, attached to nothing.
    fn build(self) -> Workspace {
        Workspace::detached(self.root, self.nodes)
    }
}

/// A snapshot before and after it has been asked its first question.
enum State {
    Captured(GraphSnapshot),
    Built(Box<Workspace>),
    Building,
}

impl State {
    /// The workspace this state holds, once [`State::build`] has run.
    fn workspace(&self) -> &Workspace {
        match self {
            State::Built(ws) => ws,
            // `build` runs to completion under its own borrow, so neither of
            // these is reachable by the time anything asks.
            State::Captured(_) | State::Building => {
                unreachable!("the snapshot is built before it is read")
            }
        }
    }

    fn build(&mut self) {
        // Only a captured snapshot has anything to do.
        if !matches!(self, State::Captured(_)) {
            return;
        }
        if let State::Captured(snapshot) = std::mem::replace(self, State::Building) {
            *self = State::Built(Box::new(snapshot.build()));
        }
    }
}

/// The script-facing snapshot: `dex.snapshot` inside a transform.
#[pyclass(unsendable, name = "Snapshot")]
pub struct PySnapshot {
    state: RefCell<State>,
    /// Child-to-parent, derived from ownership. Built on first use.
    owners: RefCell<Option<HashMap<NodeUid, NodeUid>>>,
}

impl PySnapshot {
    pub fn new(snapshot: GraphSnapshot) -> Self {
        Self {
            state: RefCell::new(State::Captured(snapshot)),
            owners: RefCell::new(None),
        }
    }

    /// Run `f` against the snapshot's workspace, building it if this is the first question asked.
    fn with_ws<R>(&self, f: impl FnOnce(&Workspace) -> R) -> R {
        self.state.borrow_mut().build();
        let state = self.state.borrow();
        f(state.workspace())
    }
}

#[pymethods]
impl PySnapshot {
    /// Send `request` to `dest` and return its response.
    fn send_request(
        &self,
        py: Python<'_>,
        dest: NodeHandle,
        request: Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        self.with_ws(|ws| send_request_py(ws, dest.0, &request, py))
    }

    /// The node held at `uid` when this snapshot was taken, if any.
    fn get_node(&self, uid: NodeHandle) -> Option<DynNode> {
        self.with_ws(|ws| ws.get_node(uid.0)).map(DynNode)
    }

    /// What `uid` calls itself, as the UI shows it.
    fn type_name(&self, uid: NodeHandle) -> Option<String> {
        self.with_ws(|ws| {
            ws.get_node(uid.0).map(|node| {
                node.type_name(NodeContext {
                    id: uid.0,
                    workspace: ws,
                })
            })
        })
    }

    /// Every id in the snapshot.
    fn node_ids(&self) -> Vec<NodeHandle> {
        self.with_ws(|ws| {
            ws.live_nodes()
                .into_iter()
                .map(|(uid, _)| NodeHandle(uid))
                .collect()
        })
    }

    /// The workspace root.
    fn root(&self) -> NodeHandle {
        NodeHandle(self.with_ws(|ws| ws.root()))
    }

    /**
        The node that owns `uid`, if one does.

        Ownership is the same relation a deep clone follows, so a `#[uid_ref]`
        pointer — a wire, say — is not an owner. Turns an id that names some
        inner part into the thing it belongs to.
    */
    fn owner_of(&self, uid: NodeHandle) -> Option<NodeHandle> {
        if self.owners.borrow().is_none() {
            let index = self.with_ws(|ws| {
                let mut index: HashMap<NodeUid, NodeUid> = HashMap::new();
                for (parent, node) in ws.live_nodes() {
                    node.owned_refs(&mut |child| {
                        if child != NodeUid::nil() {
                            index.insert(child, parent);
                        }
                    });
                }
                index
            });
            *self.owners.borrow_mut() = Some(index);
        }
        self.owners
            .borrow()
            .as_ref()
            .and_then(|index| index.get(&uid.0).copied())
            .map(NodeHandle)
    }
}

dex_dynamic::__rt::inventory::submit! {
    dex_dynamic::DynamicBinding {
        name: "Snapshot",
        register_python: |m| {
            use pyo3::types::PyModuleMethods;
            m.add_class::<PySnapshot>()
        },
    }
}

dex_dynamic::__rt::inventory::submit! {
    StubClass {
        name: "Snapshot",
        doc: "The node graph as it stood when this transform was submitted.",
        fields: &[],
        constructible: false,
        variants: &[],
    }
}

dex_dynamic::__rt::inventory::submit! {
    StubMethod {
        owner: "Snapshot",
        name: "send_request",
        doc: "Send `request` to `dest` and return its response.",
        params: &[
            StubField { name: "dest", ty: "NodeUid" },
            StubField { name: "request", ty: "Any" },
        ],
        returns: "Any",
        is_static: false,
    }
}

dex_dynamic::__rt::inventory::submit! {
    StubMethod {
        owner: "Snapshot",
        name: "get_node",
        doc: "The node held at `uid` when this snapshot was taken, if any.",
        params: &[StubField { name: "uid", ty: "NodeUid" }],
        returns: "Option<Node>",
        is_static: false,
    }
}

dex_dynamic::__rt::inventory::submit! {
    StubMethod {
        owner: "Snapshot",
        name: "type_name",
        doc: "What `uid` calls itself, as the UI shows it.",
        params: &[StubField { name: "uid", ty: "NodeUid" }],
        returns: "Option<String>",
        is_static: false,
    }
}

dex_dynamic::__rt::inventory::submit! {
    StubMethod {
        owner: "Snapshot",
        name: "node_ids",
        doc: "Every id in the snapshot.",
        params: &[],
        returns: "Vec<NodeUid>",
        is_static: false,
    }
}

dex_dynamic::__rt::inventory::submit! {
    StubMethod {
        owner: "Snapshot",
        name: "root",
        doc: "The workspace root.",
        params: &[],
        returns: "NodeUid",
        is_static: false,
    }
}

dex_dynamic::__rt::inventory::submit! {
    StubMethod {
        owner: "Snapshot",
        name: "owner_of",
        doc: "The node that owns `uid`, if one does.",
        params: &[StubField { name: "uid", ty: "NodeUid" }],
        returns: "Option<NodeUid>",
        is_static: false,
    }
}
