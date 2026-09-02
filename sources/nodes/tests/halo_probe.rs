//! The inspector can only find what the draw pass offers it, so check the hit-test
//! end to end: a real frame, a real pointer position.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::AddCanvasItem;
use dex_nodes::layouts::desktops::Desktops;
use dex_nodes::primitives::text::Label;

/// Run one frame with the pointer at `pointer`, and report what the inspector found.
fn target_under(ws: &mut Workspace, pointer: egui::Pos2) -> Option<NodeUid> {
    let egui_ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 900.0));
    let input = egui::RawInput {
        screen_rect: Some(screen),
        events: vec![egui::Event::PointerMoved(pointer)],
        ..Default::default()
    };

    // Two passes: the first settles layout, the second sees a stable pointer.
    let mut found = None;
    for _ in 0..2 {
        let _ = egui_ctx.run_ui(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ws.draw_frame(ui, screen);
            });
        });
        found = ws.inspect_target().map(|t| t.node);
    }
    found
}

#[test]
fn the_probe_finds_a_canvas_item_under_the_pointer() {
    use dex_nodes::layouts::canvas::layout::{CanvasChildren, NodeScreenRect};
    use dex_nodes::layouts::desktops::ActiveCanvas;

    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let root = ws.root();

    let _ = target_under(&mut ws, egui::pos2(-100.0, -100.0));

    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(Label::new("Hello, world!".to_owned())),
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

    // Ask where it actually landed rather than guessing.
    let rect = ws
        .send_request(root, NodeScreenRect { node: item.erase() })
        .flatten()
        .expect("the item has an on-screen region");
    let centre = egui::pos2(
        (rect.min.x + rect.max.x) * 0.5,
        (rect.min.y + rect.max.y) * 0.5,
    );

    assert_eq!(
        target_under(&mut ws, centre),
        Some(item.erase()),
        "the pointer over a canvas item finds that item"
    );
}

/// A node clipped out of view cannot be pointed at, even where it would have
/// drawn: the canvas is clipped below the tab bar, so an item panned up under
/// the bar is unreachable there.
#[test]
fn a_clipped_node_is_not_a_candidate() {
    use dex_nodes::layouts::canvas::layout::{CanvasChildren, NodeScreenRect};
    use dex_nodes::layouts::desktops::ActiveCanvas;

    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let root = ws.root();

    // Added before the first frame, so it is centred from an unknown viewport
    // and lands up and to the left — over the tab bar and the sidebar.
    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(Label::new("out of bounds".to_owned())),
            size: Vector { x: 200.0, y: 100.0 },
        }),
    });
    ws.process_pending();
    let _ = target_under(&mut ws, egui::pos2(-100.0, -100.0));

    let canvas = ws
        .send_request(root.cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let item = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the item was added");
    let rect = ws
        .send_request(root, NodeScreenRect { node: item.erase() })
        .flatten()
        .expect("the item reports where it would draw");

    // Its own top-left is above the content area, so nothing of it is there.
    assert!(
        rect.min.y < 42.0,
        "this item really does reach up under the tab bar"
    );
    assert_ne!(
        target_under(&mut ws, egui::pos2(rect.min.x + 4.0, rect.min.y + 4.0)),
        Some(item.erase()),
        "the clipped-away part of an item is not pointable"
    );
}

/// Panning is gated on where a drag begins: `ConnectableAt` is the same hit
/// test the pan uses, so it answers for an item and not for empty background.
#[test]
fn the_canvas_distinguishes_items_from_background() {
    use dex_nodes::layouts::canvas::layout::{CanvasChildren, ConnectableAt, NodeScreenRect};
    use dex_nodes::layouts::desktops::ActiveCanvas;

    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let root = ws.root();

    // A frame first, so the item is centred in a known viewport.
    let _ = target_under(&mut ws, egui::pos2(-100.0, -100.0));
    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(Label::new("body".to_owned())),
            size: Vector { x: 200.0, y: 100.0 },
        }),
    });
    ws.process_pending();
    let _ = target_under(&mut ws, egui::pos2(-100.0, -100.0));

    let canvas = ws
        .send_request(root.cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let item = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the item was added");
    let rect = ws
        .send_request(root, NodeScreenRect { node: item.erase() })
        .flatten()
        .expect("the item is on screen");

    let centre = ScreenPos {
        x: (rect.min.x + rect.max.x) * 0.5,
        y: (rect.min.y + rect.max.y) * 0.5,
    };
    assert_eq!(
        ws.send_request(canvas, ConnectableAt { pos: centre })
            .flatten(),
        Some(item.erase()),
        "a point inside the item belongs to the item, so a drag there is not a pan"
    );

    let background = ScreenPos {
        x: rect.max.x + 80.0,
        y: rect.max.y + 80.0,
    };
    assert_eq!(
        ws.send_request(canvas, ConnectableAt { pos: background })
            .flatten(),
        None,
        "empty background belongs to no item, so a drag there pans"
    );
}
