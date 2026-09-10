//! Exercises `examples/superphylogeny.py` — the bespoke circular tree whose
//! leaves, genes and atoms drill in through their inspectors.
//!
//! The drilling itself needs the network, which does not belong in a unit test.
//! What this pins is that the three bespoke views — the tree, a genome map, a
//! structure — build from stand-in data and paint. A tiny inline table, a
//! one-record GenBank string and a one-atom PDB stand in for the wired data, so
//! no fetch runs.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{Canvas, CanvasLayerNodes, Layer};
use dex_nodes::scripting::{ScriptOutput, run_script};

const SUPERPHYLO: &str = include_str!("../../../examples/superphylogeny.py");
/// Scripts run under the default prelude in the app, so the tests do too.
const PRELUDE: &str = include_str!("../src/default_prelude.py");
const SCREEN: egui::Vec2 = egui::vec2(900.0, 700.0);

/// The prelude needs pyarrow; the repo keeps an environment with it.
fn has_pyarrow() -> bool {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    dex_nodes::scripting::init_python();
    ONCE.call_once(|| {
        if pyo3::Python::attach(|py| py.import("pyarrow").is_ok()) {
            return;
        }
        let demoenv = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demoenv/.venv");
        if demoenv.is_dir() {
            let _ = dex_nodes::settings::set_venv(Some(demoenv));
        }
    });
    pyo3::Python::attach(|py| py.import("pyarrow").is_ok())
}

/// Run `body` as the example's `transform`, returning the id of the node it
/// builds. `build*` return the node itself; a script may also hand back an id.
fn run(ws: &mut Workspace, body: &str) -> NodeUid {
    let source = format!("{SUPERPHYLO}\ndef transform():\n{body}");
    let graph = GraphSnapshot::capture(ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let uid = match run_script(&source, PRELUDE, &handle, &[], graph) {
        Ok(ScriptOutput::Node(node)) => ws.action_handle().insert_node_dyn(node),
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the example returns a node it built"),
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
                egui::Shape::Circle(_) | egui::Shape::LineSegment { .. } | egui::Shape::Path(_) => {
                    true
                }
                _ => false,
            }
        }
        any = out.shapes.iter().any(|c| walk(&c.shape));
    }
    any
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

/// The tree lands on a pan/zoom plane named after itself, with the bespoke
/// `SuperPhylogeny` renderer as its one item — and it paints.
#[test]
fn the_tree_lands_on_a_pan_zoom_plane_and_paints() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let mut ws = Workspace::new_empty();
    let canvas = build_tree(&mut ws);
    assert_eq!(
        name(&ws, canvas),
        "Phylogeny",
        "the plane says what is on it, not that it is a canvas"
    );
    let mid = band(&ws, canvas, Layer::Midground);
    assert_eq!(mid.len(), 1, "the tree is the one item on the plane");
    assert_eq!(
        name(&ws, mid[0]),
        "A Super Phylogeny",
        "the item is the bespoke renderer, not a prelude plot"
    );
    let fore = band(&ws, canvas, Layer::Foreground);
    assert_eq!(fore.len(), 1, "the genome panel is pinned in the foreground");
    assert_eq!(name(&ws, fore[0]), "Genome Panel");
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    assert!(frame(&mut ws, &ctx), "the tree painted");
}

/// The tree is one node: its tips are marks, not nodes of their own, so a
/// thousand-leaf tree adds no more nodes to the workspace than a two-leaf one.
#[test]
fn the_tips_are_not_nodes() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let mut small = Workspace::new_empty();
    build_tree(&mut small);
    let mut big = Workspace::new_empty();
    run(
        &mut big,
        concat!(
            "    n = 1000\n",
            "    cols = {\n",
            "        'node': list(range(n + 1)),\n",
            "        'parent': [None] + [0] * n,\n",
            "        'depth': [0] + [1] * n,\n",
            "        'is_leaf': [0] + [1] * n,\n",
            "        'leaf_order': [None] + list(range(n)),\n",
            "        'key': [''] + ['GB_GCA_%d.1' % i for i in range(n)],\n",
            "        'label': ['root'] + ['Leaf %d' % i for i in range(n)],\n",
            "        'phylum': [''] + ['Prote'] * n,\n",
            "    }\n",
            "    return build(dex.ws, cols)\n",
        ),
    );
    assert_eq!(small.live_ids().len(), big.live_ids().len());
}

/// A picked tip's explorer is drawn in the foreground panel — whether it has
/// arrived yet or is still only an id the fetch will fill.
#[test]
fn a_picked_tip_shows_its_genome_in_the_panel() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    for result in ["ws.insert_node_dyn(_note('a genome'))", "dex.NodeUid.mint()"] {
        let mut ws = Workspace::new_empty();
        run(
            &mut ws,
            &format!(
                concat!(
                    "    ws = dex.ws\n",
                    "    cols = {{\n",
                    "        'node': [0, 1, 2], 'parent': [None, 0, 0],\n",
                    "        'depth': [0, 1, 1], 'is_leaf': [0, 1, 1],\n",
                    "        'leaf_order': [None, 0, 1],\n",
                    "    }}\n",
                    "    tips = {{1: ('GB_GCA_1.1', 'Leaf A', (1, 2, 3))}}\n",
                    "    sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(True, True, False))\n",
                    "    tree = SuperPhylogeny(cols, tips, sensor)\n",
                    "    tree.selected = 1\n",
                    "    tree.results['GB_GCA_1.1'] = {result}\n",
                    "    body = ws.insert_node_dyn(tree)\n",
                    "    canvas = on_plane(ws, body, (TREE_SIZE, TREE_SIZE))\n",
                    "    close = dex.Button.build(ws, dex.Label.new('Close'))\n",
                    "    guard = ws.insert_node_dyn(dex.InteractionBox.sensing(False, True, True))\n",
                    "    adopt(ws, canvas, GenomePanel(canvas, body, close, guard), dex.Layer.foreground())\n",
                    "    return canvas\n",
                ),
                result = result
            ),
        );
        let ctx = egui::Context::default();
        dex_nodes::fonts::install_fonts(&ctx);
        assert!(frame(&mut ws, &ctx), "the tree and its panel painted");
    }
}

/// A genome map builds to a `GenomeExplorer` and paints.
#[test]
fn a_genome_explorer_builds_and_paints() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let mut ws = Workspace::new_empty();
    let genome = run(
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
            "    return build_genome_explorer(dex.ws, gbff)\n",
        ),
    );
    assert_eq!(name(&ws, genome), "A Genome Explorer");
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    assert!(frame(&mut ws, &ctx), "the genome map painted");
}

/// A structure builds to the PDB viewer and paints.
#[test]
fn a_structure_builds_and_paints() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let mut ws = Workspace::new_empty();
    let structure = run(
        &mut ws,
        concat!(
            "    pdb = 'ATOM      1  CA  ALA A   1      ",
            "11.000  13.000  10.000  1.00  0.00           C\\n'\n",
            "    return build_protein(dex.ws, pdb)\n",
        ),
    );
    assert_eq!(name(&ws, structure), "A PDB Viewer");
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    assert!(frame(&mut ws, &ctx), "the structure painted");
}
