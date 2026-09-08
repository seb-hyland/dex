//! What a lambda does while something upstream of it is still computing.
//!
//! Two rules, and the second is what makes the first stay true. A pending
//! marker is exactly one deep, always — it is the state an output is in, not a
//! layer it collects. And a lambda whose input is pending does not run: not the
//! script, not the type check, not even the marker on its own output. Waiting
//! costs nothing, because a source settling moves its version and that is what
//! the dependency poll fires on.
//!
//! Without the second rule the first cannot hold across a chain. A downstream
//! lambda that ran anyway would compute from the gap a pending source leaves
//! behind, and — if its script handed back a reference to that source — commit a
//! *copy* of the marker into its own output, where nothing would ever clear it.

use dex_core::prelude::*;
use dex_nodes::composites::lambda::{
    AddArgAt, Lambda, LambdaArg, LambdaArgs, LambdaArgsNode, SetConnection,
};
use dex_nodes::layouts::child::LayoutChild;
use dex_nodes::layouts::pending::{PendingLayout, settled};
use dex_nodes::primitives::nothing::Nothing;
use dex_nodes::primitives::number::Integer;
use dex_nodes::primitives::text::Label;

/// Settle the queue and let every node see the result.
fn settle(ws: &mut Workspace) {
    for _ in 0..4 {
        ws.process_pending();
        ws.tick_all();
        ws.process_pending();
    }
}

/// How many pending markers `node` is wrapped in.
fn depth(mut node: Arc<dyn Node>) -> usize {
    let mut markers = 0;
    while let Some(pending) = node.as_ref().as_any_ref().downcast_ref::<PendingLayout>() {
        markers += 1;
        match &pending.child {
            LayoutChild::Node(inner) => node = inner.clone(),
            _ => break,
        }
    }
    markers
}

/// A node wrapped in `count` pending markers.
fn wrapped(count: usize) -> Arc<dyn Node> {
    let mut node: Arc<dyn Node> = Arc::new(Label::new("42".to_owned()));
    for _ in 0..count {
        node = Arc::new(PendingLayout::new(LayoutChild::Node(node)));
    }
    node
}

/**
    Unwrapping is what keeps the marker one deep, and it unwinds any depth.

    Any, rather than one, because the doubling was in a `#[portable]` node: a
    workspace saved while an output was showing two markers carries both, and
    opening it should not leave the reader peeling them off one run at a time.
*/
#[test]
fn a_pending_marker_is_taken_off_before_another_goes_on() {
    for already in 0..4 {
        let LayoutChild::Node(bare) = settled(wrapped(already)) else {
            panic!("a wrapped node settles to a node");
        };
        assert_eq!(
            depth(bare.clone()),
            0,
            "{already} markers should all come off"
        );
        assert!(
            bare.as_ref().as_any_ref().is::<Label>(),
            "what is under the markers survives them"
        );

        // Which is the whole point: wrapping what it settled to leaves one.
        let marked: Arc<dyn Node> = Arc::new(PendingLayout::new(settled(wrapped(already))));
        assert_eq!(
            depth(marked),
            1,
            "a fresh run shows one marker, not {already}"
        );
    }
}

/// A lambda holding `source`, wired to `input`. Returns it and its output slot.
fn lambda_reading(ws: &mut Workspace, input: NodeUid, source: &str) -> (NodeUid, NodeUid) {
    let handle = ws.action_handle();
    // Built with its script rather than sent one: the editor a lambda exposes
    // wraps the code editor, and `SetText` to the wrapper leaves the default
    // `return` in place — which returns `None`, and looks like a run that
    // decided on nothing rather than one that never happened.
    let args = NodeUid::<LambdaArgs>::mint();
    let output = NodeUid::mint();
    let lambda = handle
        .insert_node(Lambda::new_with(
            handle.clone(),
            args,
            output,
            "Reader".to_owned(),
            source.to_owned(),
        ))
        .erase();
    settle(ws);

    let (arg, port) = (NodeUid::mint(), NodeUid::mint());
    LambdaArg::build_with(
        ws.action_handle(),
        arg.cast(),
        port,
        "value".to_owned(),
        "value".to_owned(),
    );
    ws.submit_action(args.erase(), "Add argument", AddArgAt { arg });
    ws.submit_action(
        port,
        "Wire it",
        SetConnection {
            target: Some(input),
        },
    );
    settle(ws);

    (lambda, output)
}

