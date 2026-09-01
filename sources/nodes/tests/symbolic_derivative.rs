//! Exercises `examples/symbolic_derivative.py` against a real canvas lambda:
//! `f(x) = x * x`, differentiated into a new canvas lambda.

use dex_core::prelude::*;
use dex_nodes::composites::lambda::{
    AddArgAt, CanvasLambda, ComputeCanvasNode, DataflowInputs, Lambda, LambdaArg, LambdaArgsNode,
    LambdaBody, LambdaName, OutputPin, ParamPins, SetConnection,
};
use dex_nodes::layouts::canvas::layout::PlaceOnCanvas;
use dex_nodes::scripting::DataflowOutput;
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

const DERIVATIVE: &str = include_str!("../../../examples/symbolic_derivative.py");

const MULT_SOURCE: &str = "def transform():\n    return a * b\n";

/// Settle the queue and let every node see the result.
fn settle(ws: &mut Workspace) {
    for _ in 0..4 {
        ws.process_pending();
        ws.tick_all();
        ws.process_pending();
    }
}

/// Give `owner`'s argument row a parameter called `name`; returns its port.
fn add_arg(ws: &Workspace, owner: NodeUid, name: &str) -> NodeUid {
    let handle = ws.action_handle();
    let args = ws
        .send_request(owner, LambdaArgsNode)
        .expect("the lambda exposes its argument row");
    let arg = NodeUid::mint();
    let port = NodeUid::mint();
    LambdaArg::build_with(handle, arg.cast(), port, name.to_owned());
    ws.submit_action(args, "Add argument", AddArgAt { arg });
    port
}

/// `f(x) = x * x`, as a canvas lambda. Returns it and its `Mult`.
fn squared() -> (Workspace, NodeUid, NodeUid) {
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();

    let root = handle.insert_node_dyn(Arc::new(dex_nodes::primitives::nothing::Nothing));
    let outer = handle
        .insert_node(CanvasLambda::new(handle.clone()))
        .erase();
    settle(&mut ws);
    ws.set_root(root);

    add_arg(&ws, outer, "x");
    settle(&mut ws); // the pin for `x` is minted by the lambda's own tick

    // The body: one `Mult`, both inputs wired to the parameter.
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
            MULT_SOURCE.to_owned(),
        )),
    );
    let canvas = ws
        .send_request(outer, ComputeCanvasNode)
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

    (ws, outer, mult)
}

