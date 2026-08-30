use dex_core::prelude::*;
use egui::{Pos2, Rect};
use utils::Transient;

use crate::{
    layouts::canvas::nodes::{CanvasNode, CanvasNodeConstraints, ConstraintsTuple, SetLayout},
    primitives::interaction::{DragPointerPos, InteractionBox, WasDragged},
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
    /// Whether the drag in progress is panning this surface. Decided when the drag begins.
    panning: Transient<bool>,
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
            panning: Transient::default(),
        })
    }

    pub fn push_child(&mut self, child: NodeUid<CanvasNode>) {
        self.children.push(child);
    }

    fn screen_offset(&self) -> Vector {
        self.screen_offset.val().unwrap_or(Vector::splat(0.0))
    }

    /// The topmost item whose on-screen region contains `pos`.
    fn item_at(&self, ws: &Workspace, pos: ScreenPos) -> Option<NodeUid<CanvasNode>> {
        self.children.iter().rev().copied().find(|&child| {
            ws.send_request(child, CanvasNodeConstraints)
                .is_some_and(|tuple| {
                    Rect::from(self.map_to_screen(tuple)).contains(Pos2::from(pos))
                })
        })
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
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Canvas".into()
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
            self.drag_interaction.erase(),
            DrawConstraints {
                pos: origin,
                x: Some(AxisConstraint::Exactly(avail_x)),
                y: Some(AxisConstraint::Exactly(avail_y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );
        let ws = ctx.node.workspace;
        if let Some(drag_delta) = ws.send_request(self.drag_interaction, WasDragged).flatten() {
            // Only the background pans, not the cursor over a `CanvasNode`.
            let panning = *self.panning.val_or_else(|| {
                ws.send_request(self.drag_interaction, DragPointerPos)
                    .flatten()
                    .is_none_or(|start| self.item_at(ws, start).is_none())
            });
            if panning {
                self.screen_offset.set(self.screen_offset() - drag_delta);
            }
        } else {
            // The drag is over; the next one decides afresh.
            *self.panning.val_mut() = None;
        }

        let canvas_origin = origin - self.screen_offset();
        for &child in &self.children {
            // Canvas items are content the user points at, so they get an inspector.
            ctx.draw_inspectable_node(
                child.erase(),
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
        /*
            Put `node` on this surface as an item.

            One that is already an item joins as it is, nudged so it does not land exactly on what it came from.
        */
        PlaceOnCanvas { node: NodeUid, size: Vector } => (this, a, ctx) {
            const PLACE_OFFSET: f32 = 24.0;

            let is_item = ctx
                .workspace
                .get_node(a.node)
                .is_some_and(|n| n.as_ref().as_any_ref().is::<CanvasNode>());

            let item = if is_item {
                let item = a.node.cast::<CanvasNode>();
                if let Some(layout) = ctx.workspace.send_request(item, CanvasNodeConstraints) {
                    ctx.workspace.submit_action(
                        item,
                        "Offset the placed item",
                        SetLayout {
                            canvas_pos: layout.pos + Vector::splat(PLACE_OFFSET),
                            size: layout.size,
                        },
                    );
                }
                item
            } else {
                let visible_size = this
                    .viewport
                    .val()
                    .map(|r| r.size())
                    .unwrap_or(Vector::splat(0.0));
                let canvas_pos = this.screen_offset() + visible_size / 2.0 - a.size / 2.0;
                CanvasNode::build(ctx.workspace.action_handle(), a.node, canvas_pos, a.size)
            };

            if !this.children.contains(&item) {
                this.children.push(item);
            }
        },
        // Take an already-built canvas node onto this surface, as a copy or a
        // mirror of an existing one does.
        AdoptCanvasNode { node: NodeUid<CanvasNode> } => (this, a) {
            if !this.children.contains(&a.node) {
                this.children.push(a.node);
            }
        },
        // Drop a node from the canvas; deleting it cascades to what it wraps.
        RemoveCanvasItem { node: NodeUid<CanvasNode> } => (this, a, ctx) {
            if let Some(pos) = this.children.iter().position(|c| *c == a.node) {
                this.children.remove(pos);
            }
            ctx.workspace.delete_node(a.node.erase());
        },
    ],
    requests: [
        // The nodes on this surface, in draw order.
        CanvasChildren => (this, _q): Vec<NodeUid<CanvasNode>> { this.children.clone() },
        // The top-most connectable node whose on-screen region contains `pos`.
        // Any surface can answer with its own connectable nodes.
        ConnectableAt { pos: ScreenPos } => (this, s, ctx): Option<NodeUid> {
            this.item_at(ctx.workspace, s.pos).map(|child| child.erase())
        },
        // Map a connectable node's current layout into its on-screen region.
        NodeScreenRect { node: NodeUid } => (this, s, ctx): Option<ScreenRegion> {
            ctx.workspace
                .send_request(s.node.cast::<CanvasNode>(), CanvasNodeConstraints)
                .map(|tuple| this.map_to_screen(tuple))
        },
    ],
}}
