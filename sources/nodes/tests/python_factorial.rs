//! Exercises `examples/factorial.py`: recursion expressed as a dataflow cycle.
//!
//! Three claims, and each one is load-bearing:
//!
//!   * A transform can name the *node* behind a value argument. `n` arrives as
//!     the integer 10, which names nothing; `dex.args.n` is the `Integer` the 10
//!     was read from, and writing there is what makes the next iteration happen.
//!   * A write to an upstream node re-fires the lambda that made it. There is no
//!     loop construct anywhere — the recursion is the dependency poll noticing
//!     that a version moved.
//!   * The base case is a *declaration*, not a branch. `n` is declared
//!     `satisfying n > 0`, arguments are checked before the script is handed
//!     them, and so the step that would have run at zero never runs. The script
//!     has no `if` in it and cannot need one.
//!
//! Plus one about the example rather than the machine: what it counts down
//! from is wired in, and falls back to its own default when nothing is.

use dex_core::prelude::*;
use dex_core::snapshot::GraphSnapshot;
use dex_nodes::composites::lambda::{Lambda, LambdaOutput};
use dex_nodes::layouts::error::ErrorLayout;
use dex_nodes::layouts::mirror::Mirror;
use dex_nodes::primitives::nothing::Nothing;
use dex_nodes::primitives::number::Integer;
use dex_nodes::primitives::text::{Label, SetText};
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

const FACTORIAL: &str = include_str!("../../../examples/factorial.py");

/// What the example counts down from, and what it should reach.
const START: i64 = 10;
const EXPECTED: i64 = 3_628_800;

/// Settle the queue and let every node see the result.
fn settle(ws: &mut Workspace) {
    for _ in 0..4 {
        ws.process_pending();
        ws.tick_all();
        ws.process_pending();
    }
}

/// Every `Integer` the machine is *made of*, with what it currently holds.
///
/// Copies are skipped, and skipping them is not cosmetic. The canvas lambda's
/// result row shows what it stands for by keeping a live `Mirror` of it — a
/// deep copy under ids of its own — so the accumulator has a double in the
/// workspace holding the same number. Counting it would not merely inflate the
/// total: "the other integer that started at 1" would sometimes find the double
/// and the test would then be watching a picture of the machine instead of the
/// machine.
fn integers(ws: &Workspace) -> Vec<(NodeUid, i64)> {
    ws.live_ids()
        .into_iter()
        .filter(|uid| !inside_a_mirror(ws, *uid))
        .filter_map(|uid| Some((uid, value_at(ws, uid)?)))
        .collect()
}

/// Whether `uid` is a copy some mirror is holding, rather than a node in its
/// own right.
fn inside_a_mirror(ws: &Workspace, uid: NodeUid) -> bool {
    let mut cur = uid;
    // A copy is owned by the mirror directly, but a copy of something wrapped
    // — a canvas item around a number — sits a level or two down inside it.
    for _ in 0..8 {
        let Some(owner) = ws.owner_of(cur) else {
            return false;
        };
        if ws
            .get_node(owner)
            .is_some_and(|n| n.as_ref().as_any_ref().is::<Mirror>())
        {
            return true;
        }
        cur = owner;
    }
    false
}

/// What `uid` holds, when it holds a number.
fn value_at(ws: &Workspace, uid: NodeUid) -> Option<i64> {
    let node = ws.get_node(uid)?;
    let int = node.as_ref().as_any_ref().downcast_ref::<Integer>()?;
    Some(int.value)
}

/// Build the machine into a fresh workspace; returns it and the two numbers.
///
/// Nothing is wired into `n`, so it counts down from the example's own default.
fn factorial_machine() -> (Workspace, NodeUid, NodeUid) {
    machine_from(&[])
}

/// The machine, built with `args` bound the way a wired lambda would bind them.
fn machine_from(args: &[(String, ScriptValue)]) -> (Workspace, NodeUid, NodeUid) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();
    let root = handle.insert_node_dyn(Arc::new(Nothing));
    settle(&mut ws);
    ws.set_root(root);

    let built = match run_script(FACTORIAL, "", &handle, args, GraphSnapshot::capture(&ws)) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(ScriptOutput::Node(_)) => panic!("the example returns the canvas lambda it built"),
        Ok(ScriptOutput::Nothing) => panic!("the example returned nothing"),
        Err(e) => panic!("the example did not run: {e}"),
    };
    settle(&mut ws);
    assert!(ws.get_node(built).is_some(), "the machine was built");

    // The only two numbers on the surface, told apart by what they start as.
    // The accumulator starts at 1, so a countdown *from* 1 would be ambiguous —
    // no caller here asks for one.
    let found = integers(&ws);
    assert_eq!(found.len(), 2, "the machine is two numbers and a lambda");
    let start = args
        .iter()
        .find(|(name, _)| name == "n")
        .and_then(|(_, v)| match v {
            ScriptValue::Int(i) => Some(*i),
            _ => None,
        })
        .unwrap_or(START);
    let counter = found
        .iter()
        .find(|(_, v)| *v == start)
        .unwrap_or_else(|| panic!("a counter starting at {start}"))
        .0;
    let total = found
        .iter()
        .find(|(uid, v)| *v == 1 && *uid != counter)
        .expect("an accumulator starting at 1")
        .0;
    (ws, counter, total)
}

