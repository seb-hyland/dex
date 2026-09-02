//! Exercises `examples/scatterplot.py`: an infinite scatter plot built out of
//! the canvas itself, with the data points as items and the grid, axes and
//! captions as backgrounds.
//!
//! Three parts. The scale arithmetic is pure and is checked directly, because
//! it is where a plot goes quietly wrong — a gridline off by a cell, or a y
//! axis that forgot to flip. Then the scene the transform builds: a dot per
//! point, sitting where its value reads, with nothing behind its lens so the
//! surface leaves it alone. Then a real frame, because a Python exception mid-draw is painted as
//! an error rather than raised, so the only way to know the backgrounds ran is
//! to count what they put on the screen.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{
    CanvasChildren, CanvasLayerNodes, CanvasViewOrigin, ConnectableAt, Layer, NodeScreenRect,
};
use dex_nodes::layouts::canvas::nodes::CanvasItemBounds;
use dex_nodes::scripting::{ScriptOutput, run_script};
use pyo3::prelude::*;
use pyo3::types::PyDict;

const SCATTERPLOT: &str = include_str!("../../../examples/scatterplot.py");
const SCREEN: egui::Vec2 = egui::vec2(1000.0, 700.0);

/// Run the example module and return its namespace, for the pure functions.
fn load_example<'py>(py: Python<'py>) -> Bound<'py, PyDict> {
    let globals = PyDict::new(py);
    globals
        .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
        .unwrap();
    let src = std::ffi::CString::new(SCATTERPLOT).unwrap();
    py.run(src.as_c_str(), Some(&globals), Some(&globals))
        .expect("example module runs");
    globals
}

/// Evaluate `expr` in the example's namespace.
fn eval<T>(py: Python<'_>, globals: &Bound<'_, PyDict>, expr: &str) -> T
where
    T: for<'a, 'py> pyo3::FromPyObject<'a, 'py>,
{
    let code = std::ffi::CString::new(expr).unwrap();
    let value = py
        .eval(code.as_c_str(), Some(globals), None)
        .unwrap_or_else(|e| panic!("`{expr}` evaluates: {e}"));
    match value.extract() {
        Ok(out) => out,
        Err(_) => panic!("`{expr}` has the expected type"),
    }
}

