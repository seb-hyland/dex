use std::{
    any::Any,
    borrow::Cow,
    collections::{HashMap, VecDeque},
    sync::{Arc, mpsc},
};

use dyn_clone::clone_box;
use egui::{Align, Id, Layout, Order, Rect, Ui, UiBuilder};
use serde::{Deserialize, Serialize};
use utils::match_dyn;

use crate::{
    ActionBody, ActionFor, AxisConstraint, DrawConstraints, DrawContext, DrawResult, Node,
    NodeContext, RequestFor, ScreenPos, ScreenRegion, Vector, WrapConstraints,
    compute::{ComputeScheduler, ComputeSchedulerHandle, ComputeTask},
    inspect::{InspectProbe, InspectTarget},
    messages::{Action, ActionGroup, Request, downcast_resp},
    pool::{NodeUid, Registry},
    pycontext::{PyDrawContext, PyWorkspace},
    refs::{NodeRefs, remapped},
};

pub struct Workspace {
    /// The top-level display node
    root_node: NodeUid,

    /// A historical registry for the workspace
    registry: Registry,

    /// A cheap, clonable `Send` handle over the action queue.
    actions_handle: WorkspaceActionHandle,
    /// A queue of unprocessed actions
    actions: mpsc::Receiver<Action>,

    /// A background scheduler for compute-intensive tasks
    scheduler: ComputeSchedulerHandle,

    /// This frame's record of what the pointer is over. Rebuilt from scratch every frame.
    probe: InspectProbe,
}

#[utils::dynamic_scoped(PyWorkspace)]
impl Workspace {
    pub fn new_with_root(root: impl Node) -> Self {
        let (registry, root_node) = Registry::new(root);
        let (action_tx, action_recv) = mpsc::channel();

        Self {
            root_node,
            registry,
            actions_handle: WorkspaceActionHandle {
                action_sender: action_tx.clone(),
            },
            actions: action_recv,
            scheduler: ComputeScheduler::spawn(action_tx),
            probe: InspectProbe::default(),
        }
    }

    /// A workspace with no nodes and no root yet. Populate it synchronously with
    /// [`Workspace::insert_node_now`] and finish with [`Workspace::set_root`].
    pub fn new_empty() -> Self {
        let (action_tx, action_recv) = mpsc::channel();

        Self {
            root_node: NodeUid::nil(),
            registry: Registry::empty(),
            actions_handle: WorkspaceActionHandle {
                action_sender: action_tx.clone(),
            },
            actions: action_recv,
            scheduler: ComputeScheduler::spawn(action_tx),
            probe: InspectProbe::default(),
        }
    }

    /// An owned clone of this workspace's action-queue handle.
    pub fn action_handle(&self) -> WorkspaceActionHandle {
        self.actions_handle.clone()
    }

    /// A workspace holding nodes and nothing else: no scheduler, and an action queue nobody drains.
    pub(crate) fn detached(root: NodeUid, nodes: Vec<(NodeUid, Arc<dyn Node>)>) -> Self {
        let (action_sender, actions) = mpsc::channel();

        let mut registry = Registry::empty();
        for (uid, node) in nodes {
            registry.push(PushWorkspaceNode { node, uid });
        }

        Self {
            root_node: root,
            registry,
            actions_handle: WorkspaceActionHandle { action_sender },
            actions,
            scheduler: ComputeSchedulerHandle::disconnected(),
            probe: InspectProbe::default(),
        }
    }

    /// The id of every node currently in the workspace.
    pub fn live_ids(&self) -> Vec<NodeUid> {
        self.registry.live_ids()
    }

    /// The node that owns `uid`, by the same relation a deep clone follows.
    /// A `#[uid_ref]` pointer is not ownership, so a wire is not an owner.
    pub fn owner_of(&self, uid: NodeUid) -> Option<NodeUid> {
        self.registry.live_ids().into_iter().find(|&candidate| {
            let mut owns = false;
            if let Some(node) = self.registry.get(candidate) {
                node.owned_refs(&mut |child| owns |= child == uid);
            }
            owns
        })
    }

    /// Every live node, paired with its id.
    pub(crate) fn live_nodes(&self) -> Vec<(NodeUid, Arc<dyn Node>)> {
        self.registry
            .live_ids()
            .into_iter()
            .filter_map(|uid| self.registry.get(uid).map(|node| (uid, node)))
            .collect()
    }