/// Turn frames until `f` says the machine has stopped moving, or give up.
fn run_until(ws: &mut Workspace, mut f: impl FnMut(&Workspace) -> bool) -> bool {
    for _ in 0..400 {
        settle(ws);
        if f(ws) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

/**
    The machine counts itself down and stops holding the answer.

    Nudged once to start it, the way a reader would by pressing Run or by
    editing either number: a lambda's first tick records what its inputs are
    worth without firing on them, so a graph that arrives fully wired is at rest
    until something moves.
*/
#[test]
fn a_cycle_of_one_lambda_computes_a_factorial() {
    let (mut ws, counter, total) = factorial_machine();

    // Rewrite the counter with the value it already has. Nothing about the
    // count changes; what changes is the version, which is the whole of what a
    // lambda watches.
    ws.submit_action(
        counter,
        "Nudged the counter",
        SetText {
            value: START.to_string(),
        },
    );

    let done = run_until(&mut ws, |ws| value_at(ws, counter) == Some(0));
    assert!(
        done,
        "the countdown should reach zero; it stopped at {:?} with {:?} accumulated",
        value_at(&ws, counter),
        value_at(&ws, total)
    );
    assert_eq!(
        value_at(&ws, total),
        Some(EXPECTED),
        "the accumulator should hold {START}!"
    );

    // And it stays there: nothing is left running once the guard has refused.
    for _ in 0..20 {
        settle(&mut ws);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        (value_at(&ws, counter), value_at(&ws, total)),
        (Some(0), Some(EXPECTED)),
        "the machine should be at rest, not still stepping"
    );
}

/**
    It is the declaration that stops it, and the lambda says so.

    The distinction matters: a script with an `if` in it would stop just as
    surely, and prove nothing about the declaration. What the step lambda is
    left holding is the *type* complaint — the check refused to hand a script
    an `n` that was not what it was promised, so no step ran at zero.
*/
#[test]
fn the_recursion_is_stopped_by_the_arguments_declaration() {
    let (mut ws, counter, total) = factorial_machine();
    ws.submit_action(
        counter,
        "Nudged the counter",
        SetText {
            value: START.to_string(),
        },
    );
    assert!(
        run_until(&mut ws, |ws| value_at(ws, counter) == Some(0)),
        "the countdown should reach zero"
    );
    assert_eq!(value_at(&ws, total), Some(EXPECTED));

    let step = ws
        .live_ids()
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid)
                .is_some_and(|node| node.as_ref().as_any_ref().is::<Lambda>())
        })
        .expect("the machine has one step lambda");
    let output = ws
        .send_request(step, LambdaOutput)
        .expect("the step lambda exposes its output");

    let complaint = run_until(&mut ws, |ws| {
        ws.get_node(output)
            .is_some_and(|node| node.as_ref().as_any_ref().is::<ErrorLayout>())
    });
    assert!(
        complaint,
        "the step lambda should end holding the declaration it could not meet"
    );

    let node = ws.get_node(output).expect("the output exists");
    let error = node
        .as_ref()
        .as_any_ref()
        .downcast_ref::<ErrorLayout>()
        .expect("an error layout");
    let dex_nodes::layouts::child::LayoutChild::Node(inner) = &error.child else {
        panic!("the message is held as a node");
    };
    let message = inner
        .as_ref()
        .as_any_ref()
        .downcast_ref::<Label>()
        .expect("the message is a label");
    assert!(
        message.text.contains("n > 0"),
        "the complaint should name the test that ended the recursion, not something else: {}",
        message.text
    );
}

/// What it counts down from is an argument, not a constant to be edited.
///
/// Wiring a number into `n` is how a reader says which factorial they want, and
/// running it with nothing wired still builds something rather than raising at
/// a name nobody has connected — which is also what lets the example typecheck
/// with no globals declared.
#[test]
fn the_starting_number_is_wired_in() {
    let (mut ws, counter, total) = machine_from(&[("n".to_owned(), ScriptValue::Int(5))]);
    assert_eq!(value_at(&ws, counter), Some(5), "it starts where it was told");

    ws.submit_action(
        counter,
        "Nudged the counter",
        SetText {
            value: "5".to_owned(),
        },
    );
    assert!(
        run_until(&mut ws, |ws| value_at(ws, counter) == Some(0)),
        "the countdown should reach zero"
    );
    assert_eq!(value_at(&ws, total), Some(120), "5! is 120");
}