/// Run the example's `transform()` into `ws`, exactly as a lambda would, and
/// return the canvas it built.
fn build_plot(ws: &mut Workspace) -> NodeUid {
    let graph = GraphSnapshot::capture(ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let built = match run_script(SCATTERPLOT, "", &handle, &[], graph) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the plot returns the canvas it built"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    built
}

/// A step is a 1, 2 or 5 times a power of ten, and cuts the span into roughly
/// the number of cells asked for.
#[test]
fn a_cell_is_a_round_number_of_data_units() {
    dex_nodes::scripting::init_python();
    Python::attach(|py| {
        let globals = load_example(py);

        for (span, want) in [
            (1.0, 0.2),
            (4.7, 1.0),
            (10.0, 2.0),
            (23.0, 5.0),
            (100.0, 20.0),
            (0.037, 0.01),
        ] {
            let got: f64 = eval(py, &globals, &format!("nice_step({span}, 5)"));
            assert!(
                (got - want).abs() < 1e-12,
                "a span of {span} steps by {got}, not {want}"
            );
        }

        // Whatever the span, the step lands the cell count near the target.
        for span in [0.003, 0.4, 7.0, 55.0, 1234.0, 98765.0] {
            let step: f64 = eval(py, &globals, &format!("nice_step({span}, 5)"));
            let cells = span / step;
            assert!(
                (2.0..=10.0).contains(&cells),
                "a span of {span} splits into {cells} cells"
            );
        }

        // One step for both axes, taken from the wider of the two: a square
        // grid measuring x differently from y would misshape the scatter.
        let step: f64 = eval(py, &globals, "data_step(sample_series())");
        let (x_span, y_span): (f64, f64) = eval(
            py,
            &globals,
            "(lambda ps: (max(x for (x, _y) in ps) - min(x for (x, _y) in ps), \
                          max(y for (_x, y) in ps) - min(y for (_x, y) in ps)))\
             ([p for (_n, _c, ps) in sample_series() for p in ps])",
        );
        let want: f64 = eval(
            py,
            &globals,
            &format!("nice_step(max({x_span}, {y_span}), TARGET_CELLS)"),
        );
        assert!(
            (step - want).abs() < 1e-12,
            "the cell is {step}, not the {want} the wider axis asks for"
        );
    });
}

/// A cell is a step, y flips — data grows up, the canvas grows down — and the
/// plot's origin sits on a whole cell, so the gridlines still fall on round
/// data values.
#[test]
fn the_mapping_lands_data_on_the_gridlines() {
    dex_nodes::scripting::init_python();
    Python::attach(|py| {
        let globals = load_example(py);
        let cell: f64 = eval(py, &globals, "CELL");
        let (ox, oy): (f64, f64) = eval(py, &globals, "plot_origin()");

        // Whole cells, or a gridline would fall between two round values.
        for (edge, name) in [(ox, "x"), (oy, "y")] {
            let cells = edge / cell;
            assert!(
                (cells - cells.round()).abs() < 1e-9,
                "the plot's {name} origin is {edge}, which is not a whole cell"
            );
        }
        // Below the plane's origin, or every positive value would be plotted
        // above the top of the surface and never seen.
        assert!(oy > 0.0, "the plot hangs off the top of the plane at {oy}");

        for (point, (want_x, want_y), why) in [
            ("(0.0, 0.0)", (ox, oy), "data zero is the plot's origin"),
            (
                "(2.0, 0.0)",
                (ox + 2.0 * cell, oy),
                "a step of x is a cell right",
            ),
            (
                "(0.0, 2.0)",
                (ox, oy - 2.0 * cell),
                "a step of y is a cell *up*",
            ),
            (
                "(-1.0, -3.0)",
                (ox - cell, oy + 3.0 * cell),
                "and it works backwards",
            ),
        ] {
            let (x, y): (f64, f64) = eval(py, &globals, &format!("to_canvas({point}, 1.0)"));
            assert!(
                (x - want_x).abs() < 1e-9 && (y - want_y).abs() < 1e-9,
                "{why}: {point} mapped to ({x}, {y}), not ({want_x}, {want_y})"
            );
        }

        // Reading a coordinate back is what captions a gridline, so it has to
        // be the exact inverse — on both axes, which differ by that flip.
        for (point, step) in [
            ("(0.0, 0.0)", 1.0),
            ("(3.0, 7.0)", 1.0),
            ("(-2.5, 4.25)", 0.5),
            ("(40.0, -60.0)", 20.0),
        ] {
            let (x, y): (f64, f64) = eval(py, &globals, &format!("to_canvas({point}, {step})"));
            let back: (f64, f64) = eval(
                py,
                &globals,
                &format!("(x_value({x}, {step}), y_value({y}, {step}))"),
            );
            let want: (f64, f64) = eval(py, &globals, point);
            assert!(
                (back.0 - want.0).abs() < 1e-9 && (back.1 - want.1).abs() < 1e-9,
                "{point} mapped to ({x}, {y}) and read back as {back:?}"
            );
        }
    });
}

/// Only the gridlines inside the visible span are drawn, and they are the ones
/// on a cell — which is what keeps an infinite plane affordable.
#[test]
fn only_the_visible_gridlines_are_asked_for() {
    dex_nodes::scripting::init_python();
    Python::attach(|py| {
        let globals = load_example(py);
        let cell: f64 = eval(py, &globals, "CELL");

        for (low, high) in [
            (0.0, 3.5 * cell),
            (-2.2 * cell, 1.1 * cell),
            (1000.0 * cell, 1003.0 * cell),
        ] {
            let lines: Vec<f64> = eval(py, &globals, &format!("cell_lines({low}, {high})"));
            assert!(!lines.is_empty(), "{low}..{high} crosses a gridline");
            for line in &lines {
                assert!(
                    *line >= low - 1e-9 && *line <= high + 1e-9,
                    "a line at {line} is outside {low}..{high}"
                );
                let cells = line / cell;
                assert!(
                    (cells - cells.round()).abs() < 1e-9,
                    "a line at {line} is not on a cell"
                );
            }
            // Nothing between the last line and the edge, on either side.
            assert!(
                lines[0] - low < cell + 1e-9 && high - lines[lines.len() - 1] < cell + 1e-9,
                "{low}..{high} skipped a line at an edge"
            );
        }

        // A span narrower than a cell may cross nothing at all.
        let none: Vec<f64> = eval(py, &globals, &format!("cell_lines(0.1, {})", cell - 0.1));
        assert!(none.is_empty(), "a span between two lines crosses neither");
    });
}

/// A caption carries the decimals its step needs, and no others.
#[test]
fn a_caption_matches_the_step_it_came_from() {
    dex_nodes::scripting::init_python();
    Python::attach(|py| {
        let globals = load_example(py);
        for (value, step, want) in [
            (2.0, 1.0, "2"),
            (2.5, 0.5, "2.5"),
            (0.03, 0.01, "0.03"),
            (-4.0, 2.0, "-4"),
            (1500.0, 500.0, "1500"),
        ] {
            let got: String = eval(py, &globals, &format!("tick_text({value}, {step})"));
            assert_eq!(got, want, "{value} on a step of {step}");
        }
        // Negative zero is a rounding artefact, not a value.
        let got: String = eval(py, &globals, "tick_text(-0.0, 1.0)");
        assert_eq!(got, "0");
    });
}

/// The transform builds a canvas: one item per data point, sitting where its
/// value reads, between the two backgrounds that rule and caption the plane and
/// the two foregrounds that label and read it. Every dot declines an inspector
/// — content, not a handle.
#[test]
fn the_plot_is_a_canvas_of_plain_points_between_its_layers() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let canvas = build_plot(&mut ws);

    let backgrounds = ws
        .send_request(
            canvas,
            CanvasLayerNodes {
                layer: Layer::Background,
            },
        )
        .expect("the plot built a canvas");
    assert_eq!(
        backgrounds.len(),
        2,
        "the graph paper and the axes are both under the items"
    );
    let foregrounds = ws
        .send_request(
            canvas,
            CanvasLayerNodes {
                layer: Layer::Foreground,
            },
        )
        .unwrap_or_default();
    assert_eq!(
        foregrounds.len(),
        2,
        "the legend and the readout are both over them"
    );

    let items = ws.send_request(canvas, CanvasChildren).unwrap_or_default();

    // Where every point should have landed, by the plot's own mapping.
    let expected: Vec<(f64, f64)> = Python::attach(|py| {
        let globals = load_example(py);
        eval(
            py,
            &globals,
            "[to_canvas(p, data_step(sample_series())) \
              for (_n, _c, ps) in sample_series() for p in ps]",
        )
    });
    assert!(
        expected.len() > 30,
        "the sample data is worth plotting: {} points",
        expected.len()
    );
    assert_eq!(
        items.len(),
        expected.len(),
        "one canvas item per data point"
    );

    // Every item is a circle centred on the point it stands for. Matched by
    // position rather than by order, since the canvas owns the draw order.
    let mut centres: Vec<(f64, f64)> = items
        .iter()
        .map(|&item| {
            let bounds = ws
                .send_request(item, CanvasItemBounds)
                .unwrap_or_else(|| panic!("item {item:?} reports canvas bounds"));
            let (min, size) = (bounds.min, bounds.size());
            ((min.x + size.x / 2.0) as f64, (min.y + size.y / 2.0) as f64)
        })
        .collect();

    for (want_x, want_y) in &expected {
        match centres
            .iter()
            .position(|(x, y)| (x - want_x).abs() < 0.01 && (y - want_y).abs() < 0.01)
        {
            Some(i) => {
                centres.remove(i);
            }
            None => panic!("no dot sits at the mapped ({want_x}, {want_y})"),
        }
    }

    // No menu behind a dot, so no lens over one, no wire onto one, and no grab
    // when the plot is dragged across. The surface looks straight through the
    // whole scatter.
    for &item in &items {
        assert_eq!(
            ws.send_request(item, Inspectable),
            Some(false),
            "the dot at {item:?} is content, not a handle"
        );
    }
    let over_a_dot = ws
        .send_request(canvas, CanvasChildren)
        .and_then(|items| items.first().copied())
        .and_then(|item| ws.send_request(item, CanvasItemBounds))
        .map(|bounds| ScreenPos {
            x: (bounds.min.x + bounds.max.x) / 2.0,
            y: (bounds.min.y + bounds.max.y) / 2.0,
        })
        .expect("a dot to point at");
    assert_eq!(
        ws.send_request(canvas, ConnectableAt { pos: over_a_dot }),
        Some(None),
        "the surface finds nothing to connect to over a dot"
    );
}