/// Run the example against `f`, apply what it queued, and return its result.
fn differentiate(ws: &mut Workspace, f: NodeUid, var: &str) -> NodeUid {
    let graph = GraphSnapshot::capture(ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let args = [
        ("f".to_owned(), ScriptValue::Node(f)),
        ("var".to_owned(), ScriptValue::Str(var.to_owned())),
    ];
    let out = match run_script(DERIVATIVE, "", &handle, &args, graph) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the transform returns the derivative's id"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    settle(ws);
    out
}

/// The operator at `uid`, normalised, and what its inputs are wired to.
fn operator(ws: &Workspace, uid: NodeUid) -> (String, Vec<Option<NodeUid>>) {
    let name = ws.send_request(uid, LambdaName).unwrap_or_default();
    let inputs = ws.send_request(uid, DataflowInputs).unwrap_or_default();
    (
        name.trim().to_lowercase(),
        inputs.into_iter().map(|(_n, _p, src)| src).collect(),
    )
}

/// Follow a wire to the lambda whose output it points at.
fn wired_to(ws: &Workspace, target: Option<NodeUid>) -> NodeUid {
    let target = target.expect("the input is wired");
    // Every operator this test builds is reached through its output slot.
    let mut uid = target;
    for _ in 0..8 {
        if let Some(owner) = owner_of(ws, uid)
            && ws.send_request(owner, DataflowOutput).flatten() == Some(uid)
        {
            return owner;
        }
        match ws.send_request(uid, dex_nodes::layouts::canvas::nodes::CanvasNodeChild) {
            Some(child) => uid = child,
            None => break,
        }
    }
    uid
}

/// The node that owns `uid`, found the way the snapshot finds it.
fn owner_of(ws: &Workspace, uid: NodeUid) -> Option<NodeUid> {
    ws.live_ids().into_iter().find(|&candidate| {
        let mut owns = false;
        if let Some(node) = ws.get_node(candidate) {
            node.owned_refs(&mut |child| owns |= child == uid);
        }
        owns
    })
}

#[test]
fn the_derivative_of_x_times_x_is_the_product_rule() {
    dex_nodes::scripting::init_python();
    let (mut ws, f, _mult) = squared();

    let d = differentiate(&mut ws, f, "x");

    assert_eq!(
        ws.send_request(d, LambdaName).as_deref(),
        Some("d/dx"),
        "the derivative names itself"
    );
    assert_ne!(d, f, "the derivative is a new lambda, not the original");

    /*
        The product rule: d(u*v) = u'*v + u*v', which for `x*x` is `1*x + x*1`.
        Folding takes the multiplications by one away, so what is built is the
        sum itself with both terms pointing straight at the parameter — the
        shape a reader would draw by hand.
    */
    let body = ws
        .send_request(d, LambdaBody)
        .flatten()
        .expect("the derivative's output is wired");
    let sum = wired_to(&ws, Some(body));
    let (name, terms) = operator(&ws, sum);
    assert_eq!(name, "add", "the top of the product rule is a sum");
    assert_eq!(terms.len(), 2, "a sum of two terms");

    let pins = ws.send_request(d, ParamPins).unwrap_or_default();
    for term in terms {
        let source = term.expect("each term is wired");
        assert!(
            pins.contains(&source),
            "with the `*1` folded away, each term is the parameter itself"
        );
    }

    // The original is untouched.
    assert_eq!(
        ws.send_request(f, LambdaName).as_deref(),
        Some("Canvas Lambda"),
        "differentiating does not rename the source"
    );
}

#[test]
fn an_unknown_operator_fails_loudly() {
    dex_nodes::scripting::init_python();
    let (mut ws, f, mult) = squared();

    let name = ws
        .send_request(mult, dex_nodes::composites::lambda::LambdaNameNode)
        .expect("the lambda exposes its name label");
    ws.submit_action(
        name.cast::<dex_nodes::primitives::text::LabelEditable>(),
        "Rename",
        dex_nodes::primitives::text::SetText {
            value: "Frobnicate".to_owned(),
        },
    );
    settle(&mut ws);

    let graph = GraphSnapshot::capture(&ws);
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [
        ("f".to_owned(), ScriptValue::Node(f)),
        ("var".to_owned(), ScriptValue::Str("x".to_owned())),
    ];
    let err = run_script(DERIVATIVE, "", &handle, &args, graph)
        .err()
        .expect("an unrecognised operator is an error, not a zero");
    let message = err.to_string();
    assert!(
        message.contains("frobnicate"),
        "the error names the node: {message}"
    );
}

/// The rules on their own, checked as data rather than through a canvas.
/// `differentiate` is pure, so it can be exercised the way `tile_fractions` is.
#[test]
fn the_rules_match_the_calculus() {
    dex_nodes::scripting::init_python();
    pyo3::Python::attach(|py| {
        use pyo3::prelude::*;
        use pyo3::types::PyDict;

        let globals = PyDict::new(py);
        globals
            .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
            .unwrap();
        let src = std::ffi::CString::new(DERIVATIVE).unwrap();
        py.run(src.as_c_str(), Some(&globals), Some(&globals))
            .expect("the example module runs");

        // `x` is parameter 0, `y` parameter 1.
        let cases = [
            ("Var(0)", "Const(1)"),
            ("Var(1)", "Const(0)"),
            ("Const('4')", "Const(0)"),
            ("Op(ADD, [Var(0), Var(1)])", "add(Const(1), Const(0))"),
            ("Op(SUB, [Var(0), Var(1)])", "sub(Const(1), Const(0))"),
            // Product rule, unsimplified: u'v + uv'.
            (
                "Op(MULT, [Var(0), Var(1)])",
                "add(mult(Const(1), Var(1)), mult(Var(0), Const(0)))",
            ),
            // Quotient rule: (u'v - uv') / v*v.
            (
                "Op(DIV, [Var(0), Var(1)])",
                "div(sub(mult(Const(1), Var(1)), mult(Var(0), Const(0))), mult(Var(1), Var(1)))",
            ),
            // Power rule with a constant exponent: n * u^(n-1) * u'.
            (
                "Op(POW, [Var(0), Const('3')])",
                "mult(mult(Const(3), pow(Var(0), Const(2))), Const(1))",
            ),
        ];

        for (expr, want) in cases {
            let code = std::ffi::CString::new(format!("repr(differentiate({expr}, 0))")).unwrap();
            let got: String = py
                .eval(code.as_c_str(), Some(&globals), None)
                .unwrap_or_else(|e| panic!("differentiating {expr}: {e}"))
                .extract()
                .unwrap();
            assert_eq!(got, want, "d/dx of {expr}");
        }

        // A non-constant exponent has no rule here, and says so.
        let code = c"differentiate(Op(POW, [Var(0), Var(1)]), 0)";
        let err = py
            .eval(code, Some(&globals), None)
            .expect_err("a variable exponent is refused");
        assert!(err.to_string().contains("constant exponent"), "{err}");
    });
}

// ======================================================================
// `examples/gen_eq.py`
// ======================================================================

const GEN_EQ: &str = include_str!("../../../examples/gen_eq.py");

/// Build an equation with the generator, apply it, and return the lambda.
fn generate(ws: &mut Workspace, equation: &str, params: &str) -> NodeUid {
    let graph = GraphSnapshot::capture(ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let args = [
        ("equation".to_owned(), ScriptValue::Str(equation.to_owned())),
        ("params".to_owned(), ScriptValue::Str(params.to_owned())),
    ];
    let built = match run_script(GEN_EQ, "", &handle, &args, graph) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the generator returns the lambda it built"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    settle(ws);
    built
}

/// An empty workspace, so the generator has nothing to lean on.
fn blank() -> Workspace {
    let mut ws = Workspace::new_empty();
    let root = ws
        .action_handle()
        .insert_node_dyn(Arc::new(dex_nodes::primitives::nothing::Nothing));
    ws.process_pending();
    ws.set_root(root);
    ws
}

#[test]
fn the_generator_builds_an_equation_the_differentiator_can_read() {
    dex_nodes::scripting::init_python();
    let mut ws = blank();

    let f = generate(&mut ws, "a*x**2 + b*x + c", "x a b c");

    assert_eq!(
        ws.send_request(f, LambdaName).as_deref(),
        Some("a*x**2 + b*x + c"),
        "the lambda is named for what it computes"
    );
    let names: Vec<String> = ws
        .send_request(f, DataflowInputs)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, _port, _src)| name)
        .collect();
    assert_eq!(names, ["x", "a", "b", "c"], "one parameter each, in order");
    assert_eq!(
        ws.send_request(f, ParamPins).unwrap_or_default().len(),
        4,
        "the pins were minted with the parameters, not a tick later"
    );

    // The top of `a*x**2 + b*x + c` is the outer sum.
    let body = ws
        .send_request(f, LambdaBody)
        .flatten()
        .expect("the generated output is wired");
    let (name, terms) = operator(&ws, wired_to(&ws, Some(body)));
    assert_eq!(name, "add");
    assert_eq!(terms.len(), 2);

    // And it differentiates: d/dx is a sum, since the body is.
    let d = differentiate(&mut ws, f, "x");
    assert_eq!(ws.send_request(d, LambdaName).as_deref(), Some("d/dx"));
    let d_body = ws
        .send_request(d, LambdaBody)
        .flatten()
        .expect("the derivative's output is wired");
    let (d_name, _) = operator(&ws, wired_to(&ws, Some(d_body)));
    assert_eq!(d_name, "add", "the derivative of a sum is a sum");
}

