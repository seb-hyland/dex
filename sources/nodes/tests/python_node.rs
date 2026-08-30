//! Exercises `examples/tiled_layout.py`: a layout node defined entirely in
//! Python, including the message handlers it declares.

use dex_core::prelude::*;
use dex_nodes::primitives::text::Label;
use dex_nodes::scripting::to_dyn_node_py;
use pyo3::prelude::*;
use pyo3::types::PyDict;

const TILED_LAYOUT: &str = include_str!("../../../examples/tiled_layout.py");

/// Run the example module and return its namespace.
fn load_example<'py>(py: Python<'py>) -> Bound<'py, PyDict> {
    let globals = PyDict::new(py);
    globals
        .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
        .unwrap();
    let src = std::ffi::CString::new(TILED_LAYOUT).unwrap();
    py.run(src.as_c_str(), Some(&globals), Some(&globals))
        .expect("example module runs");
    globals
}

/// The subdivision the layout documents, checked against the spec directly.
#[test]
fn tiles_subdivide_by_repeated_halving() {
    dex_nodes::scripting::init_python();
    Python::attach(|py| {
        let globals = load_example(py);

        let expected: [(usize, Vec<f64>); 5] = [
            (1, vec![1.0]),
            (2, vec![0.5, 0.5]),
            (3, vec![0.25, 0.25, 0.5]),
            (4, vec![0.25, 0.25, 0.25, 0.25]),
            (5, vec![0.125, 0.125, 0.25, 0.25, 0.25]),
        ];

        for (n, want) in expected {
            globals.set_item("n", n).unwrap();
            let got: Vec<f64> = py
                .eval(c"tile_fractions(n)", Some(&globals), None)
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(got, want, "wrong tiling for {n} children");
            let total: f64 = got.iter().sum();
            assert!((total - 1.0).abs() < 1e-9, "{n} children cover {total}");
        }

        // Boxes form a real 2D tiling: inside bounds, pairwise disjoint, and
        // covering the whole area.
        for n in 1..=9usize {
            globals.set_item("n", n).unwrap();
            let boxes: Vec<(f64, f64, f64, f64)> = py
                .eval(
                    c"TiledLayout([None]*n).boxes(800.0, 600.0)",
                    Some(&globals),
                    None,
                )
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(boxes.len(), n);

            let mut area = 0.0;
            for (x, y, w, h) in &boxes {
                assert!(*w > 0.0 && *h > 0.0, "n={n}: degenerate tile {w}x{h}");
                assert!(
                    *x >= -1e-9 && *y >= -1e-9 && x + w <= 800.0 + 1e-9 && y + h <= 600.0 + 1e-9,
                    "n={n}: tile ({x},{y},{w},{h}) escapes the area"
                );
                area += w * h;
            }
            assert!(
                (area - 800.0 * 600.0).abs() < 1e-6,
                "n={n}: tiles cover {area} of 480000"
            );

            // Pairwise disjoint (open interiors do not intersect).
            for (i, a) in boxes.iter().enumerate() {
                for b in &boxes[i + 1..] {
                    let overlap_x = (a.0 + a.2).min(b.0 + b.2) - a.0.max(b.0);
                    let overlap_y = (a.1 + a.3).min(b.1 + b.3) - a.1.max(b.1);
                    assert!(
                        overlap_x <= 1e-9 || overlap_y <= 1e-9,
                        "n={n}: tiles {a:?} and {b:?} overlap"
                    );
                }
            }
        }

        // Axes alternate, so three children give a halved left column beside a
        // full-height right half -- not three strips.
        globals.set_item("n", 3usize).unwrap();
        let boxes: Vec<(f64, f64, f64, f64)> = py
            .eval(
                c"TiledLayout([None]*n).boxes(800.0, 600.0)",
                Some(&globals),
                None,
            )
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(boxes[0], (0.0, 0.0, 400.0, 300.0), "top-left quarter");
        assert_eq!(boxes[1], (0.0, 300.0, 400.0, 300.0), "bottom-left quarter");
        assert_eq!(
            boxes[2],
            (400.0, 0.0, 400.0, 600.0),
            "full-height right half"
        );

        // The first split follows `axis`.
        let vertical_first: Vec<(f64, f64, f64, f64)> = py
            .eval(
                c"TiledLayout([None]*2, axis=VERTICAL).boxes(800.0, 600.0)",
                Some(&globals),
                None,
            )
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(vertical_first[0], (0.0, 0.0, 800.0, 300.0), "top half");
        assert_eq!(vertical_first[1], (0.0, 300.0, 800.0, 300.0), "bottom half");

        // No children is not an error.
        globals.set_item("n", 0usize).unwrap();
        let empty: Vec<f64> = py
            .eval(c"tile_fractions(n)", Some(&globals), None)
            .unwrap()
            .extract()
            .unwrap();
        assert!(empty.is_empty());
    });
}