/// The plot draws through a real frame: the dots reach the screen as circles,
/// and the backgrounds rule and caption the whole viewport under them. A Python
/// exception mid-draw is painted as an error rather than raised, so a missing
/// caption is how a broken background shows up here.
#[test]
fn the_backgrounds_rule_and_caption_the_whole_viewport() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let canvas = build_plot(&mut ws);
    ws.set_root(canvas);
    ws.process_pending();

    let egui_ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&egui_ctx);
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let mut output = None;
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        let ws = &mut ws;
        output = Some(egui_ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                ws.draw_frame(ui, rect);
            });
        }));
    }
    let output = output.expect("the canvas drew");

    let (points, names): (usize, usize) = Python::attach(|py| {
        let globals = load_example(py);
        eval(
            py,
            &globals,
            "(sum(len(ps) for (_n, _c, ps) in sample_series()), len(sample_series()))",
        )
    });

    let circles = output
        .shapes
        .iter()
        .filter(|c| matches!(c.shape, egui::Shape::Circle(_)))
        .count();
    assert_eq!(
        circles,
        points + names,
        "a dot for each of {points} points, and a legend swatch for each of {names} series"
    );

    // The grid and the axes are ruled across the viewport, and every visible
    // gridline is captioned: how many depends on the size of the screen, so
    // this counts what the plot's own arithmetic says should be there.
    let (grid, captions): (usize, usize) = Python::attach(|py| {
        let globals = load_example(py);
        eval(
            py,
            &globals,
            &format!(
                "(lambda xs, ys: (len(xs) + len(ys), len(xs) + len(ys)))\
                 (cell_lines(0.0, {w}), cell_lines(0.0, {h}))",
                w = SCREEN.x,
                h = SCREEN.y
            ),
        )
    });
    let lines = output
        .shapes
        .iter()
        .filter(|c| matches!(c.shape, egui::Shape::Path(_)))
        .count();
    let texts = output
        .shapes
        .iter()
        .filter(|c| matches!(c.shape, egui::Shape::Text(_)))
        .count();

    // The grid, plus the two axes.
    assert_eq!(
        lines,
        grid + 2,
        "the paper ruled {grid} visible lines and the axes added two"
    );
    assert_eq!(
        texts,
        captions + names,
        "every one of the {captions} visible gridlines is captioned, \
         and the legend names {names} series"
    );

    // Nothing is hovered, so the readout drew nothing at all.
    assert!(
        !output
            .shapes
            .iter()
            .any(|c| matches!(&c.shape, egui::Shape::Text(t) if t.galley.text().contains('('))),
        "the readout stays away until something is pointed at"
    );
}

