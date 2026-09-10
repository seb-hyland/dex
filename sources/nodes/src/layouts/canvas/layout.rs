use dex_core::prelude::*;
use egui::emath::TSTransform;
use egui::{Id, LayerId, Pos2, Rect, Vec2};
use utils::Transient;

use crate::layouts::canvas::nodes::{
    CanvasEditor, CanvasItemBounds, CanvasNode, CanvasNodeChild, NudgeCanvasItem,
};
use crate::scripting::DataflowOutput;

/// Whether `node`, or what it renders, is a pan/zoom surface — a canvas.
///
/// Follows the two ways one hides behind an item: a lambda whose output is the
/// surface (a data explorer, a phylogeny hands one back), and a wrapper that
/// frames a canvas as its child. Bounded so a cycle cannot spin.
fn holds_surface(ws: &Workspace, node: NodeUid) -> bool {
    /// How many nodes the search will look at before giving up. It runs on the
    /// pointer path, and the answer only has to be right for the handful of
    /// nodes an item actually wraps.
    const BUDGET: usize = 64;

    let mut seen = std::collections::HashSet::new();
    let mut queue = vec![node];
    let mut looked = 0usize;
    while let Some(uid) = queue.pop() {
        if looked >= BUDGET {
            return false;
        }
        if !seen.insert(uid) {
            continue;
        }
        looked += 1;
        if ws.send_request(uid, CanvasViewOrigin).is_some() {
            return true;
        }
        // A lambda's output — a phylogeny hands back its plane.
        if let Some(out) = ws.send_request(uid, DataflowOutput).flatten() {
            queue.push(out);
        }
        // A canvas item's content.
        if let Some(inner) = ws.send_request(uid, CanvasNodeChild) {
            queue.push(inner);
        }
        // And whatever it merely *keeps*. A data explorer is a node of its own
        // that draws a control bar above a plane: the plane is neither its
        // output nor its child, only one of the things it owns — so without
        // this the wheel over an explorer magnified the explorer's plane and
        // the surface holding it at the same time.
        if let Some(held) = ws.get_node(uid) {
            held.owned_refs(&mut |child| queue.push(child));
        }
    }
    false
}

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

/// Where the app publishes the box a canvas counts as life size in.
const REFERENCE_ID: &str = "dex_canvas_reference";

/// Publish the box a canvas counts as life size in, for this frame.
pub fn set_canvas_reference(ctx: &egui::Context, size: Vector) {
    ctx.memory_mut(|mem| {
        mem.data
            .insert_temp(egui::Id::new(REFERENCE_ID), egui::vec2(size.x, size.y))
    });
}

/// How much of life size a canvas drawn into `viewport` is showing. See
/// [`REFERENCE_ID`].
fn reference_fit(ctx: &egui::Context, viewport: Vector) -> f32 {
    let reference: Option<egui::Vec2> =
        ctx.memory(|mem| mem.data.get_temp(egui::Id::new(REFERENCE_ID)));
    let Some(reference) = reference.filter(|r| r.x > 0.0 && r.y > 0.0) else {
        return 1.0;
    };
    // The smaller ratio, so a preview never shows *less* of the plane than the
    // full-size view does: it is the same picture, scaled down to fit.
    (viewport.x / reference.x)
        .min(viewport.y / reference.y)
        .clamp(0.02, 4.0)
}

/// Which band of a canvas a node is drawn in.
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
    /// The canvas point shown at the middle of the viewport.
    screen_offset: Transient<Vector>,
    /// The magnification of the surface: 1.0 is life size, >1 zoomed in.
    zoom: Transient<f32>,
    /**
        The canvas's region as of the last frame it was drawn — in the
        coordinates of whatever it was drawn *into*, which is the screen only
        when nothing is between this surface and the window. See [`Canvas::outer`].
    */
    viewport: Transient<ScreenRegion>,
    /// How the frame this surface was drawn into reaches the screen.
    outer: Transient<TSTransform>,
    /// How much of life size this surface was last drawn at. See [`REFERENCE_ID`].
    fit: Transient<f32>,
    /// What this surface calls itself, when it is standing for something.
    name: Option<String>,
    /// Whether the drag in progress is panning this surface. Decided when the drag begins.
    panning: Transient<bool>,
}

