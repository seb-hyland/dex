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

/// It paints, which is the only way to know the backdrop ran at all.
#[test]
fn the_sky_paints() {
    let (mut ws, canvas) = built();
    ws.set_root(canvas);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);

    // Several frames: egui's first pass over a layout it has not seen is a
    // sizing pass, and it fades a new one in over the few after that.
    let mut shapes = Vec::new();
    for _ in 0..12 {
        shapes = ctx
            .clone()
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |c| {
                    egui::CentralPanel::default().show(c, |ui| {
                        ws.draw_frame(ui, screen);
                    });
                },
            )
            .shapes;
    }

    fn meshes(shape: &egui::Shape, found: &mut usize) {
        match shape {
            egui::Shape::Vec(inner) => inner.iter().for_each(|s| meshes(s, found)),
            egui::Shape::Mesh(mesh) => *found += mesh.indices.len() / 3,
            _ => {}
        }
    }
    let mut triangles = 0;
    for clipped in &shapes {
        meshes(&clipped.shape, &mut triangles);
    }
    assert!(
        triangles > 100,
        "the sky and its cirrus painted: {triangles} triangles"
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
