//! THROWAWAY — delete after measuring. Does drawing (which warms `_paths`/
//! `_labels`/`_tree`) blow up save time?
use dex_core::prelude::*;
use dex_nodes::scripting::{ScriptOutput, run_script};

const SUPERPHYLO: &str = include_str!("../../../examples/superphylogeny.py");
const PRELUDE: &str = include_str!("../src/default_prelude.py");
const SCREEN: egui::Vec2 = egui::vec2(900.0, 700.0);

fn has_pyarrow() -> bool {
    dex_nodes::scripting::init_python();
    if pyo3::Python::attach(|py| py.import("pyarrow").is_ok()) {
        return true;
    }
    let demoenv = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demoenv/.venv");
    if demoenv.is_dir() {
        let _ = dex_nodes::settings::set_venv(Some(demoenv));
    }
    pyo3::Python::attach(|py| py.import("pyarrow").is_ok())
}

fn run(ws: &mut Workspace, body: &str) -> NodeUid {
    let source = format!("{SUPERPHYLO}\ndef transform():\n{body}");
    let graph = GraphSnapshot::capture(ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let uid = match run_script(&source, PRELUDE, &handle, &[], graph) {
        Ok(ScriptOutput::Node(node)) => ws.action_handle().insert_node_dyn(node),
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("returns a node"),
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

fn draw_frame(ws: &mut Workspace, ctx: &egui::Context) {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ws.draw_frame(ui, screen);
        });
    });
}

#[test]
fn drawing_warms_caches_and_bloats_save() {
    if !has_pyarrow() {
        eprintln!("no pyarrow; skipping");
        return;
    }
    // A star tree: root + N leaves. Big enough to matter.
    let n = 600usize;
    let mut node = String::from("[0");
    let mut parent = String::from("[None");
    let mut depth = String::from("[0");
    let mut is_leaf = String::from("[0");
    let mut leaf_order = String::from("[None");
    let mut key = String::from("['' ");
    let mut label = String::from("['root'");
    let mut phylum = String::from("['' ");
    for i in 0..n {
        node += &format!(",{}", i + 1);
        parent += ",0";
        depth += ",1";
        is_leaf += ",1";
        leaf_order += &format!(",{i}");
        key += &format!(",'GB_{i}.1'");
        label += &format!(",'Leaf {i}'");
        phylum += ",'Prote'";
    }
    for s in [
        &mut node, &mut parent, &mut depth, &mut is_leaf, &mut leaf_order, &mut key, &mut label,
        &mut phylum,
    ] {
        s.push(']');
    }
    let body = format!(
        "    cols = {{'node': {node}, 'parent': {parent}, 'depth': {depth}, \
         'is_leaf': {is_leaf}, 'leaf_order': {leaf_order}, 'key': {key}, \
         'label': {label}, 'phylum': {phylum}}}\n    return build(dex.ws, cols)\n"
    );

    let mut ws = Workspace::new_empty();
    let _canvas = run(&mut ws, &body);
    let dir = std::env::temp_dir();

    let t = std::time::Instant::now();
    ws.save_to(&dir.join("super-before.dex")).expect("saves");
    eprintln!("SAVE before draw (caches cold): {:?}", t.elapsed());

    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    draw_frame(&mut ws, &ctx);
    draw_frame(&mut ws, &ctx);

    let t = std::time::Instant::now();
    ws.save_to(&dir.join("super-after.dex")).expect("saves");
    eprintln!("SAVE after draw  (caches warm): {:?}", t.elapsed());

    let _ = std::fs::remove_file(dir.join("super-before.dex"));
    let _ = std::fs::remove_file(dir.join("super-after.dex"));
}