/// A Python node answers requests and handles actions like a Rust one.
#[test]
fn a_python_node_handles_messages() {
    use dex_nodes::layouts::horizontal_dnd::{AddChild, ChildCount, RemoveChild};

    dex_nodes::scripting::init_python();

    let mut ws = Workspace::new_empty();
    let one = ws.insert_node_now(Label::new("one".to_owned())).erase();
    let two = ws.insert_node_now(Label::new("two".to_owned())).erase();
    let three = ws.insert_node_now(Label::new("three".to_owned())).erase();
    let extra = ws.insert_node_now(Label::new("extra".to_owned())).erase();

    // Build the Python layout over those three children.
    let layout = Python::attach(|py| {
        let globals = load_example(py);
        for (name, uid) in [("a", one), ("b", two), ("c", three)] {
            globals
                .set_item(name, Py::new(py, NodeHandle(uid)).unwrap())
                .unwrap();
        }
        let obj = py
            .eval(c"TiledLayout([a, b, c])", Some(&globals), None)
            .expect("layout constructs");
        to_dyn_node_py(&obj)
    });

    let uid = ws.insert_node_dyn(layout);
    ws.process_pending();

    // A request the script answers.
    assert_eq!(ws.send_request(uid, ChildCount), Some(3));

    // An action the script handles.
    ws.submit_action(uid, "add a child", AddChild { child: extra });
    ws.process_pending();
    assert_eq!(ws.send_request(uid, ChildCount), Some(4));

    // ...and one that removes, which needs handle equality to work.
    ws.submit_action(uid, "remove a child", RemoveChild { child: two });
    ws.process_pending();
    assert_eq!(ws.send_request(uid, ChildCount), Some(3));
}

/// Declining with `NotImplemented` leaves the message unhandled, rather than
/// swallowing it or answering with `None`.
#[test]
fn a_python_node_can_decline_a_message() {
    use dex_nodes::primitives::text::GetText;

    dex_nodes::scripting::init_python();

    let mut ws = Workspace::new_empty();
    let layout = Python::attach(|py| {
        let globals = load_example(py);
        let obj = py
            .eval(c"TiledLayout([])", Some(&globals), None)
            .expect("layout constructs");
        to_dyn_node_py(&obj)
    });
    let uid = ws.insert_node_dyn(layout);
    ws.process_pending();

    // `request` returns NotImplemented for anything but ChildCount.
    assert_eq!(ws.send_request(uid, GetText), None);
}

/// A handler mutates a copy, so the version already committed to history is
/// untouched — `DynamicNode::clone` only bumps a refcount, so this is the
/// property that would silently break undo if handlers mutated in place.
#[test]
fn handling_an_action_does_not_mutate_past_versions() {
    use dex_nodes::layouts::horizontal_dnd::{AddChild, ChildCount};

    dex_nodes::scripting::init_python();

    let mut ws = Workspace::new_empty();
    let child = ws.insert_node_now(Label::new("c".to_owned())).erase();
    let seed = ws.insert_node_now(Label::new("seed".to_owned())).erase();

    // Seeded with a handle and a colour, so the copy has to reach through
    // bound pyclasses — an empty layout would pass even without them.
    let layout = Python::attach(|py| {
        let globals = load_example(py);
        globals
            .set_item("seed", Py::new(py, NodeHandle(seed)).unwrap())
            .unwrap();
        let obj = py
            .eval(c"TiledLayout([seed])", Some(&globals), None)
            .unwrap();
        to_dyn_node_py(&obj)
    });
    let uid = ws.insert_node_dyn(layout);
    ws.process_pending();

    // Hold the node as the registry has it now.
    let before = ws.get_node(uid).expect("node is live");

    ws.submit_action(uid, "add a child", AddChild { child });
    ws.process_pending();

    // The committed version sees the child...
    assert_eq!(ws.send_request(uid, ChildCount), Some(2));
    // ...while the version captured beforehand still does not.
    let ctx_id = uid;
    let earlier = before.request_dyn(
        Box::new(ChildCount),
        NodeContext {
            id: ctx_id,
            workspace: &ws,
        },
    );
    let earlier: Option<usize> = earlier.ok().map(dex_core::messages::downcast_resp);
    assert_eq!(earlier, Some(1), "the past version was mutated in place");
}

