//! A path's inspector edits the path it was built for.
//!
//! Its controls are polled against the shape rather than handing over one-shot
//! toggles, so what reaches the path is the state the boxes are showing — and
//! ticking one box never writes back a stale value for its neighbour. These
//! drive real clicks through real frames to check that.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::nodes::editors::PathEditor;
use dex_nodes::primitives::checkbox::Checkbox;
use dex_nodes::primitives::color_picker::ColorPicker;
use dex_nodes::primitives::shapes::{
    HasEndArrow, HasStartArrow, IsPathClosed, IsPathFilled, Path,
};

const SCREEN: egui::Vec2 = egui::vec2(1200.0, 900.0);
/// The spacing `PathEditorMenu` stacks its controls with.
const ROW_GAP: f32 = 3.0;

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

/// The height one control takes drawn on its own, which sets where the rows
/// below it in the column start.
fn row_height(ctx: &egui::Context, build: impl FnOnce(&Workspace) -> NodeUid) -> f32 {
    let mut ws = Workspace::new_empty();
    let uid = build(&ws);
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
                    id: uid,
                    workspace: &ws,
                },
                constraints,
                &mut ui,
            );
            height = draw
                .draw_workspace_node(uid, constraints)
                .and_then(|r| r.region())
                .map(|r| r.size().y)
                .expect("the control draws");
        });
    });
    assert!(height > 0.0, "a control has a height");
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
        .expect("a path editor offers an inspector");
    ws.process_pending();
    ws.set_root(inspector);
}

/// A workspace whose root is the inspector for a path, and the path itself.
fn inspecting_path(ctx: &egui::Context, path: Path, is_line: bool) -> (Workspace, NodeUid) {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let path_closed = path.closed;
    let child = ws.insert_node_now(path).erase();
    // Closed shapes start with point editing off, as the canvas builds them.
    let editable = !path_closed;
    let editor =
        PathEditor::build(ws.action_handle(), child, Vector::splat(0.0), is_line, editable);
    ws.process_pending();
    inspecting(&mut ws, editor.erase());
    // A frame first: egui only reports a click on a rect it already knows.
    frame(&mut ws, ctx, vec![]);
    (ws, child)
}

fn line() -> Path {
    Path::polyline(
        vec![Vector::new(0.0, 0.0), Vector::new(140.0, 60.0)],
        Stroke::new(2.5, Color::BLACK),
    )
}

/// Ticking one box must not write back its neighbour's stale value. Reading a
/// box's committed state straight after a click still saw the old value, so
/// arming one arrow disarmed the other.
#[test]
fn each_arrow_box_arms_its_own_end() {
    let ctx = egui::Context::default();
    let (mut ws, child) = inspecting_path(&ctx, line(), true);
    let tick = row_height(&ctx, |ws| {
        Checkbox::build(ws.action_handle(), "Start arrow".to_owned(), false).erase()
    });
    let row = |i: usize| egui::pos2(6.0, i as f32 * (tick + ROW_GAP) + tick * 0.5);

    let arrows = |ws: &Workspace| {
        (
            ws.send_request(child, HasStartArrow).unwrap_or(false),
            ws.send_request(child, HasEndArrow).unwrap_or(false),
        )
    };
    assert_eq!(arrows(&ws), (false, false), "a line starts with no arrows");

    click_at(&mut ws, &ctx, row(0));
    assert_eq!(arrows(&ws), (true, false), "the first row is the start arrow");

    click_at(&mut ws, &ctx, row(1));
    assert_eq!(
        arrows(&ws),
        (true, true),
        "arming the end arrow leaves the start one armed"
    );

    click_at(&mut ws, &ctx, row(0));
    assert_eq!(
        arrows(&ws),
        (false, true),
        "and disarming one leaves the other alone"
    );
}

/// A polygon is offered the same arrows, below its own controls — they only
/// draw once it is opened, but it should not have to be reopened to arm them.
#[test]
fn a_polygon_is_offered_arrows_too() {
    let ctx = egui::Context::default();
    let polygon = Path::polygon(
        vec![
            Vector::new(0.0, 0.0),
            Vector::new(90.0, 0.0),
            Vector::new(90.0, 90.0),
        ],
        Path::default_fill(),
        Stroke::new(2.0, Color::BLACK),
    );
    let (mut ws, child) = inspecting_path(&ctx, polygon, false);

    let tick = row_height(&ctx, |ws| {
        Checkbox::build(ws.action_handle(), "Edit points".to_owned(), false).erase()
    });
    let picker = row_height(&ctx, |ws| {
        ColorPicker::build(ws.action_handle(), "Fill".to_owned(), Color::WHITE).erase()
    });

    // Edit points, Closed, Filled, Fill, Border, Start arrow, End arrow, Delete.
    let ticks_above = 3.0 * (tick + ROW_GAP);
    let pickers_above = 2.0 * (picker + ROW_GAP);
    let arrow_row =
        |i: f32| egui::pos2(6.0, ticks_above + pickers_above + i * (tick + ROW_GAP) + tick * 0.5);

    assert_eq!(ws.send_request(child, HasStartArrow), Some(false));
    click_at(&mut ws, &ctx, arrow_row(0.0));
    assert_eq!(
        ws.send_request(child, HasStartArrow),
        Some(true),
        "the polygon's start arrow is armed"
    );

    click_at(&mut ws, &ctx, arrow_row(1.0));
    assert_eq!(
        ws.send_request(child, HasEndArrow),
        Some(true),
        "and its end arrow too"
    );

    // Its own controls are untouched by the rows added below them.
    assert_eq!(ws.send_request(child, IsPathClosed), Some(true));
    assert_eq!(ws.send_request(child, IsPathFilled), Some(true));
}

/// The polygon's own boxes still drive the shape, polled the same way.
#[test]
fn the_polygon_boxes_open_and_unfill_it() {
    let ctx = egui::Context::default();
    let polygon = Path::polygon(
        vec![
            Vector::new(0.0, 0.0),
            Vector::new(90.0, 0.0),
            Vector::new(90.0, 90.0),
        ],
        Path::default_fill(),
        Stroke::new(2.0, Color::BLACK),
    );
    let (mut ws, child) = inspecting_path(&ctx, polygon, false);
    let tick = row_height(&ctx, |ws| {
        Checkbox::build(ws.action_handle(), "Edit points".to_owned(), false).erase()
    });
    let row = |i: usize| egui::pos2(6.0, i as f32 * (tick + ROW_GAP) + tick * 0.5);

    click_at(&mut ws, &ctx, row(1));
    assert_eq!(
        ws.send_request(child, IsPathClosed),
        Some(false),
        "the second row opens the polygon"
    );
    click_at(&mut ws, &ctx, row(2));
    assert_eq!(
        ws.send_request(child, IsPathFilled),
        Some(false),
        "the third row empties it, and opening it did not tick back"
    );
}
