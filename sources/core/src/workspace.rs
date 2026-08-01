use std::any::Any;

use egui::{Rect, Ui, UiBuilder};
use serde::{Deserialize, Serialize};
use utils::{Timestamp, match_dyn};

use crate::{
    ActionBody, AxisConstraint, DrawConstraints, DrawContext, DrawResult, Id, LocalId, Node,
    PositionConstraint, ScreenRegion, Vector, WrapConstraints,
    messages::{
        action::{Action, ActionGroup},
        request::{Region, Request, TypedRequest, TypedRequestBody, downcast_resp},
    },
    pool::{NodeUid, Registry},
};

#[derive(Serialize, Deserialize)]
pub struct Workspace {
    /// The top-level display node
    root_node: NodeUid,

    /// A queue of unprocessed requests
    actions: Vec<Action>,

    /// A historical registry for the workspace
    registry: Registry,
}

impl Workspace {
    pub fn new_with_root(root: Box<dyn Node>) -> Self {
        let (registry, root_node) = Registry::new(root);
        let requests = Vec::new();

        Self {
            root_node,
            actions: requests,
            registry,
        }
    }

    pub fn set_root(&mut self, new_root: NodeUid) {
        self.root_node = new_root;
    }

    pub fn send_request<Resp: Any>(&self, q: TypedRequest<Resp>) -> Option<Resp> {
        self.send_request_dyn(q.into()).map(downcast_resp)
    }

    fn send_request_dyn(&self, q: Request) -> Option<Box<dyn Any>> {
        let (dest_node, last_draw_region) = self.registry.get(q.dest)?;
        if q.body.as_any_ref().is::<Region>() {
            // Draw region is being requested
            Some(
                Box::new(last_draw_region) as Box<<Region as TypedRequestBody>::Response>
                    as Box<dyn Any>,
            )
        } else {
            dest_node.request(q.body)
        }
    }

    pub fn insert_node(&mut self, node: Box<dyn Node>) -> NodeUid {
        let uid = NodeUid::new();
        self.actions.push(Action {
            dest: None,
            brand: Timestamp::now(),
            description: format!("Inserted node of type {}", node.type_name()).into(),
            body: Box::new(PushWorkspaceNode { node, uid }),
        });
        uid
    }

    pub fn submit_action(&mut self, action: Action) {
        self.actions.push(action);
    }

    pub fn draw_frame(&mut self, ui: &mut Ui, draw_area: Rect) {
        self.draw_root(ui, draw_area);
        self.process_actions();
        self.registry.tick_frame();
    }

    fn draw_root(&mut self, ui: &mut Ui, draw_area: Rect) {
        // Do not allow drawing outside the area
        ui.set_clip_rect(draw_area);

        let root_node = self.root_node;

        let constraints = DrawConstraints {
            pos: PositionConstraint::TopLeft(draw_area.min.into()),
            x: Some(AxisConstraint::Exactly(draw_area.width())),
            y: Some(AxisConstraint::Exactly(draw_area.height())),
            wrap: WrapConstraints::NotAllowed,
            should_clip: true,
        };
        let mut ctx = DrawContext {
            id: Id::Workspace(root_node),
            workspace: self,
            constraints,
            ui,
        };

        ctx.draw_workspace_node(root_node, constraints);
    }

    fn process_actions(&mut self) {
        let actions: Vec<_> = self.actions.drain(..).collect();
        for act in actions {
            self.registry.start_epoch(act.clone());

            match_dyn! { act.body,
                req_group: ActionGroup => {
                    for req in req_group.actions {
                        self.registry.apply_action(req);
                    }
                },
                push_action: PushWorkspaceNode => {
                    self.registry.push(push_action);
                },
                _ => self.registry.apply_action(act)
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PushWorkspaceNode {
    pub node: Box<dyn Node>,
    pub uid: NodeUid,
}

#[typetag::serde]
impl ActionBody for PushWorkspaceNode {}

impl<'ctx> DrawContext<'ctx> {
    pub fn draw_workspace_node(
        &mut self,
        id: NodeUid,
        constraints: DrawConstraints,
    ) -> Option<DrawResult> {
        let (node, _) = self.workspace.registry.get(id)?;

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
        let clip_region = match constraints.pos {
            PositionConstraint::Center(c) => ScreenRegion::from_center_size(c, clip_size),
            PositionConstraint::TopLeft(tl) => ScreenRegion::from_min_size(tl, clip_size),
        };

        let res = if constraints.should_clip {
            // Draw within a new child UI that is clipped

            let mut child_ui = self.ui.new_child(UiBuilder::new());
            child_ui.set_clip_rect(clip_region.into());

            let temp_ctx = DrawContext {
                id: Id::Workspace(id),
                ui: &mut child_ui,
                constraints,
                ..self.reborrow()
            };

            #[allow(deprecated)] // Private call
            node.draw(temp_ctx)
        } else {
            let temp_ctx = DrawContext {
                id: Id::Workspace(id),
                constraints,
                ..self.reborrow()
            };

            #[allow(deprecated)] // Private call
            node.draw(temp_ctx)
        };

        let maybe_region = res.region().and_then(|reg| {
            if constraints.should_clip {
                // Clip the output region to match the actual drawn area
                reg.intersect(clip_region)
            } else {
                Some(reg)
            }
        });
        self.workspace.registry.update_node_region(id, maybe_region);

        Some(res)
    }

    pub fn draw_node(
        &mut self,
        node: &impl Node,
        id: LocalId,
        constraints: DrawConstraints,
    ) -> DrawResult {
        let temp_ctx = DrawContext {
            id: Id::Local(id),
            constraints,
            ..self.reborrow()
        };

        #[allow(deprecated)] // Private call
        node.draw(temp_ctx)
    }
}
