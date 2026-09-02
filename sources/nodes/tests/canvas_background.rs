//! What a canvas paints under its items.
//!
//! A background is drawn the whole visible area in screen coordinates and asks
//! the surface where that area sits on the plane, so the two halves have to
//! agree: a background painting at a canvas-space point must land exactly where
//! an item placed at that point does, and must keep landing there across a pan.

use dex_core::prelude::*;
use dex_nodes::{
    layouts::canvas::{
        layout::{
            AddCanvasItem, AdoptCanvasNode, Canvas, CanvasChildren, CanvasLayerNodes,
            CanvasViewOrigin, Layer, NodeScreenRect, RemoveCanvasItem,
        },
        nodes::CanvasItemBounds,
    },
    primitives::{nothing::Nothing, shapes::Circle, text::Label},
};

const SCREEN: egui::Vec2 = egui::vec2(900.0, 640.0);
/// The canvas-space point the probe marks, and where the item is placed.
const MARK: Vector = Vector { x: 260.0, y: 180.0 };
const MARK_RADIUS: f32 = 6.0;

/**
    A background that marks one canvas-space point.

    This is the whole protocol in one node: it holds a reference back to the
    surface, asks it for the view origin, and maps its own coordinate through
    the mapping that origin defines.
*/
#[utils::portable]
struct Probe {
    /// The surface this is a background of. A reference: the canvas owns the
    /// background, not the other way round.
    #[uid_ref]
    canvas: NodeUid<Canvas>,
    /// The canvas-space point to mark.
    point: Vector,
}

#[utils::dynamic_node(skip)]
impl Node for Probe {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Probe".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let origin = ctx
            .node
            .workspace
            .send_request(self.canvas, CanvasViewOrigin)
            .unwrap_or(Vector::splat(0.0));
        // The documented mapping: a canvas-space point `p` lands on screen at
        // `constraints.pos + (p - origin)`.
        let at = constraints.pos + (self.point - origin);
        let mark = Circle::new(MARK_RADIUS, Color::rgb(255, 0, 0));
        ctx.draw_node(
            &mark,
            DrawConstraints {
                pos: at - Vector::splat(MARK_RADIUS),
                x: None,
                y: None,
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            },
        );
        DrawResult::Complete { region: None }
    }
}

defhandlers! { Probe {} }

struct Harness {
    ws: Workspace,
    ctx: egui::Context,
    canvas: NodeUid<Canvas>,
    pos: egui::Pos2,
}

