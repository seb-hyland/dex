use std::any::Any;

use egui::{Rect, Ui};
use serde::{Deserialize, Serialize};
use utils::match_dyn;

use crate::{
    DrawContext,
    messages::{
        action::{Action, ActionGroup},
        request::{Region, Request, TypedRequest, downcast_resp},
    },
    pool::{NodeUid, Registry},
    region::DrawRegion,
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

        let mut ctx = DrawContext {
            id: root_node,
            ui,
            pos: area.min,
            width: Some(area.width()),
            height: Some(area.height()),
            workspace: self,
        };
        let draw_res = ctx.draw_node(root_node);

        assert!(draw_res.is_some(), "Root node should exist")
    }

    pub fn message<Resp: Any>(&self, q: TypedRequest<Resp>) -> Option<Resp> {
        self.message_dyn(q.into()).map(downcast_resp)
    }

    pub fn message_dyn(&self, q: Request) -> Option<Box<dyn Any>> {
        let (dest_node, last_draw_region) = self.registry.get(q.dest)?;
        if q.body.as_any_ref().is::<Region>() {
            // Draw region is being requested
            Some(Box::new(last_draw_region) as Box<dyn Any>)
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
    pub fn draw_node(&mut self, id: NodeUid) -> Option<DrawRegion> {
        let maybe_node = self.workspace.registry.get(id);
        let (node, _) = maybe_node?;

        // Cheap clone of active node for display purposes
        let node_clone = dyn_clone::clone_box(node);
        self.id = id;
        let maybe_region = node_clone.draw(self);

        if let Some(region) = &maybe_region {
            self.workspace
                .registry
                .update_node_region(id, region.clone());
        }

        maybe_region
    }
}