    /// Insert a node into the registry immediately, without going through the action queue.
    #[dynamic(skip)] // host lifecycle: not for scripts
    pub fn insert_node_now<T: Node>(&mut self, node: T) -> NodeUid<T> {
        let uid = NodeUid::mint();
        self.insert_node_now_at(uid, node);
        uid
    }

    /// Insert a node under a caller-chosen id (minted with [`NodeUid::mint`]).
    #[dynamic(skip)] // host lifecycle: not for scripts
    pub fn insert_node_now_at<T: Node>(&mut self, uid: NodeUid<T>, node: T) {
        self.registry.push(PushWorkspaceNode {
            node: Arc::new(node),
            uid: uid.erase(),
        });
    }

    #[dynamic(skip)] // host lifecycle: not for scripts
    pub fn set_root(&mut self, new_root: NodeUid) {
        self.root_node = new_root;
    }

    /// The workspace's root node.
    pub fn root(&self) -> NodeUid {
        self.root_node
    }

    #[dynamic(skip)] // generic over the message type
    pub fn submit_action<N, A>(
        &self,
        dest: NodeUid<N>,
        description: impl Into<Cow<'static, str>>,
        body: A,
    ) where
        N: Node + ?Sized,
        A: ActionFor<N> + 'static,
    {
        self.actions_handle.submit_action(dest, description, body);
    }

    #[dynamic(skip)] // generic over the message type
    pub fn send_request<N, R>(&self, dest: NodeUid<N>, body: R) -> Option<R::Response>
    where
        N: Node + ?Sized,
        R: RequestFor<N> + 'static,
    {
        self.send_request_dyn(Request {
            dest: dest.erase(),
            body: Box::new(body),
        })
        .map(downcast_resp)
    }

    /// Route an erased request to its destination, forwarding it down the
    /// dereference chain while it goes unanswered.
    #[dynamic(skip)] // erased plumbing; scripts use the message registry
    pub fn send_request_dyn(&self, mut q: Request) -> Option<Box<dyn Any>> {
        loop {
            // Get the request target
            let dest = q.dest;
            let dest_node = self.registry.get(dest)?;
            let ctx = NodeContext {
                id: dest,
                workspace: self,
            };
            match dest_node.request_dyn(q.body, ctx) {
                Ok(resp) => return Some(resp),
                Err(body) => {
                    // Not answered here; dereference to the child, if any, and try again
                    let target = dest_node.deref_target()?;
                    q = Request { dest: target, body };
                }
            }
        }
    }

    pub fn insert_node<T: Node>(&self, node: T) -> NodeUid<T> {
        self.actions_handle.insert_node(node)
    }

    /**
        Queue node insertion with a caller-chosen id.
        Functions similar to [`Workspace::insert_node`], but for a node whose id must be known.
    */
    pub fn insert_node_at<T: Node>(&self, uid: NodeUid<T>, node: T) {
        self.actions_handle.insert_node_at(uid, node);
    }

    /// Drain the pending action queue now. Used after building the initial node
    /// graph so it is live by the time this returns.
    #[dynamic(skip)] // host lifecycle: not for scripts
    pub fn process_pending(&mut self) {
        self.process_actions();
    }

    pub fn insert_node_dyn(&self, node: Arc<dyn Node>) -> NodeUid {
        self.actions_handle.insert_node_dyn(node)
    }

    /// Copy `source` and everything it owns, returning the copy's root id.
    pub fn deep_clone(&self, source: NodeUid) -> NodeUid {
        self.actions_handle.deep_clone(source)
    }

    #[dynamic(skip)] // host lifecycle: not for scripts
    pub fn draw_frame(&mut self, ui: &mut Ui, draw_area: Rect) {
        self.probe
            .begin_frame(ui.ctx().pointer_latest_pos().map(ScreenPos::from));
        self.tick_all();
        self.draw_root(ui, draw_area);
        self.process_actions();
    }

    /// Tick every live node, drawn or not. Part of a frame; exposed so a host
    /// (or a test) can drive one without drawing.
    #[dynamic(skip)] // host lifecycle: not for scripts
    pub fn tick_all(&self) {
        for id in self.registry.live_ids() {
            if let Some(node) = self.registry.get(id) {
                node.tick(NodeContext {
                    id,
                    workspace: self,
                });
            }
        }
    }