impl Harness {
    /// A workspace whose root is one canvas, with a probe background and an
    /// item both sitting at `MARK`.
    fn new() -> Harness {
        dex_nodes::scripting::init_python();
        let mut ws = Workspace::new_empty();
        let canvas = Canvas::build(ws.action_handle());
        ws.set_root(canvas.erase());
        ws.process_pending();

        let probe = ws.insert_node_now(Probe {
            canvas,
            point: MARK,
        });
        ws.submit_action(
            canvas,
            "background",
            AdoptCanvasNode {
                node: probe.erase(),
                layer: Layer::Background,
            },
        );
        ws.process_pending();

        let ctx = egui::Context::default();
        dex_nodes::fonts::install_fonts(&ctx);
        let mut h = Harness {
            ws,
            ctx,
            canvas,
            pos: egui::pos2(-100.0, -100.0),
        };
        // The canvas centres a new item in the viewport, which it only knows
        // once it has drawn, so the item is nudged into place afterwards.
        h.frame(vec![]);
        h.ws.submit_action(
            canvas,
            "item",
            AddCanvasItem {
                child: Arc::new(Label::new("here".to_owned())),
                size: Vector { x: 40.0, y: 20.0 },
            },
        );
        h.ws.process_pending();
        h.frame(vec![]);
        h.place_item_at(MARK);
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

    /// Nudge the item until its canvas-space top-left is `target`.
    fn place_item_at(&mut self, target: Vector) {
        use dex_nodes::layouts::canvas::nodes::NudgeCanvasItem;
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

    /// Where the item's top-left corner is on screen, as the canvas maps it.
    fn item_on_screen(&self) -> ScreenPos {
        self.ws
            .send_request(self.canvas, NodeScreenRect { node: self.item() })
            .flatten()
            .expect("the item is on screen")
            .min
    }

    /// Where the probe painted its mark this frame, by its centre.
    fn mark_on_screen(&mut self) -> egui::Pos2 {
        let output = self.frame(vec![]);
        let mut found = None;
        for clipped in &output.shapes {
            // Matched on hue, not on the exact colour: egui fades a panel in
            // over its first frames, and synthetic frames advance no clock, so
            // the mark arrives at whatever opacity that animation is at.
            if let egui::Shape::Circle(circle) = &clipped.shape
                && circle.fill.r() > 0
                && circle.fill.g() == 0
                && circle.fill.b() == 0
            {
                found = Some(circle.center);
            }
        }
        found.expect("the probe painted its mark")
    }

    fn drag(&mut self, from: egui::Pos2, to: egui::Pos2) {
        self.move_to(from);
        self.move_to(from);
        self.button(true);
        for step in 1..=3 {
            let t = step as f32 / 3.0;
            self.move_to(egui::pos2(
                from.x + (to.x - from.x) * t,
                from.y + (to.y - from.y) * t,
            ));
        }
        self.button(false);
        self.frame(vec![]);
    }

    fn move_to(&mut self, p: egui::Pos2) {
        self.pos = p;
        self.frame(vec![egui::Event::PointerMoved(p)]);
    }

    fn button(&mut self, pressed: bool) {
        self.frame(vec![egui::Event::PointerButton {
            pos: self.pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }]);
    }
}

/// The mapping a background is handed puts it in the same place the canvas puts
/// an item, and keeps it there when the surface is panned under both.
#[test]
fn a_background_and_an_item_agree_on_where_a_canvas_point_is() {
    let mut h = Harness::new();

    let item = h.item_on_screen();
    let mark = h.mark_on_screen();
    assert!(
        (mark.x - item.x).abs() < 0.5 && (mark.y - item.y).abs() < 0.5,
        "the mark landed at {mark:?}, the item at ({}, {})",
        item.x,
        item.y
    );

    // Pan the surface by dragging its background, well clear of the item.
    let before = h.item_on_screen();
    h.drag(egui::pos2(700.0, 520.0), egui::pos2(580.0, 450.0));
    let after = h.item_on_screen();
    assert!(
        (after.x - before.x).abs() > 1.0 || (after.y - before.y).abs() > 1.0,
        "the drag panned the surface: the item was at ({}, {}) and is at ({}, {})",
        before.x,
        before.y,
        after.x,
        after.y
    );

    let mark = h.mark_on_screen();
    assert!(
        (mark.x - after.x).abs() < 0.5 && (mark.y - after.y).abs() < 0.5,
        "the mark followed the pan to {mark:?}, not ({}, {})",
        after.x,
        after.y
    );

    // The view origin is the canvas-space coordinate of the visible top-left,
    // which is what moved.
    let origin =
        h.ws.send_request(h.canvas, CanvasViewOrigin)
            .expect("the surface reports its view origin");
    assert!(
        origin.x.abs() > 1.0 || origin.y.abs() > 1.0,
        "a panned surface is no longer showing its own origin: ({}, {})",
        origin.x,
        origin.y
    );
}

/// An unpanned surface shows its own origin, so a background needs no special
/// case for the first frame.
#[test]
fn an_unpanned_surface_shows_the_plane_origin() {
    let mut ws = Workspace::new_empty();
    let canvas = Canvas::build(ws.action_handle());
    ws.process_pending();
    let origin = ws
        .send_request(canvas, CanvasViewOrigin)
        .expect("a surface that has never drawn still answers");
    assert!(
        origin.x == 0.0 && origin.y == 0.0,
        "an undrawn surface sits at its own origin, not ({}, {})",
        origin.x,
        origin.y
    );
}

/// Backgrounds stack in the order they are added, and are the canvas's to keep:
/// removing one deletes it, and so does deleting the surface.
#[test]
fn a_surface_owns_the_backgrounds_it_is_given() {
    let mut ws = Workspace::new_empty();
    let canvas = Canvas::build(ws.action_handle());
    ws.process_pending();

    let first = ws.insert_node_now(Nothing).erase();
    let second = ws.insert_node_now(Nothing).erase();
    for node in [first, second] {
        ws.submit_action(
            canvas,
            "background",
            AdoptCanvasNode {
                node,
                layer: Layer::Background,
            },
        );
    }
    // Adding one twice does not stack it twice.
    ws.submit_action(
        canvas,
        "background",
        AdoptCanvasNode {
            node: first,
            layer: Layer::Background,
        },
    );
    ws.process_pending();
    assert_eq!(
        ws.send_request(
            canvas,
            CanvasLayerNodes {
                layer: Layer::Background
            }
        )
        .unwrap_or_default(),
        vec![first, second],
        "backgrounds stack in the order they were added"
    );

    ws.submit_action(canvas, "drop", RemoveCanvasItem { node: first });
    ws.process_pending();
    assert_eq!(
        ws.send_request(
            canvas,
            CanvasLayerNodes {
                layer: Layer::Background
            }
        )
        .unwrap_or_default(),
        vec![second],
        "the removed background is gone from the stack"
    );
    assert!(ws.get_node(first).is_none(), "and deleted with it");

    ws.delete_node(canvas.erase());
    ws.process_pending();
    assert!(
        ws.get_node(second).is_none(),
        "deleting the surface takes its backgrounds with it"
    );
}
