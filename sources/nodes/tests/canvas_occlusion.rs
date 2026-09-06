//! What a canvas's bands may and may not take from one another.
//!
//! A surface paints in three bands — background, items, foreground — and the
//! items ride a layer of their own so the plane can be panned and zoomed. Bands
//! are a *painting* order: putting them on separate egui layers must not change
//! who the pointer belongs to. Three things have to keep holding:
//!
//!   * a foreground that only watches the pointer does not take it from the
//!     items underneath,
//!   * a wheel over an item scrolls that item,
//!   * chrome the app draws over the surface — the inspector's lens — is still
//!     reachable where it is drawn.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{
    AddCanvasItem, AdoptCanvasNode, Canvas, CanvasChildren, CanvasViewOrigin, Layer,
    NodeScreenRect, SwapCanvasItem,
};
use dex_nodes::layouts::canvas::nodes::{CanvasItemBounds, CanvasNodeChild, NudgeCanvasItem};
use dex_nodes::layouts::{LayoutChild, ScrollLayout};
use dex_nodes::primitives::interaction::{InteractionBox, WasClicked, WasDragged, WasHovered};
use dex_nodes::primitives::shapes::Rect;

const SCREEN: egui::Vec2 = egui::vec2(900.0, 640.0);
/// The canvas-space point the item is placed at, well clear of the edges.
const PLACE: Vector = Vector { x: 200.0, y: 160.0 };
const ITEM_SIZE: Vector = Vector { x: 120.0, y: 90.0 };
/// The colour of the tall block inside the scrolled item, so it can be found
/// again among the frame's shapes.
const BLOCK: Color = Color::rgb(0, 200, 0);

struct Harness {
    ws: Workspace,
    ctx: egui::Context,
    canvas: NodeUid<Canvas>,
    pos: egui::Pos2,
}

impl Harness {
    /// A workspace whose root is one canvas holding a single item built from
    /// `child`, placed at [`PLACE`].
    fn new(child: Arc<dyn Node>) -> Harness {
        dex_nodes::scripting::init_python();
        let mut ws = Workspace::new_empty();
        let canvas = Canvas::build(ws.action_handle());
        ws.set_root(canvas.erase());
        ws.process_pending();

        let ctx = egui::Context::default();
        dex_nodes::fonts::install_fonts(&ctx);
        let mut h = Harness {
            ws,
            ctx,
            canvas,
            pos: egui::pos2(-100.0, -100.0),
        };
        // A new item is centred in the viewport, which the canvas only knows
        // once it has drawn, so it is nudged into place afterwards.
        h.frame(vec![]);
        h.ws.submit_action(
            canvas,
            "item",
            AddCanvasItem {
                child,
                size: ITEM_SIZE,
            },
        );
        h.ws.process_pending();
        h.frame(vec![]);
        h.place_item_at(PLACE);
        h
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let input = egui::RawInput {
            screen_rect: Some(rect),
            events,
            ..Default::default()
        };
        let ws = &mut self.ws;
        self.ctx.clone().run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ws.draw_frame(ui, rect);
            });
        })
    }

    fn item(&self) -> NodeUid {
        *self
            .ws
            .send_request(self.canvas, CanvasChildren)
            .unwrap_or_default()
            .first()
            .expect("the item was added")
    }

    /// The node the item frames.
    fn child(&self) -> NodeUid {
        self.ws
            .send_request(self.item(), CanvasNodeChild)
            .expect("the item frames a child")
    }

    fn place_item_at(&mut self, target: Vector) {
        let item = self.item();
        let at = self
            .ws
            .send_request(item, CanvasItemBounds)
            .expect("the item reports its bounds")
            .min
            .to_vector();
        self.ws
            .submit_action(item, "place", NudgeCanvasItem { delta: target - at });
        self.ws.process_pending();
        self.frame(vec![]);
    }

    /// Where the item sits on screen, as the canvas maps it.
    fn item_rect(&self) -> ScreenRegion {
        self.ws
            .send_request(self.canvas, NodeScreenRect { node: self.item() })
            .flatten()
            .expect("the item is on screen")
    }

    /// The middle of the item, on screen.
    fn item_centre(&self) -> egui::Pos2 {
        let r = self
            .ws
            .send_request(self.canvas, NodeScreenRect { node: self.item() })
            .flatten()
            .expect("the item is on screen");
        egui::pos2((r.min.x + r.max.x) * 0.5, (r.min.y + r.max.y) * 0.5)
    }

    fn move_to(&mut self, p: egui::Pos2) {
        self.pos = p;
        self.frame(vec![egui::Event::PointerMoved(p)]);
    }

    /// Press and release at `p`, with the settling frames a click needs: egui
    /// hit-tests against the widgets of the previous pass.
    fn click(&mut self, p: egui::Pos2) {
        self.move_to(p);
        self.move_to(p);
        self.frame(vec![
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ]);
    }

    /// Press at `from`, move to `to` in steps, and release. `watch` is called
    /// after each step of the movement, while the button is still down.
    fn drag<T>(
        &mut self,
        from: egui::Pos2,
        to: egui::Pos2,
        mut watch: impl FnMut(&Self) -> Option<T>,
    ) -> Vec<T> {
        self.move_to(from);
        self.move_to(from);
        self.frame(vec![egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        let mut seen = Vec::new();
        for step in 1..=3 {
            let t = step as f32 / 3.0;
            self.move_to(egui::pos2(
                from.x + (to.x - from.x) * t,
                from.y + (to.y - from.y) * t,
            ));
            seen.extend(watch(self));
        }
        self.frame(vec![egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        seen
    }

    /// Turn the wheel `by` points at the current pointer position.
    fn wheel(&mut self, by: f32) -> egui::FullOutput {
        self.frame(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, by),
            modifiers: Default::default(),
            phase: egui::TouchPhase::Move,
        }])
    }
}

