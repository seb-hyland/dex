//! A canvas lambda shows what it stands for, rather than describing it.
//!
//! Both places a canvas lambda stands in for something it does not own — the
//! result at the bottom of the card, and each parameter pin inside — used to
//! render the value to text through `ScriptValue::display`. That reads a number
//! back perfectly well and turns everything that is genuinely a *node* into the
//! literal word `<node>`: a drawing, a view, a whole canvas gathered from
//! upstream. Which is most of what anyone builds a canvas lambda around.
//!
//! Drawing the node itself is not an option — it is already drawn where it
//! lives, and one node drawn twice hands its widgets the same egui ids twice —
//! so each of them keeps a `Mirror`, which is a live deep copy under ids of its
//! own.

use std::time::Duration;

use dex_core::prelude::*;
use dex_nodes::composites::lambda::{
    AddArgAt, CanvasLambda, ComputeCanvas, LambdaArg, LambdaArgs, LambdaArgsNode, OutputPin,
    SetConnection,
};
use dex_nodes::layouts::desktops::Desktops;
use dex_nodes::layouts::mirror::{Mirror, MirrorTarget};
use dex_nodes::primitives::number::Integer;
use dex_nodes::primitives::shapes::Rect;

/// Big enough that a card and its rows are all laid out.
const SCREEN: egui::Vec2 = egui::vec2(1400.0, 1000.0);

fn settle(ws: &mut Workspace) {
    for _ in 0..40 {
        ws.tick_all();
        ws.process_pending();
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Every word painted on one frame.
///
/// The old rendering was never inserted anywhere — `draw` built a `Label` and
/// handed it straight to `ctx.draw_node` — so scanning the workspace would find
/// nothing either way. What reached the screen is the only evidence.
fn painted(ws: &mut Workspace, ctx: &egui::Context) -> Vec<String> {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let out = ctx.clone().run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |c| {
            egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
        },
    );
    fn walk(shape: &egui::Shape, found: &mut Vec<String>) {
        match shape {
            egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, found)),
            egui::Shape::Text(t) => found.push(t.galley.text().to_owned()),
            _ => {}
        }
    }
    let mut found = Vec::new();
    out.shapes.iter().for_each(|c| walk(&c.shape, &mut found));
    found
}

/// Every mirror in the workspace, with what it is pointed at.
fn mirrors(ws: &Workspace) -> Vec<(NodeUid, NodeUid)> {
    ws.live_ids()
        .into_iter()
        .filter(|uid| {
            ws.get_node(*uid)
                .is_some_and(|n| n.as_ref().as_any_ref().is::<Mirror>())
        })
        .filter_map(|uid| Some((uid, ws.send_request(uid.cast::<Mirror>(), MirrorTarget)?)))
        .collect()
}

/// How many `Rect`s are in the workspace.
fn rect_count(ws: &Workspace) -> usize {
    ws.live_ids()
        .into_iter()
        .filter(|uid| {
            ws.get_node(*uid)
                .is_some_and(|n| n.as_ref().as_any_ref().is::<Rect>())
        })
        .count()
}

/// A canvas lambda with one argument, and `result` wired to its output pin.
struct Built {
    ws: Workspace,
    /// The node the output pin points at.
    result: NodeUid,
    /// The port carrying the lambda's one argument.
    port: NodeUid,
}

fn canvas_lambda_returning(result: Arc<dyn Node>) -> Built {
    let mut ws = Desktops::new_workspace();
    let handle = ws.action_handle();

    let uid = NodeUid::mint();
    let args = NodeUid::<LambdaArgs>::mint();
    let canvas = NodeUid::<ComputeCanvas>::mint();
    let pin = NodeUid::mint();
    handle.insert_node_at_dyn(
        uid,
        Arc::new(CanvasLambda::new_with(
            handle.clone(),
            args,
            canvas,
            pin,
            "Under test".to_owned(),
        )),
    );
    // One argument, so there is a parameter pin to look at as well.
    let (arg, port) = (NodeUid::<LambdaArg>::mint(), NodeUid::mint());
    LambdaArg::build_with(handle.clone(), arg, port, "for".to_owned(), "x".to_owned());
    ws.process_pending();
    let args_node = ws
        .send_request(uid, LambdaArgsNode)
        .expect("the lambda has an argument row");
    ws.submit_action(args_node, "add an argument", AddArgAt { arg: arg.erase() });

    let result = handle.insert_node_dyn(result);
    ws.process_pending();
    ws.set_root(uid);
    settle(&mut ws);

    // The pin the caller minted is the *outer* port; the canvas's own output
    // pin is what a result is wired to.
    let output_pin = ws
        .send_request(canvas, OutputPin)
        .expect("the compute canvas has an output pin");
    ws.submit_action(
        output_pin,
        "Wire up the result",
        SetConnection {
            target: Some(result),
        },
    );
    settle(&mut ws);
    Built { ws, result, port }
}