    fn draw_root(&mut self, ui: &mut Ui, draw_area: Rect) {
        let root_node = self.root_node;

        let constraints = DrawConstraints {
            pos: draw_area.min.into(),
            x: Some(AxisConstraint::Exactly(draw_area.width())),
            y: Some(AxisConstraint::Exactly(draw_area.height())),
            wrap: WrapConstraints::NotAllowed,
            should_clip: true,
        };

        // Using an `Area` allows egui to recognise the pointer as being over this area for scroll behaviour.
        egui::Area::new(Id::new("root_node_painter"))
            .order(Order::Middle)
            .fixed_pos(draw_area.min)
            .movable(false)
            .constrain(false)
            .show(ui.ctx(), |ui| {
                ui.set_clip_rect(draw_area);
                // Claim the whole draw area so the layer's hit-test rect covers it.
                ui.allocate_rect(draw_area, egui::Sense::hover());
                let mut ctx = DrawContext {
                    node: NodeContext {
                        id: root_node,
                        workspace: self,
                    },
                    constraints,
                    ui,
                    depth: 0,
                };
                ctx.draw_workspace_node(root_node, constraints);
            });
    }

    /// Delete a node from the workspace; its [`Node::on_delete`] handler will run.
    pub fn delete_node(&self, uid: NodeUid) {
        self.actions_handle.delete_node(uid);
    }

    fn remove_node(&mut self, uid: NodeUid) {
        // Run cleanup before removing the node.
        if let Some(node) = self.registry.get(uid) {
            node.on_delete(NodeContext {
                id: uid,
                workspace: self,
            });
        }
        self.registry.remove(uid);
    }

    #[dynamic(skip)] // erased plumbing; scripts use the message registry
    pub fn submit_action_dyn(&self, action: Action) {
        self.actions_handle.submit_action_dyn(action);
    }

    fn process_actions(&mut self) {
        while let Ok(act) = self.actions.try_recv() {
            // One epoch per queued action, so a group is a single undo step.
            self.registry.start_epoch(act.clone());
            self.dispatch(act);
        }
    }

    /// Carry out one action. Workspace-level bodies are handled here; anything
    /// else is routed to its destination node.
    fn dispatch(&mut self, act: Action) {
        match_dyn! { act.body,
            req_group: ActionGroup => {
                // Recurse rather than routing: a group may carry workspace-level
                // bodies, and those have no destination node to route to.
                for req in req_group.actions {
                    self.dispatch(req);
                }
            },
            push_action: PushWorkspaceNode => {
                self.registry.push(push_action);
            },
            remove_action: RemoveWorkspaceNode => {
                self.remove_node(remove_action.uid);
            },
            commit: CommitOutput => {
                // Point `target` at the node `source` currently holds.
                // `target`'s id remains stable.
                if let Some(node) = self.registry.get(commit.source) {
                    self.registry.push(PushWorkspaceNode {
                        node,
                        uid: commit.target,
                    });
                }
            },
            clone_action: CloneSubtree => {
                let seed = HashMap::from([(clone_action.source, clone_action.dest)]);
                self.clone_subtree(clone_action.source, seed);
            },
            clone_as: CloneSubtreeAs => {
                self.clone_subtree(clone_as.source, clone_as.ids.into_iter().collect());
            },
            _ => self.apply_action(act),
        }
    }