/// Pointing at a dot names its series and reads out its value. One sensor
/// covers the whole plot and the nearest point is searched for, so this is the
/// only thing that proves the search found the right one.
#[test]
fn pointing_at_a_dot_reads_out_its_value() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let canvas = build_plot(&mut ws);
    ws.set_root(canvas);
    ws.process_pending();

    let egui_ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&egui_ctx);
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let frame = |ws: &mut Workspace, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(rect),
            events,
            ..Default::default()
        };
        egui_ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                ws.draw_frame(ui, rect);
            });
        })
    };
    frame(&mut ws, vec![]);

    // A dot that is actually on screen, and where its centre landed.
    let dot = ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .into_iter()
        .find_map(|item| {
            let screen = ws
                .send_request(canvas, NodeScreenRect { node: item })
                .flatten()?;
            let centre = egui::pos2(
                (screen.min.x + screen.max.x) / 2.0,
                (screen.min.y + screen.max.y) / 2.0,
            );
            rect.shrink(60.0).contains(centre).then_some(centre)
        })
        .expect("some dot is well inside the viewport");

    // Two frames: the sensor learns the pointer on one, the readout reads it
    // on the next.
    frame(&mut ws, vec![egui::Event::PointerMoved(dot)]);
    let output = frame(&mut ws, vec![egui::Event::PointerMoved(dot)]);

    let captions: Vec<String> = output
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect();

    // Exactly what the plot says that point should read as.
    let expected: Vec<String> = Python::attach(|py| {
        let globals = load_example(py);
        eval(
            py,
            &globals,
            "[f'{n}  ' + readout_text(p, data_step(sample_series())) \
              for (n, _c, ps) in sample_series() for p in ps]",
        )
    });
    let read = captions
        .iter()
        .find(|caption| expected.contains(caption))
        .unwrap_or_else(|| {
            panic!("nothing read out over a dot at {dot:?}; captions were {captions:?}")
        });
    assert!(
        read.contains('(') && read.contains(','),
        "the readout carries the value, not just the name: {read}"
    );

    // And the ring: one circle more than the dots and the legend swatches.
    let (points, names): (usize, usize) = Python::attach(|py| {
        let globals = load_example(py);
        eval(
            py,
            &globals,
            "(sum(len(ps) for (_n, _c, ps) in sample_series()), len(sample_series()))",
        )
    });
    let circles = output
        .shapes
        .iter()
        .filter(|c| matches!(c.shape, egui::Shape::Circle(_)))
        .count();
    assert_eq!(
        circles,
        points + names + 1,
        "the point being read is ringed"
    );

    // Point at empty sky and it goes away again.
    let empty = egui::pos2(rect.max.x - 30.0, rect.max.y - 30.0);
    frame(&mut ws, vec![egui::Event::PointerMoved(empty)]);
    let output = frame(&mut ws, vec![egui::Event::PointerMoved(empty)]);
    let still_there = output.shapes.iter().any(|c| match &c.shape {
        egui::Shape::Text(t) => expected.iter().any(|e| e == t.galley.text()),
        _ => false,
    });
    assert!(
        !still_there,
        "the readout clears when nothing is under the pointer"
    );
}

