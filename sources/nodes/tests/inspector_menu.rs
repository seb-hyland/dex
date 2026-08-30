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

    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(Label::new("Hello, world!".to_owned())),
            size: Vector { x: 200.0, y: 100.0 },
        }),
    });
    ws.process_pending();

    let ctx = egui::Context::default();
    // A frame first: the item is centred from the canvas viewport, which is not
    // known until the canvas has drawn once.
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

    // The lens sits in the margin to the left of the item.
    let lens = egui::pos2(rect.min.x - 15.0, rect.min.y + 11.0);
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(lens)]);
    assert_eq!(
        ws.inspect_target().map(|t| t.node),
        Some(item.erase()),
        "the lens targets the item it sits beside"
    );

    // Clicking it builds the menu. Before the fix this never returned.
    frame(&mut ws, &ctx, vec![click(lens, true), click(lens, false)]);
    for _ in 0..3 {
        frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(lens)]);
    }
}
