//! `ctx.to_global` — a drawing node's own coordinates, as they land on screen.
//!
//! A plot that says where it drew a row is only useful to a *second* view if
//! both are talking about the same space. On a pan/zoom canvas they are not:
//! items are painted on a layer carrying the transform, so a mark at `(20, 30)`
//! is somewhere else by the time it reaches the glass. These tests pin the
//! mapping in both directions and off a plane, where it must be the identity.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{AdoptCanvasNode, Canvas, Layer, NodeScreenRect};
use dex_nodes::layouts::canvas::nodes::StaticCanvasItem;
use dex_nodes::scripting::to_dyn_node_py;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// A node that records, every frame, where its own top-left landed on screen —
/// and proves the round trip by mapping it straight back.
const PROBE: &str = r#"
class Probe:
    def __init__(self):
        self.local = None
        self.glob = None
        self.back = None

    def type_name(self):
        return "A Probe"

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else 100.0
        h = base.y.provided_value() if base.y is not None else 100.0
        self.local = (base.pos.x, base.pos.y)
        g = ctx.to_global(base.pos)
        self.glob = (g.x, g.y)
        b = ctx.from_global(g)
        self.back = (b.x, b.y)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h))
        )

    def request(self, req, ctx):
        tag = getattr(req, "name", None)
        if tag == "probe.local":
            return self.local
        if tag == "probe.global":
            return self.glob
        if tag == "probe.back":
            return self.back
        return NotImplemented
"#;

/// The prelude the asking script runs under: the probe's own three questions.
const ASK_PRELUDE: &str = r#"
class Local(dex.Request):
    name = "probe.local"


class Global(dex.Request):
    name = "probe.global"


class Back(dex.Request):
    name = "probe.back"
"#;

fn probe_node() -> std::sync::Arc<dyn Node> {
    Python::attach(|py| {
        let globals = PyDict::new(py);
        globals
            .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
            .unwrap();
        let src = std::ffi::CString::new(PROBE).unwrap();
        py.run(src.as_c_str(), Some(&globals), Some(&globals))
            .expect("the probe module runs");
        let obj = py
            .eval(c"Probe()", Some(&globals), None)
            .expect("the probe constructs");
        to_dyn_node_py(&obj)
    })
}

/// Ask the probe for one of its recorded points, through a script the way any
/// other node would.
fn recorded(ws: &Workspace, probe: NodeUid, which: &str) -> (f32, f32) {
    use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

    let script = format!(
        "def transform():\n                 (x, y) = dex.snapshot.send_request(target, {which}())\n                 return \"%f,%f\" % (x, y)\n"
    );
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("target".to_owned(), ScriptValue::Node(probe))];
    let out = run_script(
        &script,
        ASK_PRELUDE,
        &handle,
        &args,
        GraphSnapshot::capture(ws),
    )
    .expect("the asking script runs");
    let ScriptOutput::Node(node) = out else {
        panic!("returns a coordinate string")
    };
    let text = dex_nodes::scripting::node_to_value(&*node)
        .map(|v| v.display())
        .expect("a coordinate string");
    let (x, y) = text.split_once(',').expect("x,y");
    (x.parse().unwrap(), y.parse().unwrap())
}

/// One frame, with the given input events.
fn frame(ws: &mut Workspace, ctx: &egui::Context, screen: egui::Rect, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(screen),
        events,
        ..Default::default()
    };
    let _ = ctx.clone().run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| {
            ws.draw_frame(ui, screen);
        });
    });
}

/// Draw twice — the first pass sizes, the second paints.
fn painted(ws: &mut Workspace, ctx: &egui::Context, screen: egui::Rect) {
    for _ in 0..2 {
        frame(ws, ctx, screen, Vec::new());
    }
}

fn close(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5
}

/// Off a transformed layer there is nothing to map: the two agree.
#[test]
fn off_a_plane_to_global_is_the_identity() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let probe = ws.insert_node_dyn(probe_node());
    ws.set_root(probe);
    ws.process_pending();

    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(600.0, 400.0));
    painted(&mut ws, &ctx, screen);

    let local = recorded(&ws, probe, "Local");
    let global = recorded(&ws, probe, "Global");
    assert!(
        close(local, global),
        "no layer transform, so nothing moves: local {local:?}, global {global:?}"
    );
}

/// On a pan/zoom plane the mapping is the canvas's own: what the probe reports
/// is where the canvas says its item is.
#[test]
fn on_a_plane_to_global_lands_where_the_canvas_puts_it() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();

    let probe = handle.insert_node_dyn(probe_node());
    let canvas = Canvas::build(handle.clone());
    let item = StaticCanvasItem::build(
        handle.clone(),
        probe,
        Vector { x: 40.0, y: 25.0 },
        Vector { x: 180.0, y: 120.0 },
    );
    handle.submit_action(
        canvas,
        "place the probe",
        AdoptCanvasNode {
            node: item.erase(),
            layer: Layer::Midground,
        },
    );
    ws.process_pending();
    ws.set_root(canvas.erase());
    ws.process_pending();

    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(600.0, 400.0));
    painted(&mut ws, &ctx, screen);

    // Magnify the plane about a point well away from the probe, so the two
    // coordinate spaces genuinely come apart.
    let over = egui::pos2(420.0, 300.0);
    frame(&mut ws, &ctx, screen, vec![egui::Event::PointerMoved(over)]);
    frame(&mut ws, &ctx, screen, vec![egui::Event::Zoom(2.0)]);
    painted(&mut ws, &ctx, screen);

    let local = recorded(&ws, probe, "Local");
    let global = recorded(&ws, probe, "Global");
    let back = recorded(&ws, probe, "Back");

    // The canvas is the authority on where its item ended up.
    let rect = ws
        .send_request(canvas, NodeScreenRect { node: item.erase() })
        .flatten()
        .expect("the canvas places its item");
    assert!(
        close(global, (rect.min.x, rect.min.y)),
        "the probe's own origin maps to where the canvas put it: \
         global {global:?}, canvas {:?}",
        (rect.min.x, rect.min.y)
    );
    assert!(
        !close(local, global),
        "and that is not where it drew: local {local:?}, global {global:?}"
    );
    assert!(
        close(back, local),
        "mapping back returns the point it started at: back {back:?}, local {local:?}"
    );
}
