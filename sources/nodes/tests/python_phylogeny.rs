//! Exercises `examples/circos3.py`: a circular phylogeny whose branches are
//! nodes rather than paint.
//!
//! The point of the example is that the tree is not one drawing. Every branch
//! is its own `Path` in the workspace, so the inspector can address one and
//! recolour it — and that only works if three things hold, which is what is
//! pinned here. The branches have to exist, one per stroke, coloured by the
//! clade they sit in. Their geometry has to reach them, since it depends on a
//! radius nothing knows until the plot is drawn. And each one has to reach the
//! screen through the probe, or there is nothing for a click to land on.

use dex_core::prelude::*;
use dex_core::refs::NodeRefs;
use dex_nodes::primitives::shapes::{GetAnchors, GetStroke, Path, SetPathStrokeColor};
use dex_nodes::scripting::{ScriptOutput, run_script};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;

/// Colours are compared channelwise; `Color` is not `PartialEq`.
fn channels(c: Color) -> (u8, u8, u8) {
    (c.r, c.g, c.b)
}

const CIRCOS3: &str = include_str!("../../../examples/circos3.py");
const SCREEN: egui::Vec2 = egui::vec2(900.0, 900.0);

/// Run the example module and return its namespace, for the pure functions.
fn load<'py>(py: Python<'py>) -> Bound<'py, PyDict> {
    let globals = PyDict::new(py);
    globals
        .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
        .unwrap();
    let src = std::ffi::CString::new(CIRCOS3).unwrap();
    py.run(src.as_c_str(), Some(&globals), Some(&globals))
        .expect("example module runs");
    globals
}

fn eval<T>(py: Python<'_>, globals: &Bound<'_, PyDict>, expr: &str) -> T
where
    T: for<'a, 'py> pyo3::FromPyObject<'a, 'py>,
{
    let code = std::ffi::CString::new(expr).unwrap();
    let value = py
        .eval(code.as_c_str(), Some(globals), None)
        .unwrap_or_else(|e| panic!("`{expr}` evaluates: {e}"));
    match value.extract() {
        Ok(out) => out,
        Err(_) => panic!("`{expr}` has the expected type"),
    }
}

/// Run the example's `transform()` into `ws`, exactly as a lambda would.
///
/// It has to go through a real script run rather than a bare `eval`, because
/// the branch nodes are built against `dex.ws` — which only exists inside a
/// transform.
fn build_phylogeny(ws: &mut Workspace) -> Arc<dyn Node> {
    let graph = GraphSnapshot::capture(ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let built = match run_script(CIRCOS3, "", &handle, &[], graph) {
        Ok(ScriptOutput::Node(node)) => node,
        Ok(_) => panic!("the example returns the phylogeny it built"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    built
}

/// Draw `frames` frames of the whole workspace at `SCREEN`.
///
/// More than one, because the branch geometry is queued as actions the frame
/// that notices the size — so the first frame a size is seen is the one that
/// asks, and a later one is the first to draw the answer.
fn draw(ws: &mut Workspace, frames: usize) {
    let egui_ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&egui_ctx);
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    for _ in 0..frames {
        let input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        let ws = &mut *ws;
        let _ = egui_ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                ws.draw_frame(ui, rect);
            });
        });
        ws.process_pending();
    }
}

/// The uids of the branch paths the phylogeny owns, in tree order.
fn branches(node: &Arc<dyn Node>) -> Vec<NodeUid> {
    let mut out = Vec::new();
    node.owned_refs(&mut |uid| out.push(uid));
    out
}

/// A branch per stroke, each starting in the colour of the clade it sits in.
#[test]
fn every_branch_is_its_own_path_coloured_by_its_clade() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let plot = build_phylogeny(&mut ws);
    let uids = branches(&plot);

    // One node per stroke the tree asks for: the arc under each fork, plus the
    // radial line out to each child.
    let wanted: usize = Python::attach(|py| {
        let globals = load(py);
        eval(
            py,
            &globals,
            "(lambda r: (layout(r), len(branch_inks(r, CLADES)))[1])(parse_newick(NEWICK))",
        )
    });
    assert_eq!(uids.len(), wanted, "a path per branch stroke");
    assert!(wanted > 100, "the tree is a real one, not a sketch");

    // Every one of them really is a path, and the clade colours reached them:
    // a subtree reads as one colour, and the backbone above every clade — which
    // belongs to no clade — stays plain.
    let mut per_colour = std::collections::HashMap::new();
    for uid in &uids {
        // A `GetStroke` only a path answers: the branch is a `Path`, and so
        // it carries a path's inspector.
        let stroke = ws
            .send_request(*uid, GetStroke)
            .expect("a branch is a path");
        *per_colour
            .entry((stroke.color.r, stroke.color.g, stroke.color.b))
            .or_insert(0usize) += 1;
    }
    let clades: Vec<(u8, u8, u8)> = Python::attach(|py| {
        let globals = load(py);
        eval(py, &globals, "list(CLADES.values())")
    });
    for clade in &clades {
        assert!(
            per_colour.get(clade).copied().unwrap_or(0) > 10,
            "the {clade:?} clade colours a subtree, not a single branch"
        );
    }
    let backbone: (u8, u8, u8) = Python::attach(|py| {
        let globals = load(py);
        eval(py, &globals, "BRANCH_INK")
    });
    assert!(
        per_colour.contains_key(&backbone),
        "the branches above every clade belong to none of them"
    );
}

