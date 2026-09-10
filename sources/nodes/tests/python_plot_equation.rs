//! Exercises `examples/plot_equation.py`: an equation run, sampled and drawn.
//!
//! The example is about *running* a lambda, not reading it — so what these pin
//! is that the runner gets the right numbers out of a real canvas lambda, that
//! which variable is bound decides what kind of picture comes out, and that
//! sampling the same ground twice does not run it twice.
//!
//! And two things about the picture, both regressions: that it draws at all —
//! the example shares a namespace with the prelude, so a constant named here
//! that the prelude also names is a live hazard — and that the curve holds
//! still while the plane moves under it.

use dex_core::prelude::*;
use dex_nodes::composites::lambda::{
    AddArgAt, CanvasLambda, Lambda, LambdaArg, LambdaArgsNode, OutputPin, ParamPins, SetConnection,
};
use dex_nodes::layouts::canvas::layout::PlaceOnCanvas;
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

const EQUATION: &str = include_str!("../../../examples/plot_equation.py");
const PRELUDE: &str = include_str!("../src/default_prelude.py");

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

fn settle(ws: &mut Workspace) {
    for _ in 0..4 {
        ws.process_pending();
        ws.tick_all();
        ws.process_pending();
    }
}

/// Give `owner` a parameter called `name`; returns its port.
fn add_arg(ws: &Workspace, owner: NodeUid, name: &str) -> NodeUid {
    let handle = ws.action_handle();
    let args = ws
        .send_request(owner, LambdaArgsNode)
        .expect("the lambda exposes its argument row");
    let arg = NodeUid::mint();
    let port = NodeUid::mint();
    LambdaArg::build_with(handle, arg.cast(), port, "of".to_owned(), name.to_owned());
    ws.submit_action(args, "Add argument", AddArgAt { arg });
    port
}

/// `f(v) = v * v`, as a canvas lambda taking one parameter called `name`.
fn squared(name: &str) -> (Workspace, NodeUid) {
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();
    let root = handle.insert_node_dyn(Arc::new(dex_nodes::primitives::nothing::Nothing));
    let outer = handle
        .insert_node(CanvasLambda::new(handle.clone()))
        .erase();
    settle(&mut ws);
    ws.set_root(root);

    add_arg(&ws, outer, name);
    settle(&mut ws);

    let mult = NodeUid::mint();
    let mult_args: NodeUid = NodeUid::mint();
    let mult_output = NodeUid::mint();
    handle.insert_node_at_dyn(
        mult,
        Arc::new(Lambda::new_with(
            handle.clone(),
            mult_args.cast(),
            mult_output,
            "Mult".to_owned(),
            "def transform():\n    return a * b\n".to_owned(),
        )),
    );
    let canvas = ws
        .send_request(outer, dex_nodes::composites::lambda::ComputeCanvasNode)
        .expect("the canvas lambda exposes its canvas");
    settle(&mut ws);
    ws.submit_action(
        canvas,
        "Place the operator",
        PlaceOnCanvas {
            node: mult,
            size: Vector { x: 200.0, y: 140.0 },
        },
    );
    let a = add_arg(&ws, mult, "a");
    let b = add_arg(&ws, mult, "b");
    settle(&mut ws);

    let pin = ws.send_request(outer, ParamPins).unwrap_or_default()[0];
    for port in [a, b] {
        ws.submit_action(port, "Wire", SetConnection { target: Some(pin) });
    }
    let out_pin = ws.send_request(outer, OutputPin).expect("an output pin");
    ws.submit_action(
        out_pin,
        "Wire the output",
        SetConnection {
            target: Some(mult_output),
        },
    );
    settle(&mut ws);
    (ws, outer)
}

/// Run `script` against the example, with the equation bound.
fn ask(ws: &Workspace, equation: NodeUid, script: &str) -> String {
    let source = format!("{EQUATION}\n{script}");
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("equation".to_owned(), ScriptValue::Node(equation))];
    let out = run_script(&source, PRELUDE, &handle, &args, GraphSnapshot::capture(ws))
        .expect("the script runs");
    let ScriptOutput::Node(node) = out else {
        panic!("returns text")
    };
    dex_nodes::scripting::node_to_value(&*node)
        .map(|v| v.display())
        .expect("an answer")
}

