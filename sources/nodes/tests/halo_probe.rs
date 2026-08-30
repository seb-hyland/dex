//! The halo can only find what the draw pass offers it, so check the hit-test
//! end to end: a real frame, a real pointer position.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::AddCanvasItem;
use dex_nodes::layouts::desktops::Desktops;
use dex_nodes::primitives::text::Label;

/// Run one frame with the pointer at `pointer`, and report what the halo found.
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

    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(Label::new("Hello, world!".to_owned())),
            size: Vector { x: 200.0, y: 100.0 },
        }),
    });
    ws.process_pending();

    // A frame off-screen first: the item is centred using the canvas viewport,
    // which is only known once the canvas has drawn.
    let _ = target_under(&mut ws, egui::pos2(-100.0, -100.0));

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
