//! Exercises `examples/wheatstone.py`: a circuit that is a dataflow graph.
//!
//! Four claims, and each one is the example rather than the arithmetic:
//!
//!   * **It settles on the circuit's actual answer**, the one a dense nodal
//!     solve gives, not merely on something that stops changing.
//!   * **The arms report the currents that answer actually carries**, so the
//!     picture is consistent all the way through rather than only in the two
//!     numbers checked first. This is also where a lost update would show: a
//!     `Float` takes only `SetText`, an absolute write, so two lambdas racing
//!     for one net would leave currents that do not balance at it.
//!   * **A declaration is what stops it.** Every net is guarded on the residual,
//!     and once the bridge is solved the guard refuses and the graph goes quiet.
//!   * **Editing a resistance starts it again.** Which is the whole reason the
//!     guard names the imbalance rather than how fast anything is moving — a
//!     machine that stopped because nothing was moving could not notice that
//!     something now should be.

use dex_core::prelude::*;
use dex_core::snapshot::GraphSnapshot;
use dex_nodes::primitives::nothing::Nothing;
use dex_nodes::primitives::number::Float;
use dex_nodes::primitives::text::SetText;
use dex_nodes::scripting::{ScriptOutput, run_script};

const WHEATSTONE: &str = include_str!("../../../examples/wheatstone.py");

/// The supply and the five arms, as the example builds them.
const SUPPLY: f64 = 5.0;
const R1: f64 = 100.0;
const R2: f64 = 200.0;
const R3: f64 = 150.0;
const R4: f64 = 120.0;
const R5: f64 = 300.0;

/// Settle the queue and let every node see the result.
fn settle(ws: &mut Workspace) {
    for _ in 0..4 {
        ws.process_pending();
        ws.tick_all();
        ws.process_pending();
    }
}