/// The whole point of a scatter with nothing behind its lens: a drag that
/// starts on a dot pans the
/// plot. With an ordinary canvas item there, the surface would decide the drag
/// had grabbed the dot and stay put.
#[test]
fn dragging_across_the_scatter_pans_the_plot() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let canvas = build_plot(&mut ws);
    ws.set_root(canvas);
    ws.process_pending();

    let egui_ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&egui_ctx);
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let frame = |ws: &mut Workspace, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(rect),
            events,
            ..Default::default()
        };
        let _ = egui_ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                ws.draw_frame(ui, rect);
            });
        });
    };
    frame(&mut ws, vec![]);

    // Somewhere a dot actually is, on screen.
    let dot = ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .into_iter()
        .find_map(|item| {
            let bounds = ws.send_request(item, CanvasItemBounds)?;
            let screen = ws
                .send_request(canvas, NodeScreenRect { node: item })
                .flatten()?;
            let centre = egui::pos2(
                (screen.min.x + screen.max.x) / 2.0,
                (screen.min.y + screen.max.y) / 2.0,
            );
            (rect.contains(centre) && bounds.size().x > 0.0).then_some(centre)
        })
        .expect("some dot is on screen");

    let before = ws
        .send_request(canvas, CanvasViewOrigin)
        .expect("the surface reports its view origin");

    frame(&mut ws, vec![egui::Event::PointerMoved(dot)]);
    frame(&mut ws, vec![egui::Event::PointerMoved(dot)]);
    frame(
        &mut ws,
        vec![egui::Event::PointerButton {
            pos: dot,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }],
    );
    for step in 1..=3 {
        let t = step as f32 / 3.0;
        frame(
            &mut ws,
            vec![egui::Event::PointerMoved(egui::pos2(
                dot.x - 90.0 * t,
                dot.y - 60.0 * t,
            ))],
        );
    }
    frame(
        &mut ws,
        vec![egui::Event::PointerButton {
            pos: egui::pos2(dot.x - 90.0, dot.y - 60.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );

    let after = ws
        .send_request(canvas, CanvasViewOrigin)
        .expect("the surface reports its view origin");
    assert!(
        (after.x - before.x).abs() > 1.0 || (after.y - before.y).abs() > 1.0,
        "the drag panned the plot: the view was at ({}, {}) and is at ({}, {})",
        before.x,
        before.y,
        after.x,
        after.y
    );
}
