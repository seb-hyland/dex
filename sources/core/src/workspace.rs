use std::{any::Any, borrow::Cow, sync::mpsc};

use egui::{Rect, Ui, UiBuilder};
use serde::{Deserialize, Serialize};
use utils::match_dyn;

use crate::{
    ActionBody, ActionFor, AxisConstraint, DrawConstraints, DrawContext, DrawResult, Node,
    NodeContext, RequestFor, ScreenRegion, Vector, WrapConstraints,
    compute::{ComputeScheduler, ComputeSchedulerHandle, ComputeTask},
    messages::{Action, ActionGroup, Request, downcast_resp},
    pool::{NodeUid, Registry},
};

pub struct Workspace {
    /// The top-level display node
    root_node: NodeUid,

    /// A historical registry for the workspace
    registry: Registry,

    /// A sender for actions
    action_sender: mpsc::Sender<Action>,
    /// A queue of unprocessed actions
    actions: mpsc::Receiver<Action>,

    /// A background scheduler for compute-intensive tasks
    scheduler: ComputeSchedulerHandle,
}

impl Workspace {
    pub fn new_with_root(root: Box<dyn Node>) -> Self {
        let (registry, root_node) = Registry::new(root);
        let (action_tx, action_recv) = mpsc::channel();

        Self {
            root_node,
            registry,
            action_sender: action_tx.clone(),
            actions: action_recv,
            scheduler: ComputeScheduler::spawn(action_tx),
        }
    }

    /// A workspace with no nodes and no root yet. Populate it synchronously with
    /// [`Workspace::insert_node_now`] and finish with [`Workspace::set_root`].
    /// Intended for building the initial node graph before the frame loop.
    pub fn new_empty() -> Self {
        let (action_tx, action_recv) = mpsc::channel();

        Self {
            root_node: NodeUid::nil(),
            registry: Registry::empty(),
            action_sender: action_tx.clone(),
            actions: action_recv,
            scheduler: ComputeScheduler::spawn(action_tx),
        }
    }

    /// Insert a node into the registry immediately, without going through the action queue.
    pub fn insert_node_now<T: Node>(&mut self, node: Box<T>) -> NodeUid<T> {
        let uid = NodeUid::new_workspace();
        self.insert_node_now_at(uid, node);
        uid
    }

    /// Insert a node under a caller-chosen id (minted with [`NodeUid::new_workspace`]).
    pub fn insert_node_now_at<T: Node>(&mut self, uid: NodeUid<T>, node: Box<T>) {
        self.registry.push(PushWorkspaceNode {
            node,
            uid: uid.erase(),
        });
    }