/// Drives a real `draw` through an egui context, so the painting path is
/// exercised rather than only the geometry that feeds it.
#[test]
fn the_layout_draws_a_coloured_tile_per_child() {
    dex_nodes::scripting::init_python();

    let mut ws = Workspace::new_empty();
    let layout = Python::attach(|py| {
        let globals = load_example(py);
        let obj = py
            .eval(
                c"TiledLayout([dex.Label.new('a'), dex.Label.new('b'), dex.Label.new('c')])",
                Some(&globals),
                None,
            )
            .expect("layout constructs");
        to_dyn_node_py(&obj)
    });
    let uid = ws.insert_node_dyn(layout);
    ws.set_root(uid);
    ws.process_pending();

    let egui_ctx = egui::Context::default();
    // Areas fade in; without this the tiles are still part-transparent.
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        egui_ctx.style_mut_of(theme, |style| style.animation_time = 0.0);
    }
    // Without a screen rect the viewport is degenerate and every shape clips away.
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    };
    // The workspace paints into an `egui::Area`, whose layout state only exists
    // from the second frame onward, so one frame alone paints nothing.
    let mut output = None;
    for _ in 0..2 {
        output = Some(egui_ctx.run_ui(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let area =
                    egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
                ws.draw_frame(ui, area);
            });
        }));
    }
    let output = output.unwrap();

    // One filled background rect per tile, each a different palette colour.
    let fills: Vec<egui::Color32> = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r) => Some(r.fill),
            _ => None,
        })
        .collect();

    let palette: Vec<egui::Color32> = [(232, 110, 110), (232, 168, 96), (226, 208, 104)]
        .into_iter()
        .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b))
        .collect();
    for (i, want) in palette.iter().enumerate() {
        assert!(fills.contains(want), "tile {i} was not painted {want:?}");
    }

    // The layout reported the area it occupied, the way a Rust node does —
    // rather than the host inferring it from what got painted.
    let reported = Python::attach(|py| {
        let globals = load_example(py);
        let node = py
            .eval(
                c"TiledLayout([dex.Label.new('a'), dex.Label.new('b')])",
                Some(&globals),
                None,
            )
            .unwrap();
        let arc = to_dyn_node_py(&node);
        let ws2 = Workspace::new_empty();
        let mut out = None;
        let ctx2 = egui::Context::default();
        let _ = ctx2.run_ui(input.clone(), |c| {
            egui::CentralPanel::default().show(c, |ui| {
                let mut ui = ui.new_child(egui::UiBuilder::new());
                let mut dc = DrawContext::for_ui(
                    NodeContext {
                        id: NodeUid::nil(),
                        workspace: &ws2,
                    },
                    DrawConstraints {
                        pos: ScreenPos { x: 10.0, y: 20.0 },
                        x: Some(AxisConstraint::Exactly(400.0)),
                        y: Some(AxisConstraint::Exactly(300.0)),
                        wrap: WrapConstraints::NotAllowed,
                        should_clip: true,
                    },
                    &mut ui,
                );
                let cs = dc.constraints;
                out = dc.draw_node(&*arc, cs).region();
            });
        });
        out
    })
    .expect("the layout reported a region");
    assert_eq!(reported.min.x, 10.0);
    assert_eq!(reported.min.y, 20.0);
    assert_eq!(reported.size().x, 400.0);
    assert_eq!(reported.size().y, 300.0);

    // The tiles really are distinct colours, not one repeated.
    let mut distinct = palette.clone();
    distinct.dedup();
    assert_eq!(distinct.len(), 3, "palette colours must differ");

    // And the labels came through as text.
    let texts: Vec<String> = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    for want in ["a", "b", "c"] {
        assert!(
            texts.iter().any(|t| t == want),
            "missing label {want} in {texts:?}"
        );
    }
}

