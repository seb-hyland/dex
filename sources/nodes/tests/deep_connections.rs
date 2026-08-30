//! A wire points at whatever the user can point at.
//!
//! Connections used to land only on top-level canvas items, so a container that
//! wanted its innards wired up had to keep its own register of their rects, and
//! a lambda had to pretend to *be* its output. The inspect probe already walks
//! every addressable node each frame and knows where it drew, so that is now
//! the one hit test, at any depth.

use dex_core::prelude::*;
use dex_nodes::composites::lambda::{
    ComputeCanvas, ComputeParam, Lambda, LambdaOutput, SyncParams,
};
use dex_nodes::layouts::canvas::layout::{AddCanvasItem, CanvasChildren};
use dex_nodes::layouts::canvas::nodes::{
    CanvasNode, CanvasNodeChild, CanvasNodeConstraints, SetLayout,
};
use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
use dex_nodes::primitives::text::Label;
use dex_nodes::scripting::{ScriptValue, resolve_arg};
use std::collections::HashSet;

const SCREEN: egui::Vec2 = egui::vec2(1200.0, 900.0);

fn frame(ws: &mut Workspace, ctx: &egui::Context) {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let input = egui::RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
    });
}

/// Two frames: a lookup reads the last *finished* frame, so that a node asking
/// where another sits does not depend on which of them draws first.
fn settle(ws: &mut Workspace, ctx: &egui::Context) {
    frame(ws, ctx);
    frame(ws, ctx);
}

fn centre(region: ScreenRegion) -> ScreenPos {
    ScreenPos {
        x: (region.min.x + region.max.x) * 0.5,
        y: (region.min.y + region.max.y) * 0.5,
    }
}

/// Every node reachable from the root, so a test can find one by type.
fn all_nodes(ws: &Workspace) -> Vec<NodeUid> {
    let mut seen = HashSet::new();
    let mut queue = vec![ws.root()];
    let mut out = Vec::new();
    while let Some(uid) = queue.pop() {
        if !seen.insert(uid) {
            continue;
        }
        out.push(uid);
        if let Some(node) = ws.get_node(uid) {
            node.owned_refs(&mut |child| queue.push(child));
        }
    }
    out
}

/// A desktop holding one lambda, whose output already carries a value.
struct Wired {
    ws: Workspace,
    item: NodeUid,
    lambda: NodeUid,
    output: NodeUid,
}

fn a_lambda_on_a_canvas() -> Wired {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let root = ws.root();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    frame(&mut ws, &ctx);

    let lambda = Lambda::new(ws.action_handle());
    ws.submit_action_dyn(Action {
        dest: root,
        description: "add lambda".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(lambda),
            size: Vector { x: 360.0, y: 300.0 },
        }),
    });
    ws.process_pending();

    let canvas = ws
        .send_request(root.cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let item = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the lambda was added");
    let lambda = ws
        .send_request(item, CanvasNodeChild)
        .expect("the item wraps the lambda");
    let output = ws
        .send_request(lambda.cast::<Lambda>(), LambdaOutput)
        .expect("the lambda has an output");

    // A freshly built lambda outputs `Nothing`, which draws nothing and so has
    // nowhere to point at. Give it a result, as a run would.
    ws.insert_node_now_at(output.cast::<Label>(), Label::new("42".to_owned()));
    settle(&mut ws, &ctx);

    Wired {
        ws,
        item: item.erase(),
        lambda,
        output,
    }
}

#[test]
fn a_wire_lands_on_the_output_inside_the_item_not_the_item() {
    let Wired {
        ws,
        item,
        output,
        lambda,
    } = a_lambda_on_a_canvas();

    let rect = ws
        .inspectable_rect(output)
        .expect("the output drew somewhere pointable");
    let hit = ws.inspectable_at(centre(rect));

    assert_eq!(
        hit,
        Some(output),
        "dropping on the output wires to the output"
    );
    assert_ne!(hit, Some(item), "not to the canvas item around it");
    assert_ne!(hit, Some(lambda), "and not to the lambda either");
}

#[test]
fn the_item_still_answers_away_from_its_inner_parts() {
    let Wired {
        ws, item, output, ..
    } = a_lambda_on_a_canvas();

    let item_rect = ws
        .inspectable_rect(item)
        .expect("the canvas item drew somewhere pointable");
    let output_rect = ws.inspectable_rect(output).expect("so did the output");

    // The lambda's name sits at the top of the item, well clear of the output
    // along the bottom.
    let near_the_top = ScreenPos {
        x: centre(item_rect).x,
        y: item_rect.min.y + 4.0,
    };
    assert!(
        near_the_top.y < output_rect.min.y,
        "the probe point is above the output, or this proves nothing"
    );
    assert_eq!(
        ws.inspectable_at(near_the_top),
        Some(item),
        "the whole item is still the target away from its inner parts"
    );
}

