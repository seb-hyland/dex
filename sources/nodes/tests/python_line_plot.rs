//! Exercises `examples/line_plot.py`: a picture, and a line that is a shape.
//!
//! The example's claim is not that it draws a line — a `Path` does that — but
//! that the line on the plane is the same editable polygon anything drawn by
//! hand is: a `Path` inside a `PathEditor`, so its vertices can be dragged.
//! The captions are static `Label`s, one `CAPTION =` away from being fields.
//! And the plane opens looking at the picture rather than at its own corner.
//!
//! A Python exception mid-draw is painted as an error rather than raised, so a
//! part that quietly stopped drawing would still leave a plausible-looking
//! plot behind.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{Canvas, CanvasViewOrigin, CanvasZoom};
use dex_nodes::layouts::canvas::nodes::editors::{PathAnchorOrigin, PathEditor};
use dex_nodes::primitives::dynamic::DynamicNode;
use dex_nodes::primitives::shapes::{GetAnchors, Path};
use dex_nodes::primitives::text::{Label, LabelEditable};
use dex_nodes::scripting::{ScriptOutput, run_script};

const PRELUDE: &str = include_str!("../src/default_prelude.py");
const LINE_PLOT: &str = include_str!("../../../examples/line_plot.py");

const SCREEN: egui::Vec2 = egui::vec2(1000.0, 700.0);

/// What the example says the picture and its body measure.
const SIZE: egui::Vec2 = egui::vec2(760.0, 480.0);
const BODY_POS: Vector = Vector { x: 56.0, y: 24.0 };

/// Run the example as a lambda would, and give back the surface it built.
fn built() -> (Workspace, NodeUid) {
    dex_nodes::scripting::init_python();
    // The prelude wants pyarrow available; the repo keeps an environment with
    // it under `demoenv/`, as `render_example.rs` does.
    if pyo3::Python::attach(|py| py.import("pyarrow").is_err()) {
        let demoenv = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demoenv/.venv");
        if demoenv.is_dir() {
            let _ = dex_nodes::settings::set_venv(Some(demoenv));
        }
    }
    let mut ws = Workspace::new_empty();
    let graph = GraphSnapshot::capture(&ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let canvas = match run_script(LINE_PLOT, PRELUDE, &handle, &[], graph) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the example returns the plane it built"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    (ws, canvas)
}

/// What a node calls itself.
fn named(ws: &Workspace, uid: NodeUid) -> String {
    ws.get_node(uid)
        .map(|node| {
            node.type_name(NodeContext {
                id: uid,
                workspace: ws,
            })
        })
        .unwrap_or_default()
}

/// Every node reachable from `root` by ownership, `root` included.
fn reachable(ws: &Workspace, root: NodeUid) -> Vec<NodeUid> {
    let mut seen = std::collections::HashSet::new();
    let mut found = Vec::new();
    let mut queue = vec![root];
    while let Some(uid) = queue.pop() {
        if !seen.insert(uid) {
            continue;
        }
        found.push(uid);
        if let Some(node) = ws.get_node(uid) {
            node.owned_refs(&mut |child| queue.push(child));
        }
    }
    found
}

/// Every node under `root` that is a `T`.
fn all_of<T: Node>(ws: &Workspace, root: NodeUid) -> Vec<NodeUid> {
    reachable(ws, root)
        .into_iter()
        .filter(|uid| {
            ws.get_node(*uid)
                .is_some_and(|node| (*node).as_any_ref().is::<T>())
        })
        .collect()
}

/// The one node under `root` that is a `T`.
fn only<T: Node>(ws: &Workspace, root: NodeUid) -> NodeUid {
    let hits = all_of::<T>(ws, root);
    assert_eq!(hits.len(), 1, "exactly one {}", std::any::type_name::<T>());
    hits[0]
}

/// A workspace showing the plane, and the context it is drawn with.
fn shown() -> (Workspace, NodeUid, egui::Context) {
    let (mut ws, canvas) = built();
    ws.set_root(canvas);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    (ws, canvas, ctx)
}

/// Draw until it has settled. egui's first pass over a layout it has not seen
/// is a sizing pass, and it fades a new one in over the frames after that.
fn paint(ws: &mut Workspace, ctx: &egui::Context) {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    for _ in 0..12 {
        let _ = ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    ws.draw_frame(ui, screen);
                });
            },
        );
        ws.process_pending();
    }
}

