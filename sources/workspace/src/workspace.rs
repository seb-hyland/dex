use std::any::Any;

use egui::{Rect, Ui};
use serde::{Deserialize, Serialize};
use utils::match_dyn;

use crate::{
    AxisConstraint, DrawConstraints, DrawContext, DrawResult, PositionConstraint,
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
    requests: Vec<Action>,

    /// A historical registry for the workspace
    registry: Registry,
}

impl Workspace {
    pub fn draw_root(&mut self, ui: &mut Ui, area: Rect) {
        let root_node = self.root_node;

        let constraints = DrawConstraints {
            pos: PositionConstraint::TopLeft(area.min.into()),
            x: Some(AxisConstraint::Exactly(area.width())),
            y: Some(AxisConstraint::Exactly(area.height())),
            can_request_wrap: false,
            continuation: None,
        };
        let mut ctx = DrawContext {
            id: root_node,
            workspace: self,
            constraints,
            ui,
        };

        let draw_res = ctx.draw_node(root_node, constraints);

        assert!(draw_res.is_some(), "Root node should exist")
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

    pub fn process_requests(&mut self) {
        let requests: Vec<_> = self.requests.drain(..).collect();
        for req in requests {
            if req.body.is_history_defining() {
                self.registry.start_epoch(req.clone());
            }

            match_dyn! { req.body,
                req_group: ActionGroup => {
                    for req in req_group.actions {
                        self.registry.apply_request(req);
                    }
                },
                _ => self.registry.apply_request(req)
            }
        }
    }
}

impl<'ctx> DrawContext<'ctx> {
    pub fn draw_node(&mut self, id: NodeUid, constraints: DrawConstraints) -> Option<DrawResult> {
        let (node, _) = self.workspace.registry.get(id)?;
        // Cheap clone of node for display, releasing the borrow on the registry
        let node_clone = dyn_clone::clone_box(node);

        let mut temp_ctx = DrawContext {
            id,
            constraints,
            ..self.reborrow()
        };

        let res = node_clone.draw(&mut temp_ctx);

        let maybe_region = res.region();
        self.workspace.registry.update_node_region(id, maybe_region);

        Some(res)
    }
}
