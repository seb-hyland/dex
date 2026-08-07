use dex_core::prelude::*;
use serde::{Deserialize, Serialize};
use utils::{Reset, Transient};

use crate::{
    layouts::canvas::{nodes::CanvasNode, sidebar::CanvasSidebar},
    primitives::interaction::{InteractionBox, WasDragged},
};

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct CanvasLayout {
    children: Vec<NodeUid<CanvasNode>>,
    drag_interaction: Option<InteractionBox>,
    screen_offset: Transient<Vector>,
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
    pub fn push_child(&mut self, child: NodeUid<CanvasNode>) {
        self.children.push(child);
    }

    fn screen_offset(&self) -> Vector {
        self.screen_offset.val().unwrap_or(Vector::splat(0.0))
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
        let region = ScreenRegion::from_min_size(origin, size);

        // Paint the canvas background.
        ctx.ui
            .painter()
            .rect_filled(region.into(), 0.0, egui::Color32::WHITE);

        if let Some(interact) = &self.drag_interaction {
            ctx.draw_node(
                interact,
                NodeUid::new_local(ctx.node.id, "background drag"),
                DrawConstraints {
                    pos: PositionConstraint::TopLeft(origin),
                    x: Some(AxisConstraint::Exactly(avail_x)),
                    y: Some(AxisConstraint::Exactly(avail_y)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
            );

            let drag_res = interact
                .request(WasDragged, ctx.node)
                .expect("Message should be understood");
            if let Some(drag_delta) = drag_res {
                // Update the offset
                self.screen_offset.set(self.screen_offset() - drag_delta);
            }
        }

        let canvas_origin = origin - self.screen_offset();
        for &child in &self.children {
            ctx.draw_workspace_node(
                child,
                DrawConstraints {
                    // `CanvasNode` children will draw relative to the origin
                    pos: PositionConstraint::TopLeft(canvas_origin),
                    x: None,
                    y: None,
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                },
            );
        }

        DrawResult::Complete {
            region: Some(region),
        }
    }

    fn draw_sidebar(&self, ctx: DrawContext) -> Option<Box<dyn Node>> {
        Some(Box::new(CanvasSidebar {
            parent: ctx.node.id,
        }))
    }
}

defhandlers! { CanvasLayout {
    actions: [
        AddChild { child: Box<dyn Node> } => (this, a, ctx) {
            const DEFAULT_CHILD_SIZE: Vector = Vector { x: 160.0, y: 40.0 };

            let child_id = ctx.workspace.insert_node_dyn(a.child);
            // Place new nodes near the top-left of the current view.
            let canvas_pos = this.screen_offset() + Vector::splat(40.0);
            let node_id = ctx
                .workspace
                .insert_node(Box::new(CanvasNode::new(child_id, canvas_pos, DEFAULT_CHILD_SIZE)));
            this.children.push(node_id);
        },
    ],
}}
