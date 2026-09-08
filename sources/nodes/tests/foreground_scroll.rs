//! Chrome on a plane can be pointed at.
//!
//! A canvas registers an `egui::Area` for its *items*, which is how egui
//! decides the pointer is over that layer at all. Its chrome had none, so
//! anything in the foreground that wanted the pointer for itself — a readout
//! with a table in it, scrolled sideways to reach the rest of a wide record —
//! was told the pointer belonged to whatever was underneath and would not move.
//!
//! The claim has to be the size of the thing claiming it: a viewport-wide one
//! takes the pointer from every item drawn beneath, which is most of what a
//! surface is for. So it is `host_widgets` that claims, over its own box.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{AdoptCanvasNode, Canvas, Layer};
use dex_nodes::layouts::canvas::nodes::StaticCanvasItem;
use pyo3::types::PyDictMethods;

const SCREEN: egui::Vec2 = egui::vec2(900.0, 700.0);

/// A table wide enough that most of it is off the right-hand edge.
fn wide_table() -> Arc<dyn Node> {
    dex_nodes::scripting::init_python();
    pyo3::Python::attach(|py| {
        let globals = pyo3::types::PyDict::new(py);
        globals
            .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
            .unwrap();
        let src = std::ffi::CString::new(
            "import pyarrow as pa\n\
             columns = {'column_number_%02d' % i: ['value %02d' % i] for i in range(14)}\n\
             table = pa.table(columns)\n",
        )
        .unwrap();
        py.run(src.as_c_str(), Some(&globals), Some(&globals))
            .expect("the table builds");
        let obj = globals.get_item("table").unwrap().unwrap();
        dex_nodes::scripting::to_dyn_node_py(&obj)
    })
}

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

/// A plane with `node` in the band named, drawn small so the table overflows.
fn planed(node: Arc<dyn Node>, layer: Layer) -> (Workspace, egui::Context) {
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();
    let body = handle.insert_node_dyn(node);
    let canvas = Canvas::build(handle.clone());
    let item = StaticCanvasItem::build(
        handle.clone(),
        body,
        Vector { x: 0.0, y: 0.0 },
        Vector { x: 300.0, y: 90.0 },
    );
    handle.submit_action(
        canvas,
        "place it",
        AdoptCanvasNode {
            node: if matches!(layer, Layer::Foreground) {
                body
            } else {
                item.erase()
            },
            layer,
        },
    );
    ws.process_pending();
    ws.set_root(canvas.erase());
    ws.process_pending();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    (ws, ctx)
}

/// The column names painted this frame.
fn shown(ws: &mut Workspace, ctx: &egui::Context, events: Vec<egui::Event>) -> Vec<String> {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
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
    fn walk(shape: &egui::Shape, seen: &mut Vec<String>) {
        match shape {
            egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, seen)),
            egui::Shape::Text(t) if t.galley.text().starts_with("column_number_") => {
                seen.push(t.galley.text().to_owned());
            }
            _ => {}
        }
    }
    let mut seen = Vec::new();
    out.shapes.iter().for_each(|c| walk(&c.shape, &mut seen));
    seen
}

/// A table in a plane's *foreground* scrolls when the pointer is over it.
#[test]
fn a_table_in_the_chrome_can_be_scrolled() {
    if !has_pyarrow() {
        eprintln!("no pyarrow in this interpreter; skipping");
        return;
    }
    let (mut ws, ctx) = planed(wide_table(), Layer::Foreground);
    for _ in 0..2 {
        shown(&mut ws, &ctx, Vec::new());
    }
    let before = shown(&mut ws, &ctx, Vec::new());
    assert!(
        before.len() > 1 && before.len() < 14,
        "some columns show and some are off the edge: {before:?}"
    );

    // Scroll sideways, with the pointer over the table.
    let over = egui::pos2(120.0, 40.0);
    shown(&mut ws, &ctx, vec![egui::Event::PointerMoved(over)]);
    for _ in 0..4 {
        shown(
            &mut ws,
            &ctx,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(-90.0, 0.0),
                modifiers: Default::default(),
                phase: egui::TouchPhase::Move,
            }],
        );
    }
    let after = shown(&mut ws, &ctx, Vec::new());
    assert_ne!(
        before, after,
        "scrolling reached the table and moved it along"
    );
}