/// The runner gets real numbers out of a real canvas lambda, and reuses them.
#[test]
fn it_runs_the_equation_and_caches_what_it_ran() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (ws, f) = squared("x");
    let script = r#"
def transform():
    run = LambdaRunner(dex.snapshot, equation)
    assert run.params == ["x"], run.params
    # It is `x * x`, and it comes back as a number rather than as a graph.
    assert run(x=3.0) == 9.0, run(x=3.0)
    assert run(x=-4.0) == 16.0
    # Asked again, it answers from the memo rather than running anything.
    before = len(run.cache)
    assert run(x=3.0) == 9.0
    assert len(run.cache) == before, "a repeat sample cost nothing"
    return "%d" % before
"#;
    assert_eq!(ask(&ws, f, script), "2", "two distinct inputs were run");
}

/// Which variable the lambda takes decides what is drawn.
#[test]
fn the_bound_variable_decides_the_picture() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    // A function of x is a curve sampled across x, and its output is y.
    let (ws, fx) = squared("x");
    assert_eq!(
        ask(
            &ws,
            fx,
            "def transform():\n    \
             return plotted_variable(LambdaRunner(dex.snapshot, equation))\n"
        ),
        "x",
    );
    let columns = ask(
        &ws,
        fx,
        r#"
def transform():
    run = LambdaRunner(dex.snapshot, equation)
    table = curve_table(run, "x", -3.0, 3.0)
    # Sampled across x, so x runs over the range and y is what came back.
    assert abs(min(table["x"]) + 3.0) < 1e-9, table["x"][:3]
    assert min(table["y"]) >= 0.0, "x * x is never negative"
    return "%d" % len(table["x"])
"#,
    );
    assert_eq!(columns, "480", "the curve was sampled across the range");

    // A function of y is the same curve on its side: what it returns is the x.
    let (ws, fy) = squared("y");
    assert_eq!(
        ask(
            &ws,
            fy,
            "def transform():\n    \
             return plotted_variable(LambdaRunner(dex.snapshot, equation))\n"
        ),
        "y",
    );
    assert_eq!(
        ask(
            &ws,
            fy,
            r#"
def transform():
    run = LambdaRunner(dex.snapshot, equation)
    table = curve_table(run, "y", -3.0, 3.0)
    # Sampled across y, so it is y that runs over the range.
    assert abs(min(table["y"]) + 3.0) < 1e-9, table["y"][:3]
    assert min(table["x"]) >= 0.0, "the output is the x of each point"
    return "ok"
"#
        ),
        "ok",
    );
}

/// A lambda taking neither x nor y cannot be plotted, and says what it takes.
#[test]
fn a_lambda_of_neither_says_so() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (ws, f) = squared("t");
    assert_eq!(
        ask(
            &ws,
            f,
            r#"
def transform():
    try:
        plotted_variable(LambdaRunner(dex.snapshot, equation))
    except Unrunnable as e:
        return str(e)
    return "it should not have plotted"
"#
        ),
        "this plots a function of x or y, and that lambda takes t",
    );
}

/// The whole example, drawn: it paints, and nothing on the plane raises.
///
/// The example's own module-level names land in the same namespace as the
/// prelude's, so one that collides silently replaces the prelude's — which is
/// how a sample count called `GRID` became the colour the axes are drawn in,
/// and every gridline a `TypeError`. A draw error is caught and painted as
/// text rather than raised, so the only way to see it is to look at the frame.
#[test]
fn the_whole_plane_draws_without_error() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, f) = squared("x");
    place_curve(&mut ws, f);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 700.0));
    let mut drawn = Vec::new();
    for _ in 0..4 {
        drawn = frame_text(&ctx, &mut ws, screen, Vec::new());
    }
    let complaints: Vec<&String> = drawn
        .iter()
        .filter(|s| s.contains("error") || s.contains("Error"))
        .collect();
    assert!(
        complaints.is_empty(),
        "the plane drew an error: {complaints:?}"
    );
    assert!(
        drawn.iter().any(|s| s == "y = f(x)"),
        "the curve was titled for the variable it is a function of: {drawn:?}"
    );
}

