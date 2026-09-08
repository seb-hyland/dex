//! Exercises `examples/scatterplot.py`: two views of one table, side by side,
//! with lines between the same record in each.
//!
//! This is the example that proves the prelude's protocol does what it is for.
//! A scatter and a violin share nothing but the table they were read from; the
//! lines between them are drawn by a node that asks both where they put every
//! row this frame, and knows nothing else about either. So the test is about
//! the join, not the drawing: the pair answers as one view, both halves place
//! the same rows, and selecting in one selects in the other.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::CanvasChildren;
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

const SCATTERPLOT: &str = include_str!("../../../examples/scatterplot.py");
const PRELUDE: &str = include_str!("../src/default_prelude.py");
const SCREEN: egui::Vec2 = egui::vec2(1000.0, 760.0);

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

/// Run the example over the prelude's own sample, and seat what it returns.
fn built() -> (Workspace, NodeUid) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let (handle, actions) = WorkspaceActionHandle::buffered();
    // `transform` reads `thisData` when it is called, so seeding it after the
    // module has run is the same as wiring a table in.
    let source = format!("{SCATTERPLOT}\nthisData = as_arrow(sample_table())\n");
    let args: [(String, ScriptValue); 0] = [];
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

/// The pair builds, paints both halves, and sits on a plane.
#[test]
fn it_draws_two_views_on_one_plane() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    assert!(painted(&mut ws, &ctx) > 40, "both views painted");

    let midground = ws
        .send_request(root, CanvasChildren)
        .expect("the example returns a canvas");
    assert_eq!(midground.len(), 1, "the pair is one item on the plane");
}

/// Both halves place the same rows, in the same screen space — which is what
/// makes a line between them mean anything.
#[test]
fn both_halves_place_the_same_records() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    painted(&mut ws, &ctx);

    // The pair is the canvas item's child; its own owned nodes are the two
    // views and the joiner.
    let script = r#"
def transform():
    item = dex.snapshot.owned_refs(root)[0]
    pair = dex.snapshot.owned_refs(item)[0]
    (left, right, _joiner) = dex.snapshot.owned_refs(pair)
    here = dex.snapshot.send_request(left, DrawnPoints())
    there = dex.snapshot.send_request(right, DrawnPoints())
    pairs = row_correspondence(dex.snapshot, left, right)
    apart = sum(1 for (a, b) in pairs if here[a][0] < there[b][0])
    return "%d %d %d %d" % (len(here), len(there), len(pairs), apart)
"#;
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("root".to_owned(), ScriptValue::Node(root))];
    let out = run_script(script, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("the query runs");
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
    assert_eq!(parts[0], 90, "the scatter drew every row");
    assert_eq!(parts[1], 90, "so did the violin");
    assert_eq!(parts[2], 90, "and they correspond row for row");
    assert_eq!(
        parts[3], 90,
        "every left mark is left of its partner — the two views really are \
         side by side in one screen space, not each in its own"
    );
}

/// Selecting in one view selects in the other: the write half of the protocol.
#[test]
fn a_selection_crosses_between_the_views() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, root) = built();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    painted(&mut ws, &ctx);

    let script = r#"
def transform():
    item = dex.snapshot.owned_refs(root)[0]
    pair = dex.snapshot.owned_refs(item)[0]
    (left, right, _joiner) = dex.snapshot.owned_refs(pair)
    dex.snapshot.send_request(left, SetSelection(12))
    return "%s" % (dex.snapshot.send_request(right, Selection()),)
"#;
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("root".to_owned(), ScriptValue::Node(root))];
    // Selecting, then a frame, then reading: the crossing happens while drawing.
    let out = run_script(script, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("the selection runs");
    let ScriptOutput::Node(_) = out else {
        panic!("returns the mirrored row")
    };
    painted(&mut ws, &ctx);

    let read = r#"
def transform():
    item = dex.snapshot.owned_refs(root)[0]
    pair = dex.snapshot.owned_refs(item)[0]
    (_left, right, _joiner) = dex.snapshot.owned_refs(pair)
    return "%s" % (dex.snapshot.send_request(right, Selection()),)
"#;
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let out = run_script(read, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("the read runs");
    let ScriptOutput::Node(node) = out else {
        panic!("returns the row")
    };
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node).map(|v| v.display()),
        Some("12".to_owned()),
        "the row selected on the left crossed to the right"
    );
}