/// The magnification stays within these bounds, so the plane can never be
/// zoomed to a degenerate scale or lost entirely off the far end.
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.0;

#[utils::dynamic_methods]
impl Canvas {
    /// Build an empty canvas into `ws`.
    pub fn build(ws: WorkspaceActionHandle) -> NodeUid<Canvas> {
        ws.insert_node(Self {
            children: Vec::new(),
            background: Vec::new(),
            foreground: Vec::new(),
            screen_offset: Transient::default(),
            zoom: Transient::default(),
            viewport: Transient::default(),
            outer: Transient::default(),
            fit: Transient::default(),
            name: None,
            panning: Transient::default(),
        })
    }

    /**
        The canvas point at the middle of the viewport.

        Unset means "not looked at yet", and a surface that has not been looked
        at shows its origin at the top-left, because that is where content is
        authored from. Which point that is depends on the box, so it cannot be a
        constant and is worked out from the last one drawn.
    */
    fn view_centre(&self) -> Vector {
        self.screen_offset
            .val()
            .unwrap_or_else(|| self.visible_size() / 2.0)
    }

    /// The span of canvas the viewport covers, in canvas units.
    fn visible_size(&self) -> Vector {
        self.viewport
            .val()
            .map(|r| r.size())
            .unwrap_or(Vector::splat(0.0))
            / self.scale().max(f32::EPSILON)
    }

    /// The canvas point at the *top-left* of the viewport.
    ///
    /// What [`CanvasViewOrigin`] publishes, and the frame a background draws
    /// its own points in — so this stays the corner however the surface is
    /// anchored, and nothing outside this file has to know that changed.
    fn view_origin(&self) -> Vector {
        self.view_centre() - self.visible_size() / 2.0
    }

    /// The magnification the *user* set: 1.0 is life size, whatever box this
    /// surface happens to be drawn in.
    fn zoom(&self) -> f32 {
        self.zoom.val().unwrap_or(1.0)
    }

    /// How much of life size the box it was last drawn in comes to.
    fn fit(&self) -> f32 {
        self.fit.val().unwrap_or(1.0)
    }

    /// What the plane is actually drawn at: the magnification asked for, taken
    /// down to the box there is room in.
    fn scale(&self) -> f32 {
        self.zoom() * self.fit()
    }

    /// How the frame this surface was drawn into reaches the screen. Identity
    /// for a canvas drawn straight onto the window. See the field.
    fn outer(&self) -> TSTransform {
        self.outer.val().unwrap_or(TSTransform::IDENTITY)
    }

    /// The transform taking a canvas-space point into the frame this surface
    /// was drawn in — pan and zoom, and nothing else.
    fn to_parent(&self) -> TSTransform {
        let zoom = self.scale();
        let anchor = self.viewport_centre();
        // parent = viewport_centre + (p - view_centre) * zoom
        let centre = self.view_centre();
        let translation = Vec2::new(anchor.x, anchor.y) - zoom * Vec2::new(centre.x, centre.y);
        TSTransform::new(translation, zoom)
    }

    /// The middle of the box this surface was last drawn in, in that box's frame.
    fn viewport_centre(&self) -> ScreenPos {
        self.viewport
            .val()
            .map(|r| r.min + r.size() / 2.0)
            .unwrap_or(ScreenPos::zero())
    }