/// Nothing is stacked: every node the generator places gets its own spot.
#[test]
fn generated_nodes_do_not_land_on_top_of_each_other() {
    use dex_nodes::layouts::canvas::layout::CanvasChildren;
    use dex_nodes::layouts::canvas::nodes::CanvasNodeConstraints;

    dex_nodes::scripting::init_python();
    let mut ws = blank();
    let f = generate(&mut ws, "a*x**2 + b*x + c", "x a b c");

    let canvas = ws.send_request(f, ComputeCanvasNode).expect("a canvas");
    let items = ws.send_request(canvas, CanvasChildren).unwrap_or_default();
    assert!(
        items.len() > 4,
        "the body has several nodes: {}",
        items.len()
    );

    let mut seen: Vec<(i64, i64)> = Vec::new();
    for item in items {
        let layout = ws
            .send_request(item, CanvasNodeConstraints)
            .expect("a placed item reports where it is");
        let spot = (layout.pos.x as i64, layout.pos.y as i64);
        assert!(
            !seen.contains(&spot),
            "two nodes share the spot {spot:?} — the layout is stacking"
        );
        seen.push(spot);
    }
}

/// A transform's result can be copied and kept, the way a canvas item can.
/// Nothing offered these before: an output slot is not a canvas item.
#[test]
fn a_result_slot_offers_the_placement_commands() {
    use dex_nodes::layouts::inspector::{Inspector, OpenInspector};

    dex_nodes::scripting::init_python();
    let (mut ws, f, mult) = squared();

    // Both kinds of result: a plain lambda's output slot, and a canvas
    // lambda's proxy.
    for (owner, what) in [(mult, "a lambda's output"), (f, "a canvas lambda's result")] {
        let slot = ws
            .send_request(owner, DataflowOutput)
            .flatten()
            .unwrap_or_else(|| panic!("{what} has a uid"));

        let inspector = ws.action_handle().insert_node(Inspector::new());
        ws.process_pending();
        ws.submit_action(
            inspector,
            "Open",
            OpenInspector {
                node: slot,
                size: Vector { x: 40.0, y: 20.0 },
            },
        );
        settle(&mut ws);

        let menu = ws.live_ids().into_iter().find(|&uid| {
            ws.get_node(uid).is_some_and(|node| {
                node.as_ref()
                    .as_any_ref()
                    .is::<dex_nodes::layouts::inspector::PlacementCommands>()
            })
        });
        assert!(menu.is_some(), "{what} offers Copy/Mirror/Backpack");
        ws.delete_node(inspector.erase());
        settle(&mut ws);
    }
}

