//! Keeping a node in the sidebar's backpack, and getting it back out.
//!
//! The two ways in behave differently once out again: a copy is detached the
//! moment it is kept, so the original may change or go away; a mirror keeps
//! pointing at what it was taken from. Both are stamped, not moved — clicking
//! an entry places another one and leaves the entry where it is.

use std::collections::HashSet;

use dex_core::prelude::*;
use dex_nodes::composites::button::Button;
use dex_nodes::layouts::Mirror;
use dex_nodes::layouts::canvas::backpack::BackpackItem;
use dex_nodes::layouts::canvas::layout::{AddCanvasItem, CanvasChildren, NodeScreenRect};
use dex_nodes::layouts::canvas::nodes::CanvasNodeChild;
use dex_nodes::layouts::canvas::nodes::editors::PathEditor;
use dex_nodes::layouts::canvas::sidebar::BackpackList;
use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
use dex_nodes::layouts::horizontal_dnd::{ChildCount, Children};
use dex_nodes::primitives::shapes::Path;
use dex_nodes::primitives::text::{IsInteractive, Label};

const SCREEN: egui::Vec2 = egui::vec2(1200.0, 900.0);

fn popup_id() -> egui::Id {
    egui::Id::new("dex_halo_handle").with("popup")
}

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

/// The on-screen rect of the button labelled `label`, which must be drawing.
fn button_rect(ws: &Workspace, ctx: &egui::Context, label: &str) -> egui::Rect {
    let button = all_nodes(ws)
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid).is_some_and(|node| {
                (*node)
                    .as_any_ref()
                    .downcast_ref::<Button>()
                    .is_some_and(|b| b.label.text == label)
            })
        })
        .unwrap_or_else(|| panic!("a button labelled {label:?} exists"));
    // A button's polling falls through to its sensor, which is what egui knows.
    let sensor = ws
        .get_node(button)
        .and_then(|node| node.deref_target())
        .expect("the button owns a click sensor");
    ctx.read_response(egui::Id::new(sensor))
        .unwrap_or_else(|| panic!("the button labelled {label:?} drew this frame"))
        .rect
}

/// A canvas holding one label, with that item's inspector menu open and settled.
struct Opened {
    ws: Workspace,
    ctx: egui::Context,
    /// The canvas item wrapping the label.
    item: NodeUid,
}

fn open_the_menu() -> Opened {
    open_the_menu_on(
        Arc::new(Label::new("Hello".to_owned())),
        Vector { x: 200.0, y: 100.0 },
    )
}

fn open_the_menu_on(child: Arc<dyn Node>, size: Vector) -> Opened {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let root = ws.root();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    frame(&mut ws, &ctx, vec![]);

    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem { child, size }),
    });
    ws.process_pending();
    frame(&mut ws, &ctx, vec![]);

    let canvas = ws
        .send_request(root.cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let item = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the item was added");
    let region = ws
        .send_request(root, NodeScreenRect { node: item.erase() })
        .flatten()
        .expect("the item is on screen");

    // Hover the item, then take the lens position the inspector reports.
    let centre = egui::pos2(
        (region.min.x + region.max.x) * 0.5,
        (region.min.y + region.max.y) * 0.5,
    );
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(centre)]);
    let found = ws.inspect_target().expect("the item is under the pointer");
    let lens = egui::pos2(found.region.min.x - 15.0, found.region.min.y + 11.0);

    // A synthetic click does not land on the lens in this harness, so the menu
    // is opened the way the lens would open it, then left to settle.
    frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(lens)]);
    egui::Popup::open_id(&ctx, popup_id());
    for _ in 0..8 {
        frame(&mut ws, &ctx, vec![egui::Event::PointerMoved(lens)]);
    }
    assert!(
        egui::Popup::is_id_open(&ctx, popup_id()),
        "the menu opened; the rest of the test means nothing otherwise"
    );

    Opened {
        ws,
        ctx,
        item: item.erase(),
    }
}

