use dex_core::prelude::*;
use egui::{Pos2, Rect};
use utils::Transient;

use crate::{
    composites::button::Button,
    layouts::canvas::nodes::{CanvasEditor, CanvasItemBounds, CanvasNode, NudgeCanvasItem},
    primitives::interaction::{DragStartPos, InteractionBox, TakeClicked, WasDragged},
};

/**
    Where the canvas publishes the clip a wire should honour.

    Wires are painted on a foreground layer, which has none of its own. A port
    cannot use the `Ui` it sits in either: `CanvasNode` draws its child clipped
    to the card, so a wire would be cut off at the edge of the node it leaves.
    The surface it crosses is the right bound, and only the surface knows it.
*/
const WIRE_CLIP_ID: &str = "dex_canvas_wire_clip";

/// The clip a wire drawn over this frame's canvas should honour, if a canvas
/// has drawn. See [`WIRE_CLIP_ID`].
pub fn wire_clip(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.memory(|mem| mem.data.get_temp(egui::Id::new(WIRE_CLIP_ID)))
}

/// Publish `clip` for the duration of `draw`, restoring whatever a surrounding
/// canvas had published.
fn with_wire_clip<R>(ctx: &egui::Context, clip: egui::Rect, draw: impl FnOnce() -> R) -> R {
    let id = egui::Id::new(WIRE_CLIP_ID);
    let previous: Option<egui::Rect> = ctx.memory_mut(|mem| {
        let previous = mem.data.get_temp(id);
        mem.data.insert_temp(id, clip);
        previous
    });
    let out = draw();
    ctx.memory_mut(|mem| match previous {
        Some(outer) => {
            mem.data.insert_temp(id, outer);
        }
        None => mem.data.remove::<egui::Rect>(id),
    });
    out
}

/// Which pass of a canvas a node is drawn in.
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub enum Layer {
    Background,
    Midground,
    Foreground,
}

#[utils::dynamic_methods]
impl Layer {
    pub fn background() -> Self {
        Self::Background
    }
    pub fn midground() -> Self {
        Self::Midground
    }
    pub fn foreground() -> Self {
        Self::Foreground
    }
}

#[utils::dynamic_type]
#[utils::portable]
pub struct Canvas {
    /// The items on this surface ([`Layer::Midground`] members), in draw order.
    children: Vec<NodeUid>,
    /// [`Layer::Background`] members, painted before the items.
    background: Vec<NodeUid>,
    /// [`Layer::Foreground`] members, painted after them.
    foreground: Vec<NodeUid>,
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
            background: Vec::new(),
            foreground: Vec::new(),
            drag_interaction,
            screen_offset: Transient::default(),
            viewport: Transient::default(),
            panning: Transient::default(),
        })
    }

    fn screen_offset(&self) -> Vector {
        self.screen_offset.val().unwrap_or(Vector::splat(0.0))
    }

    /// The topmost item whose on-screen region contains `pos`.
    fn item_at(&self, ws: &Workspace, pos: ScreenPos) -> Option<NodeUid> {
        self.children.iter().rev().copied().find(|&child| {
            ws.send_request(child, Inspectable).unwrap_or(true)
                && ws
                    .send_request(child, CanvasItemBounds)
                    .is_some_and(|bounds| {
                        Rect::from(self.map_to_screen(bounds)).contains(Pos2::from(pos))
                    })
        })
    }

    /// The members of `layer`, in draw order.
    fn layer(&self, layer: Layer) -> &Vec<NodeUid> {
        match layer {
            Layer::Background => &self.background,
            Layer::Midground => &self.children,
            Layer::Foreground => &self.foreground,
        }
    }

    fn layer_mut(&mut self, layer: Layer) -> &mut Vec<NodeUid> {
        match layer {
            Layer::Background => &mut self.background,
            Layer::Midground => &mut self.children,
            Layer::Foreground => &mut self.foreground,
        }
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

    /// Map a canvas-space bounding region into its on-screen region.
    fn map_to_screen(&self, bounds: ScreenRegion) -> ScreenRegion {
        ScreenRegion::from_min_size(self.canvas_origin() + bounds.min.to_vector(), bounds.size())
    }
}

/// What a canvas adds to the inspector.
#[utils::portable]
pub struct CanvasInspector {
    /// The surface to open.
    #[uid_ref]
    target: NodeUid,
    fullscreen_button: NodeUid<Button>,
}

impl CanvasInspector {
    fn build(ws: WorkspaceActionHandle, target: NodeUid) -> NodeUid<CanvasInspector> {
        // The same row, worded the same way, as the one every placement menu now carries.
        let fullscreen_button =
            crate::layouts::inspector::menu_button(ws.clone(), "Open Fullscreen");
        ws.insert_node(Self {
            target,
            fullscreen_button,
        })
    }
}

#[utils::dynamic_node(skip)]
impl Node for CanvasInspector {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Canvas Menu".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let drawn = ctx.draw_workspace_node(self.fullscreen_button.erase(), constraints);

