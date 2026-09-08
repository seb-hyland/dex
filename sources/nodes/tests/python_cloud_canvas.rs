//! Exercises `examples/cloud_canvas.py`: a transform that returns a whole
//! *surface*, which is the thing a desktop shows.
//!
//! The example exists to make one claim reachable — that a canvas a script
//! built can be lifted onto a tab of its own — so that is what these check,
//! along with the sky actually painting. A Python exception mid-draw is painted
//! as an error rather than raised, so a backdrop that quietly stopped would
//! look much like a canvas nobody put anything on.

use dex_core::prelude::*;
use dex_nodes::composites::button::Button;
use dex_nodes::layouts::canvas::layout::{Canvas, CanvasChildren, CanvasLayerNodes, Layer};
use dex_nodes::layouts::inspector::PlacementCommands;
use dex_nodes::scripting::{ScriptOutput, run_script};

const CLOUDS: &str = include_str!("../../../examples/cloud_canvas.py");
const SCREEN: egui::Vec2 = egui::vec2(640.0, 440.0);
/// The surface is drawn *inset* in the window, as the app's own sidebar insets
/// it. A band that misplaces itself by its own origin is exactly right at zero.
const INSET: egui::Vec2 = egui::vec2(120.0, 60.0);

/// Where the surface is drawn, and the window it is drawn in.
fn pane() -> (egui::Rect, egui::Rect) {
    (
        egui::Rect::from_min_size(egui::pos2(INSET.x, INSET.y), SCREEN),
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN + INSET * 2.0),
    )
}

/// Run the example as a lambda would, and give back the surface it built.
fn built() -> (Workspace, NodeUid) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let graph = GraphSnapshot::capture(&ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let canvas = match run_script(CLOUDS, "", &handle, &[], graph) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the example returns the surface it built"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    (ws, canvas)
}

/// What it hands back is a surface, with the sky behind what stands on it.
#[test]
fn the_example_builds_a_surface_with_a_backdrop() {
    let (ws, canvas) = built();
    assert!(
        ws.get_node(canvas)
            .is_some_and(|node| (*node).as_any_ref().is::<Canvas>()),
        "a canvas, not a picture of one"
    );

    let background = ws
        .send_request(
            canvas.cast::<Canvas>(),
            CanvasLayerNodes {
                layer: Layer::Background,
            },
        )
        .unwrap_or_default();
    assert_eq!(background.len(), 1, "the sky is one background member");
    assert_eq!(
        ws.send_request(canvas.cast::<Canvas>(), CanvasChildren)
            .unwrap_or_default()
            .len(),
        2,
        "and two things stand on it"
    );
}

/// Draw the surface until it has settled, and hand back what it painted.
///
/// Several frames: egui's first pass over a layout it has not seen is a sizing
/// pass, and it fades a new one in over the few after that.
fn painted(ws: &mut Workspace, ctx: &egui::Context) -> Vec<egui::epaint::ClippedShape> {
    let (pane, window) = pane();
    let mut shapes = Vec::new();
    for _ in 0..12 {
        shapes = ctx
            .clone()
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(window),
                    ..Default::default()
                },
                |c| {
                    egui::CentralPanel::default().show(c, |ui| {
                        ws.draw_frame(ui, pane);
                    });
                },
            )
            .shapes;
    }
    shapes
}

/// Walk the frame's meshes.
fn meshes(shape: &egui::Shape, found: &mut impl FnMut(&egui::Mesh)) {
    match shape {
        egui::Shape::Vec(inner) => inner.iter().for_each(|s| meshes(s, found)),
        egui::Shape::Mesh(mesh) => found(mesh),
        _ => {}
    }
}

/// A workspace showing the surface, and the context it was drawn with.
fn shown() -> (Workspace, egui::Context) {
    let (mut ws, canvas) = built();
    ws.set_root(canvas);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    (ws, ctx)
}

/// It paints, which is the only way to know the backdrop ran at all.
#[test]
fn the_sky_paints() {
    let (mut ws, ctx) = shown();
    let shapes = painted(&mut ws, &ctx);

    let mut triangles = 0;
    for clipped in &shapes {
        meshes(&clipped.shape, &mut |mesh| {
            triangles += mesh.indices.len() / 3;
        });
    }
    assert!(
        triangles > 100,
        "the sky and its cirrus painted: {triangles} triangles"
    );
}

/// And it covers the viewport it was handed, corner to corner.
///
/// A path is painted from the position its constraints carry, so a background
/// that adds its own origin to its points as well lands at twice the origin —
/// and paints a picture of a viewport in the corner of one. The widest mesh in
/// the frame is the sky itself; nothing else here is anywhere near its size.
#[test]
fn the_sky_covers_the_whole_viewport() {
    let (mut ws, ctx) = shown();
    let shapes = painted(&mut ws, &ctx);

    let mut sky = egui::Rect::NOTHING;
    for clipped in &shapes {
        meshes(&clipped.shape, &mut |mesh| {
            let bounds = mesh.calc_bounds();
            if bounds.area() > sky.area() {
                sky = bounds;
            }
        });
    }
    let (screen, _window) = pane();
    assert!(
        (sky.min.x - screen.min.x).abs() < 1.0
            && (sky.min.y - screen.min.y).abs() < 1.0
            && (sky.max.x - screen.max.x).abs() < 1.0
            && (sky.max.y - screen.max.y).abs() < 1.0,
        "the sky spans {sky:?}, and the window is {screen:?}"
    );
}

/// And the whole point: the surface it computed can be taken to a desktop.
#[test]
fn the_surface_it_computed_can_be_lifted_onto_a_desktop() {
    let (ws, canvas) = built();
    let commands = PlacementCommands::for_result(&ws, canvas, Vector { x: 320.0, y: 240.0 });
    // Wait for the queued build, then read the rows it offers.
    let mut ws = ws;
    ws.process_pending();

    let mut labels = Vec::new();
    let mut queue = vec![commands.erase()];
    let mut seen = std::collections::HashSet::new();
    while let Some(uid) = queue.pop() {
        if !seen.insert(uid) {
            continue;
        }
        let Some(node) = ws.get_node(uid) else {
            continue;
        };
        if let Some(button) = (*node).as_any_ref().downcast_ref::<Button>() {
            labels.push(button.label.text.clone());
            continue;
        }
        node.owned_refs(&mut |child| queue.push(child));
    }
    assert!(
        labels.contains(&"New desktop".to_owned()),
        "a computed surface is offered one: {labels:?}"
    );
}