/// Click a command in the open inspector menu, then let the workspace settle.
fn run_command(open: &mut Opened, label: &str) {
    let rect = button_rect(&open.ws, &open.ctx, label);
    click(&mut open.ws, &open.ctx, rect.center());
    for _ in 0..3 {
        frame(&mut open.ws, &open.ctx, vec![]);
    }
}

/// The click target of a backpack row, which must be drawing.
fn row_rect(ws: &Workspace, ctx: &egui::Context, entry: NodeUid) -> egui::Rect {
    let sensor = ws
        .get_node(entry)
        .and_then(|node| node.deref_target())
        .expect("the entry owns a click sensor");
    ctx.read_response(egui::Id::new(sensor))
        .expect("the entry drew in the sidebar")
        .rect
}

/// A row's editable name.
fn name_of(ws: &Workspace, entry: NodeUid) -> NodeUid {
    let mut owned = Vec::new();
    ws.get_node(entry)
        .expect("the entry is live")
        .owned_refs(&mut |child| owned.push(child));
    owned
        .into_iter()
        .find(|uid| ws.send_request(*uid, IsInteractive).is_some())
        .expect("the entry owns an editable name")
}

/// Run frames until a click has outlived the window a double-click could arrive in.
fn wait_out_the_double_click(ws: &mut Workspace, ctx: &egui::Context) {
    // The harness advances time by one predicted frame each pass, so this is
    // ~0.3s of egui time.
    for _ in 0..25 {
        frame(ws, ctx, vec![]);
    }
}

/// The entries the sidebar's backpack holds, in order.
fn entries(ws: &Workspace) -> Vec<NodeUid> {
    ws.send_request(backpack_list(ws), Children)
        .unwrap_or_default()
}

/// The sidebar's list of kept entries.
fn backpack_list(ws: &Workspace) -> NodeUid<dex_nodes::layouts::VerticalDnD> {
    all_nodes(ws)
        .into_iter()
        .find_map(|uid| ws.send_request(uid, BackpackList))
        .expect("the sidebar has a backpack")
}

