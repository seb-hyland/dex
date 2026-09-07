//! Choosing a boolean, and what one is worth to a script.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{AddCanvasItem, CanvasChildren, NodeScreenRect};
use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
use dex_nodes::primitives::boolean::{Bool, GetBool};
use dex_nodes::primitives::text::GetText;
use dex_nodes::scripting::{ScriptValue, node_to_value};

const SCREEN: egui::Vec2 = egui::vec2(1200.0, 900.0);
/// The control's own size on the canvas, matching the sidebar's prototype.
const SIZE: Vector = Vector { x: 90.0, y: 28.0 };

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

fn button_event(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

fn click(ws: &mut Workspace, ctx: &egui::Context, pos: egui::Pos2) {
    frame(ws, ctx, vec![egui::Event::PointerMoved(pos)]);
    frame(ws, ctx, vec![button_event(pos, true)]);
    frame(ws, ctx, vec![button_event(pos, false)]);
    frame(ws, ctx, vec![]);
}

/// A workspace with one boolean on the desktop, drawn and settled.
struct Placed {
    ws: Workspace,
    ctx: egui::Context,
    /// The `Bool` itself, not the item framing it.
    boolean: NodeUid,
    /// Where that item drew, in screen coordinates.
    rect: egui::Rect,
}

fn place_a_boolean(start: bool) -> Placed {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    frame(&mut ws, &ctx, vec![]);

    let canvas = ws
        .send_request(ws.root().cast::<Desktops>(), ActiveCanvas)
        .expect("a desktop is showing");
    ws.submit_action(
        canvas,
        "place a boolean",
        AddCanvasItem {
            child: Arc::new(Bool::new(start)),
            size: SIZE,
        },
    );
    ws.process_pending();
    frame(&mut ws, &ctx, vec![]);

    let item = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .last()
        .expect("the boolean landed on the surface");
    let rect: egui::Rect = ws
        .send_request(canvas, NodeScreenRect { node: item })
        .flatten()
        .expect("the item knows where it drew")
        .into();
    let boolean = ws
        .live_ids()
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid)
                .is_some_and(|node| (*node).as_any_ref().is::<Bool>())
        })
        .expect("the boolean is live");

    Placed {
        ws,
        ctx,
        boolean,
        rect,
    }
}

/// The list only opens when it is asked to: a control that showed both values
/// all the time would be twice its own height on every canvas it sat on.
#[test]
fn the_list_stays_shut_until_the_row_is_pressed() {
    let mut placed = place_a_boolean(true);
    // Where the second row would be, were the list open.
    let below = placed.rect.center() + egui::vec2(0.0, placed.rect.height());
    click(&mut placed.ws, &placed.ctx, below);

    assert_eq!(
        placed.ws.send_request(placed.boolean, GetBool),
        Some(true),
        "a press below a shut control changes nothing"
    );
}

/// Opening the list and pressing the other value takes it.
#[test]
fn choosing_the_other_value_takes_it() {
    let mut placed = place_a_boolean(true);
    let header = placed.rect.center();
    click(&mut placed.ws, &placed.ctx, header);

    // The rows hang under the header, one control-height each: "True" then
    // "False", so the second one is two heights below the header's middle.
    let false_row = header + egui::vec2(0.0, placed.rect.height() * 1.5);
    click(&mut placed.ws, &placed.ctx, false_row);

    assert_eq!(
        placed.ws.send_request(placed.boolean, GetBool),
        Some(false),
        "the value chosen from the list is the one it holds"
    );
    // And the list is shut again, so the next press lands on the header.
    click(&mut placed.ws, &placed.ctx, false_row);
    assert_eq!(
        placed.ws.send_request(placed.boolean, GetBool),
        Some(false),
        "a press where the list used to be does not reopen and re-choose it"
    );
}

/// A boolean crosses into a script as a boolean, not as the word for one.
#[test]
fn a_boolean_is_worth_a_boolean() {
    let node = Bool::new(false);
    assert!(
        matches!(node_to_value(&node), Some(ScriptValue::Bool(false))),
        "a script sees the value, not its rendering"
    );

    let mut ws = Workspace::new_with_root(Bool::new(true));
    ws.process_pending();
    assert_eq!(
        ws.send_request(ws.root(), GetText).as_deref(),
        Some("True"),
        "and a text-shaped sink still gets a word"
    );
}
