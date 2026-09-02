//! Restacking an item on the canvas.
//!
//! Stacking is the order of a canvas's children and nothing else: an item lands
//! on top when it is placed and stays there, so an item covered by a later one
//! could never be brought back out. The inspector's Bring to Front and Send to
//! Back are the way to say so, and they offer themselves only for a target that
//! sits in some surface's draw order.

use dex_core::prelude::*;
use dex_nodes::{
    composites::button::Button,
    layouts::{
        canvas::layout::{
            AddCanvasItem, BringCanvasItemToFront, Canvas, CanvasChildren, ConnectableAt,
            SendCanvasItemToBack,
        },
        inspector::PlacementCommands,
    },
    primitives::{nothing::Nothing, text::Label},
};
use std::collections::HashSet;

/// An empty workspace with a throwaway root, drained and ready.
fn workspace() -> Workspace {
    let mut ws = Workspace::new_empty();
    let root = ws.insert_node_now(Nothing);
    ws.set_root(root.erase());
    ws
}

/// A canvas holding `count` labels, every one of them stacked on the same spot:
/// an item is centred in the viewport, and nothing has drawn to move it.
fn stacked_canvas(count: usize) -> (Workspace, NodeUid<Canvas>, Vec<NodeUid>) {
    let mut ws = workspace();
    let canvas = Canvas::build(ws.action_handle());
    ws.process_pending();

    for i in 0..count {
        ws.submit_action(
            canvas,
            "add item",
            AddCanvasItem {
                child: Arc::new(Label::new(format!("item {i}"))),
                size: Vector { x: 200.0, y: 100.0 },
            },
        );
    }
    ws.process_pending();

    let children = ws.send_request(canvas, CanvasChildren).unwrap_or_default();
    assert_eq!(children.len(), count, "every item joined the surface");
    (ws, canvas, children)
}

/// The topmost item at the point they all overlap on.
fn topmost(ws: &Workspace, canvas: NodeUid<Canvas>) -> NodeUid {
    ws.send_request(
        canvas,
        ConnectableAt {
            pos: ScreenPos::zero(),
        },
    )
    .flatten()
    .expect("the stack is under the point")
}

#[test]
fn the_bottom_item_can_be_brought_out_from_under_the_stack() {
    let (mut ws, canvas, items) = stacked_canvas(3);
    let (bottom, top) = (items[0], items[2]);
    assert_eq!(topmost(&ws, canvas), top, "the last placed item covers it");

    ws.submit_action(canvas, "raise", BringCanvasItemToFront { node: bottom });
    ws.process_pending();

    let children = ws.send_request(canvas, CanvasChildren).unwrap_or_default();
    assert_eq!(
        children,
        vec![items[1], items[2], bottom],
        "it draws last now, and the order under it is otherwise untouched"
    );
    assert_eq!(topmost(&ws, canvas), bottom, "so it is what is pointed at");
}

#[test]
fn the_top_item_can_be_put_back_under_the_stack() {
    let (mut ws, canvas, items) = stacked_canvas(3);
    let top = items[2];

    ws.submit_action(canvas, "lower", SendCanvasItemToBack { node: top });
    ws.process_pending();

    let children = ws.send_request(canvas, CanvasChildren).unwrap_or_default();
    assert_eq!(
        children,
        vec![top, items[0], items[1]],
        "it draws first now, and the order over it is otherwise untouched"
    );
    assert_eq!(
        topmost(&ws, canvas),
        items[1],
        "what it was covering is pointed at instead"
    );
}

#[test]
fn restacking_something_this_surface_does_not_hold_changes_nothing() {
    let (mut ws, canvas, items) = stacked_canvas(2);
    let stranger = ws
        .insert_node_now(Label::new("elsewhere".to_owned()))
        .erase();

    ws.submit_action(canvas, "raise", BringCanvasItemToFront { node: stranger });
    ws.submit_action(canvas, "lower", SendCanvasItemToBack { node: stranger });
    ws.process_pending();

    assert_eq!(
        ws.send_request(canvas, CanvasChildren).unwrap_or_default(),
        items,
        "the surface's own order stands"
    );
}

/// The label of every button `commands` owns, sorted, and each counted once:
/// the commands hold their buttons and so does the column they sit in.
fn command_labels(ws: &Workspace, commands: NodeUid<PlacementCommands>) -> Vec<String> {
    let mut labels = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = vec![commands.erase()];
    while let Some(uid) = queue.pop() {
        if !seen.insert(uid) {
            continue;
        }
        let Some(node) = ws.get_node(uid) else {
            continue;
        };
        if let Some(button) = (*node).as_any_ref().downcast_ref::<Button>() {
            labels.push(button.label.text.clone());
            continue;
        }
        node.owned_refs(&mut |child| queue.push(child));
    }
    labels.sort();
    labels
}

#[test]
fn only_a_canvas_item_is_offered_the_restacking_commands() {
    let mut ws = workspace();
    let target = ws.insert_node_now(Label::new("target".to_owned())).erase();
    let size = Vector { x: 120.0, y: 80.0 };

    let plain = PlacementCommands::build(ws.action_handle(), target, size);
    let on_canvas = PlacementCommands::build_for_canvas_item(ws.action_handle(), target, size);
    ws.process_pending();

    assert_eq!(
        command_labels(&ws, plain),
        vec![
            "Copy".to_owned(),
            "Copy to Backpack".to_owned(),
            "Mirror".to_owned(),
            "Mirror to Backpack".to_owned(),
        ],
        "a result has no draw order to move within"
    );
    assert_eq!(
        command_labels(&ws, on_canvas),
        vec![
            "Bring to Front".to_owned(),
            "Copy".to_owned(),
            "Copy to Backpack".to_owned(),
            "Mirror".to_owned(),
            "Mirror to Backpack".to_owned(),
            "Send to Back".to_owned(),
        ],
        "an item on a surface can be raised and lowered"
    );
}
