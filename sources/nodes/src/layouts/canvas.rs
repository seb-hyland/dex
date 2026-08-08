use dex_core::prelude::*;
use egui::{Pos2, Vec2};
use serde::{Deserialize, Serialize};
use utils::{Reset, Transient};

use crate::primitives::interaction::{InteractionBox, WasDragged};

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct CanvasLayout {
    children: Vec<CanvasNode>,
    drag_interaction: Option<InteractionBox>,
    screen_offset: Transient<Vector>,
}

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct CanvasNode {
    pub canvas_pos: Pos2,
    pub id: NodeUid,
}

impl Default for CanvasLayout {
    fn default() -> Self {
        Self {
            children: Vec::new(),
            drag_interaction: Some({
                let mut interact = InteractionBox::default();
                interact.senses_drags = true;
                interact
            }),
            screen_offset: Transient::default(),
        }
    }
}

impl CanvasLayout {
    pub fn push_child(&mut self, child: CanvasNode) {
        self.children.push(child);
    }

    fn screen_offset(&self) -> Vector {
        self.screen_offset.val().unwrap_or(Vector::splat(0.0))
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
            wrap: WrapConstraints::NotAllowed,
            should_clip: false,
        };
        ctx.draw_workspace_node(child.id, constraints)
    }
}

#[typetag::serde]
impl Node for CanvasLayout {
    fn type_name(&self) -> String {
        "Canvas".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
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
            ctx.draw_node(
                interact,
                NodeUid::new_local(ctx.id, "background drag"),
                DrawConstraints {
                    pos: PositionConstraint::TopLeft(origin),
                    x: Some(AxisConstraint::Exactly(avail_x)),
                    y: Some(AxisConstraint::Exactly(avail_y)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );

            let drag_res = interact
                .request(WasDragged)
                .expect("Message should be understood");

            if let Some(drag_delta) = drag_res {
                // Update the offset
                self.screen_offset.set(self.screen_offset() - drag_delta);
            }
        }

        for child in &self.children {
            self.draw_child(child, origin, &mut ctx);
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, size)),
        }
    }

}

defhandlers! { CanvasLayout {
    extern_actions: [
        MoveChild => (this, m) {
            if let Some(child) = this.children.iter_mut().find(|c| c.id == m.child) {
                child.canvas_pos += Vec2::from(m.delta);
            }
        },
    ],
}}

/**
   Move a canvas child by [`delta`](Self::delta) in canvas-space.
*/
#[derive(Clone, Serialize, Deserialize)]
pub struct MoveChild {
    pub child: NodeUid,
    pub delta: Vector,
}

#[typetag::serde]
impl ActionBody for MoveChild {}