/// `eq.py` is `gen_eq.py` with the equation written in, so it needs no
/// arguments at all — the quickest way to get something to differentiate.
#[test]
fn the_inline_generator_needs_no_arguments() {
    dex_nodes::scripting::init_python();
    let mut ws = blank();

    let graph = GraphSnapshot::capture(&ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let source = include_str!("../../../examples/eq.py");
    let built = match run_script(source, "", &handle, &[], graph) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the generator returns the lambda it built"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    settle(&mut ws);

    let names: Vec<String> = ws
        .send_request(built, DataflowInputs)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, _port, _src)| name)
        .collect();
    assert_eq!(names, ["x", "a", "b", "c"]);

    // And it is something the differentiator can take straight away.
    let d = differentiate(&mut ws, built, "x");
    assert_eq!(ws.send_request(d, LambdaName).as_deref(), Some("d/dx"));
}

/// Nothing on the derivative's canvas is an empty frame.
///
/// An item wrapping a dead or empty node draws as a bare `CanvasNode` border
/// with nothing inside, and still attracts wires.
#[test]
fn the_derivative_leaves_no_empty_items() {
    use dex_nodes::layouts::canvas::layout::CanvasChildren;
    use dex_nodes::layouts::canvas::nodes::CanvasNodeChild;
    use dex_nodes::primitives::nothing::Nothing;

    dex_nodes::scripting::init_python();
    let mut ws = blank();
    let f = generate(&mut ws, "a*x**2 + b*x + c", "x a b c");
    let d = differentiate(&mut ws, f, "x");

    let canvas = ws.send_request(d, ComputeCanvasNode).expect("a canvas");
    let items = ws.send_request(canvas, CanvasChildren).unwrap_or_default();
    assert!(!items.is_empty(), "the derivative has a body");

    let mut empty = Vec::new();
    for item in items {
        match ws.send_request(item, CanvasNodeChild) {
            None => empty.push((item, "is not an item at all")),
            Some(child) => match ws.get_node(child) {
                None => empty.push((item, "wraps a uid with no node")),
                Some(node) if node.as_ref().as_any_ref().is::<Nothing>() => {
                    empty.push((item, "wraps Nothing"))
                }
                // An item inside an item draws its child at its *own* canvas
                // position, which is somewhere else — so the outer frame is
                // left empty. The one my first pass missed.
                Some(node)
                    if node
                        .as_ref()
                        .as_any_ref()
                        .is::<dex_nodes::layouts::canvas::nodes::CanvasNode>() =>
                {
                    empty.push((item, "wraps another canvas item"))
                }
                Some(_) => {}
            },
        }
    }
    assert!(
        empty.is_empty(),
        "{} empty item(s) on the derivative's canvas: {:?}",
        empty.len(),
        empty.iter().map(|(_, why)| *why).collect::<Vec<_>>()
    );
}

