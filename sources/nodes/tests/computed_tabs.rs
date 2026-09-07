//! A surface sitting on a canvas can be lifted onto a desktop of its own.

use std::collections::HashSet;

use dex_core::prelude::*;
use dex_nodes::composites::button::Button;
use dex_nodes::layouts::canvas::layout::{AddCanvasItem, Canvas, CanvasChildren, PlaceOnCanvas};
use dex_nodes::layouts::canvas::nodes::CanvasNodeChild;
use dex_nodes::layouts::desktops::{
    ActiveCanvas, DesktopTabView, Desktops, OpenCanvasAsTab, TabCanvas, TabName, Tabs,
};
use dex_nodes::layouts::inspector::PlacementCommands;
use dex_nodes::primitives::text::Label;

fn workspace() -> Workspace {
    dex_nodes::scripting::init_python();
    Desktops::new_workspace()
}

/// The label of every button `commands` owns, sorted.
fn command_labels(ws: &Workspace, commands: NodeUid<PlacementCommands>) -> Vec<String> {
    let mut labels = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = vec![commands.erase()];
    while let Some(uid) = queue.pop() {
        if !seen.insert(uid) {
            continue;
        }
        let Some(node) = ws.get_node(uid) else {
            continue;
        };
        if let Some(button) = (*node).as_any_ref().downcast_ref::<Button>() {
            labels.push(button.label.text.clone());
            continue;
        }
        node.owned_refs(&mut |child| queue.push(child));
    }
    labels.sort();
    labels
}

/// Put `child` on the active desktop and give back the item framing it.
fn place(ws: &mut Workspace, child: Arc<dyn Node>) -> NodeUid {
    let canvas = ws
        .send_request(ws.root().cast::<Desktops>(), ActiveCanvas)
        .expect("the root has a desktop showing");
    let before: HashSet<NodeUid> = ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .into_iter()
        .collect();
    ws.submit_action(
        canvas,
        "place",
        AddCanvasItem {
            child,
            size: Vector { x: 200.0, y: 150.0 },
        },
    );
    ws.process_pending();
    ws.send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .into_iter()
        .find(|item| !before.contains(item))
        .expect("the item landed on the surface")
}

/// Only an item framing a surface is offered a desktop of its own.
#[test]
fn a_desktop_is_offered_for_a_surface_and_nothing_else() {
    let mut ws = workspace();
    let plain = place(&mut ws, Arc::new(Label::new("Hello".to_owned())));
    let surface = {
        let canvas = Canvas::build(ws.action_handle());
        ws.process_pending();
        // Placed by uid, so the item frames this very canvas rather than a copy.
        let desktop = ws
            .send_request(ws.root().cast::<Desktops>(), ActiveCanvas)
            .expect("a desktop is showing");
        let before: HashSet<NodeUid> = ws
            .send_request(desktop, CanvasChildren)
            .unwrap_or_default()
            .into_iter()
            .collect();
        ws.submit_action(
            desktop,
            "place a surface",
            PlaceOnCanvas {
                node: canvas.erase(),
                size: Vector { x: 300.0, y: 200.0 },
            },
        );
        ws.process_pending();
        ws.send_request(desktop, CanvasChildren)
            .unwrap_or_default()
            .into_iter()
            .find(|item| !before.contains(item))
            .expect("the surface landed as an item")
    };

    let size = Vector { x: 200.0, y: 150.0 };
    let for_label = PlacementCommands::build_for_canvas_item(&ws, plain, size);
    let for_surface = PlacementCommands::build_for_canvas_item(&ws, surface, size);
    ws.process_pending();

    assert!(
        !command_labels(&ws, for_label).contains(&"New desktop".to_owned()),
        "a label is not a surface, so it cannot become one"
    );
    assert!(
        command_labels(&ws, for_surface).contains(&"New desktop".to_owned()),
        "an item framing a surface can be lifted onto a desktop"
    );
}