/// Where the tall block inside the scrolled item painted this frame.
fn block_top(output: &egui::FullOutput) -> Option<f32> {
    // Matched on hue, not on the exact colour: egui fades a panel in over its
    // first frames, and synthetic frames advance no clock, so the block arrives
    // at whatever opacity that animation is at.
    output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Rect(rect)
                if rect.fill.r() == 0 && rect.fill.g() > 0 && rect.fill.b() == 0 =>
            {
                Some(rect.rect.min.y)
            }
            _ => None,
        })
}

/**
    A foreground that only watches must not take the pointer from the items.

    The scatterplot and phylogeny examples both put one hover-only sensor over
    the whole surface and read the pointer off it, precisely so that a click
    still reaches the item underneath. A sensor that senses hover alone claims
    nothing, whichever band it is drawn in.
*/
#[test]
fn a_watching_foreground_leaves_the_items_clickable() {
    let mut h = Harness::new(Arc::new(InteractionBox::sensing(false, true, false)));
    let button = h.child();

    // One hover-only sensor across the whole surface, in the foreground.
    let watcher =
        h.ws.insert_node_now(InteractionBox::sensing(true, false, false));
    h.ws.submit_action(
        h.canvas,
        "watcher",
        AdoptCanvasNode {
            node: watcher.erase(),
            layer: Layer::Foreground,
        },
    );
    h.ws.process_pending();
    h.frame(vec![]);

    let centre = h.item_centre();
    h.click(centre);

    assert!(
        h.ws.send_request(button, WasClicked).unwrap_or(false),
        "the click reached the item under the watching foreground"
    );
    assert!(
        h.ws.send_request(watcher.erase(), WasHovered)
            .unwrap_or(false),
        "and the foreground still saw the pointer"
    );
}

/// A wheel turned over an item scrolls that item.
#[test]
fn the_wheel_scrolls_the_item_under_it() {
    let tall = Rect::new(ITEM_SIZE.x, ITEM_SIZE.y * 6.0, BLOCK);
    let h_child: Arc<dyn Node> =
        Arc::new(ScrollLayout::vertical(LayoutChild::Node(Arc::new(tall))));
    let mut h = Harness::new(h_child);

    let centre = h.item_centre();
    h.move_to(centre);
    h.move_to(centre);
    let before = block_top(&h.frame(vec![])).expect("the block painted");

    // Several turns: egui smooths a wheel over the frames that follow it.
    for _ in 0..6 {
        h.wheel(-40.0);
    }
    let after = block_top(&h.frame(vec![])).expect("the block painted");

    assert!(
        after < before - 10.0,
        "the wheel scrolled the item: the block moved up from {before} to {after}"
    );
}

