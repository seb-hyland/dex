//! Exercises `examples/phylogeny.py`: it builds a vertical tree from lineage
//! strings, paints it, and answers the prelude's query protocol keyed by row.

use dex_core::prelude::*;
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

const PHYLOGENY: &str = include_str!("../../../examples/phylogeny.py");
const PRELUDE: &str = include_str!("../../../examples/prelude.py");
const SCREEN: egui::Vec2 = egui::vec2(760.0, 520.0);

fn apply(ws: &mut Workspace, actions: std::sync::mpsc::Receiver<Action>) {
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
}

/// Build the tree over its built-in sample and seat it (with its taxa and hover
/// sensor) as the root of a fresh workspace.
fn phylogeny_workspace() -> (Workspace, NodeUid) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();

    let (handle, actions) = WorkspaceActionHandle::buffered();
    let node = match run_script(PHYLOGENY, PRELUDE, &handle, &[], GraphSnapshot::capture(&ws)) {
        Ok(ScriptOutput::Node(node)) => node,
        Ok(_) => panic!("the tree is returned as a node"),
        Err(e) => panic!("{e}"),
    };
    let root = ws.action_handle().insert_node_dyn(node);
    drop(handle);
    apply(&mut ws, actions);
    ws.set_root(root);
    (ws, root)
}

fn painted(ws: &mut Workspace, ctx: &egui::Context) -> Vec<egui::epaint::ClippedShape> {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        shapes = ctx
            .clone()
            .run_ui(input, |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    ws.draw_frame(ui, screen);
                });
            })
            .shapes;
    }
    shapes
}

fn painted_something(shapes: &[egui::epaint::ClippedShape]) -> bool {
    fn walk(shape: &egui::Shape) -> bool {
        match shape {
            egui::Shape::Mesh(mesh) => !mesh.vertices.is_empty(),
            egui::Shape::Vec(inner) => inner.iter().any(walk),
            egui::Shape::Circle(_) | egui::Shape::LineSegment { .. } | egui::Shape::Path(_) => true,
            _ => false,
        }
    }
    shapes.iter().any(|c| walk(&c.shape))
}

fn frame(ws: &mut Workspace, ctx: &egui::Context, events: Vec<egui::Event>) -> usize {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let input = egui::RawInput {
        screen_rect: Some(screen),
        events,
        ..Default::default()
    };
    let out = ctx.clone().run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| {
            ws.draw_frame(ui, screen);
        });
    });
    fn count(shape: &egui::Shape) -> usize {
        match shape {
            egui::Shape::Vec(inner) => inner.iter().map(count).sum(),
            _ => 1,
        }
    }
    out.shapes.iter().map(|c| count(&c.shape)).sum()
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

/// A drawn leaf's screen coordinate, read off the tree after a frame.
fn a_leaf(ws: &Workspace, target: NodeUid) -> egui::Pos2 {
    let script = r#"
def transform():
    pts = dex.snapshot.send_request(target, DrawnPoints())
    (_rid, (x, y)) = sorted(pts.items())[0]
    return "%f,%f" % (x, y)
"#;
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("target".to_owned(), ScriptValue::Node(target))];
    let out = run_script(script, PRELUDE, &handle, &args, GraphSnapshot::capture(ws)).unwrap();
    let ScriptOutput::Node(node) = out else {
        panic!("returns a coordinate string")
    };
    let text = dex_nodes::scripting::node_to_value(&*node)
        .map(|v| v.display())
        .expect("a coordinate string");
    let (x, y) = text.split_once(',').expect("x,y");
    egui::pos2(x.parse().unwrap(), y.parse().unwrap())
}

#[test]
fn the_tree_builds_and_paints() {
    let (mut ws, _root) = phylogeny_workspace();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    assert!(
        painted_something(&painted(&mut ws, &ctx)),
        "the tree painted its sample"
    );
}

/// Clicking a leaf overlays its record; clicking empty space dismisses it. No
/// inspectable nodes — one sensor over the whole tree.
#[test]
fn clicking_a_leaf_opens_and_dismisses_the_overlay() {
    let (mut ws, root) = phylogeny_workspace();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);

    let corner = egui::pos2(4.0, 4.0); // top-left margin, over no node
    frame(&mut ws, &ctx, vec![]);
    let leaf = a_leaf(&ws, root);

    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(corner)]);
    let baseline = frame(&mut ws, &ctx, vec![]);

    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(leaf)]);
    frame(&mut ws, &ctx, vec![button(leaf, true)]);
    frame(&mut ws, &ctx, vec![button(leaf, false)]);
    frame(&mut ws, &ctx, vec![]);
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(corner)]);
    let selected = frame(&mut ws, &ctx, vec![]);
    assert!(
        selected > baseline,
        "the overlay adds shapes (baseline {baseline}, selected {selected})"
    );

    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(corner)]);
    frame(&mut ws, &ctx, vec![button(corner, true)]);
    frame(&mut ws, &ctx, vec![button(corner, false)]);
    frame(&mut ws, &ctx, vec![]);
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(corner)]);
    let dismissed = frame(&mut ws, &ctx, vec![]);
    assert!(
        dismissed < selected,
        "dismissing removes the overlay (selected {selected}, dismissed {dismissed})"
    );
}

/// After a frame, the tree answers the protocol: it lists its rows, locates the
/// leaf each was drawn at, and hands back the record behind a row.
#[test]
fn the_tree_answers_the_query_protocol() {
    let (mut ws, root) = phylogeny_workspace();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    let _ = painted(&mut ws, &ctx);

    let sender = r#"
def transform():
    keys = dex.snapshot.send_request(target, RowKeys())
    assert keys == list(range(len(keys))) and len(keys) > 0, keys

    drawn = dex.snapshot.send_request(target, DrawnPoints())
    assert isinstance(drawn, dict) and len(drawn) == len(keys), (len(drawn), len(keys))

    xy = dex.snapshot.send_request(target, DrawnPoint(keys[0]))
    assert xy is not None and len(xy) == 2, xy

    values = dex.snapshot.send_request(target, RowValues(keys[0]))
    assert isinstance(values, dict) and values, values
    return "ok"
"#;

    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("target".to_owned(), ScriptValue::Node(root))];
    let out = run_script(sender, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("the query script runs and its assertions hold");
    let ScriptOutput::Node(node) = out else {
        panic!("the sender returns \"ok\"")
    };
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node).map(|v| v.display()),
        Some("ok".to_owned()),
    );
}