/**
    A surface a node *computed* is offered a desktop too.

    A script that builds a map has built a desktop, and the only thing between
    the two is being asked. The result of a lambda is not a canvas item — it has
    no frame around it and nothing to restack it among — so it took a second
    path to the same question.
*/
#[test]
fn a_computed_surface_is_offered_a_desktop() {
    let mut ws = workspace();
    let size = Vector { x: 200.0, y: 150.0 };

    // What a transform returning a canvas leaves behind: the surface itself,
    // with no item wrapped round it.
    let computed = Canvas::build(ws.action_handle());
    let plain = ws
        .action_handle()
        .insert_node(Label::new("a number".to_owned()));
    ws.process_pending();

    let for_canvas = PlacementCommands::for_result(&ws, computed.erase(), size);
    let for_value = PlacementCommands::for_result(&ws, plain.erase(), size);
    ws.process_pending();

    assert!(
        command_labels(&ws, for_canvas).contains(&"New desktop".to_owned()),
        "a computed surface can be lifted onto a desktop"
    );
    assert!(
        !command_labels(&ws, for_value).contains(&"New desktop".to_owned()),
        "a computed label cannot: it is not a surface"
    );
    // And neither is offered somewhere to be restacked, having no place in a
    // draw order to move within.
    assert!(
        !command_labels(&ws, for_canvas).contains(&"Bring to Front".to_owned()),
        "a result has no draw order to move within"
    );
}

/// A surface handed to the root gets a tab, and that tab is the one showing.
#[test]
fn a_surface_opened_as_a_tab_takes_the_screen() {
    let mut ws = workspace();
    let root = ws.root().cast::<Desktops>();
    let before = ws.send_request(root, Tabs).unwrap_or_default().len();

    let canvas = Canvas::build(ws.action_handle());
    ws.submit_action(
        root,
        "open it as a tab",
        OpenCanvasAsTab {
            canvas,
            name: "Lifted copy".to_owned(),
        },
    );
    ws.process_pending();

    let tabs = ws.send_request(root, Tabs).unwrap_or_default();
    assert_eq!(tabs.len(), before + 1, "one tab was added");
    let opened = *tabs.last().expect("the new tab");
    assert_eq!(
        ws.send_request(opened.cast::<DesktopTabView>(), TabCanvas),
        Some(canvas),
        "the tab shows the surface it was handed"
    );
    assert_eq!(
        ws.send_request(opened.cast::<DesktopTabView>(), TabName)
            .as_deref(),
        Some("Lifted copy"),
        "and wears the name it came with"
    );
    assert_eq!(
        ws.send_request(root, ActiveCanvas),
        Some(canvas),
        "opening a desktop goes there"
    );
}

/// The clone that reaches a new desktop is of the surface, not of the frame
/// around it: a desktop showing a canvas item would show the grips.
#[test]
fn the_surface_is_cloned_rather_than_the_item_framing_it() {
    let mut ws = workspace();
    let canvas = Canvas::build(ws.action_handle());
    ws.process_pending();
    let desktop = ws
        .send_request(ws.root().cast::<Desktops>(), ActiveCanvas)
        .expect("a desktop is showing");
    ws.submit_action(
        desktop,
        "place a surface",
        PlaceOnCanvas {
            node: canvas.erase(),
            size: Vector { x: 300.0, y: 200.0 },
        },
    );
    ws.process_pending();
    let item = *ws
        .send_request(desktop, CanvasChildren)
        .unwrap_or_default()
        .last()
        .expect("the item is on the surface");

    assert_eq!(
        ws.send_request(item, CanvasNodeChild),
        Some(canvas.erase()),
        "the item frames the surface it was given"
    );
    let copy = ws.deep_clone(canvas.erase());
    ws.submit_action(
        ws.root().cast::<Desktops>(),
        "lift it",
        OpenCanvasAsTab {
            canvas: copy.cast(),
            name: "A Canvas copy".to_owned(),
        },
    );
    ws.process_pending();

    let shown = ws
        .send_request(ws.root().cast::<Desktops>(), ActiveCanvas)
        .expect("the copy is showing");
    assert_ne!(shown, canvas, "a copy, not the original");
    assert!(
        ws.get_node(shown.erase())
            .is_some_and(|node| (*node).as_any_ref().is::<Canvas>()),
        "and it is a surface, not the frame that was around it"
    );
}
