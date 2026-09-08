//! Exercises the planes and panels in `examples/superphylogeny.py`.
//!
//! The drilling itself needs the network, which does not belong in a unit test.
//! What this pins is the wiring the overlay change rests on: the tree and the
//! genome each land on a pan/zoom `Canvas` named after what it holds, with their
//! hover-and-click panel in the plane's *foreground* — no node per tip or per
//! gene, which is the whole point of the change. A tiny inline table and a
//! one-record GenBank string stand in for the wired data, so no fetch runs.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{Canvas, CanvasLayerNodes, Layer};
use dex_nodes::scripting::{ScriptOutput, run_script};

const SUPERPHYLO: &str = include_str!("../../../examples/superphylogeny.py");
const SCREEN: egui::Vec2 = egui::vec2(900.0, 700.0);

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

/// A three-node tree — a root and two leaves — built by the real `build`.
fn build_tree(ws: &mut Workspace) -> NodeUid {
    run(
        ws,
        concat!(
            "    cols = {\n",
            "        'node': [0, 1, 2],\n",
            "        'parent': [None, 0, 0],\n",
            "        'depth': [0, 1, 1],\n",
            "        'is_leaf': [0, 1, 1],\n",
            "        'leaf_order': [None, 0, 1],\n",
            "        'key': ['', 'GB_GCA_1.1', 'GB_GCA_2.1'],\n",
            "        'label': ['root', 'Leaf A', 'Leaf B'],\n",
            "        'phylum': ['', 'Prote', 'Prote'],\n",
            "    }\n",
            "    return build(dex.ws, cols)\n",
        ),
    )
}

fn frame(ws: &mut Workspace, ctx: &egui::Context) -> bool {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let mut any = false;
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let out = ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                ws.draw_frame(ui, screen);
            });
        });
        fn walk(s: &egui::Shape) -> bool {
            match s {
                egui::Shape::Mesh(m) => !m.vertices.is_empty(),
                egui::Shape::Vec(v) => v.iter().any(walk),
                egui::Shape::Circle(_)
                | egui::Shape::LineSegment { .. }
                | egui::Shape::Path(_) => true,
                _ => false,
            }
        }
        any = out.shapes.iter().any(|c| walk(&c.shape));
    }
    any
}

/// The tree lands on a plane named after itself, as one midground item, with its
/// leaf panel — and nothing else — in the foreground.
#[test]
fn the_tree_is_placed_on_a_plane_with_its_leaf_panel_in_front() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let canvas_id = build_tree(&mut ws);

    assert_eq!(
        name(&ws, canvas_id),
        "Phylogeny",
        "the plane says what is on it, not that it is a plane"
    );
    assert_eq!(
        band(&ws, canvas_id, Layer::Midground).len(),
        1,
        "the tree is the one item on the plane"
    );
    let front = band(&ws, canvas_id, Layer::Foreground);
    assert_eq!(front.len(), 1, "its leaf panel is the one thing in front");
    assert_eq!(name(&ws, front[0]), "Leaf Readout");
    assert!(
        band(&ws, canvas_id, Layer::Background).is_empty(),
        "nothing is pinned under it"
    );
}

/// And it paints: the new sensor-and-search draw path runs without raising.
#[test]
fn the_tree_paints() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    build_tree(&mut ws);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    assert!(frame(&mut ws, &ctx), "the tree painted");
}

/// A genome lands on its own named plane, with the key and the gene panel — and
/// nothing per gene — in the foreground.
#[test]
fn a_genome_is_placed_on_a_named_plane_with_its_gene_panel() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let canvas_id = run(
        &mut ws,
        concat!(
            "    gbff = (\n",
            "        'LOCUS       C1  600 bp\\n'\n",
            "        'FEATURES\\n'\n",
            "        '     CDS  1..300\\n'\n",
            "        '                     /gene=\"abcA\"\\n'\n",
            "        '     CDS  complement(320..590)\\n'\n",
            "        '                     /gene=\"xyzB\"\\n'\n",
            "        'ORIGIN\\n'\n",
            "    )\n",
            "    return build_genome_plane(dex.ws, gbff)\n",
        ),
    );

    assert_eq!(name(&ws, canvas_id), "Genome");
    assert_eq!(
        band(&ws, canvas_id, Layer::Midground).len(),
        1,
        "the genome map is the one item on the plane"
    );
    let front: Vec<String> = band(&ws, canvas_id, Layer::Foreground)
        .into_iter()
        .map(|u| name(&ws, u))
        .collect();
    assert!(
        front.iter().any(|n| n == "Genome Key") && front.iter().any(|n| n == "Gene Readout"),
        "the key and the gene panel are in front, got {front:?}"
    );
}

/// A structure gets a plane too, named after itself, with its atom readout in
/// front — unchanged by this refactor, and the pattern the panels follow.
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

    assert_eq!(name(&ws, canvas_id), "Structure");
    let items = band(&ws, canvas_id, Layer::Midground);
    assert_eq!(items.len(), 1, "the structure is the one item on it");
    assert_eq!(name(&ws, items[0]), "A PDB Viewer");

    let chrome = band(&ws, canvas_id, Layer::Foreground);
    assert_eq!(chrome.len(), 1, "and the readout is the one piece of chrome");
    assert_eq!(name(&ws, chrome[0]), "Atom Readout");
}
