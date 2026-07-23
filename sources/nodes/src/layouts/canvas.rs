use egui::{Pos2, Vec2};
use serde::{Deserialize, Serialize};
use utils::Transient;
use workspace::{messages::request::TypedRequestable, prelude::*};

use crate::primitives::interaction::{InteractionBox, WasDragged};

#[derive(Clone, Serialize, Deserialize)]
pub struct CanvasLayout {
    children: Vec<CanvasNode>,
    drag_interaction: Option<InteractionBox>,
    screen_offset: Transient<Vector>,
}

#[derive(Clone, Serialize, Deserialize)]
struct CanvasNode {
    canvas_pos: Pos2,
    id: NodeUid,
}

impl CanvasLayout {
    fn screen_offset(&self) -> Vector {
        self.screen_offset
            .val()
            .map(|d| *d)
            .unwrap_or(Vector::splat(0.0))
    }

    fn canvas_to_screen(&self, canvas_pos: Pos2) -> Pos2 {
        canvas_pos - Vec2::from(self.screen_offset())
    }

    fn draw_child(
        &self,
        child: &CanvasNode,
        origin: ScreenPos,
        ctx: &mut DrawContext,
    ) -> Option<DrawResult> {
        let child_pos = self.canvas_to_screen(child.canvas_pos);
        let child_screen_pos = origin + ScreenPos::from(child_pos);

        let constraints = DrawConstraints {
            pos: PositionConstraint::TopLeft(child_screen_pos),
            x: None,
            y: None,
            can_request_wrap: false,
            continuation: None,
        };
        ctx.draw_node(child.id, constraints)
    }
}

#[typetag::serde]
impl Node for CanvasLayout {
    fn draw(&self, ctx: &mut DrawContext) -> DrawResult {
        let avail_x = ctx
            .constraints
            .x
            .map(|x_ax| x_ax.provided_value())
            .unwrap_or(f32::INFINITY);
        let avail_y = ctx
            .constraints
            .y
            .map(|y_ax| y_ax.provided_value())
            .unwrap_or(f32::INFINITY);

        let size = Vector {
            x: avail_x,
            y: avail_y,
        };
        let origin = ctx.constraints.pos.to_top_left(size);

        if let Some(interact) = &self.drag_interaction {
            interact.draw(ctx);

            let drag_res = interact
                .request_typed(Box::new(WasDragged))
                .expect("Message should be understood");

            if let Some(drag_delta) = drag_res {
                // Update the offset
                self.screen_offset.set(self.screen_offset() + drag_delta);
            }
        }

        for child in &self.children {
            self.draw_child(child, origin, ctx);
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, size)),
        }
    }

    fn handle_action(&mut self, r: Box<dyn ActionBody>) {}
}

impl Requestable for CanvasLayout {
    fn request(&self, body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}