#[test]
fn a_copy_kept_in_the_backpack_stamps_out_placements() {
    let mut open = open_the_menu();
    run_command(&mut open, "Copy to Backpack");

    let kept = entries(&open.ws);
    assert_eq!(kept.len(), 1, "the command kept exactly one entry");
    let entry = kept[0];
    assert_eq!(
        open.ws
            .get_node(entry)
            .map(|node| node.type_name(NodeContext {
                id: entry,
                workspace: &open.ws
            })),
        Some("A Backpack Template".to_owned()),
        "a copy is kept as a template, not as a mirror"
    );

    let canvas = open
        .ws
        .send_request(open.ws.root().cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let before = open
        .ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .len();

    // The row is a click target in the sidebar, like the primitive buttons.
    let rect = row_rect(&open.ws, &open.ctx, entry);
    click(&mut open.ws, &open.ctx, rect.center());
    wait_out_the_double_click(&mut open.ws, &open.ctx);

    let after = open
        .ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default();
    assert_eq!(
        after.len(),
        before + 1,
        "clicking the entry put one on the canvas"
    );
    assert_eq!(
        entries(&open.ws).len(),
        1,
        "and the entry stayed in the backpack, ready to stamp another"
    );
}

#[test]
fn a_mirror_kept_in_the_backpack_places_mirrors_of_the_original() {
    let mut open = open_the_menu();
    let content = open
        .ws
        .send_request(open.item, CanvasNodeChild)
        .expect("the canvas item wraps a label");

    run_command(&mut open, "Mirror to Backpack");

    let kept = entries(&open.ws);
    assert_eq!(kept.len(), 1, "the command kept exactly one entry");
    let entry = kept[0];

    let canvas = open
        .ws
        .send_request(open.ws.root().cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let before: HashSet<NodeUid> = open
        .ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .into_iter()
        .collect();

    open.ws.submit_action(
        entry.cast::<BackpackItem>(),
        "place",
        dex_nodes::layouts::canvas::backpack::PlaceFromBackpack,
    );
    for _ in 0..3 {
        frame(&mut open.ws, &open.ctx, vec![]);
    }

    let placed = open
        .ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .into_iter()
        .find(|uid| !before.contains(uid))
        .expect("a new item landed on the canvas");
    let mirrored = open
        .ws
        .send_request(placed, CanvasNodeChild)
        .and_then(|child| open.ws.get_node(child))
        .and_then(|node| {
            (*node)
                .as_any_ref()
                .downcast_ref::<Mirror>()
                .map(|m| m.target())
        });
    assert_eq!(
        mirrored,
        Some(content),
        "what was placed mirrors the node the entry was taken from"
    );
}

#[test]
fn the_backpack_starts_empty() {
    dex_nodes::scripting::init_python();
    let ws = Desktops::new_workspace();
    assert_eq!(
        ws.send_request(backpack_list(&ws), ChildCount),
        Some(0),
        "nothing is kept until the user keeps something"
    );
}

#[test]
fn double_clicking_a_row_renames_it_instead_of_placing() {
    let mut open = open_the_menu();
    run_command(&mut open, "Copy to Backpack");
    let entry = entries(&open.ws)[0];

    let canvas = open
        .ws
        .send_request(open.ws.root().cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let before = open
        .ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .len();

    let centre = row_rect(&open.ws, &open.ctx, entry).center();
    click(&mut open.ws, &open.ctx, centre);
    click(&mut open.ws, &open.ctx, centre);
    wait_out_the_double_click(&mut open.ws, &open.ctx);

    let editing = open
        .ws
        .send_request(name_of(&open.ws, entry), IsInteractive)
        .expect("the name answers whether it is being edited");
    assert!(editing, "the second click opened the name for editing");
    assert_eq!(
        open.ws
            .send_request(canvas, CanvasChildren)
            .unwrap_or_default()
            .len(),
        before,
        "and the click it was made of placed nothing"
    );
}

/// A path wraps itself in its own editor rather than sitting in a plain canvas
/// frame, so it has to offer the placement commands itself — without them a
/// line or a polygon was the one thing on the canvas that could not be copied.
#[test]
fn a_polygon_offers_the_same_commands_and_comes_back_as_an_editor() {
    let polygon = Path::polygon(
        vec![
            Vector::new(0.0, 0.0),
            Vector::new(90.0, 0.0),
            Vector::new(90.0, 90.0),
        ],
        Path::default_fill(),
        Stroke::new(2.0, Color::BLACK),
    );
    let mut open = open_the_menu_on(Arc::new(polygon), Vector { x: 90.0, y: 90.0 });
    assert!(
        open.ws
            .get_node(open.item)
            .is_some_and(|node| (*node).as_any_ref().is::<PathEditor>()),
        "the polygon is held by its own editor, not a canvas frame"
    );

    // Panics if the command is missing, which is the whole point of the test.
    run_command(&mut open, "Copy to Backpack");
    let entry = entries(&open.ws)[0];

    let canvas = open
        .ws
        .send_request(open.ws.root().cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let before: HashSet<NodeUid> = open
        .ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .into_iter()
        .collect();

    open.ws.submit_action(
        entry.cast::<BackpackItem>(),
        "place",
        dex_nodes::layouts::canvas::backpack::PlaceFromBackpack,
    );
    for _ in 0..3 {
        frame(&mut open.ws, &open.ctx, vec![]);
    }

    let placed = open
        .ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .into_iter()
        .find(|uid| !before.contains(uid))
        .expect("a new item landed on the canvas");
    assert!(
        open.ws
            .get_node(placed)
            .is_some_and(|node| (*node).as_any_ref().is::<PathEditor>()),
        "and it came back editable, not wrapped in a plain frame"
    );
}
