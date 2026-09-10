//! What a surface holds still when the box it is drawn in changes shape.
//!
//! The middle. Not the top-left corner, which is what it used to be and which
//! is anchored to something that moves: opening the sidebar both narrows the
//! content area and starts it further right, so a plane pinned to that corner
//! slid across the screen and fell off the far edge. Anchored at the middle, a
//! box that changes size or aspect crops and reveals evenly on both sides, and
//! whatever you were looking at is still in front of you.
//!
//! Which is the difference between a canvas you work on and a canvas you can
//! present from: a desktop shown at two different window sizes is the same
//! picture, not two different crops of one.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{
    AddCanvasItem, Canvas, CanvasChildren, CanvasViewOrigin, NodeScreenRect,
};
use dex_nodes::layouts::canvas::nodes::{CanvasItemBounds, NudgeCanvasItem};
use dex_nodes::primitives::text::Label;

/// Where the item is parked, well away from any corner or centre so that a
/// coordinate coming out right is not a coincidence.
const MARK: Vector = Vector { x: 220.0, y: 140.0 };
const ITEM: Vector = Vector { x: 40.0, y: 20.0 };

struct Harness {
    ws: Workspace,
    ctx: egui::Context,
    canvas: NodeUid<Canvas>,
}

impl Harness {
    fn new() -> Harness {
        dex_nodes::scripting::init_python();
        let mut ws = Workspace::new_empty();
        let canvas = Canvas::build(ws.action_handle());
        ws.set_root(canvas.erase());
        ws.process_pending();

        let ctx = egui::Context::default();
        dex_nodes::fonts::install_fonts(&ctx);
        let mut h = Harness { ws, ctx, canvas };
        // A surface centres a new item in the viewport, which it only knows
        // once it has drawn.
        h.draw(egui::pos2(0.0, 0.0), egui::vec2(900.0, 640.0));
        h.ws.submit_action(
            canvas,
            "item",
            AddCanvasItem {
                child: Arc::new(Label::new("here".to_owned())),
                size: ITEM,
            },
        );
        h.ws.process_pending();
        h.draw(egui::pos2(0.0, 0.0), egui::vec2(900.0, 640.0));

        let item = h.item();
        let at = h
            .ws
            .send_request(item, CanvasItemBounds)
            .expect("the item reports its bounds")
            .min
            .to_vector();
        h.ws
            .submit_action(item, "place", NudgeCanvasItem { delta: MARK - at });
        h.ws.process_pending();
        h.draw(egui::pos2(0.0, 0.0), egui::vec2(900.0, 640.0));
        h
    }

    /// Draw one frame into a box of the given origin and size — which is what a
    /// sidebar opening does to a desktop: it moves and it shrinks.
    fn draw(&mut self, at: egui::Pos2, size: egui::Vec2) {
        let rect = egui::Rect::from_min_size(at, size);
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1600.0, 900.0));
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let ws = &mut self.ws;
        self.ctx.clone().run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ws.draw_frame(ui, rect);
            });
        });
    }

    fn item(&self) -> NodeUid {
        *self
            .ws
            .send_request(self.canvas, CanvasChildren)
            .unwrap_or_default()
            .first()
            .expect("the item was added")
    }

    /// Where the item's middle sits on screen.
    fn item_centre(&self) -> ScreenPos {
        let r: ScreenRegion = self
            .ws
            .send_request(self.canvas, NodeScreenRect { node: self.item() })
            .flatten()
            .expect("the item is on screen");
        r.min + r.size() / 2.0
    }
}

/// How far the item is from the middle of the box it is drawn in.
fn from_centre(at: egui::Pos2, size: egui::Vec2, item: ScreenPos) -> Vector {
    Vector {
        x: item.x - (at.x + size.x / 2.0),
        y: item.y - (at.y + size.y / 2.0),
    }
}

/**
    A box that narrows and moves keeps the picture where it was.

    This is the sidebar, exactly: the content area both loses width and starts
    further right. Under the old anchor the item moved with the corner, by the
    whole width of the sidebar.
*/
#[test]
fn opening_a_sidebar_leaves_the_view_where_it_was() {
    let mut h = Harness::new();

    let (wide_at, wide) = (egui::pos2(0.0, 0.0), egui::vec2(900.0, 640.0));
    h.draw(wide_at, wide);
    let before = from_centre(wide_at, wide, h.item_centre());

    // The sidebar opens: 300px of it, taken off the left.
    let (narrow_at, narrow) = (egui::pos2(300.0, 0.0), egui::vec2(600.0, 640.0));
    h.draw(narrow_at, narrow);
    let after = from_centre(narrow_at, narrow, h.item_centre());

    assert!(
        (after.x - before.x).abs() < 0.5 && (after.y - before.y).abs() < 0.5,
        "the item should sit the same distance from the middle: ({}, {}) became ({}, {})",
        before.x,
        before.y,
        after.x,
        after.y
    );
}

/**
    A box that changes aspect crops evenly rather than off one side.

    Squashing the height alone must not move anything sideways, and must take
    the same amount off the top as off the bottom.
*/
#[test]
fn a_change_of_aspect_crops_from_both_sides() {
    let mut h = Harness::new();

    let (at, tall) = (egui::pos2(0.0, 0.0), egui::vec2(900.0, 640.0));
    h.draw(at, tall);
    let before = from_centre(at, tall, h.item_centre());

    let short = egui::vec2(900.0, 360.0);
    h.draw(at, short);
    let after = from_centre(at, short, h.item_centre());

    assert!(
        (after.x - before.x).abs() < 0.5 && (after.y - before.y).abs() < 0.5,
        "changing the aspect should not move the picture within the box: ({}, {}) became ({}, {})",
        before.x,
        before.y,
        after.x,
        after.y
    );
}

/**
    The corner is derived, and says so.

    `CanvasViewOrigin` still publishes the canvas point at the *top-left* — the
    prelude's backgrounds map their own points from it — so it must move when
    the box does, by exactly half of what the box gained or lost. If it came
    back unchanged, the surface would still be corner-anchored and the tests
    above would be passing for some other reason.
*/
#[test]
fn the_published_origin_follows_the_box() {
    let mut h = Harness::new();

    let (at, wide) = (egui::pos2(0.0, 0.0), egui::vec2(900.0, 640.0));
    h.draw(at, wide);
    let before = h
        .ws
        .send_request(h.canvas, CanvasViewOrigin)
        .expect("an origin");

    let narrow = egui::vec2(600.0, 640.0);
    h.draw(at, narrow);
    let after = h
        .ws
        .send_request(h.canvas, CanvasViewOrigin)
        .expect("an origin");

    // 300 narrower, so the left edge sits 150 further into the plane.
    assert!(
        (after.x - before.x - 150.0).abs() < 0.5,
        "the left edge should move in by half the width lost: {} to {}",
        before.x,
        after.x
    );
    assert!(
        (after.y - before.y).abs() < 0.5,
        "and the top edge should not move at all: {} to {}",
        before.y,
        after.y
    );
}