/**
    A lambda whose input is recomputing leaves its own output alone.

    Deliberately provoked: the argument row grows a second parameter, which is a
    change of *shape* and fires a rerun on its own — the dependency poll would
    have skipped a pending source without ever getting as far as `run_update`.
    So this is the gate being asked the awkward question, not the poll declining
    to ask it.
*/
#[test]
fn a_lambda_whose_input_is_pending_does_not_run() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();
    let root = handle.insert_node_dyn(Arc::new(Nothing));

    // The source, mid-recompute: a marker over the number it last settled on.
    let upstream = handle.insert_node_dyn(Arc::new(PendingLayout::new(LayoutChild::Node(
        Arc::new(Integer::new(7)),
    ))));
    settle(&mut ws);
    ws.set_root(root);

    let (lambda, output) =
        lambda_reading(&mut ws, upstream, "def transform():\n    return value\n");

    // Something recognisable in the output, so anything at all happening to it
    // is visible — a marker over it, an error in place of it, or a result.
    ws.action_handle()
        .insert_node_at_dyn(output, Arc::new(Label::new("untouched".to_owned())));
    settle(&mut ws);

    // Now provoke a rerun the dependency poll cannot swallow.
    let args = ws.send_request(lambda, LambdaArgsNode).expect("args");
    let (second, second_port) = (NodeUid::mint(), NodeUid::mint());
    LambdaArg::build_with(
        ws.action_handle(),
        second.cast(),
        second_port,
        "other".to_owned(),
        "other".to_owned(),
    );
    ws.submit_action(args, "Add another argument", AddArgAt { arg: second });

    for _ in 0..40 {
        settle(&mut ws);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let held = ws.get_node(output).expect("the output still exists");
    assert_eq!(
        depth(held.clone()),
        0,
        "a pending input must not put a marker on this lambda's own output"
    );
    let label = held
        .as_ref()
        .as_any_ref()
        .downcast_ref::<Label>()
        .expect("the output was left exactly as it was");
    assert_eq!(
        label.text, "untouched",
        "nothing ran, so nothing replaced it"
    );
}

/**
    And it runs the moment the source settles, without being asked again.

    The wait is only safe because of this: settling replaces the marker, which
    moves the source's version, which is what the dependency poll fires on. If
    that did not hold, a lambda downstream of anything slow would simply stop.
*/
#[test]
fn the_run_happens_once_the_source_settles() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();
    let root = handle.insert_node_dyn(Arc::new(Nothing));
    let upstream = handle.insert_node_dyn(Arc::new(PendingLayout::new(LayoutChild::Node(
        Arc::new(Integer::new(7)),
    ))));
    settle(&mut ws);
    ws.set_root(root);

    let (_lambda, output) = lambda_reading(
        &mut ws,
        upstream,
        "def transform():\n    return value * 2\n",
    );
    for _ in 0..20 {
        settle(&mut ws);
    }
    assert!(
        !ws.get_node(output)
            .is_some_and(|n| n.as_ref().as_any_ref().is::<Integer>()),
        "nothing should have been computed while the source was pending"
    );

    // The source settles: the marker comes off and a real number takes its place.
    ws.action_handle()
        .insert_node_at_dyn(upstream, Arc::new(Integer::new(7)));

    let mut settled_to = None;
    for _ in 0..200 {
        settle(&mut ws);
        if let Some(node) = ws.get_node(output)
            && let Some(int) = node.as_ref().as_any_ref().downcast_ref::<Integer>()
        {
            settled_to = Some(int.value);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        settled_to,
        Some(14),
        "the deferred run should fire on its own when the source settles"
    );
}