/// Every wire in the derivative points at a *value*, never at the node that
/// computes one.
///
/// `read` tags each term with the operator node that produces it, so reusing a
/// term is one `DataflowOutput` away from binding the operator itself — which
/// hands the consumer a node handle instead of a number.
#[test]
fn the_derivative_wires_to_values_not_to_operators() {
    use dex_nodes::composites::lambda::ConnectedTarget;

    dex_nodes::scripting::init_python();
    let mut ws = blank();
    let f = generate(&mut ws, "a*x**2 + b*x + c", "x a b c");
    let _d = differentiate(&mut ws, f, "x");

    let mut wires = 0;
    let mut bad = Vec::new();
    for uid in ws.live_ids() {
        let Some(Some(target)) = ws.send_request(uid, ConnectedTarget) else {
            continue;
        };
        wires += 1;
        // An operator answers `LambdaName`; so does the item wrapping one. A
        // value — an output slot, a pin, a constant — does not, so a non-empty
        // name here means the wire bound the computer rather than what it
        // computes.
        let name = ws.send_request(target, LambdaName).unwrap_or_default();
        if !name.trim().is_empty() {
            bad.push(name);
        }
    }

    assert!(wires >= 10, "the derivative has many connections: {wires}");
    assert!(
        bad.is_empty(),
        "{} wire(s) bound an operator rather than its output: {bad:?}",
        bad.len()
    );
}

/// The derivative is small enough to read.
///
/// Unfolded, `d/dx(a*x**2 + b*x + c)` is twelve operators and eight constants,
/// nearly all of them multiplying by one or adding zero, on top of a full copy
/// of the original body. Folding and pruning are what make the canvas legible.
#[test]
fn the_derivative_is_small_enough_to_read() {
    use dex_nodes::layouts::canvas::layout::CanvasChildren;

    dex_nodes::scripting::init_python();
    let mut ws = blank();
    let f = generate(&mut ws, "a*x**2 + b*x + c", "x a b c");
    let d = differentiate(&mut ws, f, "x");

    let canvas = ws.send_request(d, ComputeCanvasNode).expect("a canvas");
    let items = ws.send_request(canvas, CanvasChildren).unwrap_or_default();
    let pins = ws.send_request(d, ParamPins).unwrap_or_default().len();
    let body = items.len() - pins;

    // `a*(2*x) + b`: three operators and the constant 2.
    assert_eq!(
        body, 4,
        "d/dx of a quadratic is four nodes; unfolded it was twenty, over a \
         six-node copy of the original"
    );
}

/// The derivative sits near its parameters and reads as a cascade.
#[test]
fn the_derivative_cascades_down_from_the_pins() {
    use dex_nodes::layouts::canvas::layout::CanvasChildren;
    use dex_nodes::layouts::canvas::nodes::CanvasNodeConstraints;

    dex_nodes::scripting::init_python();
    let mut ws = blank();
    let f = generate(&mut ws, "a*x**2 + b*x + c", "x a b c");
    let d = differentiate(&mut ws, f, "x");

    let canvas = ws.send_request(d, ComputeCanvasNode).expect("a canvas");
    let pins = ws.send_request(d, ParamPins).unwrap_or_default();

    let mut body: Vec<(f32, f32)> = Vec::new();
    for item in ws.send_request(canvas, CanvasChildren).unwrap_or_default() {
        if pins.contains(&item) {
            continue;
        }
        let layout = ws
            .send_request(item, CanvasNodeConstraints)
            .expect("a placed item");
        body.push((layout.pos.x, layout.pos.y));
    }
    body.sort_by(|a, b| a.0.total_cmp(&b.0));

    let top = body.iter().map(|(_, y)| *y).fold(f32::MAX, f32::min);
    assert!(
        top < 400.0,
        "the body starts near the pins, not far below where the original was: {top}"
    );

    // Successive steps move right *and* down, so a chain reads diagonally.
    let mut previous: Option<(f32, f32)> = None;
    for (x, y) in body {
        if let Some((px, py)) = previous
            && x > px
        {
            assert!(
                y > py,
                "a step to the right also steps down: ({px}, {py}) then ({x}, {y})"
            );
        }
        previous = Some((x, y));
    }
}
