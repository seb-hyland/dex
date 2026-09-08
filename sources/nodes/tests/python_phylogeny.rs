//! Exercises `examples/phylogeny.py`.
//!
//! The example is now thin — it picks the lineage column and hands the rest to
//! the default prelude's `Phylogeny` — so what is worth testing is the part it
//! still owns: that it finds the right column in a table with several, and that
//! what it hands back is a plane with a working tree on it.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{CanvasChildren, Layer};
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

const PHYLOGENY: &str = include_str!("../../../examples/phylogeny.py");
const PRELUDE: &str = include_str!("../src/default_prelude.py");
const SCREEN: egui::Vec2 = egui::vec2(760.0, 560.0);

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

/// A table of lineages, plus columns that must not be mistaken for them.
const TABLE: &str = r#"
def table():
    lineages = [
        "Bacteria;Proteobacteria;Gammaproteobacteria;Escherichia",
        "Bacteria;Proteobacteria;Gammaproteobacteria;Salmonella",
        "Bacteria;Proteobacteria;Alphaproteobacteria;Rhizobium",
        "Bacteria;Firmicutes;Bacilli;Bacillus",
        "Bacteria;Firmicutes;Bacilli;Staphylococcus",
        "Archaea;Euryarchaeota;Methanobacteria;Methanobrevibacter",
    ]
    return {
        "name": [l.split(";")[-1] for l in lineages],
        "taxonomy": lineages,
        "phylum": [l.split(";")[1] for l in lineages],
        "genes": [4200, 4500, 6100, 4100, 2700, 1800],
    }
"#;

/// Run the example, with its own globals seeded, and seat what it returns.
fn built(view: &[(&str, &str)]) -> (Workspace, NodeUid) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let (handle, actions) = WorkspaceActionHandle::buffered();

    // The example reads `thisData` off its globals when `transform` is *called*,
    // so seeding it after the module has run is the same as wiring a table in.
    let source = format!("{PHYLOGENY}\n{TABLE}\nthisData = as_arrow(table())\n");
    let args: Vec<(String, ScriptValue)> = view
        .iter()
        .map(|(k, v)| (k.to_string(), ScriptValue::Str(v.to_string())))
        .collect();
    let output = run_script(
        &source,
        PRELUDE,
        &handle,
        &args,
        GraphSnapshot::capture(&ws),
    )
    .expect("the example runs");
    let root = match output {
        ScriptOutput::Node(node) => ws.action_handle().insert_node_dyn(node),
        ScriptOutput::Handle(uid) => uid,
        ScriptOutput::Nothing => panic!("the example returns a plane"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    ws.set_root(root);
    ws.process_pending();
    (ws, root)
}

fn painted(ws: &mut Workspace, ctx: &egui::Context) -> usize {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let mut count = 0;
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
        fn walk(shape: &egui::Shape) -> usize {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().map(walk).sum(),
                _ => 1,
            }
        }
        count = out.shapes.iter().map(|c| walk(&c.shape)).sum();
    }
    count
}

/// The column guess is the part the example still owns: several text columns,
/// and only one of them holds lineages.
#[test]
fn it_finds_the_lineage_column() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    dex_nodes::scripting::init_python();
    let ask = r#"
def transform():
    frame = Frame(table())
    lineage = lineage_column(frame)
    return "%s|%s" % (lineage, clade_column(frame, lineage))
"#;
    let script = format!("{PHYLOGENY}\n{TABLE}\n{ask}");
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let ws = Workspace::new_empty();
    let args: [(String, ScriptValue); 0] = [];
    let out = run_script(
        &script,
        PRELUDE,
        &handle,
        &args,
        GraphSnapshot::capture(&ws),
    )
    .expect("the helpers run");
    let ScriptOutput::Node(node) = out else {
        panic!("returns the two column names")
    };
    let answer = dex_nodes::scripting::node_to_value(&*node)
        .map(|v| v.display())
        .expect("an answer");
    assert_eq!(
        answer, "taxonomy|phylum",
        "the lineage column is the one with the semicolons, not the first text one"
    );
}

/// What it hands back is a plane with a tree on it, and the tree works.
#[test]
fn it_builds_a_tree_on_a_plane() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built(&[]);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    assert!(painted(&mut ws, &ctx) > 20, "the tree painted");

    // A plane, with the tree in the middle and its readout in front.
    let midground = ws
        .send_request(root, CanvasChildren)
        .expect("the example returns a canvas");
    assert_eq!(midground.len(), 1, "the tree is the one thing on the plane");
    let foreground = ws
        .send_request(
            root,
            dex_nodes::layouts::canvas::layout::CanvasLayerNodes {
                layer: Layer::Foreground,
            },
        )
        .unwrap_or_default();
    assert_eq!(
        foreground.len(),
        3,
        "its title, its figures and its readout pinned in front, all drawn at \
         their own size rather than scaling with the tree"
    );
}

/// The same tree, bent round a circle, on the example's own switch.
#[test]
fn it_can_draw_the_tree_round_a_circle() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, _root) = built(&[("tree_shape", "circular")]);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    assert!(painted(&mut ws, &ctx) > 20, "the circular tree painted");
}