    /**
        Copy `source` and everything it owns into fresh ids, rooting the copy at
        the pre-minted `dest`.

        Ownership is the default for a uid field, with `#[uid_ref]` marking the
        exceptions, so the walk follows children and stops at references. Every uid the copies hold is rewritten; if a referential uid is found in the copied set (e.g., a self- or back-reference), it is replaced with the new reference.
    */
    fn clone_subtree(&mut self, source: NodeUid, mut ids: HashMap<NodeUid, NodeUid>) {
        // Whatever the caller did not name gets a fresh id.
        ids.entry(source).or_insert_with(NodeUid::mint);
        let mut order: Vec<NodeUid> = Vec::new();
        let mut queue: VecDeque<NodeUid> = VecDeque::from([source]);
        // A caller-supplied map already has entries, so `ids` no longer doubles
        // as the record of what has been walked.
        let mut visited: std::collections::HashSet<NodeUid> =
            std::collections::HashSet::from([source]);

        // Breadth-first over owned edges, minting an id for each node as it is discovered.
        while let Some(uid) = queue.pop_front() {
            let Some(node) = self.registry.get(uid) else {
                continue;
            };
            order.push(uid);
            node.owned_refs(&mut |child| {
                if child == NodeUid::nil() || visited.contains(&child) {
                    return;
                }
                visited.insert(child);
                ids.entry(child).or_insert_with(NodeUid::mint);
                queue.push_back(child);
            });
        }

        for uid in order {
            let Some(node) = self.registry.get(uid) else {
                continue;
            };
            let copy = remapped(&*node, &ids);
            // A copy starts with no cached state of its own.
            copy.reset();
            self.registry.push(PushWorkspaceNode {
                node: copy,
                uid: ids[&uid],
            });
        }
    }

    /// Route an action to its destination node, forwarding it down the
    /// dereference chain while it goes unhandled.
    fn apply_action(&mut self, mut action: Action) {
        loop {
            let dest = action.dest;
            let Some(node) = self.registry.get(dest) else {
                return;
            };
            let mut node_owned = clone_box(&*node);

            let ctx = NodeContext {
                id: dest,
                workspace: self,
            };
            let leftover = node_owned.handle_action(dyn_clone::clone_box(&*action.body), ctx);
            self.registry
                .commit_node(dest, action.clone(), Arc::from(node_owned));

            let Some(body) = leftover else {
                // The action was understood and handled
                return;
            };
            // Not understood here; dereference to the child, if any, and retry
            let Some(target) = self.registry.get(dest).and_then(|node| node.deref_target()) else {
                return;
            };
            action = Action {
                dest: target,
                description: action.description,
                body,
            };
        }
    }

    #[dynamic(skip)] // a compute task is not constructible from a script
    pub fn submit_task(&self, task: ComputeTask) {
        self.scheduler.submit_task(task);
    }

    /// The node currently held at `uid`, if any.
    pub fn get_node(&self, uid: NodeUid) -> Option<Arc<dyn Node>> {
        self.registry.get(uid)
    }

    /// This frame's addressable node under the pointer, if any.
    #[dynamic(skip)] // borrows a per-frame record
    pub fn inspect_target(&self) -> Option<InspectTarget> {
        self.probe.target()
    }

    /// The innermost addressable node drawn over `pos`.
    pub fn inspectable_at(&self, pos: ScreenPos) -> Option<NodeUid> {
        self.probe.at(pos)
    }

    /// Where an addressable `node` last drew, for anchoring to it on screen.
    pub fn inspectable_rect(&self, node: NodeUid) -> Option<ScreenRegion> {
        self.probe.region_of(node)
    }

    /// [`Node::version`] for the node at `uid`, or `0` if there is none.
    pub fn version_of(&self, uid: NodeUid) -> u64 {
        self.get_node(uid)
            .map(|node| {
                node.version(NodeContext {
                    id: uid,
                    workspace: self,
                })
            })
            .unwrap_or(0)
    }

    /// `uid`'s own version folded with every version beneath it.
    pub fn subtree_version(&self, uid: NodeUid) -> u64 {
        let mut hash: u64 = 0;
        let mut seen: std::collections::HashSet<NodeUid> = std::collections::HashSet::new();
        let mut queue = vec![uid];

        /// Spread a `u64` out over its whole range using SplitMix64's finalizer.
        fn scramble(mut z: u64) -> u64 {
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        while let Some(current) = queue.pop() {
            if !seen.insert(current) {
                continue;
            }

            // Combined by XOR and scrambled to avoid version collisions.
            hash ^= scramble(current.bits() ^ self.registry.version(current));
            if let Some(node) = self.registry.get(current) {
                node.owned_refs(&mut |child| {
                    if child != NodeUid::nil() {
                        queue.push(child);
                    }
                });
            }
        }
        hash
    }

    pub fn cancel_all_tasks_for(&self, uid: NodeUid) {
        self.scheduler.cancel_all_tasks_for(uid);
    }
}

/// A cheap, `Send` handle over a [`Workspace`]'s action queue.
// Wraps a live channel, so there is nothing meaningful to (de)serialize.
#[utils::dynamic_type(no_reduce)]
#[derive(Clone)]
pub struct WorkspaceActionHandle {
    action_sender: mpsc::Sender<Action>,
}

#[utils::dynamic_methods]
impl WorkspaceActionHandle {
    /**
       A standalone handle that can be used in place of one connected to a [`Workspace`].

       [`mpsc::Receiver::iter`] should be called on the returned [`mpsc::Receiver`] to drain [`actions`](Action).
    */
    pub fn buffered() -> (Self, mpsc::Receiver<Action>) {
        let (action_sender, rx) = mpsc::channel();
        (Self { action_sender }, rx)
    }