/// The curve holds still while the plane moves under it.
///
/// It is sampled once over a fixed range, so the mapping from a value to a
/// place on the plane is fixed too. Re-deriving that mapping from whatever is
/// on screen is what made the curve fly apart on its own: the window is read
/// *through* the axis, so an axis worked out from the last sample and rounded
/// outward moved the window that decided the next one.
#[test]
fn the_curve_does_not_move_when_the_plane_does() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, f) = squared("x");
    let plane = place_curve(&mut ws, f);
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 700.0));
    for _ in 0..3 {
        frame_text(&ctx, &mut ws, screen, Vec::new());
    }

    let plot = ws
        .send_request(plane, dex_nodes::layouts::canvas::layout::CanvasChildren)
        .and_then(|items| items.first().copied())
        .and_then(|item| ws.send_request(item, dex_nodes::layouts::canvas::nodes::CanvasNodeChild))
        .expect("the curve is on the plane");
    // How the view maps x onto the plane, as the axis reports it.
    let mapping = |ws: &Workspace| -> String {
        let script = "def transform():\n    \
            a = (dex.snapshot.send_request(target, PlotScale()) or {}).get('x') or {}\n    \
            return '%.6g %.6g' % (a.get('lo', 0.0), a.get('hi', 0.0))\n";
        let (handle, _actions) = WorkspaceActionHandle::buffered();
        let args = [("target".to_owned(), ScriptValue::Node(plot))];
        let out = run_script(
            &format!("{EQUATION}\n{script}"),
            PRELUDE,
            &handle,
            &args,
            GraphSnapshot::capture(ws),
        )
        .expect("the question runs");
        let ScriptOutput::Node(node) = out else {
            panic!("returns the mapping")
        };
        dex_nodes::scripting::node_to_value(&*node)
            .map(|v| v.display())
            .expect("a mapping")
    };
    let before = mapping(&ws);
    assert_ne!(before, "0 0", "the view published an x axis");

    let over = egui::pos2(450.0, 350.0);
    frame_text(&ctx, &mut ws, screen, vec![egui::Event::PointerMoved(over)]);
    for _ in 0..4 {
        frame_text(&ctx, &mut ws, screen, vec![egui::Event::Zoom(1.4)]);
    }
    for _ in 0..4 {
        frame_text(&ctx, &mut ws, screen, Vec::new());
    }
    assert_eq!(
        before,
        mapping(&ws),
        "magnifying the plane moved the picture, not the mapping"
    );
}

/// The example's plane, rooted and settled, ready to be drawn.
fn place_curve(ws: &mut Workspace, equation: NodeUid) -> NodeUid {
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let args = [("equation".to_owned(), ScriptValue::Node(equation))];
    let out = run_script(
        &format!("{EQUATION}\n"),
        PRELUDE,
        &handle,
        &args,
        GraphSnapshot::capture(ws),
    )
    .expect("the example runs");
    let plane = match out {
        ScriptOutput::Handle(uid) => uid,
        _ => panic!("the example returns a plane"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    ws.set_root(plane);
    ws.process_pending();
    plane
}

/// Draw one frame, and hand back every string it painted.
///
/// The text, because that is where a failure shows: a dynamic node that raises
/// while drawing is caught and painted as its message rather than propagated.
fn frame_text(
    ctx: &egui::Context,
    ws: &mut Workspace,
    screen: egui::Rect,
    events: Vec<egui::Event>,
) -> Vec<String> {
    let out = ctx.clone().run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        },
        |c| {
            egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
        },
    );
    ws.process_pending();
    fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, out)),
            egui::Shape::Text(t) => out.push(t.galley.text().to_owned()),
            _ => {}
        }
    }
    let mut text = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut text);
    }
    text
}
