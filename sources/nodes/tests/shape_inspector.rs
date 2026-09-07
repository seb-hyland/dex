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
use dex_nodes::primitives::dropdown::Dropdown;
use dex_nodes::primitives::shapes::{
    FillMode, GetFillMode, HasEndArrow, HasStartArrow, IsPathClosed, IsPathFilled, Path,
};

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

/// Press and release at `pos`, then settle, so the queued action has landed.
fn click_at(ws: &mut Workspace, ctx: &egui::Context, pos: egui::Pos2) {
    frame(ws, ctx, vec![egui::Event::PointerMoved(pos)]);
    frame(ws, ctx, vec![button(pos, true)]);
    frame(ws, ctx, vec![button(pos, false)]);
    frame(ws, ctx, vec![]);
}

/**
    Where the tick box labelled `label` drew, which is where to press it.

    Asked for rather than worked out. Adding up the heights of the rows above a
    control means the test has to know the menu's whole contents, and breaks the
    moment a row is added anywhere above the one it is looking for — which is
    not what any of these tests are about.
*/
fn tick_rect(ws: &Workspace, ctx: &egui::Context, label: &str) -> egui::Rect {
    let boxed = ws
        .live_ids()
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid).is_some_and(|node| {
                (*node)
                    .as_any_ref()
                    .downcast_ref::<Checkbox>()
                    .is_some_and(|tick| tick.label == label)
            })
        })
        .unwrap_or_else(|| panic!("a box labelled {label:?} exists"));
    // A tick box senses through a sensor of its own, which is what egui knows.
    let mut sensor = None;
    ws.get_node(boxed)
        .expect("the box is live")
        .owned_refs(&mut |child| sensor = Some(child));
    ctx.read_response(egui::Id::new(sensor.expect("the box owns a sensor")))
        .unwrap_or_else(|| panic!("the box labelled {label:?} drew this frame"))
        .rect
}

/// Press the box labelled `label`.
fn tick(ws: &mut Workspace, ctx: &egui::Context, label: &str) {
    let at = tick_rect(ws, ctx, label).center();
    click_at(ws, ctx, at);
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
    let editor = PathEditor::build(
        ws.action_handle(),
        child,
        Vector::splat(0.0),
        is_line,
        editable,
    );
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
    let arrows = |ws: &Workspace| {
        (
            ws.send_request(child, HasStartArrow).unwrap_or(false),
            ws.send_request(child, HasEndArrow).unwrap_or(false),
        )
    };
    assert_eq!(arrows(&ws), (false, false), "a line starts with no arrows");

    tick(&mut ws, &ctx, "Start arrow");
    assert_eq!(arrows(&ws), (true, false), "the start arrow is armed");

    tick(&mut ws, &ctx, "End arrow");
    assert_eq!(
        arrows(&ws),
        (true, true),
        "arming the end arrow leaves the start one armed"
    );

    tick(&mut ws, &ctx, "Start arrow");
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

    assert_eq!(ws.send_request(child, HasStartArrow), Some(false));
    tick(&mut ws, &ctx, "Start arrow");
    assert_eq!(
        ws.send_request(child, HasStartArrow),
        Some(true),
        "the polygon's start arrow is armed"
    );

    tick(&mut ws, &ctx, "End arrow");
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

    tick(&mut ws, &ctx, "Closed");
    assert_eq!(
        ws.send_request(child, IsPathClosed),
        Some(false),
        "the box opens the polygon"
    );
    tick(&mut ws, &ctx, "Filled");
    assert_eq!(
        ws.send_request(child, IsPathFilled),
        Some(false),
        "and the next one empties it, without ticking the last one back"
    );
}

/**
    A filled polygon reaches its own colours, gradient and all.

    They live on the path rather than on the editor wrapped round it, and the
    editor used to carry a second, plainer copy of them — so the shapes anyone
    actually draws had no way to reach a gradient at all.
*/
#[test]
fn a_polygon_reaches_the_path_s_own_fill_controls() {
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

    let modes = ws
        .live_ids()
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid)
                .is_some_and(|node| (*node).as_any_ref().is::<Dropdown>())
        })
        .expect("the fill mode is offered");
    let rect = ctx
        .read_response(egui::Id::new(modes))
        .expect("and it drew")
        .rect;
    click_at(&mut ws, &ctx, rect.center());

    let radial = ctx
        .read_response(egui::Id::new(modes).with(("dex_dropdown", FillMode::Radial.index())))
        .expect("the modes are listed")
        .rect;
    click_at(&mut ws, &ctx, radial.center());
    frame(&mut ws, &ctx, vec![]);

    assert_eq!(
        ws.send_request(child, GetFillMode),
        Some(FillMode::Radial),
        "the mode chosen is the one the path takes"
    );
    // And the colour it runs to appears without the menu being reopened. A
    // picker senses its own row, which is what says it is on screen at all.
    let showing = |label: &str| {
        ws.live_ids().into_iter().any(|uid| {
            ws.get_node(uid).is_some_and(|node| {
                (*node)
                    .as_any_ref()
                    .downcast_ref::<ColorPicker>()
                    .is_some_and(|picker| picker.label == label)
            }) && ctx.read_response(egui::Id::new((uid, "row"))).is_some()
        })
    };
    for label in ["Stroke", "Fill", "Fill to"] {
        assert!(showing(label), "`{label}` is on screen");
    }
}
