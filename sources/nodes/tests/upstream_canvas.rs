//! Gathering a chain of lambdas into a canvas lambda of its own.
//!
//! The chain under test is `(a + b) * a`: two operators, one of them feeding
//! the other, and one outside value used twice. That last part is the case
//! worth having — two arguments fed by one node outside must arrive as *one*
//! argument of the gathered lambda, or the copy asks for the same value twice.

use std::collections::HashSet;
use std::time::Duration;

use dex_core::prelude::*;
use dex_nodes::composites::lambda::DataflowInputs;
use dex_nodes::composites::lambda::{
    AddArgAt, ArgBindings, CanvasLambda, ComputeCanvas, ComputeCanvasNode, ConnectionPort,
    InnerCanvas, Lambda, LambdaArg, LambdaArgs, LambdaArgsNode, LambdaOutput, OutputConnected,
    ParamEntries, SetConnection,
};
use dex_nodes::composites::upstream::canvas_from_upstream;
use dex_nodes::layouts::canvas::layout::{Canvas, CanvasChildren};
use dex_nodes::layouts::canvas::nodes::CanvasNodeChild;
use dex_nodes::layouts::desktops::Desktops;
use dex_nodes::primitives::number::Integer;
use dex_nodes::scripting::{ScriptValue, resolve_arg};

/// Every lambda is the same shape: two named arguments and a body.
const ADD: &str = "def transform():\n    return a + b\n";
const MULT: &str = "def transform():\n    return a * b\n";

struct Chain {
    ws: Workspace,
    /// The far end: `(a + b) * a`.
    product: NodeUid,
    /// The two numbers the chain is fed by.
    first: NodeUid,
    second: NodeUid,
}

/// Build one two-argument lambda, returning it and its two ports.
fn operator(ws: &Workspace, name: &str, source: &str) -> (NodeUid, [NodeUid; 2]) {
    let handle = ws.action_handle();
    let uid = NodeUid::mint();
    let args = NodeUid::<LambdaArgs>::mint();
    let output = NodeUid::mint();
    handle.insert_node_at_dyn(
        uid,
        Arc::new(Lambda::new_with(
            handle.clone(),
            args,
            output,
            name.to_owned(),
            source.to_owned(),
        )),
    );
    let ports = [NodeUid::mint(), NodeUid::mint()];
    for (port, label) in ports.iter().zip(["a", "b"]) {
        let arg = NodeUid::mint();
        LambdaArg::build_with(handle.clone(), arg.cast(), *port, label.to_owned());
        handle.submit_action(args, "add", AddArgAt { arg });
    }
    (uid, ports)
}

fn wire(ws: &Workspace, port: NodeUid, target: NodeUid) {
    ws.submit_action(
        port.cast::<ConnectionPort>(),
        "wire",
        SetConnection {
            target: Some(target),
        },
    );
}

fn chain() -> Chain {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let handle = ws.action_handle();

    let first = handle.insert_node(Integer::new(4)).erase();
    let second = handle.insert_node(Integer::new(6)).erase();
    let (sum, sum_ports) = operator(&ws, "Add", ADD);
    let (product, product_ports) = operator(&ws, "Mult", MULT);
    ws.process_pending();
    // A tick before anything is wired, so each lambda has seen the shape it
    // starts in — exactly as it would have on the frame before a hand reached
    // for the first wire. Without it the wiring is the first thing they ever
    // see, and a lambda does not re-run over a change it was never without.
    ws.tick_all();
    ws.process_pending();

    wire(&ws, sum_ports[0], first);
    wire(&ws, sum_ports[1], second);
    let sum_output = ws
        .send_request(sum, LambdaOutput)
        .expect("the sum has a result");
    wire(&ws, product_ports[0], sum_output);
    // The same number feeds the chain twice: once through the sum, once straight.
    wire(&ws, product_ports[1], first);
    ws.process_pending();

    Chain {
        ws,
        product,
        first,
        second,
    }
}