    pub fn submit_action<N, A>(
        &self,
        dest: NodeUid<N>,
        description: impl Into<Cow<'static, str>>,
        body: A,
    ) where
        N: Node + ?Sized,
        A: ActionFor<N> + 'static,
    {
        self.submit_action_dyn(Action {
            dest: dest.erase(),
            description: description.into(),
            body: Box::new(body),
        });
    }

    #[dynamic(skip)] // erased plumbing; scripts use the message registry
    pub fn submit_action_dyn(&self, action: Action) {
        self.action_sender
            .send(action)
            .expect("Actions should not fail to send!");
    }

    pub fn insert_node<T: Node>(&self, node: T) -> NodeUid<T> {
        self.insert_node_dyn(Arc::new(node)).cast()
    }

    pub fn insert_node_at<T: Node>(&self, uid: NodeUid<T>, node: T) {
        self.submit_action_dyn(Action {
            dest: NodeUid::nil(),
            description: "Inserted node".into(),
            body: Box::new(PushWorkspaceNode {
                node: Arc::new(node),
                uid: uid.erase(),
            }),
        });
    }

    pub fn insert_node_dyn(&self, node: Arc<dyn Node>) -> NodeUid {
        let uid = NodeUid::mint();

        self.submit_action_dyn(Action {
            dest: NodeUid::nil(),
            description: "Inserted node".into(),
            body: Box::new(PushWorkspaceNode {
                node,
                uid: uid.erase(),
            }),
        });

        uid
    }

    pub fn delete_node(&self, uid: NodeUid) {
        self.submit_action_dyn(Action {
            dest: NodeUid::nil(),
            description: "Deleted node".into(),
            body: Box::new(RemoveWorkspaceNode { uid }),
        });
    }

    /// Commit `node` as the content of an existing `uid`, keeping the id stable.
    pub fn insert_node_at_dyn(&self, uid: NodeUid, node: Arc<dyn Node>) {
        self.submit_action_dyn(Action {
            dest: NodeUid::nil(),
            description: "Committed node output".into(),
            body: Box::new(PushWorkspaceNode { node, uid }),
        });
    }

    /// Copy `source` and everything it owns, returning the copy's root id.
    pub fn deep_clone(&self, source: NodeUid) -> NodeUid {
        let dest = NodeUid::mint();
        self.submit_action_dyn(Action {
            dest: NodeUid::nil(),
            description: "Cloned node".into(),
            body: Box::new(CloneSubtree { source, dest }),
        });
        dest
    }

    /// Copy `source` and everything it owns into the ids named by `ids`.
    pub fn deep_clone_as(&self, source: NodeUid, ids: Vec<(NodeUid, NodeUid)>) {
        self.submit_action_dyn(Action {
            dest: NodeUid::nil(),
            description: "Cloned node".into(),
            body: Box::new(CloneSubtreeAs { source, ids }),
        });
    }

    /// Point `target` at the node `source` currently holds, keeping `target`'s
    /// id stable. Resolved workspace-side, so `source` must already be queued.
    pub fn commit_output(&self, target: NodeUid, source: NodeUid) {
        self.submit_action_dyn(Action {
            dest: NodeUid::nil(),
            description: "Committed node output".into(),
            body: Box::new(CommitOutput { target, source }),
        });
    }
}

/// The parts of the handle that cannot be bound automatically.
#[pyo3::pymethods]
impl WorkspaceActionHandle {
    /// Queue `action` against `dest`.
    #[pyo3(name = "submit_action", signature = (dest, action, description=None))]
    fn submit_action_py(
        &self,
        dest: crate::NodeHandle,
        action: pyo3::Bound<'_, pyo3::PyAny>,
        description: Option<String>,
    ) -> pyo3::PyResult<()> {
        self.submit_action_dyn(build_action(dest, &action, description)?);
        Ok(())
    }