        let ws = ctx.node.workspace;
        if ws
            .send_request(self.fullscreen_button.erase(), TakeClicked)
            .unwrap_or(false)
        {
            // Onto the root's override stack.
            let root = ws.root();
            ws.submit_action_dyn(Action {
                dest: root,
                description: "Opened a canvas fullscreen".into(),
                body: Box::new(crate::layouts::desktops::PushOverride { node: self.target }),
            });
        }

        drawn.unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.fullscreen_button.erase());
    }
}

defhandlers! { CanvasInspector {} }

#[utils::dynamic_node]
impl Node for Canvas {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Canvas".into()
    }

    /// A canvas offers to fill the window with itself.
    fn build_inspector(&self, ctx: NodeContext) -> Option<NodeUid> {
        Some(CanvasInspector::build(ctx.workspace.action_handle(), ctx.id).erase())
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let avail = ctx.constraints.available();

        let size = Vector {
            x: avail.x,
            y: avail.y,
        };
        let origin = ctx.constraints.pos;
        let region = ScreenRegion::from_min_size(origin, size);
        self.viewport.set(region);

        ctx.draw_workspace_node(
            self.drag_interaction.erase(),
            DrawConstraints {
                pos: origin,
                x: Some(AxisConstraint::Exactly(avail.x)),
                y: Some(AxisConstraint::Exactly(avail.y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            },
        );
        let ws = ctx.node.workspace;
        if let Some(drag_delta) = ws.send_request(self.drag_interaction, WasDragged).flatten() {
            // Only the background pans, not the cursor over a `CanvasNode`.
            let panning = *self.panning.val_or_else(|| {
                ws.send_request(self.drag_interaction, DragStartPos)
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

        // A layer paints across the whole viewport and is clipped to it.
        let layer = DrawConstraints {
            pos: origin,
            x: Some(AxisConstraint::Exactly(avail.x)),
            y: Some(AxisConstraint::Exactly(avail.y)),
            wrap: WrapConstraints::NotAllowed,
            should_clip: true,
        };
        for &member in &self.background {
            ctx.draw_workspace_node(member, layer);
        }

        let canvas_origin = origin - self.screen_offset();
        // This surface is what bounds the wires its items draw between them.
        let surface_clip = ctx.ui.clip_rect();
        let egui_ctx = ctx.ui.ctx().clone();
        with_wire_clip(&egui_ctx, surface_clip, || {
            for &child in &self.children {
                let constraints = DrawConstraints {
                    // `CanvasNode` children will draw relative to the origin
                    pos: canvas_origin,
                    x: None,
                    y: None,
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: false,
                };
                // Canvas items are content the user points at, so they are offered to the inspector.
                ctx.draw_inspectable_node(child.erase(), constraints);
            }
        });

        // Over everything, so chrome is never lost under the content.
        for &member in &self.foreground {
            ctx.draw_workspace_node(member, layer);
        }

        DrawResult::Complete {
            region: Some(region),
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.drag_interaction.erase());
        for child in self
            .children
            .iter()
            .chain(&self.background)
            .chain(&self.foreground)
        {
            ctx.workspace.delete_node(child.erase());
        }
    }
}

/// Turn a placed `child` (its id and its node) into a canvas item.
fn build_canvas_item(
    ws: &Workspace,
    node: &dyn Node,
    child: NodeUid,
    pos: Vector,
    size: Vector,
) -> NodeUid {
    let child_ctx = NodeContext {
        id: child,
        workspace: ws,
    };
    node.request(
        CanvasEditor {
            canvas_pos: pos,
            size,
        },
        child_ctx,
    )
    .unwrap_or_else(|| CanvasNode::build(ws.action_handle(), child, pos, size).erase())
}

defhandlers! { Canvas {
    actions: [
        AddCanvasItem { child: Arc<dyn Node>, size: Vector } => (this, a, ctx) {
            let child_id = ctx.workspace.insert_node_dyn(a.child.clone());
            // Center new nodes in the currently visible section of the canvas.
            let visible_size = this
                .viewport
                .val()
                .map(|r| r.size())
                .unwrap_or(Vector::splat(0.0));
            let canvas_pos = this.screen_offset() + visible_size / 2.0 - a.size / 2.0;
            let item = build_canvas_item(
                ctx.workspace,
                a.child.as_ref(),
                child_id,
                canvas_pos,
                a.size,
            );
            this.children.push(item);
        },
        /*
            Put `node` on this surface as an item.

            One that is already an item joins as it is, nudged so it does not land exactly on what it came from.
        */
        PlaceOnCanvas { node: NodeUid, size: Vector } => (this, a, ctx) {
            const PLACE_OFFSET: f32 = 24.0;
            let ws = ctx.workspace;

            // Anything that answers the bounds protocol is already a canvas item.
            let is_canvas_item = ws.send_request(a.node, CanvasItemBounds).is_some();

            let item = if is_canvas_item {
                ws.submit_action(
                    a.node,
                    "Offset the placed item",
                    NudgeCanvasItem { delta: Vector::splat(PLACE_OFFSET) },
                );
                a.node
            } else {
                let visible_size = this
                    .viewport
                    .val()
                    .map(|r| r.size())
                    .unwrap_or(Vector::splat(0.0));
                let canvas_pos = this.screen_offset() + visible_size / 2.0 - a.size / 2.0;
                // `a.node` is already live here, so it can be fetched to dispatch.
                match ws.get_node(a.node) {
                    Some(node) => {
                        build_canvas_item(ws, node.as_ref(), a.node, canvas_pos, a.size)
                    }
                    None => CanvasNode::build(ws.action_handle(), a.node, canvas_pos, a.size)
                        .erase(),
                }
            };

            if !this.children.contains(&item) {
                this.children.push(item);
            }
        },
        // Take an already-built node onto this surface, in `layer`.
        AdoptCanvasNode { node: NodeUid, layer: Layer } => (this, a) {
            if !this.layer(a.layer).contains(&a.node) {
                this.layer_mut(a.layer).push(a.node);
            }
        },
        // Draw `node` last, so it sits over everything else on this surface.
        BringCanvasItemToFront { node: NodeUid } => (this, a) {
            if let Some(pos) = this.children.iter().position(|c| *c == a.node) {
                let item = this.children.remove(pos);
                this.children.push(item);
            }
        },
        // Draw `node` first, so everything else on this surface sits over it.
        SendCanvasItemToBack { node: NodeUid } => (this, a) {
            if let Some(pos) = this.children.iter().position(|c| *c == a.node) {
                let item = this.children.remove(pos);
                this.children.insert(0, item);
            }
        },
        // Drop a node from the canvas, whichever layer holds it.
        RemoveCanvasItem { node: NodeUid } => (this, a, ctx) {
            for layer in [Layer::Background, Layer::Midground, Layer::Foreground] {
                if let Some(pos) = this.layer(layer).iter().position(|c| *c == a.node) {
                    this.layer_mut(layer).remove(pos);
                    break;
                }
            }
            ctx.workspace.delete_node(a.node);
        },
        // Swap `old` out for a fresh item built from `child`, in place.
        SwapCanvasItem { old: NodeUid, child: Arc<dyn Node>, pos: Vector, size: Vector } => (this, a, ctx) {
            let child_id = ctx.workspace.insert_node_dyn(a.child.clone());
            // Dispatch on the node we still hold; it is not yet live in the registry.
            let item = build_canvas_item(ctx.workspace, a.child.as_ref(), child_id, a.pos, a.size);
            match this.children.iter().position(|c| *c == a.old) {
                Some(i) => this.children[i] = item,
                None => this.children.push(item),
            }
            ctx.workspace.delete_node(a.old);
        },
    ],
    requests: [
        // The items on this surface, in draw order.
        CanvasChildren => (this, _q): Vec<NodeUid> { this.children.clone() },
        // The members of one layer, in draw order.
        CanvasLayerNodes { layer: Layer } => (this, s): Vec<NodeUid> {
            this.layer(s.layer).clone()
        },
        // The canvas-space point at the top-left of what is currently visible.
        CanvasViewOrigin => (this, _q): Vector { this.screen_offset() },
        // The top-most connectable item whose on-screen region contains `pos`.
        ConnectableAt { pos: ScreenPos } => (this, s, ctx): Option<NodeUid> {
            this.item_at(ctx.workspace, s.pos)
        },
        // Map a connectable item's current layout into its on-screen region.
        NodeScreenRect { node: NodeUid } => (this, s, ctx): Option<ScreenRegion> {
            ctx.workspace
                .send_request(s.node, CanvasItemBounds)
                .map(|bounds| this.map_to_screen(bounds))
        },
    ],
}}

#[cfg(test)]
mod tests {
    use super::{wire_clip, with_wire_clip};

    /// A nested surface publishes its own bound and gives the outer one back,
    /// so a canvas opened over a canvas does not leave the wires beneath it
    /// clipped to the wrong rectangle.
    #[test]
    fn the_wire_clip_nests_and_unwinds() {
        let ctx = egui::Context::default();
        let outer = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let inner = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(200.0, 100.0));

        assert_eq!(wire_clip(&ctx), None, "nothing is published to begin with");

        with_wire_clip(&ctx, outer, || {
            assert_eq!(wire_clip(&ctx), Some(outer));
            with_wire_clip(&ctx, inner, || {
                assert_eq!(wire_clip(&ctx), Some(inner), "the innermost surface wins");
            });
            assert_eq!(wire_clip(&ctx), Some(outer), "and the outer one comes back");
        });

        assert_eq!(
            wire_clip(&ctx),
            None,
            "a port drawn outside any canvas inherits nothing stale"
        );
    }
}
