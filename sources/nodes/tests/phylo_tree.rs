//! Exercises `examples/phylo_tree.py` and `examples/lineage_table.py`.
//!
//! The tree itself is the default prelude's now — `Phylogeny` in its
//! `"sideways"` shape — and is tested there. What is left here is the pair of
//! things these two examples still own: a generator that hands back a real
//! Arrow table, and a chrome node that bands a column per rank *by asking the
//! tree how deep it goes*, so the axis cannot fall out of step with the picture
//! behind it. Plus the end-to-end path, because a generator and a view wired
//! together is what these two examples are for.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{CanvasChildren, CanvasLayerNodes, Layer};
use dex_nodes::primitives::table::Table;
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

const PHYLO: &str = include_str!("../../../examples/phylo_tree.py");
const LINEAGE_TABLE: &str = include_str!("../../../examples/lineage_table.py");
const PRELUDE: &str = include_str!("../src/default_prelude.py");
const SCREEN: egui::Vec2 = egui::vec2(1000.0, 800.0);

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

/// Run a script against the default prelude and seat what it hands back.
fn built(ws: &mut Workspace, source: &str) -> NodeUid {
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let args: [(String, ScriptValue); 0] = [];
    let output = run_script(source, PRELUDE, &handle, &args, GraphSnapshot::capture(ws))
        .expect("the script runs");
    let root = match output {
        ScriptOutput::Node(node) => ws.action_handle().insert_node_dyn(node),
        ScriptOutput::Handle(uid) => uid,
        ScriptOutput::Nothing => panic!("the script returns something"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    ws.set_root(root);
    ws.process_pending();
    root
}

/// Draw twice — the first pass sizes, the second paints — and count the shapes.
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

/// A transform can hand back a table. Anything carrying Arrow columns becomes
/// one, which is what lets a script build the input the tree reads.
#[test]
fn a_script_can_return_a_table() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let ws = Workspace::new_empty();
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args: [(String, ScriptValue); 0] = [];
    let node = match run_script(
        LINEAGE_TABLE,
        PRELUDE,
        &handle,
        &args,
        GraphSnapshot::capture(&ws),
    ) {
        Ok(ScriptOutput::Node(node)) => node,
        _ => panic!("the generator returns a table"),
    };
    let table = (*node)
        .as_any_ref()
        .downcast_ref::<Table>()
        .expect("a pyarrow table became a Table node");

    let batch = table.batch();
    let schema = batch.schema();
    let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
    assert!(
        names.contains(&"lineage") && names.contains(&"reads"),
        "the columns survived the crossing: {names:?}"
    );
    assert!(batch.num_rows() > 10, "and so did the rows");
}

/// The example's own message: the rank axis asks the tree how deep it is rather
/// than being told, so the two cannot disagree about how many columns there are.
#[test]
fn the_rank_axis_reads_its_depth_off_the_tree() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let mut ws = Workspace::new_empty();
    // Lineages five ranks deep, so the answer is not the default.
    let source = format!(
        "{PHYLO}\n\
         table = as_arrow({{\"lineage\": [\n    \
             \"d__A;p__B;c__C;o__D;f__E\",\n    \
             \"d__A;p__B;c__C;o__D;f__F\",\n    \
             \"d__A;p__B;c__G;o__H;f__I\",\n\
         ]}})\n"
    );
    let root = built(&mut ws, &source);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    assert!(painted(&mut ws, &ctx) > 10, "the dendrogram painted");

    // The tree is the one thing on the plane; the axis and the readout are in
    // front of it.
    let midground = ws
        .send_request(root, CanvasChildren)
        .expect("the example returns a canvas");
    assert_eq!(midground.len(), 1, "the tree is the body");
    let foreground = ws
        .send_request(
            root,
            CanvasLayerNodes {
                layer: Layer::Foreground,
            },
        )
        .unwrap_or_default();
    assert_eq!(foreground.len(), 2, "the rank axis and the readout");

    // And the tree answers the axis's question with the depth it actually has:
    // five ranks means a deepest node at depth 4.
    let ask = r#"
def transform():
    item = dex.snapshot.owned_refs(root)[0]
    tree = dex.snapshot.owned_refs(item)[0]
    return str(dex.snapshot.send_request(tree, RankCount()))
"#;
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("root".to_owned(), ScriptValue::Node(root))];
    let out = run_script(
        &format!("{PHYLO}\n{ask}"),
        PRELUDE,
        &handle,
        &args,
        GraphSnapshot::capture(&ws),
    )
    .expect("the question runs");
    let ScriptOutput::Node(node) = out else {
        panic!("returns the depth")
    };
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node).map(|v| v.display()),
        Some("4".to_owned()),
        "five ranks, so the deepest node sits at depth four"
    );
}

/// End to end: the generator builds a table, the tree draws it, and the tree
/// still answers the shared protocol about the rows it placed.
#[test]
fn the_tree_draws_the_table_the_generator_built() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let mut ws = Workspace::new_empty();
    // The generator's own module, then the tree over what it returns.
    let source = format!(
        "{LINEAGE_TABLE}\n\
         _generated = transform()\n\
         {PHYLO}\n\
         table = _generated\n"
    );
    let root = built(&mut ws, &source);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    assert!(painted(&mut ws, &ctx) > 20, "the tree drew the built table");

    let ask = r#"
def transform():
    item = dex.snapshot.owned_refs(root)[0]
    tree = dex.snapshot.owned_refs(item)[0]
    drawn = dex.snapshot.send_request(tree, DrawnPoints())
    keys = dex.snapshot.send_request(tree, RowKeys())
    return "%d %d" % (len(keys), len(drawn))
"#;
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("root".to_owned(), ScriptValue::Node(root))];
    let out = run_script(ask, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("the question runs");
    let ScriptOutput::Node(node) = out else {
        panic!("returns counts")
    };
    let counts = dex_nodes::scripting::node_to_value(&*node)
        .map(|v| v.display())
        .expect("counts");
    let parts: Vec<usize> = counts
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect();
    assert!(parts[0] > 10, "the generator's rows reached the tree");
    assert_eq!(
        parts[1], parts[0],
        "and every one of them was placed on a leaf"
    );
}