    /// Queue `actions` — `(dest, action)` pairs — as a single step.
    #[pyo3(signature = (actions, description=None))]
    fn batch(
        &self,
        actions: Vec<(crate::NodeHandle, pyo3::Py<pyo3::PyAny>)>,
        description: Option<String>,
    ) -> pyo3::PyResult<()> {
        let group = pyo3::Python::attach(|py| {
            actions
                .into_iter()
                .map(|(dest, action)| build_action(dest, action.bind(py), None))
                .collect::<pyo3::PyResult<Vec<Action>>>()
        })?;

        self.submit_action_dyn(Action {
            dest: NodeUid::nil(),
            description: description
                .map(Cow::Owned)
                .unwrap_or(Cow::Borrowed("Batch")),
            body: Box::new(ActionGroup { actions: group }),
        });
        Ok(())
    }
}

/// Resolve a Python message into an addressed [`Action`].
fn build_action(
    dest: crate::NodeHandle,
    action: &pyo3::Bound<'_, pyo3::PyAny>,
    description: Option<String>,
) -> pyo3::PyResult<Action> {
    let entry = crate::messages::action_for(action)
        .ok_or_else(|| crate::pycontext::not_a_message(action, false))?;
    Ok(Action {
        dest: dest.0,
        description: description
            .map(Cow::Owned)
            .unwrap_or(Cow::Borrowed(entry.name)),
        body: (entry.build)(action)?,
    })
}

dex_dynamic::__rt::inventory::submit! {
    crate::stubs::StubMethod {
        owner: "WorkspaceActionHandle",
        name: "submit_action",
        doc: "Queue `action` against `dest`.",
        params: &[
            crate::stubs::StubField { name: "dest", ty: "NodeUid" },
            crate::stubs::StubField { name: "action", ty: "Any" },
            crate::stubs::StubField { name: "description", ty: "Option<String>" },
        ],
        returns: "",
        is_static: false,
    }
}

dex_dynamic::__rt::inventory::submit! {
    crate::stubs::StubMethod {
        owner: "WorkspaceActionHandle",
        name: "batch",
        doc: "Queue `actions` \u{2014} `(dest, action)` pairs \u{2014} as a single undo step.",
        params: &[
            crate::stubs::StubField { name: "actions", ty: "Vec<(NodeUid, Any)>" },
            crate::stubs::StubField { name: "description", ty: "Option<String>" },
        ],
        returns: "",
        is_static: false,
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PushWorkspaceNode {
    pub node: Arc<dyn Node>,
    pub uid: NodeUid,
}

#[typetag::serde]
impl ActionBody for PushWorkspaceNode {}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct RemoveWorkspaceNode {
    pub uid: NodeUid,
}

#[typetag::serde]
impl ActionBody for RemoveWorkspaceNode {}

/// Copy a subtree into fresh ids, rooted at a caller-minted `dest`.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CloneSubtree {
    pub source: NodeUid,
    pub dest: NodeUid,
}

#[typetag::serde]
impl ActionBody for CloneSubtree {}

/// Copy a subtree into ids the caller chose.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CloneSubtreeAs {
    pub source: NodeUid,
    /// Old id to new. Anything unnamed is copied under a fresh id.
    pub ids: Vec<(NodeUid, NodeUid)>,
}

#[typetag::serde]
impl ActionBody for CloneSubtreeAs {}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CommitOutput {
    pub target: NodeUid,
    pub source: NodeUid,
}

#[typetag::serde]
impl ActionBody for CommitOutput {}

#[utils::dynamic_scoped(PyDrawContext)]
impl<'ctx> DrawContext<'ctx> {
    pub fn for_ui(node: NodeContext<'ctx>, constraints: DrawConstraints, ui: &'ctx mut Ui) -> Self {
        DrawContext {
            node,
            constraints,
            ui,
            depth: 0,
        }
    }

    pub fn get_workspace_node(&self, id: NodeUid) -> Option<Arc<dyn Node>> {
        self.node.workspace.registry.get(id)
    }