#[test]
fn a_lambda_no_longer_stands_in_for_its_output() {
    let Wired {
        ws,
        lambda,
        output,
        item,
        ..
    } = a_lambda_on_a_canvas();

    assert!(
        matches!(resolve_arg(&ws, output).value, ScriptValue::Str(ref s) if s == "42"),
        "the output resolves to what it holds"
    );
    assert!(
        matches!(resolve_arg(&ws, lambda).value, ScriptValue::Node(uid) if uid == lambda),
        "the lambda resolves to itself, not to its output: wire the output up \
         directly instead"
    );
    // The canvas item is a wrapper, so it still speaks for what it wraps.
    assert!(
        matches!(resolve_arg(&ws, item).value, ScriptValue::Node(uid) if uid == lambda),
        "and the item around it resolves to the lambda"
    );
}

#[test]
fn a_compute_canvas_pin_is_wirable_where_it_drew() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let compute = ComputeCanvas::build(ws.action_handle());
    ws.process_pending();
    ws.set_root(compute.erase());

    ws.submit_action(
        compute,
        "sync params",
        SyncParams {
            entries: vec![
                ("first".to_owned(), "one".to_owned()),
                ("second".to_owned(), "two".to_owned()),
            ],
        },
    );
    ws.process_pending();

    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    settle(&mut ws, &ctx);

    let pins: Vec<NodeUid> = all_nodes(&ws)
        .into_iter()
        .filter(|uid| {
            ws.get_node(*uid)
                .is_some_and(|node| (*node).as_any_ref().is::<ComputeParam>())
        })
        .collect();
    assert_eq!(pins.len(), 2, "both pins are live");

    // The pins used to be found through a register the compute canvas kept by
    // hand; they are found now because they are drawn as pointable.
    for pin in pins {
        let rect = ws
            .inspectable_rect(pin)
            .expect("the pin drew somewhere pointable");
        assert_eq!(
            ws.inspectable_at(centre(rect)),
            Some(pin),
            "dropping on a pin wires to that pin"
        );
    }
}

/// A canvas holding one label, ready to be shoved around.
fn a_label_on_a_canvas() -> (Workspace, egui::Context, NodeUid) {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let root = ws.root();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    frame(&mut ws, &ctx);

    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(Label::new("Hello".to_owned())),
            size: Vector { x: 200.0, y: 100.0 },
        }),
    });
    ws.process_pending();

    let canvas = ws
        .send_request(root.cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let item = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the item was added");
    settle(&mut ws, &ctx);
    (ws, ctx, item.erase())
}

/// Shift `item` by `dx` in canvas space, which shifts it by `dx` on screen too.
fn shove(ws: &mut Workspace, ctx: &egui::Context, item: NodeUid, dx: f32) {
    let at = ws
        .send_request(item.cast::<CanvasNode>(), CanvasNodeConstraints)
        .expect("the item has a layout");
    ws.submit_action(
        item.cast::<CanvasNode>(),
        "move item",
        SetLayout {
            canvas_pos: Vector {
                x: at.pos.x + dx,
                y: at.pos.y,
            },
            size: at.size,
        },
    );
    ws.process_pending();
    settle(ws, ctx);
}

#[test]
fn a_node_shoved_off_screen_can_still_be_wired_to() {
    let (mut ws, ctx, item) = a_label_on_a_canvas();
    let before = ws.inspectable_rect(item).expect("it drew on screen");

    shove(&mut ws, &ctx, item, 4000.0);

    let after = ws
        .inspectable_rect(item)
        .expect("a node off the edge of the window still exists, and still drew");
    assert!(
        after.min.x > SCREEN.x,
        "it really is off screen: {} vs a {}-wide window",
        after.min.x,
        SCREEN.x
    );
    assert!(
        (after.min.x - (before.min.x + 4000.0)).abs() < 1.0,
        "and it is reported where it actually went, not where it was clipped"
    );
    // Nothing to aim at, though: the pointer cannot reach off-screen.
    assert_eq!(
        ws.inspectable_at(centre(after)),
        None,
        "and it is not something the pointer can land on"
    );
}

#[test]
fn a_wire_anchors_to_the_whole_node_not_the_sliver_on_screen() {
    let (mut ws, ctx, item) = a_label_on_a_canvas();
    let before = ws.inspectable_rect(item).expect("it drew on screen");

    // Leave a 40px sliver of the item against the right edge of the window.
    shove(&mut ws, &ctx, item, SCREEN.x - 40.0 - before.min.x);

    let after = ws.inspectable_rect(item).expect("the sliver still drew");
    assert!(
        after.max.x > SCREEN.x,
        "the region runs past the window edge rather than stopping at it: \
         {} vs {}",
        after.max.x,
        SCREEN.x
    );
    assert!(
        (after.size().x - before.size().x).abs() < 1.0,
        "the node has not shrunk, so a wire to it stays pinned to the node \
         rather than sliding along with the window edge"
    );
    // The sliver is still pointable.
    let on_the_sliver = ScreenPos {
        x: SCREEN.x - 20.0,
        y: centre(after).y,
    };
    assert_eq!(
        ws.inspectable_at(on_the_sliver),
        Some(item),
        "and the part still showing can be dropped on"
    );
}
