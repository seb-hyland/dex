//! The sidebar shows one tab at a time.
//!
//! Each tab owns its nodes for the whole session — the prelude editor keeps
//! its text while Prototypes is showing — so "which tab is open" has to decide
//! what *draws*, not what exists.

use std::collections::HashSet;

use dex_core::prelude::*;
use dex_nodes::composites::button::Button;
use dex_nodes::layouts::canvas::sidebar::OpenSidebarTab;
use dex_nodes::layouts::desktops::Desktops;

const SCREEN: egui::Vec2 = egui::vec2(1200.0, 900.0);

fn frame(ws: &mut Workspace, ctx: &egui::Context) {
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
    let input = egui::RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    let _ = ctx.run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
    });
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

/// Whether the button labelled `label` drew in the last frame.
fn drew(ws: &Workspace, ctx: &egui::Context, label: &str) -> bool {
    all_nodes(ws)
        .into_iter()
        .filter(|uid| {
            ws.get_node(*uid).is_some_and(|node| {
                (*node)
                    .as_any_ref()
                    .downcast_ref::<Button>()
                    .is_some_and(|b| b.label.text == label)
            })
        })
        .any(|button| {
            ws.get_node(button)
                .and_then(|node| node.deref_target())
                .and_then(|sensor| ctx.read_response(egui::Id::new(sensor)))
                .is_some()
        })
}

fn open_tab(ws: &mut Workspace, ctx: &egui::Context, tab: usize) {
    let sidebar = all_nodes(ws)
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid).is_some_and(|node| {
                node.type_name(NodeContext {
                    id: *uid,
                    workspace: ws,
                }) == "A Canvas Sidebar"
            })
        })
        .expect("the root builds a sidebar");
    ws.submit_action(
        sidebar.cast::<dex_nodes::layouts::canvas::sidebar::CanvasSidebar>(),
        "open tab",
        OpenSidebarTab { tab },
    );
    // Three: the action applies at the end of the first, and `read_response`
    // still answers for the frame before the one just drawn.
    for _ in 0..3 {
        frame(ws, ctx);
    }
}

/// How opaque a button's fill is, which is how the strip marks the open tab.
fn tab_fill_alpha(ws: &Workspace, label: &str) -> u8 {
    all_nodes(ws)
        .into_iter()
        .find_map(|uid| {
            ws.get_node(uid).and_then(|node| {
                (*node)
                    .as_any_ref()
                    .downcast_ref::<Button>()
                    .filter(|b| b.label.text == label)
                    .map(|b| b.fill_color.a)
            })
        })
        .unwrap_or_else(|| panic!("a button labelled {label:?} exists"))
}

#[test]
fn the_strip_marks_the_open_tab() {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    frame(&mut ws, &ctx);

    assert!(
        tab_fill_alpha(&ws, "Prototypes") > 0,
        "the tab it opens on is filled"
    );
    assert_eq!(tab_fill_alpha(&ws, "Prelude"), 0, "and the rest are plain");

    open_tab(&mut ws, &ctx, 1);
    assert!(tab_fill_alpha(&ws, "Prelude") > 0, "opening a tab fills it");
    assert_eq!(
        tab_fill_alpha(&ws, "Prototypes"),
        0,
        "and unmarks the one it replaced"
    );
}

#[test]
fn only_the_open_tab_draws() {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    frame(&mut ws, &ctx);

    assert!(
        drew(&ws, &ctx, "Text"),
        "Prototypes is the tab the sidebar opens on"
    );
    assert!(
        !drew(&ws, &ctx, "Open in IDE"),
        "and the prelude's chrome stays out of it"
    );

    open_tab(&mut ws, &ctx, 1);
    assert!(
        drew(&ws, &ctx, "Open in IDE"),
        "the Prelude tab brings its editor and IDE button"
    );
    assert!(
        !drew(&ws, &ctx, "Text"),
        "and the prototype buttons stand down"
    );

    // History and Settings hold nothing yet, so neither tab's chrome shows.
    for tab in [2, 3] {
        open_tab(&mut ws, &ctx, tab);
        assert!(
            !drew(&ws, &ctx, "Open in IDE") && !drew(&ws, &ctx, "Text"),
            "tab {tab} is empty for now"
        );
    }

    open_tab(&mut ws, &ctx, 0);
    assert!(
        drew(&ws, &ctx, "Text"),
        "and going back to Prototypes brings the buttons back"
    );
}