    fn draw_node_with(
        &mut self,
        node: &dyn Node,
        id: NodeUid,
        constraints: DrawConstraints,
    ) -> DrawResult {
        let workspace = self.node.workspace;

        let clip_x = constraints
            .x
            .map(|x_ax| x_ax.provided_value())
            .unwrap_or(f32::INFINITY);
        let clip_y = constraints
            .y
            .map(|y_ax| y_ax.provided_value())
            .unwrap_or(f32::INFINITY);
        let clip_size = Vector {
            x: clip_x,
            y: clip_y,
        };
        let clip_region = ScreenRegion::from_min_size(constraints.pos, clip_size);

        if constraints.should_clip {
            // Draw within a new child UI that is clipped

            let mut child_ui = self.ui.new_child(UiBuilder::new());
            // Intersect rather than replace: a child's clip narrows what its ancestors already allowed.
            let clip = Rect::from(clip_region).intersect(self.ui.clip_rect());
            child_ui.set_clip_rect(clip);

            let temp_ctx = DrawContext {
                node: NodeContext { id, workspace },
                ui: &mut child_ui,
                constraints,
                depth: self.depth + 1,
            };

            #[allow(deprecated)] // Private call
            node.draw(temp_ctx)
        } else {
            let temp_ctx = DrawContext {
                node: NodeContext {
                    id: id.erase(),
                    workspace,
                },
                constraints,
                depth: self.depth + 1,
                ..self.reborrow()
            };

            #[allow(deprecated)] // Private call
            node.draw(temp_ctx)
        }
    }

    pub fn draw_node(&mut self, node: &dyn Node, constraints: DrawConstraints) -> DrawResult {
        self.draw_node_with(node, NodeUid::nil(), constraints)
    }

    /// Draw `node` under this node's own id, so anything it addresses to itself comes back here.
    pub fn draw_child_as_self(
        &mut self,
        node: &dyn Node,
        constraints: DrawConstraints,
    ) -> DrawResult {
        let id = self.node.id;
        self.draw_node_with(node, id, constraints)
    }

    /// Draw a workspace node and offer it to the inspector as an addressable element.
    pub fn draw_inspectable_node(
        &mut self,
        id: NodeUid,
        constraints: DrawConstraints,
    ) -> Option<DrawResult> {
        // Copied out so the probe is reachable across the `&mut self` draw.
        let workspace = self.node.workspace;
        let depth = self.depth;

        let result = self.draw_workspace_node(id, constraints);

        // A node that declines an inspector is not addressable.
        if !workspace
            .send_request(id, crate::inspect::Inspectable)
            .unwrap_or(true)
        {
            return result;
        }

        let clip = ScreenRegion::from(self.ui.clip_rect());
        // A node that draws nothing is still addressable over the space it was given.
        let region = result
            .as_ref()
            .and_then(|r| r.region())
            .or_else(|| allotted(&constraints));
        let visible = region.and_then(|region| region.intersect(clip));
        workspace.probe.record(id, depth, region, visible);
        result
    }

    /// Host raw egui widgets in a `Ui` bounded and clipped to `region`, returning the region they actually occupied.
    pub fn host_widgets(
        &mut self,
        region: ScreenRegion,
        add: impl FnOnce(&mut Ui),
    ) -> ScreenRegion {
        let rect: Rect = region.into();
        self.ui
            .scope_builder(
                UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::top_down(Align::Min)),
                |ui| {
                    // Intersect with the inherited clip, so an unbounded `region` paints only within the real viewport.
                    let clip = rect.intersect(ui.clip_rect());
                    ui.set_clip_rect(clip);
                    add(ui);
                    ScreenRegion::from(ui.min_rect())
                },
            )
            .inner
    }

    pub fn draw_workspace_node(
        &mut self,
        id: NodeUid,
        constraints: DrawConstraints,
    ) -> Option<DrawResult> {
        let node = self.node.workspace.registry.get(id)?;
        Some(self.draw_node_with(&*node, id, constraints))
    }
}

/// The box `constraints` hand out, when they bound both axes.
fn allotted(constraints: &DrawConstraints) -> Option<ScreenRegion> {
    let width = constraints.x?.provided_value();
    let height = constraints.y?.provided_value();
    (width.is_finite() && height.is_finite()).then(|| {
        ScreenRegion::from_min_size(
            constraints.pos,
            Vector {
                x: width,
                y: height,
            },
        )
    })
}