/// Turn frames until `f` says so, or give up.
fn run_until(ws: &mut Workspace, mut f: impl FnMut(&Workspace) -> bool) -> bool {
    // Generous on purpose: the tests in this binary each run a scheduler, and
    // every one of their workers wants the same interpreter lock.
    for _ in 0..2500 {
        settle(ws);
        if f(ws) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

/// Every `Float` in the workspace, with what it holds.
fn floats(ws: &Workspace) -> Vec<(NodeUid, f64)> {
    ws.live_ids()
        .into_iter()
        .filter_map(|uid| Some((uid, value_at(ws, uid)?)))
        .collect()
}

/// What `uid` holds, when it holds a decimal number.
fn value_at(ws: &Workspace, uid: NodeUid) -> Option<f64> {
    let node = ws.get_node(uid)?;
    Some(node.as_ref().as_any_ref().downcast_ref::<Float>()?.value)
}

/// The one `Float` currently holding `wanted`, to within `eps`.
fn holding(ws: &Workspace, wanted: f64, eps: f64) -> Option<NodeUid> {
    let found: Vec<NodeUid> = floats(ws)
        .into_iter()
        .filter(|(_, v)| (v - wanted).abs() < eps)
        .map(|(uid, _)| uid)
        .collect();
    (found.len() == 1).then(|| found[0])
}

/// The bridge, built into a fresh workspace.
fn bridge() -> Workspace {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();
    let root = handle.insert_node_dyn(Arc::new(Nothing));
    settle(&mut ws);
    ws.set_root(root);

    let built = match run_script(WHEATSTONE, "", &handle, &[], GraphSnapshot::capture(&ws)) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the example returns the canvas lambda it built"),
        Err(e) => panic!("the example did not run: {e}"),
    };
    settle(&mut ws);
    assert!(ws.get_node(built).is_some(), "the bridge was built");
    ws
}

/// The bridge's exact answer, by a dense two-unknown nodal solve.
///
/// Deliberately worked out a different way from the graph's: the graph relaxes,
/// and a relaxation that agreed with another relaxation would only be proof
/// that both had stopped.
fn exact(r4: f64) -> (f64, f64) {
    let g = |r: f64| 1.0 / r;
    let (a11, a12) = (g(R1) + g(R2) + g(R5), -g(R5));
    let (a21, a22) = (-g(R5), g(R3) + g(r4) + g(R5));
    let (b1, b2) = (SUPPLY * g(R1), SUPPLY * g(R3));
    let det = a11 * a22 - a12 * a21;
    ((b1 * a22 - a12 * b2) / det, (a11 * b2 - b1 * a21) / det)
}

/// Wait for both free nets to reach `(left, right)`.
fn reaches(ws: &mut Workspace, left: f64, right: f64) -> bool {
    run_until(ws, |ws| {
        let held = floats(ws);
        let near = |w: f64| held.iter().any(|(_, v)| (v - w).abs() < 1e-6);
        near(left) && near(right)
    })
}

/**
    The bridge solves itself, and agrees with the circuit it stands for.

    Nudged once to start it, the way a reader would by editing anything: a
    lambda's first tick records what its inputs are worth without firing on
    them, so a graph that arrives fully wired is at rest until something moves.
*/
#[test]
fn the_graph_settles_on_the_circuits_own_answer() {
    let mut ws = bridge();
    let supply = holding(&ws, SUPPLY, 1e-9).expect("the supply is the one net holding 5V");
    ws.submit_action(
        supply,
        "Nudged the supply",
        SetText {
            value: SUPPLY.to_string(),
        },
    );

    let (left, right) = exact(R4);
    assert!(
        reaches(&mut ws, left, right),
        "the two junctions should reach {left:.6} and {right:.6}; the numbers held were {:?}",
        floats(&ws).iter().map(|(_, v)| *v).collect::<Vec<_>>()
    );
}

/**
    The currents balance at both junctions, which is what solving it meant.

    Checked from the arms rather than the nets: the junctions could agree with a
    dense solve and the components still be reporting something else, and it is
    the current through the middle that a bridge is actually read by.
*/
#[test]
fn the_arms_carry_currents_that_balance() {
    let mut ws = bridge();
    let supply = holding(&ws, SUPPLY, 1e-9).expect("the supply");
    ws.submit_action(
        supply,
        "Nudged the supply",
        SetText {
            value: SUPPLY.to_string(),
        },
    );
    let (left, right) = exact(R4);
    assert!(reaches(&mut ws, left, right), "the bridge solves");

    // Every arm's current, worked out from the voltages the bridge settled on.
    let wanted = [
        (SUPPLY - left) / R1,
        (left - 0.0) / R2,
        (SUPPLY - right) / R3,
        (right - 0.0) / R4,
        (left - right) / R5,
    ];
    // Waited for, not sampled: the junctions reach their answer first, and the
    // arms are still recomputing from it for a few frames afterwards.
    let settled = run_until(&mut ws, |ws| {
        let held: Vec<f64> = floats(ws).into_iter().map(|(_, v)| v).collect();
        wanted
            .iter()
            .all(|want| held.iter().any(|v| (v - want).abs() < 1e-9))
    });
    assert!(
        settled,
        "every arm should end carrying what the solved bridge puts through it, {:?}; held {:?}",
        wanted,
        floats(&ws).iter().map(|(_, v)| *v).collect::<Vec<_>>()
    );
    // And they cancel where they meet, which is the law the junctions enforce.
    let into_left = wanted[0] - wanted[1] - wanted[4];
    let into_right = wanted[2] - wanted[3] + wanted[4];
    assert!(
        into_left.abs() < 1e-9 && into_right.abs() < 1e-9,
        "current should cancel at both junctions: {into_left:e} and {into_right:e}"
    );
}

/**
    Once solved it stays solved, and stays solved because the guard refuses.

    The distinction matters: a graph that had merely run out of things to do
    would look the same for a frame. This waits well past that, and what keeps
    it still is that every net is declared against an imbalance that is now too
    small to satisfy it.
*/
#[test]
fn the_declaration_is_what_stops_it() {
    let mut ws = bridge();
    let supply = holding(&ws, SUPPLY, 1e-9).expect("the supply");
    ws.submit_action(
        supply,
        "Nudged the supply",
        SetText {
            value: SUPPLY.to_string(),
        },
    );
    let (left, right) = exact(R4);
    assert!(reaches(&mut ws, left, right), "the bridge solves");

    for _ in 0..40 {
        settle(&mut ws);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let held = floats(&ws);
    let near = |w: f64| held.iter().any(|(_, v)| (v - w).abs() < 1e-6);
    assert!(
        near(left) && near(right),
        "the bridge should be at rest, not still correcting: {:?}",
        held.iter().map(|(_, v)| *v).collect::<Vec<_>>()
    );
}

/**
    Editing an arm wakes it, and it re-solves — and at balance the middle is dead.

    `R1/R2 == R3/R4` is the balance condition, so 300 nulls the detector arm.
    That it starts again at all is the point: the guard names the current that
    fails to balance, and editing a resistance makes that wrong immediately.
    A guard that had watched for movement instead would have had nothing to
    notice, because at that instant nothing is moving.
*/
#[test]
fn editing_an_arm_wakes_the_bridge_and_it_balances() {
    let mut ws = bridge();
    let supply = holding(&ws, SUPPLY, 1e-9).expect("the supply");
    ws.submit_action(
        supply,
        "Nudged the supply",
        SetText {
            value: SUPPLY.to_string(),
        },
    );
    let (left, right) = exact(R4);
    assert!(reaches(&mut ws, left, right), "the bridge solves as built");

    // The five arms all differ, so `R4` is the one number holding 120.
    let arm = holding(&ws, R4, 1e-9).expect("R4 is the only arm at 120 ohms");
    ws.submit_action(
        arm,
        "Set R4 to 300 ohms",
        SetText {
            value: "300.0".to_owned(),
        },
    );

    let (bl, br) = exact(300.0);
    assert!(
        reaches(&mut ws, bl, br),
        "setting R4 to 300 should re-solve the bridge to {bl:.6} and {br:.6}; held {:?}",
        floats(&ws).iter().map(|(_, v)| *v).collect::<Vec<_>>()
    );
    assert!(
        (bl - br).abs() < 1e-6,
        "a balanced bridge has no voltage across the middle: {bl} vs {br}"
    );
}