/// The geometry reaches the branch nodes, and each of them reaches the probe.
///
/// Both halves matter and neither implies the other: a branch with no anchors
/// draws nothing to click on, and a branch drawn as paint has anchors nobody
/// can address.
#[test]
fn the_branches_are_sized_to_the_plot_and_land_in_the_inspector() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let plot = build_phylogeny(&mut ws);
    let uids = branches(&plot);

    // Built empty: the radius depends on a box nothing has been given yet.
    for uid in &uids {
        let anchors = ws.send_request(*uid, GetAnchors).unwrap_or_default();
        assert!(anchors.is_empty(), "a branch starts with no geometry");
    }

    let root = ws.insert_node_dyn(plot.clone());
    ws.set_root(root);
    ws.process_pending();
    draw(&mut ws, 4);

    // The tree fills the middle of the plot: `R_LEAF` of the radius, and the
    // radius itself is most of the half-box.
    let (r_leaf, padding): (f32, f32) = Python::attach(|py| {
        let globals = load(py);
        eval(py, &globals, "(R_LEAF, PADDING)")
    });
    let outermost = uids
        .iter()
        .flat_map(|uid| ws.send_request(*uid, GetAnchors).unwrap_or_default())
        .map(|a| a.pos.x.hypot(a.pos.y))
        .fold(0.0f32, f32::max);
    assert!(outermost > 0.0, "the branches were given their geometry");

    let half = SCREEN.x.min(SCREEN.y) / 2.0 - padding;
    assert!(
        outermost > half * r_leaf * 0.75,
        "the tip ring sits at {outermost}, well short of the {} it has room for",
        half * r_leaf
    );
    assert!(
        outermost <= half * r_leaf + 1.0,
        "the tree at {outermost} overruns the {} the bands leave it",
        half * r_leaf
    );

    // Every branch is addressable: the probe knows where each one drew, which
    // is what a click on one has to find.
    let addressable = uids
        .iter()
        .filter(|uid| ws.inspectable_rect(**uid).is_some())
        .count();
    assert_eq!(
        addressable,
        uids.len(),
        "every branch offered itself to the inspector"
    );
}

/// Recolouring one branch recolours that branch, and nothing else.
///
/// This is what the whole arrangement is for, and it is the part a plot that
/// merely paints its tree cannot do: the colour lives in the node, so it
/// survives the next frame instead of being overwritten by the redraw.
#[test]
fn a_branch_keeps_a_colour_chosen_for_it() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let plot = build_phylogeny(&mut ws);
    let uids = branches(&plot);
    let root = ws.insert_node_dyn(plot.clone());
    ws.set_root(root);
    ws.process_pending();
    draw(&mut ws, 4);

    let chosen = Color::rgb(255, 0, 128);
    let target = uids[7];
    let before: Vec<_> = uids
        .iter()
        .map(|uid| ws.send_request(*uid, GetStroke).map(|s| channels(s.color)))
        .collect();
    ws.submit_action(
        target,
        "Set stroke colour",
        SetPathStrokeColor { color: chosen },
    );
    ws.process_pending();
    draw(&mut ws, 3);

    let after: Vec<_> = uids
        .iter()
        .map(|uid| ws.send_request(*uid, GetStroke).map(|s| channels(s.color)))
        .collect();
    let changed: Vec<usize> = (0..uids.len())
        .filter(|i| before[*i] != after[*i])
        .collect();
    assert_eq!(changed, [7], "one branch changed colour, and only that one");
    assert_eq!(
        after[7],
        Some((255, 0, 128)),
        "and it kept the colour across the redraws"
    );
}

/// A path carries its own inspector, which is what makes a branch clickable to
/// any effect. An unfilled one is offered its stroke; there is no interior to
/// colour.
#[test]
fn a_path_inspects_as_its_colours() {
    let mut ws = Workspace::new_empty();
    let line = ws.insert_node(Path::polyline(
        vec![Vector::ZERO, Vector { x: 10.0, y: 10.0 }],
        Stroke::new(1.0, Color::BLACK),
    ));
    ws.process_pending();
    let node = ws.get_node(line.erase()).expect("the path is there");
    let menu = node
        .build_inspector(NodeContext {
            id: line.erase(),
            workspace: &ws,
        })
        .expect("a path offers an inspector");
    ws.process_pending();
    assert!(
        ws.get_node(menu).is_some(),
        "the menu it names was really built"
    );
}
