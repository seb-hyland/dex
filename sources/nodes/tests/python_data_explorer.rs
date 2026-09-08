//! Exercises `examples/data_explorer.py`: it builds, it paints, and it answers
//! the prelude's query protocol about the rows it drew.

use dex_core::prelude::*;
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

const EXPLORER: &str = include_str!("../../../examples/data_explorer.py");
const PRELUDE: &str = include_str!("../../../examples/prelude.py");
const SCREEN: egui::Vec2 = egui::vec2(720.0, 520.0);

/// Queue everything a finished script produced, then let the workspace apply it.
fn apply(ws: &mut Workspace, actions: std::sync::mpsc::Receiver<Action>) {
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
}

/// Build the explorer over its built-in sample and seat it (with its owned
/// dropdowns, sensor and row nodes) as the root of a fresh workspace.
fn explorer_workspace() -> (Workspace, NodeUid) {
    explorer_view(&[])
}

/// The same, opened on a particular view via the example's `explorer_*` globals.
fn explorer_view(view: &[(&str, &str)]) -> (Workspace, NodeUid) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();

    let (handle, actions) = WorkspaceActionHandle::buffered();
    let args: Vec<(String, ScriptValue)> = view
        .iter()
        .map(|(k, v)| (k.to_string(), ScriptValue::Str(v.to_string())))
        .collect();
    let node = match run_script(EXPLORER, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws)) {
        Ok(ScriptOutput::Node(node)) => node,
        Ok(_) => panic!("the explorer is returned as a node"),
        Err(e) => panic!("{e}"),
    };
    let root = ws.action_handle().insert_node_dyn(node);
    // The transform also queued the inserts of the explorer's own children.
    drop(handle);
    apply(&mut ws, actions);
    ws.set_root(root);
    (ws, root)
}

/// Draw twice (the first pass sizes, the second paints) and return the shapes.
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

/// One frame with the given input events, returning the number of shapes drawn.
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

/// A Drawn point's screen coordinate, read off the tree/plot after a frame.
fn a_point(ws: &Workspace, target: NodeUid) -> egui::Pos2 {
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
fn the_explorer_builds_and_paints() {
    let (mut ws, _root) = explorer_workspace();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    assert!(
        painted_something(&painted(&mut ws, &ctx)),
        "the explorer painted its sample"
    );
}

/// Every plot in the type matrix draws without raising — univariate strip and
/// bars, and bivariate scatter, strip-by-category and heatmap.
#[test]
fn every_view_in_the_matrix_paints() {
    let views: &[&[(&str, &str)]] = &[
        &[("explorer_mode", "Univariate"), ("explorer_x", "body_mass_g")],
        &[("explorer_mode", "Univariate"), ("explorer_x", "species")],
        &[
            ("explorer_mode", "Bivariate"),
            ("explorer_x", "body_mass_g"),
            ("explorer_y", "flipper_mm"),
        ],
        &[
            ("explorer_mode", "Bivariate"),
            ("explorer_x", "species"),
            ("explorer_y", "body_mass_g"),
        ],
        &[
            ("explorer_mode", "Bivariate"),
            ("explorer_x", "body_mass_g"),
            ("explorer_y", "species"),
        ],
        &[
            ("explorer_mode", "Bivariate"),
            ("explorer_x", "species"),
            ("explorer_y", "island"),
        ],
    ];
    for view in views {
        let (mut ws, _root) = explorer_view(view);
        let ctx = egui::Context::default();
        dex_nodes::fonts::install_fonts(&ctx);
        assert!(
            painted_something(&painted(&mut ws, &ctx)),
            "view {view:?} painted"
        );
    }
}

/// Clicking a point selects it and overlays its row; clicking empty space
/// dismisses the overlay. No inspectable nodes involved — one sensor, a search.
#[test]
fn clicking_a_point_opens_and_dismisses_the_overlay() {
    let (mut ws, root) = explorer_view(&[
        ("explorer_mode", "Bivariate"),
        ("explorer_x", "body_mass_g"),
        ("explorer_y", "flipper_mm"),
    ]);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);

    // Empty space *inside the plot*: down in the axis gutter, clear of every
    // mark. The panel's own margin is the surface's, not the plot's, so a click
    // there never reaches the plot's sensor.
    let corner = egui::pos2(24.0, 480.0);
    frame(&mut ws, &ctx, vec![]); // settle so points have positions
    let point = a_point(&ws, root);

    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(corner)]);
    let baseline = frame(&mut ws, &ctx, vec![]);

    // A click at the point: move, press, release, then a quiet frame to consume.
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(point)]);
    frame(&mut ws, &ctx, vec![button(point, true)]);
    frame(&mut ws, &ctx, vec![button(point, false)]);
    frame(&mut ws, &ctx, vec![]);
    // Move away so the extra shapes are the persistent overlay, not the hover.
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(corner)]);
    let selected = frame(&mut ws, &ctx, vec![]);
    assert!(
        selected > baseline,
        "the overlay adds shapes (baseline {baseline}, selected {selected})"
    );

    // A click on empty space clears it again.
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

