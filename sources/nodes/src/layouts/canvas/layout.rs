use dex_core::prelude::*;
use egui::{Pos2, Rect};
use utils::Transient;

use crate::{
    layouts::canvas::nodes::{CanvasNode, CanvasNodeConstraints, ConstraintsTuple},
    primitives::interaction::{InteractionBox, WasDragged},
};

#[utils::dynamic_type]
#[utils::portable]
pub struct Canvas {
    children: Vec<NodeUid<CanvasNode>>,
    /// Background drag sensor (a registered child) used for panning.
    drag_interaction: NodeUid<InteractionBox>,
    screen_offset: Transient<Vector>,
    /// The canvas's on-screen region as of the last frame it was drawn.
    viewport: Transient<ScreenRegion>,
}

#[utils::dynamic_methods]
impl Canvas {
    /// Build an empty canvas into `ws`.
    pub fn build(ws: WorkspaceActionHandle) -> NodeUid<Canvas> {
        let drag_interaction = ws.insert_node(InteractionBox::sensing(false, false, true));
        ws.insert_node(Self {
            children: Vec::new(),
            drag_interaction,
            screen_offset: Transient::default(),
            viewport: Transient::default(),
        })
    }

    pub fn push_child(&mut self, child: NodeUid<CanvasNode>) {
        self.children.push(child);
    }

    fn screen_offset(&self) -> Vector {
        self.screen_offset.val().unwrap_or(Vector::splat(0.0))
    }

    /// Screen position corresponding to canvas-space origin `(0, 0)`.
    fn canvas_origin(&self) -> ScreenPos {
        let origin = self
            .viewport
            .val()
            .map(|r| r.min)
            .unwrap_or(ScreenPos::zero());
        origin - self.screen_offset()
    }

    /// Map a canvas-space layout into its on-screen region.
    fn map_to_screen(&self, tuple: ConstraintsTuple) -> ScreenRegion {
        ScreenRegion::from_min_size(self.canvas_origin() + tuple.pos, tuple.size)
    }
}

#[utils::dynamic_node]
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
        self.viewport.set(region);

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
        AddCanvasItem { child: Arc<dyn Node>, size: Vector } => (this, a, ctx) {
            let child_id = ctx.workspace.insert_node_dyn(a.child);
            // Center new nodes in the currently visible section of the canvas.
            let visible_size = this
                .viewport
                .val()
                .map(|r| r.size())
                .unwrap_or(Vector::splat(0.0));
            let canvas_pos = this.screen_offset() + visible_size / 2.0 - a.size / 2.0;
            let node_id =
                CanvasNode::build(ctx.workspace.action_handle(), child_id, canvas_pos, a.size);
            this.children.push(node_id);
        },
    ],
    requests: [
        // The top-most connectable node whose on-screen region contains `pos`.
        // Any surface can answer with its own connectable nodes.
        ConnectableAt { pos: ScreenPos } => (this, s, ctx): Option<NodeUid> {
            this.children.iter().rev().copied().find(|&child| {
                ctx.workspace
                    .send_request(child, CanvasNodeConstraints)
                    .is_some_and(|tuple| {
                        Rect::from(this.map_to_screen(tuple)).contains(Pos2::from(s.pos))
                    })
            })
            .map(|child| child.erase())
        },
        // Map a connectable node's current layout into its on-screen region.
        NodeScreenRect { node: NodeUid } => (this, s, ctx): Option<ScreenRegion> {
            ctx.workspace
                .send_request(s.node.cast::<CanvasNode>(), CanvasNodeConstraints)
                .map(|tuple| this.map_to_screen(tuple))
        },
    ],
}}