/// Tick and drain until `done`, or give up.
fn settle(ws: &mut Workspace, mut done: impl FnMut(&Workspace) -> bool) -> bool {
    for _ in 0..400 {
        ws.tick_all();
        ws.process_pending();
        if done(ws) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn value_of(ws: &Workspace, uid: NodeUid) -> ScriptValue {
    resolve_arg(ws, uid).value
}

/// The gathered lambda has one argument per loose end, wired to what the
/// original was wired to, and computes the same thing.
#[test]
fn a_chain_becomes_a_lambda_with_the_chain_s_inputs() {
    let mut c = chain();
    // Let the originals compute, so there is an answer to compare against.
    let product_output =
        c.ws.send_request(c.product, LambdaOutput)
            .expect("the product has a result");
    assert!(
        settle(&mut c.ws, |ws| matches!(
            value_of(ws, product_output),
            ScriptValue::Int(40)
        )),
        "(4 + 6) * 4 is 40, and the original says so: {:?}",
        value_of(&c.ws, product_output)
    );

    let gathered = canvas_from_upstream(&c.ws, c.product);
    c.ws.process_pending();

    // Two operators went in, so two items are on the new surface.
    let canvas =
        c.ws.send_request(gathered, ComputeCanvasNode)
            .and_then(|cc| c.ws.send_request(cc.cast::<ComputeCanvas>(), InnerCanvas))
            .expect("the gathered lambda has a surface");
    let bodies: Vec<NodeUid> =
        c.ws.send_request(
            canvas.cast::<dex_nodes::layouts::canvas::layout::Canvas>(),
            CanvasChildren,
        )
        .unwrap_or_default()
        .into_iter()
        .filter(|&item| {
            c.ws.send_request(item, CanvasNodeChild)
                .is_some_and(|child| {
                    c.ws.get_node(child)
                        .is_some_and(|node| (*node).as_any_ref().is::<Lambda>())
                })
        })
        .collect();
    assert_eq!(bodies.len(), 2, "both operators were copied onto it");

    // Three ports fed the chain from outside, but only two distinct nodes did,
    // so the gathered lambda takes two arguments.
    let args =
        c.ws.send_request(gathered.erase(), LambdaArgsNode)
            .expect("the gathered lambda has an argument row");
    let bindings = c.ws.send_request(args, ArgBindings).unwrap_or_default();
    assert_eq!(
        bindings.len(),
        2,
        "one argument per node fed in: {bindings:?}"
    );
    let sources: HashSet<Option<NodeUid>> =
        bindings.iter().map(|(_name, target)| *target).collect();
    assert_eq!(
        sources,
        HashSet::from([Some(c.first), Some(c.second)]),
        "and each keeps pointing at what the original was fed by"
    );

    // The pins inside match the arguments outside, so nothing is left unnamed
    // for the next sync to mint over the top of.
    let pins =
        c.ws.send_request(gathered, ComputeCanvasNode)
            .map(|cc| cc.cast::<ComputeCanvas>())
            .and_then(|cc| c.ws.send_request(cc, ParamEntries))
            .unwrap_or_default();
    assert_eq!(
        pins.len(),
        2,
        "the surface has a pin for each argument: {pins:?}"
    );

    // And it computes the same answer.
    let result =
        c.ws.send_request(gathered, ComputeCanvasNode)
            .and_then(|cc| {
                c.ws.send_request(cc.cast::<ComputeCanvas>(), OutputConnected)
            })
            .flatten()
            .expect("the gathered lambda's result is wired to something");
    assert!(
        settle(&mut c.ws, |ws| matches!(
            value_of(ws, result),
            ScriptValue::Int(40)
        )),
        "the copy computes what the original did: {:?}",
        value_of(&c.ws, result)
    );
}

/// Nothing about the original changes.
#[test]
fn the_original_is_left_alone() {
    let mut c = chain();
    let before: Vec<NodeUid> = c.ws.live_ids();
    let product_output =
        c.ws.send_request(c.product, LambdaOutput)
            .expect("the product has a result");
    settle(&mut c.ws, |ws| {
        matches!(value_of(ws, product_output), ScriptValue::Int(40))
    });

    canvas_from_upstream(&c.ws, c.product);
    c.ws.process_pending();
    settle(&mut c.ws, |_| false);

    for uid in before {
        assert!(
            c.ws.get_node(uid).is_some(),
            "gathering took nothing away from the original"
        );
    }
    assert!(
        matches!(value_of(&c.ws, product_output), ScriptValue::Int(40)),
        "and it still computes what it did"
    );
}

/// A canvas lambda gathers too, nesting whole inside the new one.
#[test]
fn a_canvas_lambda_is_gathered_whole() {
    let mut c = chain();
    let gathered = canvas_from_upstream(&c.ws, c.product);
    c.ws.process_pending();
    settle(&mut c.ws, |_| false);

    // Now gather the gathered one: it is itself a lambda, so it comes along.
    let again = canvas_from_upstream(&c.ws, gathered.erase());
    c.ws.process_pending();
    settle(&mut c.ws, |_| false);

    let canvas =
        c.ws.send_request(again, ComputeCanvasNode)
            .and_then(|cc| c.ws.send_request(cc.cast::<ComputeCanvas>(), InnerCanvas))
            .expect("the outer lambda has a surface");
    let nested =
        c.ws.send_request(
            canvas.cast::<dex_nodes::layouts::canvas::layout::Canvas>(),
            CanvasChildren,
        )
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| c.ws.send_request(item, CanvasNodeChild))
        .filter(|&child| {
            c.ws.get_node(child)
                .is_some_and(|node| (*node).as_any_ref().is::<CanvasLambda>())
        })
        .count();
    assert_eq!(nested, 1, "the canvas lambda came along as one node");
}

/**
    A value wired into the middle of the chain comes along; it is not an argument.

    The chain here is `(a + b) * c`, where `c` feeds only the multiply. `a` and
    `b` feed the add, which is the tip — so those two are the function's shape
    and `c` is a coefficient inside it. Lifting `c` out would give a lambda
    asking for three things when two of them are what it is *for* and the third
    is part of how it works.
*/
#[test]
fn a_value_wired_into_the_middle_is_carried_in() {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let handle = ws.action_handle();

    let first = handle.insert_node(Integer::new(4)).erase();
    let second = handle.insert_node(Integer::new(6)).erase();
    // Fed to the multiply and to nothing else.
    let third = handle.insert_node(Integer::new(3)).erase();
    let (sum, sum_ports) = operator(&ws, "Add", ADD);
    let (product, product_ports) = operator(&ws, "Mult", MULT);
    ws.process_pending();
    ws.tick_all();
    ws.process_pending();

    wire(&ws, sum_ports[0], first);
    wire(&ws, sum_ports[1], second);
    let sum_output = ws
        .send_request(sum, LambdaOutput)
        .expect("the sum has a result");
    wire(&ws, product_ports[0], sum_output);
    wire(&ws, product_ports[1], third);
    ws.process_pending();

    let product_output = ws
        .send_request(product, LambdaOutput)
        .expect("the product has a result");
    assert!(
        settle(&mut ws, |ws| matches!(
            value_of(ws, product_output),
            ScriptValue::Int(30)
        )),
        "(4 + 6) * 3 is 30: {:?}",
        value_of(&ws, product_output)
    );

    let gathered = canvas_from_upstream(&ws, product);
    ws.process_pending();

    // Only the tip's two inputs are arguments.
    let args = ws
        .send_request(gathered.erase(), LambdaArgsNode)
        .expect("the gathered lambda has an argument row");
    let bindings = ws.send_request(args, ArgBindings).unwrap_or_default();
    assert_eq!(
        bindings.len(),
        2,
        "the tip's inputs, and nothing from the middle: {bindings:?}"
    );
    let sources: HashSet<Option<NodeUid>> = bindings.iter().map(|(_n, t)| *t).collect();
    assert_eq!(sources, HashSet::from([Some(first), Some(second)]));

    // The middle's value came along as a copy of its own.
    let canvas = ws
        .send_request(gathered, ComputeCanvasNode)
        .map(|cc| cc.cast::<ComputeCanvas>())
        .and_then(|cc| ws.send_request(cc, InnerCanvas))
        .expect("the gathered lambda has a surface");
    let children: Vec<NodeUid> = ws
        .send_request(canvas.cast::<Canvas>(), CanvasChildren)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| ws.send_request(item, CanvasNodeChild))
        .collect();
    let numbers: Vec<NodeUid> = children
        .iter()
        .copied()
        .filter(|&child| {
            ws.get_node(child)
                .is_some_and(|node| (*node).as_any_ref().is::<Integer>())
        })
        .collect();
    assert_eq!(numbers.len(), 1, "one carried value landed on the surface");
    assert_ne!(numbers[0], third, "and it is a copy, not the original");
    assert!(
        matches!(value_of(&ws, numbers[0]), ScriptValue::Int(3)),
        "worth what the original was worth"
    );

    // The copied multiply points at that copy, so the surface is self-contained.
    let copied_product = children
        .iter()
        .copied()
        .find(|&child| {
            ws.get_node(child)
                .is_some_and(|node| (*node).as_any_ref().is::<Lambda>())
                && ws
                    .send_request(child, dex_nodes::composites::lambda::LambdaName)
                    .as_deref()
                    == Some("Mult")
        })
        .expect("the multiply came along");
    let wired: Vec<Option<NodeUid>> = ws
        .send_request(copied_product, DataflowInputs)
        .unwrap_or_default()
        .into_iter()
        .map(|(_n, _p, target)| target)
        .collect();
    assert!(
        !wired.contains(&Some(third)),
        "nothing on the new surface reaches back to the original: {wired:?}"
    );
    assert!(
        wired.contains(&Some(numbers[0])),
        "it points at the copy that came with it: {wired:?}"
    );

    // And it still computes what it did.
    let result = ws
        .send_request(gathered, ComputeCanvasNode)
        .map(|cc| cc.cast::<ComputeCanvas>())
        .and_then(|cc| ws.send_request(cc, OutputConnected))
        .flatten()
        .expect("the gathered lambda's result is wired to something");
    assert!(
        settle(&mut ws, |ws| matches!(
            value_of(ws, result),
            ScriptValue::Int(30)
        )),
        "the copy computes what the original did: {:?}",
        value_of(&ws, result)
    );
}