/// After a frame, the explorer answers the prelude's protocol: it can list its
/// rows, say where it drew them, and hand back the record behind one.
#[test]
fn the_explorer_answers_the_query_protocol() {
    let (mut ws, root) = explorer_workspace();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    // Draw first, so there are frame positions to report.
    let _ = painted(&mut ws, &ctx);

    let sender = r#"
def transform():
    keys = dex.snapshot.send_request(target, RowKeys())
    assert keys and keys == list(range(len(keys))), keys

    drawn = dex.snapshot.send_request(target, DrawnPoints())
    assert isinstance(drawn, dict) and len(drawn) > 0, "some rows were drawn"
    row = next(iter(drawn))
    xy = dex.snapshot.send_request(target, DrawnPoint(row))
    assert xy is not None and len(xy) == 2, xy

    values = dex.snapshot.send_request(target, RowValues(row))
    assert isinstance(values, dict) and values, values

    # An id past the end is a clean None, not a crash.
    assert dex.snapshot.send_request(target, DrawnPoint(10 ** 9)) is None
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

/// The uids a node owns, in the order its `owned_nodes` lists them.
fn owned(ws: &Workspace, uid: NodeUid) -> Vec<NodeUid> {
    use dex_core::refs::NodeRefs;
    let mut out = Vec::new();
    if let Some(node) = ws.get_node(uid) {
        node.owned_refs(&mut |child| out.push(child));
    }
    out
}

/// Which option the mode dropdown — the explorer's first owned node — is showing.
fn mode_selection(ws: &Workspace, root: NodeUid) -> usize {
    let mode_dd = owned(ws, root)[0].cast::<dex_nodes::primitives::dropdown::Dropdown>();
    ws.send_request(mode_dd, dex_nodes::primitives::dropdown::DropdownSelection)
        .expect("the mode dropdown answers")
}

/// A press and release at `pos`, with a quiet frame after to settle.
fn click_at(ws: &mut Workspace, ctx: &egui::Context, pos: egui::Pos2) {
    frame(ws, ctx, vec![egui::Event::PointerMoved(pos)]);
    frame(ws, ctx, vec![button(pos, true)]);
    frame(ws, ctx, vec![button(pos, false)]);
    frame(ws, ctx, vec![]);
    ws.process_pending();
}

/// The control bar must stay reachable: the plot's sensor covers the plot, not
/// the whole panel, so a click on a dropdown opens its list and picks from it.
#[test]
fn the_mode_dropdown_can_be_opened_and_chosen_from() {
    let (mut ws, root) = explorer_workspace();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    frame(&mut ws, &ctx, vec![]);
    frame(&mut ws, &ctx, vec![]);
    assert_eq!(mode_selection(&ws, root), 0, "opens univariate");

    // The mode dropdown's header sits at the left of the control bar.
    let header = egui::pos2(60.0, 18.0);
    let closed = frame(&mut ws, &ctx, vec![]);
    click_at(&mut ws, &ctx, header);
    let opened = frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(header)]);
    assert!(
        opened > closed,
        "clicking the header opens the list (closed {closed}, opened {opened})"
    );

    // The second row of the open list, a little under the header.
    click_at(&mut ws, &ctx, egui::pos2(60.0, 70.0));
    assert_eq!(
        mode_selection(&ws, root),
        1,
        "choosing the second option switches the mode"
    );
}

/// The picture is a node of its own: the explorer owns it, draws it as an
/// addressable element, and it names and inspects itself as the view it shows.
#[test]
fn the_plot_is_an_inspectable_node_of_its_own() {
    let (mut ws, root) = explorer_view(&[
        ("explorer_mode", "Bivariate"),
        ("explorer_x", "body_mass_g"),
        ("explorer_y", "flipper_mm"),
    ]);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    frame(&mut ws, &ctx, vec![]);
    frame(&mut ws, &ctx, vec![]);

    // The explorer owns three dropdowns and the plot, in that order.
    let owned = owned(&ws, root);
    assert_eq!(owned.len(), 4, "three dropdowns and the plot");
    let plot = owned[3];

    // `draw_inspectable_node` registered it, so a probe can land on it.
    assert!(
        ws.inspectable_rect(plot).is_some(),
        "the plot is addressable over the box it was drawn in"
    );

    let node = ws.get_node(plot).expect("the plot is live");
    let name = node.type_name(NodeContext {
        id: plot,
        workspace: &ws,
    });
    assert_eq!(name, "A Scatter of body_mass_g x flipper_mm");

    // Its inspector is the view's own panel, not the explorer's.
    let inspector = node
        .build_inspector(NodeContext {
            id: plot,
            workspace: &ws,
        })
        .expect("the plot offers an inspector");
    ws.process_pending();
    let text = ws
        .send_request(inspector, dex_nodes::primitives::text::GetText)
        .expect("the panel is a label");
    assert!(
        text.contains("Bivariate") && text.contains("body_mass_g"),
        "the panel describes the view: {text}"
    );

    // The plot follows the controls: switching the mode renames it.
    let mode_dd = owned[0].cast::<dex_nodes::primitives::dropdown::Dropdown>();
    ws.submit_action_dyn(dex_core::messages::Action {
        dest: mode_dd.erase(),
        description: std::borrow::Cow::Borrowed("test"),
        body: Box::new(dex_nodes::primitives::dropdown::SetDropdownSelection { index: 0 }),
    });
    ws.process_pending();
    frame(&mut ws, &ctx, vec![]);
    let renamed = ws.get_node(plot).unwrap().type_name(NodeContext {
        id: plot,
        workspace: &ws,
    });
    assert_eq!(renamed, "A Strip Plot of body_mass_g");
}
