//! A label's inspector styles the label it was built for.
//!
//! The controls hold their own state, so what reaches the label is the value
//! the user just chose rather than the one the control still holds. These
//! drive real clicks through real frames to check that round trip.

use dex_core::prelude::*;
use dex_nodes::primitives::checkbox::Checkbox;
use dex_nodes::primitives::text::{Label, LabelEditable};

const SCREEN: egui::Vec2 = egui::vec2(1200.0, 900.0);
/// The spacing `TextStyleInspector` stacks its controls with.
const ROW_GAP: f32 = 3.0;
/// The width a `ColorPicker` settles on when offered the whole screen.
const PICKER_W: f32 = 240.0;
/// The picker's internal geometry, for aiming at its bars.
const PICKER_GAP: f32 = 4.0;
const SQUARE_ASPECT: f32 = 0.62;

/// Run one frame over `ws`, delivering `events`.
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

/// Press and release at `pos`, then settle, so the queued action has landed.
fn click_at(ws: &mut Workspace, ctx: &egui::Context, pos: egui::Pos2) {
    frame(ws, ctx, vec![egui::Event::PointerMoved(pos)]);
    frame(ws, ctx, vec![button(pos, true)]);
    frame(ws, ctx, vec![button(pos, false)]);
    frame(ws, ctx, vec![]);
}

/// The height one tick box takes, which sets where the rows below it start.
fn tick_row_height(ctx: &egui::Context) -> f32 {
    let mut ws = Workspace::new_empty();
    let uid = Checkbox::build(ws.action_handle(), "Bold".to_owned(), false);
    ws.process_pending();

    let mut height = 0.0;
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let input = egui::RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| {
            let mut ui = ui.new_child(egui::UiBuilder::new());
            let constraints = DrawConstraints {
                pos: ScreenPos { x: 0.0, y: 0.0 },
                x: Some(AxisConstraint::AtMost(SCREEN.x)),
                y: Some(AxisConstraint::AtMost(SCREEN.y)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: false,
            };
            let mut draw = DrawContext::for_ui(
                NodeContext {
                    id: uid.erase(),
                    workspace: &ws,
                },
                constraints,
                &mut ui,
            );
            height = draw
                .draw_workspace_node(uid.erase(), constraints)
                .and_then(|r| r.region())
                .map(|r| r.size().y)
                .expect("a tick box draws");
        });
    });
    assert!(height > 0.0, "a tick box has a height");
    height
}

/// Make `target`'s inspector the workspace root, so clicks land on it.
fn inspecting(ws: &mut Workspace, target: NodeUid) {
    let inspector = ws
        .get_node(target)
        .expect("the target is live")
        .build_inspector(NodeContext {
            id: target,
            workspace: ws,
        })
        .expect("a label offers an inspector");
    ws.process_pending();
    ws.set_root(inspector);
}

/// The centre of tick-box row `index`, counting from the top of the column.
fn tick_centre(row_h: f32, index: usize) -> egui::Pos2 {
    egui::pos2(6.0, index as f32 * (row_h + ROW_GAP) + row_h * 0.5)
}

/// `Color` carries no `PartialEq`, so compare the channels.
fn rgba(color: Color) -> [u8; 4] {
    [color.r, color.g, color.b, color.a]
}

fn label_of(ws: &Workspace, uid: NodeUid<Label>) -> Label {
    let node = ws.get_node(uid.erase()).expect("the label is live");
    // Through the `Arc`, or the blanket `AsAny` answers for the handle itself.
    (*node)
        .as_any_ref()
        .downcast_ref::<Label>()
        .expect("it is a label")
        .clone()
}

#[test]
fn the_style_boxes_restyle_the_label_they_were_built_for() {
    dex_nodes::scripting::init_python();
    let ctx = egui::Context::default();
    let row_h = tick_row_height(&ctx);

    let mut ws = Workspace::new_empty();
    let label = ws.insert_node_now(Label::new("Hello".to_owned()));
    ws.process_pending();
    inspecting(&mut ws, label.erase());
    // A frame first: egui only reports a click on a rect it already knows.
    frame(&mut ws, &ctx, vec![]);

    assert!(
        label_of(&ws, label).singleline,
        "a label starts on one line, so its box starts ticked"
    );

    // Bold, italic and underline are the first three rows; single line is the
    // fourth, and starts on, so clicking it turns it off.
    click_at(&mut ws, &ctx, tick_centre(row_h, 0));
    assert!(label_of(&ws, label).font.bold, "the first row is bold");

    click_at(&mut ws, &ctx, tick_centre(row_h, 1));
    assert!(label_of(&ws, label).font.italic, "the second row is italic");

    click_at(&mut ws, &ctx, tick_centre(row_h, 2));
    assert!(
        label_of(&ws, label).font.underline,
        "the third row is underline"
    );

    click_at(&mut ws, &ctx, tick_centre(row_h, 3));
    assert!(
        !label_of(&ws, label).singleline,
        "the fourth row is single line, and it was already on"
    );

    let styled = label_of(&ws, label);
    assert!(
        styled.font.bold && styled.font.italic && styled.font.underline,
        "each box holds its own state, so the toggles do not undo each other"
    );

    // Clicking again takes it back off.
    click_at(&mut ws, &ctx, tick_centre(row_h, 0));
    assert!(
        !label_of(&ws, label).font.bold,
        "a second click on the same box turns it off"
    );
}