/// A script-defined node holding bound values survives a save/load round trip.
///
/// `#[pyclass]` state lives in Rust rather than a Python `__dict__`, so without
/// a generated `__reduce__` the pickle protocol raises `cannot pickle` — and
/// `DynamicNode`'s serialiser swallows that into an empty buffer, silently
/// losing the node on reload.
#[test]
fn a_python_node_holding_bound_values_persists() {
    use dex_nodes::layouts::horizontal_dnd::ChildCount;
    use dex_nodes::primitives::dynamic::DynamicNode;

    dex_nodes::scripting::init_python();

    let mut ws = Workspace::new_empty();
    let child = ws.insert_node_now(Label::new("kept".to_owned())).erase();

    let node = Python::attach(|py| {
        let globals = load_example(py);
        globals
            .set_item("child", Py::new(py, NodeHandle(child)).unwrap())
            .unwrap();
        let obj = py
            .eval(
                c"TiledLayout([child, dex.Label.new('inline')], axis=VERTICAL)",
                Some(&globals),
                None,
            )
            .expect("layout constructs");
        DynamicNode::from_python(&obj)
    });

    // Exactly what the workspace save path does.
    let saved = serde_json::to_string(&node).expect("node serialises");
    let captured: Vec<u8> = serde_json::from_str(&saved).unwrap();
    assert!(
        !captured.is_empty(),
        "the node was dropped rather than captured"
    );

    let restored: DynamicNode = serde_json::from_str(&saved).expect("node deserialises");

    // The restored node is live: it still answers, and still holds both children.
    let uid = ws.insert_node_dyn(std::sync::Arc::new(restored));
    ws.process_pending();
    assert_eq!(ws.send_request(uid, ChildCount), Some(2));

    // ...and the handle among them still points at the same workspace node.
    let first: Option<NodeHandle> = Python::attach(|py| {
        let obj = ws.get_node(uid)?;
        // `.as_ref()` first: on the `Arc` itself, `as_any_ref` would erase the
        // pointer rather than the node inside it.
        let dynamic = obj.as_ref().as_any_ref().downcast_ref::<DynamicNode>()?;
        let bound = dynamic.object()?.bind(py);
        bound
            .getattr("children")
            .ok()?
            .get_item(0)
            .ok()?
            .extract::<NodeHandle>()
            .ok()
    });
    assert_eq!(
        first.map(|h| h.0),
        Some(child),
        "the handle did not survive"
    );
}

#[test]
fn a_python_node_can_report_any_draw_result() {
    dex_nodes::scripting::init_python();

    let ws = Workspace::new_empty();
    let egui_ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    };

    let source = c"
class Wrapper:
    def draw(self, ctx):
        return dex.DrawResult.Wrap(
            region=dex.ScreenRegion.from_min_size(
                dex.ScreenPos.new(1.0, 2.0), dex.Vector.new(30.0, 40.0)
            ),
            continuation=7,
        )
";

    let arc = Python::attach(|py| {
        let globals = load_example(py);
        py.run(source, Some(&globals), Some(&globals))
            .expect("class defines");
        let obj = py
            .eval(c"Wrapper()", Some(&globals), None)
            .expect("node constructs");
        to_dyn_node_py(&obj)
    });

    let mut result = None;
    let _ = egui_ctx.run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| {
            let mut ui = ui.new_child(egui::UiBuilder::new());
            let mut dc = DrawContext::for_ui(
                NodeContext {
                    id: NodeUid::nil(),
                    workspace: &ws,
                },
                DrawConstraints {
                    pos: ScreenPos { x: 0.0, y: 0.0 },
                    x: Some(AxisConstraint::Exactly(400.0)),
                    y: Some(AxisConstraint::Exactly(300.0)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                },
                &mut ui,
            );
            let cs = dc.constraints;
            result = Some(dc.draw_node(&*arc, cs));
        });
    });

    match result.expect("drew") {
        DrawResult::Wrap {
            region,
            continuation,
        } => {
            assert_eq!(continuation, 7);
            let region = region.expect("wrap carried a region");
            assert_eq!(region.min.x, 1.0);
            assert_eq!(region.size().x, 30.0);
        }
        DrawResult::Complete { .. } => panic!("the script asked to wrap, host saw Complete"),
    }
}

