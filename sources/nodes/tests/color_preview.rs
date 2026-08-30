//! A colour picker shows the colour under the pointer while a gesture is still
//! choosing it, and settles on that colour when the gesture ends. Read by
//! polling, so nothing is lost to a frame that does not look.

use dex_core::prelude::*;
use dex_nodes::primitives::color_picker::{
    ColorPicker, ColorSlot, IsPicking, PickedColor, repicked,
};
use dex_nodes::primitives::text::Label;

const SCREEN: egui::Vec2 = egui::vec2(1200.0, 900.0);

fn frame(ws: &mut Workspace, ctx: &egui::Context, events: Vec<egui::Event>) {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let input = egui::RawInput {
        screen_rect: Some(screen),
        events,
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
    });
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

fn rgba(color: Color) -> [u8; 4] {
    [color.r, color.g, color.b, color.a]
}

/// An open picker as the workspace root, plus a label for it to recolour.
fn open_picker() -> (Workspace, egui::Context, NodeUid<ColorPicker>, NodeUid) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let mut node = ColorPicker::new("Colour".to_owned(), Color::BLACK);
    node.expanded = true;
    let picker = ws.insert_node_now(node);
    let label = ws.insert_node_now(Label::new("Hello".to_owned())).erase();
    ws.set_root(picker.erase());
    let ctx = egui::Context::default();
    frame(&mut ws, &ctx, vec![]);
    (ws, ctx, picker, label)
}

/// The label's committed colour, and the one it would actually draw.
fn label_colours(ws: &Workspace, label: NodeUid) -> ([u8; 4], [u8; 4]) {
    let node = ws.get_node(label).expect("the label is live");
    let label = (*node)
        .as_any_ref()
        .downcast_ref::<Label>()
        .expect("it is a label");
    (rgba(label.color), rgba(label.shown_color()))
}

/// Well inside the saturation/value square, whose top is a little under the
/// swatch row and which is far taller than that row.
const IN_SQUARE: egui::Pos2 = egui::pos2(200.0, 100.0);
const ELSEWHERE_IN_SQUARE: egui::Pos2 = egui::pos2(120.0, 60.0);

#[test]
fn the_colour_on_show_follows_the_pointer_before_it_is_let_go() {
    let (mut ws, ctx, picker, label) = open_picker();
    assert_eq!(ws.send_request(picker, IsPicking), Some(false));
    assert_eq!(
        ws.send_request(picker, PickedColor).map(rgba),
        Some(rgba(Color::BLACK)),
        "it starts on the colour it was built with"
    );

    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(IN_SQUARE)]);
    frame(&mut ws, &ctx, vec![button(IN_SQUARE, true)]);
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(ELSEWHERE_IN_SQUARE)]);

    assert_eq!(
        ws.send_request(picker, IsPicking),
        Some(true),
        "the gesture is still choosing"
    );
    let previewed = ws
        .send_request(picker, PickedColor)
        .expect("a colour to show");
    assert_ne!(
        rgba(previewed),
        rgba(Color::BLACK),
        "and the colour on show has followed the pointer"
    );
    // Nothing was consumed, so asking again gives the same answer.
    assert_eq!(
        ws.send_request(picker, PickedColor).map(rgba),
        Some(rgba(previewed)),
        "polling it twice reads the same colour"
    );
    assert!(
        repicked(&ws, picker, label, ColorSlot::Fill, Color::BLACK).is_none(),
        "a caller holds off committing while the gesture is still going"
    );
    let (committed, on_show) = label_colours(&ws, label);
    assert_eq!(
        committed,
        rgba(Color::BLACK),
        "the label has not taken the colour for real"
    );
    assert_eq!(
        on_show,
        rgba(previewed),
        "but it is already drawing what the picker is showing"
    );

    frame(&mut ws, &ctx, vec![button(ELSEWHERE_IN_SQUARE, false)]);
    frame(&mut ws, &ctx, vec![]);

    assert_eq!(ws.send_request(picker, IsPicking), Some(false));
    assert_eq!(
        ws.send_request(picker, PickedColor).map(rgba),
        Some(rgba(previewed)),
        "letting go settles on exactly what was on show"
    );
    assert_eq!(
        repicked(&ws, picker, label, ColorSlot::Fill, Color::BLACK).map(rgba),
        Some(rgba(previewed)),
        "and the caller is now told to apply it for real"
    );
    // Told the colour has been applied for real, the helper drops the preview.
    // Nothing here actually applied it, so the label falls back to its own.
    assert!(
        repicked(&ws, picker, label, ColorSlot::Fill, previewed).is_none(),
        "once applied, there is nothing left to do"
    );
    let (committed, on_show) = label_colours(&ws, label);
    assert_eq!(committed, on_show, "and the preview is no longer standing");
}
