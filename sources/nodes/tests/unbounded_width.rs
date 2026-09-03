//! A menu is drawn with no width constraint, and a filling button must not take
//! that as a width to fill: an infinite rect becomes a NaN once egui lays it
//! out, which panics rather than misdraws.

use dex_core::prelude::*;
use dex_nodes::composites::button::Button;
use dex_nodes::layouts::vertical::VerticalLayout;
use dex_nodes::primitives::text::Label;

/// Draw `root` with an unbounded width and report the region it occupied.
fn drawn_unbounded(ws: &Workspace, root: NodeUid) -> Option<ScreenRegion> {
    let egui_ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(900.0, 2000.0),
        )),
        ..Default::default()
    };

    let mut region = None;
    let _ = egui_ctx.run_ui(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut ui = ui.new_child(egui::UiBuilder::new());
            let constraints = DrawConstraints {
                pos: ScreenPos { x: 0.0, y: 0.0 },
                // No width offered, as a popup gives its contents.
                x: None,
                y: None,
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            };
            let mut draw_ctx = DrawContext::root(
                NodeContext {
                    id: root,
                    workspace: ws,
                },
                constraints,
                &mut ui,
            );
            region = draw_ctx
                .draw_workspace_node(root, constraints)
                .and_then(|r| r.region());
        });
    });
    region
}

#[test]
fn a_filling_button_column_stays_finite_without_a_width() {
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();

    let buttons: Vec<NodeUid> = ["Delete", "Copy", "Mirror"]
        .into_iter()
        .map(|label| {
            Button::build_with(handle.clone(), Label::new(label.to_owned()), |b| {
                b.fill_width = true;
            })
            .erase()
        })
        .collect();
    let menu = VerticalLayout::build(handle, buttons, 2.0).erase();
    ws.set_root(menu);
    ws.process_pending();

    let region = drawn_unbounded(&ws, menu).expect("the column reports a region");
    let size = region.size();
    assert!(
        size.x.is_finite() && size.y.is_finite(),
        "an unbounded offer must not become an infinite region, got {} x {}",
        size.x,
        size.y
    );
    assert!(
        size.x > 0.0,
        "the column is as wide as its widest label, got {}",
        size.x
    );
}