    /// The transform taking a canvas-space point to where it lands on screen.
    fn to_global(&self) -> TSTransform {
        self.outer() * self.to_parent()
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

    /// The topmost item at `pos` that holds a pan/zoom surface.
    ///
    /// A nested canvas declines the inspector's lens, so `item_at` passes over
    /// it the same way it passes over a plane's own see-through content — right
    /// for a plot's marks, wrong for a whole surface, which will take the drag
    /// to pan itself and so must stop this surface from panning under it. A
    /// surface is what answers `CanvasViewOrigin`; the item holds it as its
    /// child, and the child may be a lambda whose *output* is the surface (a
    /// data explorer, a phylogeny), so the search follows the item's child and
    /// then the output of whatever it finds.
    fn surface_at(&self, ws: &Workspace, pos: ScreenPos) -> Option<NodeUid> {
        self.children.iter().rev().copied().find(|&child| {
            ws.send_request(child, CanvasItemBounds)
                .is_some_and(|bounds| {
                    Rect::from(self.map_to_screen(bounds)).contains(Pos2::from(pos))
                })
                && ws
                    .send_request(child, CanvasNodeChild)
                    .is_some_and(|inner| holds_surface(ws, inner))
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

    /// Map a canvas-space bounding region into its on-screen region.
    fn map_to_screen(&self, bounds: ScreenRegion) -> ScreenRegion {
        ScreenRegion::from(self.to_global() * Rect::from(bounds))
    }
}

#[utils::dynamic_node]
impl Node for Canvas {
    fn type_name(&self, _ctx: NodeContext) -> String {
        self.name.clone().unwrap_or_else(|| "A Canvas".to_owned())
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

        let ws = ctx.node.workspace;
        let egui_ctx = ctx.ui.ctx().clone();

        // Everything below works in one of two frames: the surface's own
        // (`region`, the constraints, a foreground's coordinates) and the
        // screen's (the pointer, the hit tests, what the probe records). This is
        // what stands between them, and it is the identity for a canvas drawn
        // straight onto the window.
        let parent_layer = ctx.ui.layer_id();
        let outer = egui_ctx
            .layer_transform_to_global(parent_layer)
            .unwrap_or(TSTransform::IDENTITY);
        self.outer.set(outer);
        self.fit.set(reference_fit(&egui_ctx, size));
        // Settle where this surface is looking, once, the first time it is
        // drawn into a box worth the name.
        //
        // It has to be *settled* rather than worked out afresh each frame. A
        // surface shows its origin in the corner until someone moves it, and
        // "the canvas point in the corner" is a different point in every box —
        // so deriving it per frame would anchor an untouched surface to the
        // corner again, which is the whole thing this is here to stop.
        if self.screen_offset.val().is_none() && size.x > 0.0 && size.y > 0.0 {
            self.screen_offset.set(self.visible_size() / 2.0);
        }
        // The span this surface may paint into, in its own (parent-local) frame:
        // its region, but no wider than the clip it was handed. A top-level
        // canvas is handed the whole window and this changes nothing; a canvas
        // that is itself an item on another surface is handed that item's box,
        // and this is what keeps its own items and chrome inside it. Its
        // midground and foreground ride sublayers, which egui clips on their
        // own — so without this a nested surface's marks and readouts spill past
        // the item holding them, over its neighbours and out under the sidebar.
        let visible = Rect::from(region).intersect(ctx.ui.clip_rect());
        let on_screen = outer * visible;

        // Pointer gestures move the plane. A sizing pass sees the same cached
        // gesture as the real one, so it is only acted on for real.
        if !ctx.measuring() {
            // Pinch or alt-scroll magnifies about the cursor, when the cursor is
            // over this surface. `zoom_delta` folds in both (the alt binding is
            // set at startup); it is 1.0 when neither happened, and a plain
            // scroll leaves it untouched.
            let pointer = egui_ctx
                .pointer_latest_pos()
                .filter(|p| {
                    on_screen.contains(*p)
                        // A nested surface under the cursor zooms itself, so this
                        // one must not zoom too — otherwise a wheel over an inner
                        // plane magnifies the plane *and* the surface holding it.
                        // The pan gesture below reaches through the same way.
                        && self.surface_at(ws, ScreenPos::from(*p)).is_none()
                })
                // Into this surface's own frame, where `region` is named.
                .map(|p| outer.inverse() * p);
            if let Some(pointer) = pointer {
                let zoom_delta = egui_ctx.input(|i| i.zoom_delta());
                if (zoom_delta - 1.0).abs() > f32::EPSILON {
                    let old = self.zoom();
                    let new = (old * zoom_delta).clamp(MIN_ZOOM, MAX_ZOOM);
                    // Hold the canvas point under the cursor still across the
                    // change of scale — the scale it is *drawn* at, which is the
                    // magnification taken down to the box there is room in.
                    let fit = self.fit().max(f32::EPSILON);
                    let k = 1.0 / (old * fit) - 1.0 / (new * fit);
                    // Measured from whatever the surface is anchored at, which
                    // is its middle.
                    let anchor = self.viewport_centre();
                    self.screen_offset.set(
                        self.view_centre()
                            + Vector {
                                x: (pointer.x - anchor.x) * k,
                                y: (pointer.y - anchor.y) * k,
                            },
                    );
                    self.zoom.set(new);
                }
            }

            // Dragging empty background pans the plane. This reads the pointer
            // straight off the input rather than through a full-viewport sensor:
            // a sensor would sit on one layer, and a foreground that covers the
            // surface (a readout, a legend backdrop) would shadow it. An item
            // that took the drag itself sets `dragged_id`, and its own handle
            // moves it; the press starting over an addressable item is what
            // tells a fresh drag apart from a pan.
            let (down, press, delta) = egui_ctx.input(|i| {
                (
                    i.pointer.primary_down(),
                    i.pointer.press_origin(),
                    i.pointer.delta(),
                )
            });
            if down && egui_ctx.dragged_id().is_none() {
                let panning = *self.panning.val_or_else(|| {
                    press.map(ScreenPos::from).is_some_and(|p| {
                        on_screen.contains(Pos2::from(p))
                            && self.item_at(ws, p).is_none()
                            // A press over a nested surface is that surface's to
                            // pan, not this one's: without this the drag pans the
                            // inner plane *and* the desktop it sits on, because a
                            // plane declines the inspector's lens and so slips
                            // past `item_at` the way a plane's own see-through
                            // content (plot marks, a shape) is meant to.
                            && self.surface_at(ws, p).is_none()
                    })
                });
                if panning && delta != Vec2::ZERO {
                    // The drag is in screen pixels; a zoomed-in plane travels
                    // less far in its own space for the same hand movement, and
                    // so does one inside a magnified surface.
                    let scale = (outer.scaling * self.scale()).max(f32::EPSILON);
                    self.screen_offset.set(
                        self.view_centre()
                            - Vector {
                                x: delta.x,
                                y: delta.y,
                            } / scale,
                    );
                }
            } else {
                // The gesture is over; the next one decides afresh.
                *self.panning.val_mut() = None;
            }
        }

        // A band paints across the whole viewport and is clipped to it.
        let band = DrawConstraints {
            pos: origin,
            x: Some(AxisConstraint::Exactly(avail.x)),
            y: Some(AxisConstraint::Exactly(avail.y)),
            wrap: WrapConstraints::NotAllowed,
            should_clip: true,
        };
        // The background sits under the items, in this surface's own frame: a
        // grid or axis that wants to track the plane asks for the view origin
        // and the zoom, and maps its own points.
        for &member in &self.background {
            ctx.draw_workspace_node(member, band);
        }

        /*
            Items and chrome ride sublayers of this surface's own layer.

            A sublayer sits directly above its parent in both paint and hit-test
            order, so the items draw over the background and take the pointer —
            which a plain new layer does not. The foreground is a sublayer of the
            *items* rather than a second sublayer of the surface: siblings are
            ordered by when they were registered, and the `Area` below moves the
            items' layer to the top the first time it is pressed, which flipped
            the two and put a plot's own chrome underneath its plot. Nesting says
            what is actually meant — the foreground is directly above the items —
            and cannot come apart.

            One shared midground, not a layer per item. A layer per item would
            put a nested surface in its stacking slot — its overlays no longer
            leaping above a sibling drawn over it — but it costs more than it is
            worth: egui lifts the pressed layer to the front each frame, so the
            z-order flickers as the pointer moves, the wheel and a foreground
            that takes a drag stop both holding at once, and a wire dragged
            between two items (a connection port) no longer lands. The escape a
            nested plane's overlays make is the lesser evil, and the real cure is
            not more layers but zooming a surface without an egui transform layer
            at all — a change to how a canvas renders, not to how its bands
            stack.
        */
        let mid_layer = LayerId::new(parent_layer.order, Id::new(("dex_canvas_mid", ctx.node.id)));
        let fg_layer = LayerId::new(parent_layer.order, Id::new(("dex_canvas_fg", ctx.node.id)));
        egui_ctx.set_sublayer(parent_layer, mid_layer);
        egui_ctx.set_sublayer(mid_layer, fg_layer);

        // The midground carries the pan-and-zoom transform. Items author
        // themselves in canvas space from the origin; egui places and scales
        // them, and clips them in that same space.
        let to_global = self.to_global();
        egui_ctx.set_transform_layer(mid_layer, to_global);
        let local_clip = to_global.inverse() * on_screen;

        /*
            An `Area` over the same layer, claiming the plane's visible span, so
            egui counts this layer as an area — which is how it decides which
            layer the pointer is over. Without one, a scroll area or a tooltip
            inside an item is told the pointer belongs to the surface underneath
            and never sees it (`Workspace::draw_root` registers the root painter
            for the same reason).
        */
        if !ctx.measuring() {
            // One area per layer per frame, through the shared claim: a plot
            // that draws its own readout table onto this same midground then
            // finds the layer already claimed and does not show a second area
            // that would clash on egui's `move` widget.
            dex_core::claim_layer_area(&egui_ctx, mid_layer, local_clip);
        }

        let children = &self.children;
        ctx.on_layer(mid_layer, local_clip, |ctx| {
            // The wires an item draws cross the surface; they are clipped to the
            // visible viewport, named in screen space.
            with_wire_clip(&egui_ctx, on_screen, || {
                for &child in children {
                    let constraints = DrawConstraints {
                        // Canvas space from the origin; the layer transform does
                        // the rest.
                        pos: ScreenPos::zero(),
                        x: None,
                        y: None,
                        wrap: WrapConstraints::NotAllowed,
                        should_clip: false,
                    };
                    // Items are content the user points at, so they are offered
                    // to the inspector.
                    ctx.draw_inspectable_node(child.erase(), constraints);
                }
            });
        });

        // The foreground is chrome — a legend, a readout, a title — that stays
        // put and life-size while the plane moves under it. It is drawn in this
        // surface's own frame, so its layer carries only the way out of that
        // frame; and it sits above the items, so it is never lost beneath them.
        // Clipped to the visible span so a readout pinned bottom-right stops at
        // the edge of the item holding this surface rather than spilling over
        // its neighbours.
        egui_ctx.set_transform_layer(fg_layer, outer);
        ctx.on_layer(fg_layer, visible, |ctx| {
            for &member in &self.foreground {
                ctx.draw_workspace_node(member, band);
            }
        });

        DrawResult::Complete {
            region: Some(region),
        }
    }

    fn on_delete(&self, ctx: NodeContext) {
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
            // Centre new nodes in the currently visible section of the canvas,
            // which is what the surface is anchored at.
            let canvas_pos = this.view_centre() - a.size / 2.0;
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
                let canvas_pos = this.view_centre() - a.size / 2.0;
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
        /*
            Look at `pos`: put that canvas point at the middle of the viewport.

            Pins the anchor rather than nudging it, so it survives the settling
            in `draw` — which is what lets a surface be built already looking at
            what is on it instead of at its own origin.
        */
        CentreCanvasView { pos: Vector } => (this, a) {
            this.screen_offset.set(a.pos);
        },
        // Back to life size, keeping the view where it is.
        ResetCanvasZoom => (this, _a) {
            this.zoom.set(1.0);
        },
        // Name the surface after whatever was put on it. See `Canvas::name`.
        NameCanvas { name: String } => (this, s) {
            this.name = (!s.name.is_empty()).then_some(s.name.clone());
        },
        // Back to the plane's own origin at the top-left, keeping the zoom.
        // Forgetting where it was looking is exactly that: an unlooked-at
        // surface shows its origin in the corner, whatever box it is in.
        ResetCanvasView => (this, _a) {
            *this.screen_offset.val_mut() = None;
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
        CanvasViewOrigin => (this, _q): Vector { this.view_origin() },
        // What the plane is drawn at in the box it is in: 1.0 life size, >1
        // magnified. A background that tracks the plane scales its own drawing
        // by this, so it shrinks with the plane in a preview.
        CanvasZoom => (this, _q): f32 { this.scale() },
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