/// What it hands back is a plane, with the picture and the line on it.
#[test]
fn the_example_builds_a_plot_and_a_line() {
    let (ws, canvas) = built();
    assert!(
        ws.get_node(canvas)
            .is_some_and(|node| (*node).as_any_ref().is::<Canvas>()),
        "a plane, not a picture of one"
    );

    let plot = only::<DynamicNode>(&ws, canvas);
    assert_eq!(named(&ws, plot), "A Line Plot");
    let owned = {
        let mut found = Vec::new();
        let node = ws.get_node(plot).expect("the plot is there");
        node.owned_refs(&mut |child| found.push(child));
        found
    };
    assert_eq!(owned.len(), 2, "the plot owns its two captions");
}

/// The captions are static labels: readable, not typed into, and one
/// `CAPTION =` away from being fields.
#[test]
fn the_captions_are_static_labels() {
    let (ws, canvas) = built();
    assert_eq!(
        all_of::<Label>(&ws, canvas).len(),
        2,
        "one caption per axis, and both are plain labels"
    );
    assert!(
        all_of::<LabelEditable>(&ws, canvas).is_empty(),
        "nothing on the plane is editable text"
    );
}

/// The line is a real polygon in a real editor, so its vertices can be dragged.
#[test]
fn the_line_is_a_polygon_in_an_editor() {
    let (ws, canvas) = built();
    let editor = only::<PathEditor>(&ws, canvas);
    let path = only::<Path>(&ws, canvas);

    let anchors = ws
        .send_request(path, GetAnchors)
        .expect("the editor wraps a path with anchors");
    assert_eq!(anchors.len(), 6, "one vertex per sample");
    // A polygon rather than a line: a two-point path would edit as a line,
    // with arrows and no way to add a vertex.
    assert!(
        anchors.len() > 2,
        "more than two vertices, so it edits as a polygon"
    );
    // And its anchors are measured from the plot body's corner, so the line
    // and the grid it is drawn over agree about where a value goes.
    let held = ws
        .send_request(editor, PathAnchorOrigin)
        .expect("the editor says what its anchors are measured from");
    assert!(
        (held.x - BODY_POS.x).abs() < 0.5 && (held.y - BODY_POS.y).abs() < 0.5,
        "the line sits at the body's corner: {}, {}",
        held.x,
        held.y
    );
}

/// The plane opens looking at the middle of the picture, not at its own corner.
#[test]
fn the_plane_opens_centred_on_the_picture() {
    let (mut ws, canvas, ctx) = shown();
    paint(&mut ws, &ctx);

    let origin = ws
        .send_request(canvas, CanvasViewOrigin)
        .expect("the plane publishes its corner");
    let zoom = ws.send_request(canvas, CanvasZoom).expect("and its scale");
    // The canvas point in the middle of the viewport, from the corner it
    // publishes and how much plane the viewport covers.
    let middle = Vector {
        x: origin.x + SCREEN.x / zoom / 2.0,
        y: origin.y + SCREEN.y / zoom / 2.0,
    };
    let want = Vector {
        x: SIZE.x / 2.0,
        y: SIZE.y / 2.0,
    };
    assert!(
        (middle.x - want.x).abs() < 1.0 && (middle.y - want.y).abs() < 1.0,
        "the middle of the picture is in the middle of the view: {}, {}",
        middle.x,
        middle.y
    );
}
