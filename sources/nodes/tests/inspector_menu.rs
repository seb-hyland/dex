//! Opening the inspector must terminate.
//!
//! A canvas item's inspector nests the inspector of the node it *wraps*. Asking
//! the item itself instead lands straight back in `CanvasNode::build_inspector`,
//! which recurses until the stack runs out — and it does so while an action is
//! being applied, so it never reaches drawing and looks like a UI fault.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{AddCanvasItem, CanvasChildren, NodeScreenRect};
use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
use dex_nodes::primitives::text::Label;

/// Run one frame, delivering `events`.
fn frame(ws: &mut Workspace, ctx: &egui::Context, events: Vec<egui::Event>) {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 900.0));
    let input = egui::RawInput {
        screen_rect: Some(screen),
        events,
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
    });
}

fn click(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

#[test]
fn opening_the_inspector_on_a_canvas_item_terminates() {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let root = ws.root();

    // A frame first: an item is centred from the canvas viewport, which is not
    // known until the canvas has drawn once. Added before that it lands over the
    // tab bar, where it is now correctly clipped away and so unreachable.
    let ctx = egui::Context::default();
    frame(&mut ws, &ctx, vec![]);

    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(Label::new("Hello, world!".to_owned())),
            size: Vector { x: 200.0, y: 100.0 },
        }),
    });
    ws.process_pending();
    frame(&mut ws, &ctx, vec![]);

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

    // Hover the item, then take the lens position from the region the inspector
    // itself uses, rather than recomputing it from the canvas mapping.
    let centre = egui::pos2(
        (rect.min.x + rect.max.x) * 0.5,
        (rect.min.y + rect.max.y) * 0.5,
    );
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(centre)]);
    let found = ws.inspect_target().expect("the item is under the pointer");
    assert_eq!(
        found.node,
        item.erase(),
        "the lens targets the item it sits beside"
    );
    let lens = egui::pos2(found.region.min.x - 15.0, found.region.min.y + 11.0);
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(lens)]);

    /*
        Clicking the lens must not hang. Before the fix it recursed while the
        `OpenInspector` action was being applied, so it never reached a draw.

        Whether the menu actually opens is not asserted: a synthetic click does
        not register on the lens in this harness, and the open/close path lives
        in egui's popup state rather than in the workspace.
    */
    frame(&mut ws, &ctx, vec![click(lens, true), click(lens, false)]);
    for _ in 0..3 {
        frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(lens)]);
    }
}

/// A desktop tab's inspector: clone and mirror the whole surface, and reorder.
#[test]
fn a_tab_can_be_cloned_mirrored_and_reordered() {
    use dex_nodes::layouts::canvas::nodes::CanvasNodeChild;
    use dex_nodes::layouts::desktops::{CloneCanvas, MirrorCanvas, MoveTab, Tabs};
    use dex_nodes::layouts::mirror::{Mirror, MirrorTarget};

    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let root = ws.root();
    let desktops = root.cast::<Desktops>();
    let ctx = egui::Context::default();
    frame(&mut ws, &ctx, vec![]);

    // One item on the starting desktop.
    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(Label::new("body".to_owned())),
            size: Vector { x: 120.0, y: 80.0 },
        }),
    });
    ws.process_pending();

    let first_tab = ws.send_request(desktops, Tabs).unwrap_or_default()[0];
    let first_canvas = ws
        .send_request(desktops, ActiveCanvas)
        .expect("a canvas is active");
    let source_item = ws
        .send_request(first_canvas, CanvasChildren)
        .unwrap_or_default()[0];
    let source_child = ws
        .send_request(source_item, CanvasNodeChild)
        .expect("the item wraps a label");

    // Clone: a second desktop whose item is a copy, sharing nothing.
    ws.submit_action(desktops, "clone", CloneCanvas { tab: first_tab });
    ws.process_pending();
    let cloned_canvas = ws
        .send_request(desktops, ActiveCanvas)
        .expect("the clone is active");
    assert_ne!(cloned_canvas, first_canvas, "a new canvas");
    let cloned_item = ws
        .send_request(cloned_canvas, CanvasChildren)
        .unwrap_or_default()[0];
    assert_ne!(cloned_item, source_item, "with its own item");
    assert_ne!(
        ws.send_request(cloned_item, CanvasNodeChild),
        Some(source_child),
        "wrapping its own copy of the content"
    );

    // Mirror: a third desktop whose items follow the source's.
    ws.submit_action(desktops, "mirror", MirrorCanvas { tab: first_tab });
    ws.process_pending();
    let mirrored_canvas = ws
        .send_request(desktops, ActiveCanvas)
        .expect("the mirror is active");
    let mirrored_item = ws
        .send_request(mirrored_canvas, CanvasChildren)
        .unwrap_or_default()[0];
    let mirrored_child = ws
        .send_request(mirrored_item, CanvasNodeChild)
        .expect("the mirrored item wraps something");
    assert_eq!(
        ws.send_request(mirrored_child.cast::<Mirror>(), MirrorTarget),
        Some(source_child),
        "the item is framed afresh but mirrors the original's content"
    );

    // Three tabs now; move the first to the back and check the order.
    let tabs = ws.send_request(desktops, Tabs).unwrap_or_default();
    assert_eq!(tabs.len(), 3);
    assert_eq!(tabs[0], first_tab);
    ws.submit_action(
        desktops,
        "to back",
        MoveTab {
            tab: first_tab,
            to: 2,
        },
    );
    ws.process_pending();
    let reordered = ws.send_request(desktops, Tabs).unwrap_or_default();
    assert_eq!(reordered[2], first_tab, "it moved to the back");
    assert_eq!(reordered.len(), 3, "and nothing was lost");

    // ...and back to the front.
    ws.submit_action(
        desktops,
        "to front",
        MoveTab {
            tab: first_tab,
            to: 0,
        },
    );
    ws.process_pending();
    assert_eq!(
        ws.send_request(desktops, Tabs).unwrap_or_default()[0],
        first_tab,
        "and back to the front"
    );
}
