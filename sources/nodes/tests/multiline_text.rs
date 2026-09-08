//! A multiline label stays inside the box it is drawn in.
//!
//! Switching a canvas text item to multiline used to drop its text far down and
//! clip it: egui's multiline text field reserves four rows at the least, and the
//! centred text was placed in the middle of that reservation — below a short
//! box, and cut off. A paragraph should start at the top of its box and flow
//! down instead.

use dex_core::prelude::*;
use dex_nodes::primitives::text::{LabelEditable, SetSingleline};

/// The box a default canvas text item takes.
const BOX: Vector = Vector { x: 160.0, y: 40.0 };
const BOX_TOP: f32 = 50.0;

/// Draw `uid` in an exact, clipped box and hand back the text field's rect.
fn editor_rect(ws: &mut Workspace, ctx: &egui::Context, uid: NodeUid, h: f32) -> egui::Rect {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
    let input = egui::RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| {
            let mut ui = ui.new_child(egui::UiBuilder::new());
            let constraints = DrawConstraints {
                pos: ScreenPos {
                    x: 50.0,
                    y: BOX_TOP,
                },
                x: Some(AxisConstraint::Exactly(BOX.x)),
                y: Some(AxisConstraint::Exactly(h)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            };
            let mut draw =
                DrawContext::root(NodeContext { id: uid, workspace: ws }, constraints, &mut ui);
            let _ = draw.draw_workspace_node(uid, constraints);
        });
    });
    // Two frames: egui reports a widget's rect from the frame before.
    let mut rect = ctx.read_response(egui::Id::new(uid)).map(|r| r.rect);
    if rect.is_none() {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                let mut ui = ui.new_child(egui::UiBuilder::new());
                let constraints = DrawConstraints {
                    pos: ScreenPos {
                        x: 50.0,
                        y: BOX_TOP,
                    },
                    x: Some(AxisConstraint::Exactly(BOX.x)),
                    y: Some(AxisConstraint::Exactly(h)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                };
                let mut draw =
                    DrawContext::root(NodeContext { id: uid, workspace: ws }, constraints, &mut ui);
                let _ = draw.draw_workspace_node(uid, constraints);
            });
        });
        rect = ctx.read_response(egui::Id::new(uid)).map(|r| r.rect);
    }
    rect.expect("the text field drew")
}

fn multiline_label(ws: &mut Workspace, text: &str) -> NodeUid {
    let uid = ws.insert_node_now(LabelEditable::new(text.to_owned())).erase();
    ws.submit_action(uid.cast::<LabelEditable>(), "multiline", SetSingleline { on: false });
    ws.process_pending();
    uid
}

/// A short paragraph sits at the top of its box, one row tall — not centred in a
/// four-row reservation that drops it below a short box and clips it.
#[test]
fn short_multiline_text_stays_at_the_top_of_its_box() {
    dex_nodes::scripting::init_python();
    let ctx = egui::Context::default();
    let mut ws = Workspace::new_empty();

    // A single line's height, for scale: draw one and measure its field.
    let single = ws.insert_node_now(LabelEditable::new("Hi".to_owned())).erase();
    ws.process_pending();
    let row = editor_rect(&mut ws, &ctx, single, BOX.y).height();
    assert!(row > 0.0, "a line has a height");

    let short = multiline_label(&mut ws, "Hi");
    let rect = editor_rect(&mut ws, &ctx, short, BOX.y);

    assert!(
        (rect.min.y - BOX_TOP).abs() < 1.0,
        "the text starts at the top of the box, not pushed down (top was {})",
        rect.min.y
    );
    assert!(
        rect.height() < row * 2.0,
        "a one-line paragraph is about one row tall, not egui's four-row \
         minimum (was {} against a row of {row})",
        rect.height()
    );
    assert!(
        rect.max.y <= BOX_TOP + BOX.y + 1.0,
        "and it does not spill below the box it is drawn in (bottom was {})",
        rect.max.y
    );
}

/// A paragraph that overflows a short box flows from the top and clips at the
/// bottom, rather than centring so its first lines are lost above the box.
#[test]
fn overflowing_multiline_text_flows_from_the_top() {
    dex_nodes::scripting::init_python();
    let ctx = egui::Context::default();
    let mut ws = Workspace::new_empty();

    let long = multiline_label(
        &mut ws,
        "This is a longer sentence that wraps across several lines in a narrow box",
    );
    // A tall box shows the whole paragraph; it must begin at the very top.
    let rect = editor_rect(&mut ws, &ctx, long, 200.0);
    assert!(
        (rect.min.y - BOX_TOP).abs() < 1.0,
        "the paragraph begins at the top of the box (top was {})",
        rect.min.y
    );
    assert!(
        rect.height() > 40.0,
        "and it grows to hold its several wrapped rows (was {})",
        rect.height()
    );
}
