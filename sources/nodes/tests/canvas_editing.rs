//! On-canvas shape editing, driven through real frames with a real pointer:
//! who a drag actually grabs depends on egui's hit-testing, so assert it end to
//! end rather than trusting the arithmetic.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{AddCanvasItem, CanvasChildren, NodeScreenRect};
use dex_nodes::layouts::canvas::nodes::{CanvasItemBounds, CanvasNodeChild, editors::PathAnchorOrigin};
use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
use dex_nodes::primitives::shapes::{Anchor, GetAnchors, Path, SetAnchors};

const SCREEN: egui::Vec2 = egui::vec2(1200.0, 900.0);

struct Harness {
    ws: Workspace,
    ctx: egui::Context,
    pos: egui::Pos2,
}

impl Harness {
    /// A workspace with one line on the canvas, settled over a few frames.
    fn with_a_line() -> (Self, NodeUid, NodeUid) {
        dex_nodes::scripting::init_python();
        let mut h = Self {
            ws: Desktops::new_workspace(),
            ctx: egui::Context::default(),
            pos: egui::pos2(-100.0, -100.0),
        };
        let root = h.ws.root();
        h.move_to(egui::pos2(-100.0, -100.0));

        h.ws.submit_action_dyn(Action {
            dest: root,
            description: "line".into(),
            body: Box::new(AddCanvasItem {
                child: Arc::new(Path::polyline(
                    vec![Vector::new(0.0, 0.0), Vector::new(140.0, 60.0)],
                    Stroke::new(2.5, Color::BLACK),
                )),
                size: Vector { x: 140.0, y: 60.0 },
            }),
        });
        h.ws.process_pending();
        h.move_to(egui::pos2(-100.0, -100.0));
        h.move_to(egui::pos2(-100.0, -100.0));

        let canvas = h
            .ws
            .send_request(root.cast::<Desktops>(), ActiveCanvas)
            .expect("a canvas is active");
        let item = *h
            .ws
            .send_request(canvas, CanvasChildren)
            .unwrap_or_default()
            .first()
            .expect("the line was added");
        let child = h
            .ws
            .send_request(item, CanvasNodeChild)
            .expect("the editor wraps the path");
        (h, item, child)
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let input = egui::RawInput {
            screen_rect: Some(rect),
            events,
            ..Default::default()
        };
        let ws = &mut self.ws;
        let _ = self.ctx.clone().run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ws.draw_frame(ui, rect);
            });
        });
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

    /// Press at `from`, drag to `to` in `steps` jumps, release.
    fn drag(&mut self, from: egui::Pos2, to: egui::Pos2, steps: u32) {
        self.move_to(from);
        self.move_to(from);
        self.button(true);
        for s in 1..=steps {
            let t = s as f32 / steps as f32;
            self.move_to(egui::pos2(
                from.x + (to.x - from.x) * t,
                from.y + (to.y - from.y) * t,
            ));
        }
        self.button(false);
    }

    /// Where the canvas draws its own origin, on screen.
    fn canvas_origin(&self, item: NodeUid) -> egui::Vec2 {
        let root = self.ws.root();
        let rect = self
            .ws
            .send_request(root, NodeScreenRect { node: item })
            .flatten()
            .expect("the item has an on-screen region");
        let bounds = self
            .ws
            .send_request(item, CanvasItemBounds)
            .expect("the item answers the canvas-item protocol");
        egui::vec2(rect.min.x - bounds.min.x, rect.min.y - bounds.min.y)
    }

    /// The screen position of a point in the shape's own anchor space.
    fn screen_of(&self, item: NodeUid, local: Vector) -> egui::Pos2 {
        let origin = self.canvas_origin(item);
        let pos = self
            .ws
            .send_request(item, PathAnchorOrigin)
            .expect("the editor reports its anchor origin");
        egui::pos2(origin.x + pos.x + local.x, origin.y + pos.y + local.y)
    }
}

fn points(ws: &Workspace, child: NodeUid) -> Vec<(f32, f32)> {
    ws.send_request(child, GetAnchors)
        .unwrap_or_default()
        .iter()
        .map(|a: &Anchor| (a.pos.x, a.pos.y))
        .collect()
}

/// A drag is reported a frame *after* the press, by which time a quick flick has
/// carried the pointer well clear of what it grabbed. Deciding the grab from
/// that moved position dropped the vertex and moved the whole shape instead, so
/// picking a point up was a matter of dragging slowly enough.
#[test]
fn a_flicked_drag_still_grabs_the_vertex_it_started_on() {
    let (mut h, item, child) = Harness::with_a_line();
    let vertex = h.screen_of(item, Vector::new(0.0, 0.0));
    let origin_before = h.canvas_origin(item);

    // Two frames to cross 120px: far more than the 9px grab radius per frame.
    h.drag(vertex, egui::pos2(vertex.x + 120.0, vertex.y + 120.0), 2);

    assert_eq!(
        points(&h.ws, child),
        vec![(120.0, 120.0), (140.0, 60.0)],
        "the grabbed vertex moved, and only it"
    );
    let origin_after = h.canvas_origin(item);
    assert_eq!(
        (origin_after.x, origin_after.y),
        (origin_before.x, origin_before.y),
        "and the canvas did not pan"
    );
}

/// A Bézier control point sits outside the bounds of the vertices themselves,
/// so the sensor that hit-tests it has to reach past them — otherwise the press
/// falls through to the canvas background and pans the surface.
#[test]
fn a_control_point_outside_the_vertex_bounds_is_still_grabbable() {
    let (mut h, item, child) = Harness::with_a_line();

    // Curve the first vertex, its handle reaching 80px above every vertex.
    let handle = Vector::new(0.0, -80.0);
    h.ws.submit_action(child, "curve", SetAnchors {
        anchors: vec![
            Anchor::smooth(Vector::new(0.0, 0.0), handle),
            Anchor::corner(Vector::new(140.0, 60.0)),
        ],
    });
    h.ws.process_pending();
    h.move_to(egui::pos2(-100.0, -100.0));
    h.move_to(egui::pos2(-100.0, -100.0));

    let grip = h.screen_of(item, handle);
    let origin_before = h.canvas_origin(item);
    h.drag(grip, egui::pos2(grip.x + 40.0, grip.y), 2);

    let out = h
        .ws
        .send_request(child, GetAnchors)
        .unwrap_or_default()
        .first()
        .and_then(|a: &Anchor| a.out_handle)
        .expect("the vertex is still smooth");
    assert_eq!((out.x, out.y), (40.0, -80.0), "the control point moved");
    assert_eq!(
        points(&h.ws, child),
        vec![(0.0, 0.0), (140.0, 60.0)],
        "and the vertices stayed put"
    );
    let origin_after = h.canvas_origin(item);
    assert_eq!(
        (origin_before.x, origin_before.y),
        (origin_after.x, origin_after.y),
        "and the canvas did not pan"
    );
}
