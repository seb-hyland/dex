use dex_core::prelude::*;
use serde::{Deserialize, Serialize};
use utils::{Reset, Transient};

use crate::{
    layouts::canvas::nodes::CanvasNode,
    primitives::interaction::{InteractionBox, WasDragged},
};

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Canvas {
    children: Vec<NodeUid<CanvasNode>>,
    /// Background drag sensor (a registered child) used for panning.
    drag_interaction: NodeUid<InteractionBox>,
    screen_offset: Transient<Vector>,
}

impl Canvas {
    /// Build an empty canvas into `ws`.
    pub fn build(ws: &Workspace) -> NodeUid<Canvas> {
        let drag_interaction =
            ws.insert_node(Box::new(InteractionBox::sensing(false, false, true)));
        ws.insert_node(Box::new(Self {
            children: Vec::new(),
            drag_interaction,
            screen_offset: Transient::default(),
        }))
    }

    pub fn push_child(&mut self, child: NodeUid<CanvasNode>) {
        self.children.push(child);
    }

    fn screen_offset(&self) -> Vector {
        self.screen_offset.val().unwrap_or(Vector::splat(0.0))
    }
}

#[typetag::serde]
impl Node for Canvas {
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
        let origin = ctx.constraints.pos;
        let region = ScreenRegion::from_min_size(origin, size);

        ctx.draw_workspace_node(
            self.drag_interaction,
            DrawConstraints {
                pos: origin,
                x: Some(AxisConstraint::Exactly(avail_x)),
                y: Some(AxisConstraint::Exactly(avail_y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );
        if let Some(drag_delta) = ctx
            .node
            .workspace
            .send_request(self.drag_interaction, WasDragged)
            .flatten()
        {
            // Update the offset
            self.screen_offset.set(self.screen_offset() - drag_delta);
        }

        let canvas_origin = origin - self.screen_offset();
        for &child in &self.children {
            ctx.draw_workspace_node(
                child,
                DrawConstraints {
                    // `CanvasNode` children will draw relative to the origin
                    pos: canvas_origin,
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

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.drag_interaction.erase());
        for child in &self.children {
            ctx.workspace.delete_node(child.erase());
        }
    }
}

defhandlers! { Canvas {
    actions: [
        AddCanvasItem { child: Box<dyn Node> } => (this, a, ctx) {
            const DEFAULT_CHILD_SIZE: Vector = Vector { x: 160.0, y: 40.0 };

            let child_id = ctx.workspace.insert_node_dyn(a.child);
            // Place new nodes near the top-left of the current view.
            let canvas_pos = this.screen_offset() + Vector::splat(40.0);
            let node_id = CanvasNode::build(ctx.workspace, child_id, canvas_pos, DEFAULT_CHILD_SIZE);
            this.children.push(node_id);
        },
    ],
}}