/**
    A result that is a node is drawn, not called `<node>`.

    The word is the whole bug: `ScriptValue::Node(_).display()` is the literal
    string `"<node>"`, and every canvas lambda whose result was anything but a
    number or a table showed exactly that.
*/
#[test]
fn a_node_valued_result_is_drawn_rather_than_named() {
    let Built { mut ws, result, .. } = canvas_lambda_returning(Arc::new(Rect::new(40.0, 24.0, Color::rgb(200, 90, 90))));
    let ctx = egui::Context::default();
    // egui's first pass is a sizing pass, so nothing is where it will be yet.
    let _ = painted(&mut ws, &ctx);
    let words = painted(&mut ws, &ctx);

    assert!(
        !words.iter().any(|w| w.contains("<node>")),
        "the result should be shown, not described: {words:?}"
    );
    let following: Vec<NodeUid> = mirrors(&ws)
        .into_iter()
        .filter(|(_, target)| *target == result)
        .map(|(uid, _)| uid)
        .collect();
    assert_eq!(
        following.len(),
        1,
        "exactly one mirror should be following the result"
    );
    assert!(
        rect_count(&ws) >= 2,
        "the mirror should be holding a copy of the rect, so there are two"
    );
}

/**
    The copy is the mirror's, not the original moved.

    Worth its own assertion: a version that reparented the result instead of
    copying it would pass the test above and quietly empty the canvas the result
    lives on.
*/
#[test]
fn the_original_stays_where_it_is() {
    let Built { ws, result, .. } = canvas_lambda_returning(Arc::new(Rect::new(40.0, 24.0, Color::rgb(200, 90, 90))));
    assert!(
        ws.get_node(result).is_some(),
        "the node wired to the output pin is still there"
    );
    let copies: Vec<NodeUid> = mirrors(&ws)
        .into_iter()
        .filter(|(_, target)| *target == result)
        .map(|(uid, _)| uid)
        .collect();
    assert_eq!(copies.len(), 1, "one mirror follows it");
    assert_ne!(copies[0], result, "the mirror is not the node itself");
}

/// A number still reads as its number — the cheap case must not regress.
#[test]
fn a_number_valued_result_still_reads_as_the_number() {
    let Built { mut ws, .. } = canvas_lambda_returning(Arc::new(Integer::new(4321)));
    let ctx = egui::Context::default();
    let _ = painted(&mut ws, &ctx);
    let words = painted(&mut ws, &ctx);
    assert!(
        words.iter().any(|w| w.contains("4321")),
        "the result should read as 4321: {words:?}"
    );
    assert!(
        !words.iter().any(|w| w.contains("<node>")),
        "and never as the placeholder: {words:?}"
    );
}

/**
    A parameter pin shows its argument too.

    Same rendering, same bug: an argument that was a node arrived on the canvas
    inside reading `x: <node>`, which is no use to the thing meant to be reading
    it.
*/
#[test]
fn a_parameter_pin_shows_what_is_wired_to_it() {
    let Built {
        mut ws,
        result,
        port,
    } = canvas_lambda_returning(Arc::new(Integer::new(4321)));

    // Wire the lambda's own argument to a rect, so the pin has a node in it.
    let wired = ws.action_handle().insert_node_dyn(Arc::new(Rect::new(40.0, 24.0, Color::rgb(200, 90, 90))));
    ws.process_pending();
    ws.submit_action(
        port,
        "Wire the argument",
        SetConnection {
            target: Some(wired),
        },
    );
    settle(&mut ws);

    let following: Vec<NodeUid> = mirrors(&ws)
        .into_iter()
        .filter(|(_, target)| *target == wired)
        .map(|(uid, _)| uid)
        .collect();
    assert_eq!(
        following.len(),
        1,
        "the pin's mirror should be following what the argument is wired to"
    );
    assert!(
        ws.get_node(result).is_some(),
        "and the result is untouched by any of it"
    );
}