#[test]
fn the_colour_picker_recolours_the_label() {
    dex_nodes::scripting::init_python();
    let ctx = egui::Context::default();
    let row_h = tick_row_height(&ctx);

    let mut ws = Workspace::new_empty();
    let label = ws.insert_node_now(Label::new("Hello".to_owned()));
    ws.process_pending();
    inspecting(&mut ws, label.erase());
    frame(&mut ws, &ctx, vec![]);

    assert_eq!(
        rgba(label_of(&ws, label).color),
        rgba(Color::BLACK),
        "labels start black"
    );

    // The picker is the fifth row; clicking its swatch opens it.
    let picker_top = 4.0 * (row_h + ROW_GAP);
    click_at(
        &mut ws,
        &ctx,
        egui::pos2(PICKER_W - 18.0, picker_top + row_h * 0.5),
    );

    // Black has no saturation or value to swing a hue against, so the square
    // comes first — as it would for a user.
    let square_top = picker_top + row_h + PICKER_GAP;
    let square_h = PICKER_W * SQUARE_ASPECT;
    click_at(
        &mut ws,
        &ctx,
        egui::pos2(PICKER_W * 0.8, square_top + square_h * 0.2),
    );
    let lit = label_of(&ws, label).color;
    assert_ne!(
        rgba(lit),
        rgba(Color::BLACK),
        "the saturation/value square recolours the label"
    );
    assert_eq!(lit.a, 255, "and leaves it opaque");

    // Two thirds along the hue bar, which sits under the square.
    let hue_top = square_top + square_h + PICKER_GAP;
    click_at(&mut ws, &ctx, egui::pos2(PICKER_W * 0.66, hue_top + 6.0));
    let hued = label_of(&ws, label).color;
    assert_ne!(rgba(hued), rgba(lit), "and so does the hue bar");

    // The alpha bar is last, and reaches the label's alpha.
    let alpha_top = hue_top + 12.0 + PICKER_GAP;
    click_at(&mut ws, &ctx, egui::pos2(PICKER_W * 0.5, alpha_top + 6.0));
    let faded = label_of(&ws, label).color;
    assert!(faded.a < 255, "the alpha bar reaches the label too");
}

#[test]
fn an_editable_label_takes_the_same_styling() {
    dex_nodes::scripting::init_python();
    let ctx = egui::Context::default();
    let row_h = tick_row_height(&ctx);

    let mut ws = Workspace::new_empty();
    let label = ws.insert_node_now(LabelEditable::new("Hello".to_owned()));
    ws.process_pending();
    inspecting(&mut ws, label.erase());
    frame(&mut ws, &ctx, vec![]);

    click_at(&mut ws, &ctx, tick_centre(row_h, 0));

    let node = ws.get_node(label.erase()).expect("the label is live");
    let styled = (*node)
        .as_any_ref()
        .downcast_ref::<LabelEditable>()
        .expect("it is an editable label")
        .clone();
    assert!(
        styled.font.bold,
        "the shared styling actions reach an editable label too"
    );
}

/// The controls are seeded from the target, so a click reverses what is there
/// rather than always turning styling on.
#[test]
fn the_boxes_start_from_the_label_they_describe() {
    dex_nodes::scripting::init_python();
    let ctx = egui::Context::default();
    let row_h = tick_row_height(&ctx);

    let mut ws = Workspace::new_empty();
    let mut styled = Label::new("Hello".to_owned());
    styled.font.italic = true;
    let label = ws.insert_node_now(styled);
    ws.process_pending();
    inspecting(&mut ws, label.erase());
    frame(&mut ws, &ctx, vec![]);

    click_at(&mut ws, &ctx, tick_centre(row_h, 1));
    assert!(
        !label_of(&ws, label).font.italic,
        "the italic box opened ticked, so one click clears it"
    );
}
