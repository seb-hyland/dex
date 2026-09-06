//! Exercises the canvas wrapping in `examples/superphylogeny.py`.
//!
//! The tree drawing itself needs a wired table and, to drill in, the network —
//! neither of which belongs in a unit test. What this pins is the part the
//! zoomable-canvas change added: `build` hands its tree to `_on_canvas`, which
//! puts it on a pan/zoom `Canvas` as the one midground item, and puts nothing in
//! front of it — the tree carries its own labelled clade ring, so a second key
//! pinned in the corner only covered the picture it was meant to explain. A
//! stand-in stands for the tree, so the wiring is what is under test.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{Canvas, CanvasLayerNodes, Layer};
use dex_nodes::scripting::{ScriptOutput, run_script};

const SUPERPHYLO: &str = include_str!("../../../examples/superphylogeny.py");

/// Run `body` as the example's `transform`, returning the id it hands back.
fn run(ws: &mut Workspace, body: &str) -> NodeUid {
    let source = format!("{SUPERPHYLO}\ndef transform():\n{body}");
    let graph = GraphSnapshot::capture(ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let uid = match run_script(&source, "", &handle, &[], graph) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the example returns a handle to the node it built"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    ws.set_root(uid);
    ws.process_pending();
    uid
}

/// The nodes in one band of the canvas `uid` names.
fn band(ws: &Workspace, uid: NodeUid, layer: Layer) -> Vec<NodeUid> {
    ws.send_request(uid.cast::<Canvas>(), CanvasLayerNodes { layer })
        .unwrap_or_default()
}

/// The name a live node gives itself.
fn name(ws: &Workspace, uid: NodeUid) -> String {
    ws.get_node(uid)
        .expect("the node is live")
        .type_name(NodeContext {
            id: uid,
            workspace: ws,
        })
}

/// Run the example, but with a `transform` that stands a plain label in for the
/// tree and wraps it exactly as the real `build` does. Returns the live id of
/// the canvas it produced, set as the workspace root.
fn build_canvas(ws: &mut Workspace) -> NodeUid {
    // `_on_canvas` returns the canvas's own uid, so the script's output is a
    // handle to a node it built, not a fresh node.
    run(
        ws,
        concat!(
            "    stand_in = dex.ws.insert_node_dyn(dex.Label.new('tree'))\n",
            "    return _on_canvas(dex.ws, stand_in)\n",
        ),
    )
}

/// The tree lands on a canvas as one midground item, with nothing over it.
#[test]
fn the_tree_is_placed_on_a_canvas_of_its_own() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let canvas_id = build_canvas(&mut ws);

    assert_eq!(
        ws.get_node(canvas_id)
            .expect("the canvas is live")
            .type_name(NodeContext {
                id: canvas_id,
                workspace: &ws,
            }),
        "Phylogeny",
        "the plane says what is on it, not that it is a plane"
    );

    assert_eq!(
        band(&ws, canvas_id, Layer::Midground).len(),
        1,
        "the tree is the one item on the plane"
    );
    assert!(
        band(&ws, canvas_id, Layer::Foreground).is_empty()
            && band(&ws, canvas_id, Layer::Background).is_empty(),
        "and nothing is pinned over or under it"
    );
}

/**
    A structure gets a plane too, named after itself, with its readout in front.

    Dragging the structure turns it, so the plane's own drag is given up; what
    the plane still does is magnify, and it does that against a whole desktop —
    so the same view seen in a preview is the same picture, scaled down. The
    readout that says which atom is selected is chrome and belongs in front,
    where it stays put and life-size.
*/
#[test]
fn a_structure_is_placed_on_a_named_plane_with_its_readout_in_front() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let canvas_id = run(
        &mut ws,
        concat!(
            "    pdb = 'ATOM      1  CA  ALA A   1      ",
            "11.000  13.000  10.000  1.00  0.00           C\\n'\n",
            "    return build_protein(dex.ws, pdb)\n",
        ),
    );

    assert_eq!(
        name(&ws, canvas_id),
        "Structure",
        "the plane is named after what is on it"
    );
    let items = band(&ws, canvas_id, Layer::Midground);
    assert_eq!(items.len(), 1, "the structure is the one item on it");
    assert_eq!(name(&ws, items[0]), "A PDB Viewer");

    let chrome = band(&ws, canvas_id, Layer::Foreground);
    assert_eq!(
        chrome.len(),
        1,
        "and the readout is the one piece of chrome"
    );
    assert_eq!(
        name(&ws, chrome[0]),
        "Atom Readout",
        "which is where what-is-selected is said"
    );
}