    pub fn set_root(&mut self, new_root: NodeUid) {
        self.root_node = new_root;
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

    fn send_request_dyn(&self, mut q: Request) -> Option<Box<dyn Any>> {
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

    pub fn insert_node<T: Node>(&self, node: Box<T>) -> NodeUid<T> {
        self.insert_node_dyn(node).cast()
    }

    /**
        Queue node insertion with a caller-chosen id.
        Functions similar to [`Workspace::insert_node`], but for a node whose id must be known.
    */
    pub fn insert_node_at<T: Node>(&self, uid: NodeUid<T>, node: Box<T>) {
        self.submit_action_dyn(Action {
            dest: NodeUid::nil(),
            description: format!("Inserted node of type {}", node.type_name()).into(),
            body: Box::new(PushWorkspaceNode {
                node,
                uid: uid.erase(),
            }),
        });
    }

    /// Drain the pending action queue now. Used after building the initial node
    /// graph so it is live by the time this returns.
    pub fn process_pending(&mut self) {
        self.process_actions();
    }

    pub fn insert_node_dyn(&self, node: Box<dyn Node>) -> NodeUid {
        let uid = NodeUid::new_workspace();

        self.submit_action_dyn(Action {
            dest: NodeUid::nil(),
            description: format!("Inserted node of type {}", node.type_name()).into(),
            body: Box::new(PushWorkspaceNode {
                node,
                uid: uid.erase(),
            }),
        });

        uid
    }

    pub fn draw_frame(&mut self, ui: &mut Ui, draw_area: Rect) {
        self.draw_root(ui, draw_area);
        self.process_actions();
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
        let mut ctx = DrawContext {
            node: NodeContext {
                id: root_node,
                workspace: self,
            },
            constraints,
            ui,
        };
        ctx.draw_workspace_node(root_node, constraints);
    }

    /// Delete a node from the workspace; its [`Node::on_delete`] handler will run.
    pub fn delete_node(&self, uid: NodeUid) {
        self.submit_action_dyn(Action {
            dest: NodeUid::nil(),
            description: "Deleted node".into(),
            body: Box::new(RemoveWorkspaceNode { uid }),
        });
    }

    fn remove_node(&mut self, uid: NodeUid) {
        // Run cleanup before removing the node.
        if let Some(node) = self.registry.clone_node(uid) {
            node.on_delete(NodeContext {
                id: uid,
                workspace: self,
            });
        }
        self.registry.remove(uid);
    }

    pub fn submit_action_dyn(&self, action: Action) {
        self.action_sender
            .send(action)
            .expect("Actions should not fail to send!");
    }

    fn process_actions(&mut self) {
        while let Ok(act) = self.actions.try_recv() {
            self.registry.start_epoch(act.clone());

            match_dyn! { act.body,
                req_group: ActionGroup => {
                    for req in req_group.actions {
                        self.apply_action(req);
                    }
                },
                push_action: PushWorkspaceNode => {
                    self.registry.push(push_action);
                },
                remove_action: RemoveWorkspaceNode => {
                    self.remove_node(remove_action.uid);
                },
                _ => self.apply_action(act),
            }
        }
    }

    /// Route an action to its destination node, forwarding it down the
    /// dereference chain while it goes unhandled.
    fn apply_action(&mut self, mut action: Action) {
        loop {
            let dest = action.dest;
            let Some(mut node) = self.registry.clone_node(dest) else {
                return;
            };

            let ctx = NodeContext {
                id: dest,
                workspace: self,
            };
            let leftover = node.handle_action(dyn_clone::clone_box(&*action.body), ctx);
            self.registry.commit_node(dest, action.clone(), node);

            let Some(body) = leftover else {
                // The action was understood and handled
                return;
            };
            // Not understood here; dereference to the child, if any, and retry
            let Some(n) = self.registry.get(dest) else {
                return;
            };
            let Some(target) = n.deref_target() else {
                return;
            };
            action = Action {
                dest: target,
                description: action.description,
                body,
            };
        }
    }

    pub fn submit_task(&self, task: ComputeTask) {
        self.scheduler.submit_task(task);
    }

    pub fn cancel_all_tasks_for(&self, uid: NodeUid) {
        self.scheduler.cancel_all_tasks_for(uid);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PushWorkspaceNode {
    pub node: Box<dyn Node>,
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

impl<'ctx> DrawContext<'ctx> {
    pub fn draw_workspace_node<T: ?Sized>(
        &mut self,
        id: NodeUid<T>,
        constraints: DrawConstraints,
    ) -> Option<DrawResult> {
        let workspace = self.node.workspace;
        let node = workspace.registry.get(id.erase())?;

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

        let res = if constraints.should_clip {
            // Draw within a new child UI that is clipped

            let mut child_ui = self.ui.new_child(UiBuilder::new());
            child_ui.set_clip_rect(clip_region.into());

            let temp_ctx = DrawContext {
                node: NodeContext {
                    id: id.erase(),
                    workspace,
                },
                ui: &mut child_ui,
                constraints,
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
                ..self.reborrow()
            };

            #[allow(deprecated)] // Private call
            node.draw(temp_ctx)
        };

        Some(res)
    }
}
