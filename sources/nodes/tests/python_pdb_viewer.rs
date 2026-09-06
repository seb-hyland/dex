//! Exercises the canvas + scrub restructure in `examples/pdb_viewer.py`.
//!
//! The viewer is the structure on a `Canvas` of its own: dragging it turns it,
//! and the plane is what magnifies it — measured against a whole desktop, so the
//! same zoom means the same thing in a preview as it does filling the window.
//! This pins the wiring: `build` returns a plane named after what is on it, with
//! the structure as its one item, and a frame draws without falling over.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{Canvas, CanvasLayerNodes, Layer};
use dex_nodes::scripting::{ScriptOutput, run_script};

const PDB_VIEWER: &str = include_str!("../../../examples/pdb_viewer.py");
const SCREEN: egui::Vec2 = egui::vec2(900.0, 640.0);

/// A fixed-column PDB with two chains of CA atoms and one ligand atom.
fn mini_pdb() -> String {
    let mut s = String::from("TITLE     A TEST STRUCTURE\n");
    let atom = |serial: i32,
                name: &str,
                res: &str,
                chain: char,
                seq: i32,
                x: f64,
                y: f64,
                z: f64| {
        // Columns per the PDB spec, so `parse_pdb` reads the right fields.
        format!(
            "ATOM  {serial:>5} {name:<4}{res:>3} {chain}{seq:>4}    {x:>8.3}{y:>8.3}{z:>8.3}  1.00  0.00           C\n"
        )
    };
    s.push_str(&atom(1, "CA", "ALA", 'A', 1, 11.0, 13.0, 10.0));
    s.push_str(&atom(2, "CA", "GLY", 'A', 2, 12.5, 14.0, 11.0));
    s.push_str(&atom(3, "CA", "SER", 'B', 1, -5.0, 2.0, -3.0));
    s.push_str(&atom(4, "CA", "LEU", 'B', 2, -6.0, 3.5, -2.0));
    // A non-water ligand atom, element in the last columns.
    s.push_str("HETATM    5 FE   HEM A 101       1.000   1.000   1.000  1.00  0.00          FE\n");
    s
}

/// Run the example, returning the live id of the canvas its `build` produced.
fn build_canvas(ws: &mut Workspace) -> NodeUid {
    let source = format!(
        "{PDB_VIEWER}\n\
         _TEST_PDB = {pdb:?}\n\
         def transform():\n\
        \x20   return build(dex.ws, _TEST_PDB)\n",
        pdb = mini_pdb()
    );

    let graph = GraphSnapshot::capture(ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let uid = match run_script(&source, "", &handle, &[], graph) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the viewer returns a handle to the canvas it built"),
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

fn draw(ws: &mut Workspace) {
    let egui_ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&egui_ctx);
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        let _ = egui_ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                ws.draw_frame(ui, rect);
            });
        });
        ws.process_pending();
    }
}

/// The structure lands on a plane of its own, named after what it holds.
#[test]
fn the_structure_is_on_a_plane_that_names_itself() {
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
        "Structure",
        "the plane says what is on it, not that it is a plane"
    );

    let band = |layer| {
        ws.send_request(canvas_id.cast::<Canvas>(), CanvasLayerNodes { layer })
            .unwrap_or_default()
    };
    assert_eq!(
        band(Layer::Midground).len(),
        1,
        "the structure is the one item on the plane"
    );
    assert!(
        band(Layer::Foreground).is_empty(),
        "and turning it is a drag on the structure, so nothing is pinned in front"
    );

    // The whole thing draws — the structure and its sensor — with nothing
    // raising on the way.
    draw(&mut ws);
}