/// The inspector's lens is reachable where it is drawn, over a canvas item.
#[test]
fn the_lens_is_reachable_over_an_item() {
    use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
    use dex_nodes::primitives::text::Label;

    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let root = ws.root();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);

    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let frame = |ws: &mut Workspace, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        let _ = ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
        });
    };

    frame(&mut ws, vec![]);
    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(Label::new("Hello".to_owned())),
            size: Vector { x: 200.0, y: 100.0 },
        }),
    });
    ws.process_pending();
    frame(&mut ws, vec![]);

    let canvas = ws
        .send_request(root.cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let item = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the item was added");
    let rect = ws
        .send_request(root, NodeScreenRect { node: item.erase() })
        .flatten()
        .expect("the item is on screen");

    // Hover the item so the lens appears beside it, then walk onto the lens.
    let centre = egui::pos2(
        (rect.min.x + rect.max.x) * 0.5,
        (rect.min.y + rect.max.y) * 0.5,
    );
    frame(&mut ws, vec![egui::Event::PointerMoved(centre)]);
    let found = ws.inspect_target().expect("the item is under the pointer");
    let lens = egui::pos2(found.region.min.x - 1.0, found.region.min.y - 1.0);
    for _ in 0..3 {
        frame(&mut ws, vec![egui::Event::PointerMoved(lens)]);
    }

    let handle = ctx
        .read_response(egui::Id::new(dex_nodes::layouts::inspector::HANDLE_ID))
        .expect("the lens registered a widget");
    assert!(
        handle.hovered(),
        "the lens takes the pointer where it is drawn, over the item it inspects"
    );
}

/**
    Magnifying the plane does not lose the pointer.

    The items are drawn on a layer carrying the pan-and-zoom transform, so what
    is painted and what is hit-tested are two different mappings that have to
    agree. They only do if egui is told about the transform and every screen
    answer the surface gives runs through it.
*/
#[test]
fn a_zoomed_plane_still_hands_the_pointer_to_its_items() {
    let mut h = Harness::new(Arc::new(InteractionBox::sensing(false, true, false)));
    let button = h.child();

    // Magnify about the item's top-left corner.
    let corner = {
        let r =
            h.ws.send_request(h.canvas, NodeScreenRect { node: h.item() })
                .flatten()
                .expect("the item is on screen");
        egui::pos2(r.min.x, r.min.y)
    };
    h.move_to(corner);
    h.frame(vec![
        egui::Event::PointerMoved(corner),
        egui::Event::Zoom(2.0),
    ]);

    h.click(h.item_centre());
    assert!(
        h.ws.send_request(button, WasClicked).unwrap_or(false),
        "the click reached the item on the magnified plane"
    );
}

/**
    A canvas inside a canvas item draws where it was put.

    egui gives a layer one transform to the screen and does not compose them, so
    a nested surface — a lambda's result is one — has to fold in the frame it was
    drawn into. Without that it lays its items out relative to a point that is
    not where it is, and they appear elsewhere on the window entirely, clipped to
    a box in the wrong place.
*/
#[test]
fn a_nested_surface_draws_inside_the_item_holding_it() {
    use dex_nodes::layouts::canvas::nodes::StaticCanvasItem;

    // The outer plane holds one item, and that item is a whole canvas of its own
    // with a single marker on it.
    let mut h = Harness::new(Arc::new(dex_nodes::primitives::nothing::Nothing));
    let inner = Canvas::build(h.ws.action_handle());
    let mark = h.ws.insert_node_now(Rect::new(30.0, 30.0, BLOCK)).erase();
    let placed = StaticCanvasItem::build(
        h.ws.action_handle(),
        mark,
        Vector { x: 10.0, y: 10.0 },
        Vector { x: 30.0, y: 30.0 },
    );
    h.ws.submit_action(
        inner,
        "place",
        AdoptCanvasNode {
            node: placed.erase(),
            layer: Layer::Midground,
        },
    );
    h.ws.process_pending();
    h.ws.submit_action(
        h.canvas,
        "swap in the nested surface",
        SwapCanvasItem {
            old: h.item(),
            child: Arc::new(dex_nodes::layouts::mirror::Mirror::new(inner.erase())),
            pos: PLACE,
            size: ITEM_SIZE,
        },
    );
    h.ws.process_pending();

    for _ in 0..3 {
        h.frame(vec![]);
    }
    // Magnify the outer plane, so the frame the nested surface is drawn into is
    // plainly not the screen's.
    let corner = egui::pos2(40.0, 40.0);
    h.move_to(corner);
    h.frame(vec![
        egui::Event::PointerMoved(corner),
        egui::Event::Zoom(2.0),
    ]);
    h.frame(vec![]);

    let outer_item =
        h.ws.send_request(h.canvas, NodeScreenRect { node: h.item() })
            .flatten()
            .expect("the item is on screen");
    let at = block_top(&h.frame(vec![])).expect("the nested surface painted its marker");

    assert!(
        at >= outer_item.min.y - 1.0 && at <= outer_item.max.y + 1.0,
        "the nested surface painted at y {at}, outside the item it sits in \
         ({} to {})",
        outer_item.min.y,
        outer_item.max.y
    );
}