/// A script object gets the whole `Node` surface, not just `draw`.
#[test]
fn a_python_node_participates_in_the_node_lifecycle() {
    use dex_nodes::primitives::text::Label;

    dex_nodes::scripting::init_python();

    let mut ws = Workspace::new_empty();
    let owned = ws.insert_node_now(Label::new("owned".to_owned())).erase();
    let target = ws.insert_node_now(Label::new("target".to_owned())).erase();

    let source = c"
TICKS = []

class Full:
    def __init__(self, owned, target):
        self.owned = owned
        self.target = target
    def type_name(self):
        return 'My Node'
    def deref_target(self):
        return self.target
    def tick(self, ctx):
        # Rust's `tick` takes `&self` and cannot mutate; a script must not
        # either, so record the call somewhere that is not node state.
        TICKS.append(ctx.id)
    def on_delete(self, ctx):
        ctx.workspace.delete_node(self.owned)
";

    let (node, globals) = Python::attach(|py| {
        let globals = load_example(py);
        globals
            .set_item("owned", Py::new(py, NodeHandle(owned)).unwrap())
            .unwrap();
        globals
            .set_item("target", Py::new(py, NodeHandle(target)).unwrap())
            .unwrap();
        py.run(source, Some(&globals), Some(&globals)).unwrap();
        let obj = py
            .eval(c"Full(owned, target)", Some(&globals), None)
            .unwrap();
        (to_dyn_node_py(&obj), globals.unbind())
    });

    // `type_name` comes from the script, not a hardcoded placeholder.
    assert_eq!(
        node.type_name(NodeContext {
            id: NodeUid::nil(),
            workspace: &ws,
        }),
        "My Node"
    );
    // `deref_target` forwards, so messages route on down the chain.
    assert_eq!(node.deref_target(), Some(target));

    let uid = ws.insert_node_dyn(node.clone());
    ws.process_pending();

    // `tick` forwards, and carries this node's own id.
    node.tick(NodeContext {
        id: uid,
        workspace: &ws,
    });
    Python::attach(|py| {
        let ticks: Vec<NodeHandle> = globals
            .bind(py)
            .get_item("TICKS")
            .unwrap()
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(ticks.len(), 1, "tick did not reach the script");
        assert_eq!(ticks[0].0, uid, "tick got the wrong node id");
    });

    // `on_delete` runs and can clean up the children the script owns.
    assert!(ws.get_node(owned).is_some(), "child starts alive");
    ws.delete_node(uid);
    ws.process_pending();
    assert!(
        ws.get_node(owned).is_none(),
        "on_delete did not run: the script's child leaked"
    );
}

/// An object defining none of the optional hooks still works.
#[test]
fn the_node_hooks_are_all_optional() {
    dex_nodes::scripting::init_python();

    let node = Python::attach(|py| {
        let globals = load_example(py);
        py.run(
            c"
class Bare:
    def draw(self, ctx):
        return dex.DrawResult.Complete(region=None)
",
            Some(&globals),
            Some(&globals),
        )
        .unwrap();
        to_dyn_node_py(&py.eval(c"Bare()", Some(&globals), None).unwrap())
    });

    assert_eq!(node.deref_target(), None);

    let mut ws = Workspace::new_empty();
    // Falls back to the class name rather than a generic placeholder.
    assert_eq!(
        node.type_name(NodeContext {
            id: NodeUid::nil(),
            workspace: &ws,
        }),
        "Bare"
    );
    let uid = ws.insert_node_dyn(node);
    ws.process_pending();
    // Ticking and deleting a hookless node must not raise.
    ws.delete_node(uid);
    ws.process_pending();
}
