//! Where the inspector menu sits, and that it stays there.
//!
//! The menu opens beside the lens rather than over the node, so you can see
//! what you are editing. Which side is decided from the lens, not from the
//! menu's size: egui re-picks the side every frame from the size, so a menu
//! that grows — opening the colour picker — would flip across the lens, out
//! from under the pointer, and the inspector would read that as the pointer
//! having left and shut the menu just as it was being used.

use dex_core::prelude::*;
use dex_nodes::layouts::canvas::layout::{AddCanvasItem, CanvasChildren, NodeScreenRect};
use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
use dex_nodes::primitives::color_picker::ColorPicker;
use dex_nodes::primitives::text::Label;
use std::collections::HashSet;

/// Short enough that the open picker does not fit below the lens, which is
/// what used to make the menu flip away and close.
const SHORT_SCREEN: egui::Vec2 = egui::vec2(1200.0, 640.0);
/// Tall enough for the closed menu to hang below the lens in full: where the
/// menu was *anchored* is only readable while the screen edge is not pushing
/// it back up.
const TALL_SCREEN: egui::Vec2 = egui::vec2(1200.0, 900.0);
/// How far the lens sits outside the node's top-left corner.
const HANDLE_OFFSET: f32 = 8.0;

fn popup_id() -> egui::Id {
    egui::Id::new("dex_inspector_handle").with("popup")
}

fn frame(ws: &mut Workspace, ctx: &egui::Context, size: egui::Vec2, events: Vec<egui::Event>) {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), size);
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

fn click(ws: &mut Workspace, ctx: &egui::Context, size: egui::Vec2, pos: egui::Pos2) {
    frame(ws, ctx, size, vec![egui::Event::PointerMoved(pos)]);
    frame(ws, ctx, size, vec![button(pos, true)]);
    frame(ws, ctx, size, vec![button(pos, false)]);
    frame(ws, ctx, size, vec![]);
}

/// Every node reachable from the root, so a test can find one by type.
fn all_nodes(ws: &Workspace) -> Vec<NodeUid> {
    let mut seen = HashSet::new();
    let mut queue = vec![ws.root()];
    let mut out = Vec::new();
    while let Some(uid) = queue.pop() {
        if !seen.insert(uid) {
            continue;
        }
        out.push(uid);
        if let Some(node) = ws.get_node(uid) {
            node.owned_refs(&mut |child| queue.push(child));
        }
    }
    out
}

fn find_picker(ws: &Workspace) -> Option<NodeUid> {
    all_nodes(ws).into_iter().find(|uid| {
        ws.get_node(*uid)
            .is_some_and(|node| (*node).as_any_ref().is::<ColorPicker>())
    })
}

/// A canvas holding one label, with that label's inspector menu open, settled,
/// and the pointer resting on the lens.
struct Opened {
    ws: Workspace,
    ctx: egui::Context,
    screen: egui::Vec2,
    item: ScreenRegion,
}

fn open_the_menu(screen: egui::Vec2) -> Opened {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let root = ws.root();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    frame(&mut ws, &ctx, screen, vec![]);

    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(Label::new("Hello".to_owned())),
            size: Vector { x: 200.0, y: 100.0 },
        }),
    });
    ws.process_pending();
    frame(&mut ws, &ctx, screen, vec![]);

    let canvas = ws
        .send_request(root.cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let item_uid = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the item was added");
    let item = ws
        .send_request(
            root,
            NodeScreenRect {
                node: item_uid.erase(),
            },
        )
        .flatten()
        .expect("the item is on screen");

    // Hover the item, then take the lens position the inspector reports.
    let centre = egui::pos2(
        (item.min.x + item.max.x) * 0.5,
        (item.min.y + item.max.y) * 0.5,
    );
    frame(
        &mut ws,
        &ctx,
        screen,
        vec![egui::Event::PointerMoved(centre)],
    );
    let found = ws.inspect_target().expect("the item is under the pointer");
    let lens = egui::pos2(found.region.min.x - 15.0, found.region.min.y + 11.0);

    // A synthetic click does not land on the lens in this harness, so the menu
    // is opened the way the lens would open it, then left to settle: an egui
    // area only learns its size by being drawn.
    frame(&mut ws, &ctx, screen, vec![egui::Event::PointerMoved(lens)]);
    egui::Popup::open_id(&ctx, popup_id());
    for _ in 0..8 {
        frame(&mut ws, &ctx, screen, vec![egui::Event::PointerMoved(lens)]);
    }
    assert!(
        egui::Popup::is_id_open(&ctx, popup_id()),
        "the menu opened; the rest of the test means nothing otherwise"
    );

    Opened {
        ws,
        ctx,
        screen,
        item,
    }
}

fn menu_rect(ctx: &egui::Context) -> egui::Rect {
    ctx.read_response(popup_id())
        .expect("the menu drew this frame")
        .rect
}

#[test]
fn the_menu_opens_beside_the_node_not_over_it() {
    let open = open_the_menu(TALL_SCREEN);
    let menu = menu_rect(&open.ctx);

    assert!(
        menu.max.x <= open.item.min.x - HANDLE_OFFSET + 0.5,
        "the menu sits left of the lens, clear of the node it edits: \
         menu right {} vs node left {}",
        menu.max.x,
        open.item.min.x
    );
    assert!(
        menu.min.x >= 0.0,
        "and still on screen: menu left {}",
        menu.min.x
    );
    // The lens is at the node's top-left, so the menu hangs from there.
    assert!(
        (menu.min.y - (open.item.min.y - HANDLE_OFFSET)).abs() < 1.0,
        "the menu's top lines up with the lens: menu top {} vs lens top {}",
        menu.min.y,
        open.item.min.y - HANDLE_OFFSET
    );
}

#[test]
fn opening_the_picker_keeps_the_menu_up() {
    let Opened {
        mut ws,
        ctx,
        screen,
        ..
    } = open_the_menu(SHORT_SCREEN);
    let before = menu_rect(&ctx);

    let picker = find_picker(&ws).expect("the label's inspector built a picker");
    let swatch = ctx
        .read_response(egui::Id::new((picker, "row")))
        .expect("the picker's swatch row drew")
        .rect;
    assert!(
        before.contains_rect(swatch),
        "the swatch is inside the menu, so the click below is a fair test"
    );

    click(&mut ws, &ctx, screen, swatch.center());
    // Let the taller menu settle wherever it is going to sit.
    for _ in 0..4 {
        frame(
            &mut ws,
            &ctx,
            screen,
            vec![egui::Event::PointerMoved(swatch.center())],
        );
    }

    let expanded = ws
        .get_node(picker)
        .and_then(|node| (*node).as_any_ref().downcast_ref::<ColorPicker>().cloned())
        .map(|picker| picker.expanded);
    assert_eq!(expanded, Some(true), "the swatch click opened the picker");

    assert!(
        egui::Popup::is_id_open(&ctx, popup_id()),
        "and the menu it grew is still up"
    );
    let after = menu_rect(&ctx);
    assert!(
        after.height() > before.height(),
        "the menu really did grow: {} then {}",
        before.height(),
        after.height()
    );
    assert!(
        (after.max.x - before.max.x).abs() < 1.0,
        "and it grew without changing sides: right edge {} then {}",
        before.max.x,
        after.max.x
    );
    // The pointer must still be over the menu, or the next frame closes it.
    assert!(
        after.contains(swatch.center()),
        "the pointer is still inside the menu it opened"
    );
}