/**
    Zoom means the same thing in a preview as it does filling the window.

    A surface is shown at more than one size, and its magnification is measured
    against the room a whole desktop gets rather than against whatever box it
    happens to be in. So a surface drawn into a quarter of that room shows the
    same span of its plane at a quarter of the size — the same picture, smaller —
    instead of a quarter of the picture.
*/
#[test]
fn a_surface_shows_the_same_span_whatever_box_it_is_in() {
    use dex_nodes::layouts::canvas::layout::{CanvasZoom, set_canvas_reference};

    let mut h = Harness::new(Arc::new(dex_nodes::primitives::nothing::Nothing));

    // Life size is the whole screen: drawn into it, the plane is at its zoom.
    let _ = h.ctx.clone().run_ui(egui::RawInput::default(), |c| {
        set_canvas_reference(
            c,
            Vector {
                x: SCREEN.x,
                y: SCREEN.y,
            },
        );
    });
    h.frame(vec![]);
    let full = h.ws.send_request(h.canvas, CanvasZoom).expect("a zoom");
    assert!(
        (full - 1.0).abs() < 1e-3,
        "a surface filling the reference draws at life size, not {full}"
    );
    let wide = h.item_rect().size().x;

    // Now say a desktop is four times as wide as this surface's box.
    let _ = h.ctx.clone().run_ui(egui::RawInput::default(), |c| {
        set_canvas_reference(
            c,
            Vector {
                x: SCREEN.x * 4.0,
                y: SCREEN.y * 4.0,
            },
        );
    });
    h.frame(vec![]);
    let preview = h.ws.send_request(h.canvas, CanvasZoom).expect("a zoom");
    assert!(
        (preview - 0.25).abs() < 1e-3,
        "a quarter-sized box draws at a quarter, not {preview}"
    );
    let narrow = h.item_rect().size().x;
    assert!(
        (narrow - wide * 0.25).abs() < 1.0,
        "and the item shrinks with it: {narrow} against {wide} at full size"
    );
}

/// A drag sensor pinned in a surface's foreground reports the drag.
///
/// A foreground is where a plot puts a pad to scrub, precisely because the plane
/// itself owns dragging: the pad has to be able to take a drag out from under
/// the pan.
#[test]
fn a_foreground_pad_takes_the_drag_from_the_pan() {
    let mut h = Harness::new(Arc::new(dex_nodes::primitives::nothing::Nothing));
    let sensor =
        h.ws.insert_node_now(InteractionBox::sensing(false, false, true));
    let pad = h.ws.insert_node_now(PlacedSensor {
        sensor,
        at: Vector { x: 40.0, y: 400.0 },
        size: Vector { x: 150.0, y: 40.0 },
    });
    h.ws.submit_action(
        h.canvas,
        "pad",
        AdoptCanvasNode {
            node: pad.erase(),
            layer: Layer::Foreground,
        },
    );
    h.ws.process_pending();
    h.frame(vec![]);

    let before =
        h.ws.send_request(h.canvas, CanvasViewOrigin)
            .expect("a view origin");
    // Read while the drag is still in flight: a sensor reports the movement of
    // the frame it happened on, not a total once the button is up.
    let from = egui::pos2(100.0, 420.0);
    let moved = h.drag(from, egui::pos2(160.0, 420.0), |h| {
        h.ws.send_request(sensor, WasDragged).flatten()
    });
    assert!(
        moved.iter().map(|d| d.x).sum::<f32>() > 10.0,
        "the pad saw the drag it was put there for"
    );
    let after =
        h.ws.send_request(h.canvas, CanvasViewOrigin)
            .expect("a view origin");
    assert!(
        (after.x - before.x).abs() < 0.5 && (after.y - before.y).abs() < 0.5,
        "and the plane did not pan under it: ({}, {}) to ({}, {})",
        before.x,
        before.y,
        after.x,
        after.y
    );
}

/// A sensor placed at a fixed spot in whatever band it is drawn in.
#[utils::portable]
struct PlacedSensor {
    sensor: NodeUid<InteractionBox>,
    at: Vector,
    size: Vector,
}

#[utils::dynamic_node(skip)]
impl Node for PlacedSensor {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Pad".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        ctx.draw_workspace_node(
            self.sensor.erase(),
            DrawConstraints {
                pos: ctx.constraints.pos + self.at,
                x: Some(AxisConstraint::Exactly(self.size.x)),
                y: Some(AxisConstraint::Exactly(self.size.y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );
        DrawResult::Complete { region: None }
    }
}

defhandlers! { PlacedSensor {} }
