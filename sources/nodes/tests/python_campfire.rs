//! Exercises `examples/campfire.py`, and the claim it is there to make: a node
//! that reads the clock in `draw` animates, with nothing added to the engine.
//!
//! Two parts. That the drawing actually changes between frames — which is what
//! "it animates" means, and the only way to see it is to paint twice and
//! compare. And that `draw` is reached every frame at all: a Python exception
//! mid-draw is painted as an error rather than raised, so a fire that quietly
//! stopped would look much like one nobody asked to move.

use std::time::Duration;

use dex_core::prelude::*;
use dex_nodes::scripting::{ScriptOutput, run_script};

const CAMPFIRE: &str = include_str!("../../../examples/campfire.py");
const SCREEN: egui::Vec2 = egui::vec2(400.0, 420.0);

/// The node the example's `transform()` returns.
fn campfire() -> Arc<dyn Node> {
    dex_nodes::scripting::init_python();
    let ws = Workspace::new_empty();
    let graph = GraphSnapshot::capture(&ws);
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    match run_script(CAMPFIRE, "", &handle, &[], graph) {
        Ok(ScriptOutput::Node(node)) => node,
        Ok(_) => panic!("the fire is returned as a node"),
        Err(e) => panic!("{e}"),
    }
}

/// A workspace with the fire as its root, drawn into.
fn workspace() -> Workspace {
    let node = campfire();
    let mut ws = Workspace::new_empty();
    let root = ws.action_handle().insert_node_dyn(node);
    ws.process_pending();
    ws.set_root(root);
    ws
}

/**
    Draw `ws` and hand back every shape the frame painted.

    Twice, and the second one is the answer: egui's first pass over a layout it
    has not seen is a sizing pass, which records a placeholder for every shape
    instead of the shape. Reading that one back would say the fire painted
    nothing at all.
*/
fn painted(ws: &mut Workspace, ctx: &egui::Context) -> Vec<egui::epaint::ClippedShape> {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        shapes = ctx
            .clone()
            .run_ui(input, |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    ws.draw_frame(ui, screen);
                });
            })
            .shapes;
    }
    shapes
}

/// Where every mesh vertex landed, rounded — a fingerprint of the drawing that
/// is stable under repainting the same moment twice and not under a new one.
fn fingerprint(shapes: &[egui::epaint::ClippedShape]) -> Vec<(i32, i32)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(i32, i32)>) {
        match shape {
            egui::Shape::Mesh(mesh) => out.extend(
                mesh.vertices
                    .iter()
                    .map(|v| ((v.pos.x * 4.0) as i32, (v.pos.y * 4.0) as i32)),
            ),
            egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in shapes {
        walk(&clipped.shape, &mut out);
    }
    out
}

/// The example runs, and gives back something that draws.
#[test]
fn the_fire_is_a_node_that_paints() {
    let mut ws = workspace();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);

    let shapes = painted(&mut ws, &ctx);
    assert!(
        !fingerprint(&shapes).is_empty(),
        "the fire painted something"
    );
}

/**
    The drawing moves on its own.

    Two frames a little apart, and the vertices are not where they were. That is
    the whole claim the example exists to make: the application asks for the
    next frame as soon as it has one, so `draw` runs continuously, and a `draw`
    that reads the clock is an animation. Nothing subscribes to anything.
*/
#[test]
fn the_drawing_moves_between_frames() {
    let mut ws = workspace();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);

    let first = fingerprint(&painted(&mut ws, &ctx));
    assert!(!first.is_empty(), "the first frame painted");

    // Long enough that the slowest thing on screen has moved, and short
    // enough that the test does not sit about waiting for it.
    std::thread::sleep(Duration::from_millis(350));
    let second = fingerprint(&painted(&mut ws, &ctx));

    // Not the same *count*: a fire's shapes change size as it breathes, and a
    // radial fill is chased with as much subdivision as its size asks for. What
    // matters is that what was drawn is no longer where it was.
    assert!(second.len() > 1000, "the second frame painted a fire too");
    assert_ne!(
        first, second,
        "and it is somewhere else, a third of a second later"
    );
}
