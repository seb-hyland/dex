//! The Controls tab, the folding panels, and the two ways into a canvas.
//!
//! Saving is the one with teeth. A workspace file carries the whole registry —
//! every node, and every node's history — so a load has to replace the world
//! from inside it: the sidebar that asked is one of the nodes being swapped
//! out. That is why the swap is a workspace-level action rather than one
//! addressed to a node, and why the queue is drained when it lands.

use dex_core::prelude::*;
use dex_nodes::composites::button::Button;
use dex_nodes::layouts::canvas::layout::{AddCanvasItem, Canvas, CanvasChildren};
use dex_nodes::layouts::canvas::sidebar::{CanvasSidebar, OpenSidebarTab, SetSaveDir};
use dex_nodes::layouts::desktops::{
    ActiveCanvas, AddCanvas, DesktopTabView, Desktops, PushOverride, StepTab, TabCanvas, Tabs,
    ToggleSidebar, ToggleTabBar,
};
use dex_nodes::primitives::text::{Label, LabelEditable, SetText};

const SCREEN: egui::Vec2 = egui::vec2(1200.0, 800.0);

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("dex-controls-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

fn owned_tree(ws: &Workspace, root: NodeUid) -> Vec<NodeUid> {
    let mut seen = std::collections::HashSet::new();
    let mut queue = vec![root];
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

/// The label of every button under `root`.
fn button_labels(ws: &Workspace, root: NodeUid) -> Vec<String> {
    owned_tree(ws, root)
        .into_iter()
        .filter_map(|uid| {
            let node = ws.get_node(uid)?;
            let button = (*node).as_any_ref().downcast_ref::<Button>()?;
            Some(button.label.text.clone())
        })
        .collect()
}

struct App {
    ws: Workspace,
    ctx: egui::Context,
}

impl App {
    fn new() -> App {
        dex_nodes::scripting::init_python();
        let ws = Desktops::new_workspace();
        let ctx = egui::Context::default();
        dex_nodes::fonts::install_fonts(&ctx);
        let mut app = App { ws, ctx };
        app.frame(vec![]);
        app
    }

    fn root(&self) -> NodeUid<Desktops> {
        self.ws.root().cast()
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let input = egui::RawInput {
            screen_rect: Some(rect),
            events,
            ..Default::default()
        };
        let ws = &mut self.ws;
        let out = self.ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                ws.draw_frame(ui, rect);
            });
        });
        self.ws.tick_all();
        self.ws.process_pending();
        out
    }

    /// Press and release at `pos`, then settle.
    fn click(&mut self, pos: egui::Pos2) {
        // Twice: a control that has only just drawn under the pointer needs a
        // frame to register as hovered before a press counts as a click.
        for _ in 0..2 {
            self.frame(vec![egui::Event::PointerMoved(pos)]);
        }
        for pressed in [true, false] {
            self.frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }]);
        }
        self.frame(vec![egui::Event::PointerMoved(pos)]);
    }

    /// Whether `text` was painted this frame.
    ///
    /// A folded panel is not a narrow one: the content slides over to fill the
    /// space, so position proves nothing and what was drawn proves everything.
    fn shows_text(&mut self, text: &str) -> bool {
        let output = self.frame(vec![]);
        output.shapes.iter().any(|c| match &c.shape {
            egui::Shape::Text(t) => t.galley.text().contains(text),
            _ => false,
        })
    }
}

/// The tab is called Controls, and carries the environment, the editor and the
/// workspace file.
#[test]
fn the_controls_tab_carries_the_three_things_it_is_for() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let sidebar = CanvasSidebar::build(ws.action_handle(), NodeUid::<Desktops>::nil());
    ws.process_pending();
    ws.submit_action(sidebar, "controls", OpenSidebarTab { tab: 3 });
    ws.process_pending();

    let labels = button_labels(&ws, sidebar.erase());
    assert!(
        labels.iter().any(|l| l == "Controls"),
        "the tab is named Controls, not Settings: {labels:?}"
    );

    // Drawn, not merely built. A button that exists and is never laid out is
    // a button nobody can press, and counting nodes cannot tell the two apart.
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 900.0));
    let input = egui::RawInput {
        screen_rect: Some(rect),
        ..Default::default()
    };
    let mut painted: Vec<String> = Vec::new();
    let ws_ref = &ws;
    let output = ctx.clone().run_ui(input, |c| {
        egui::CentralPanel::default().show(c, |ui| {
            let mut ui = ui.new_child(egui::UiBuilder::new());
            let constraints = DrawConstraints {
                pos: ScreenPos { x: 0.0, y: 0.0 },
                x: Some(AxisConstraint::Exactly(300.0)),
                y: Some(AxisConstraint::Exactly(880.0)),
                wrap: WrapConstraints::NotAllowed,
                should_clip: true,
            };
            let mut draw = DrawContext::for_ui(
                NodeContext {
                    id: sidebar.erase(),
                    workspace: ws_ref,
                },
                constraints,
                &mut ui,
            );
            draw.draw_workspace_node(sidebar.erase(), constraints);
        });
    });
    painted.extend(output.shapes.iter().filter_map(|c| match &c.shape {
        egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
        _ => None,
    }));

    for expected in [
        "Global environment",
        "Choose…",
        "External editor",
        "Workspace",
        "Folder…",
        "Save",
        "Load",
    ] {
        assert!(
            painted.iter().any(|p| p == expected),
            "the tab draws {expected:?}: {painted:?}"
        );
    }
}

/// A workspace round-trips through a file: what was on the canvas is on it
/// again, and the history that put it there came too.
#[test]
fn a_workspace_saves_and_loads_with_its_history() {
    dex_nodes::scripting::init_python();
    let dir = scratch("roundtrip");
    let path = dir.join("saved.dex");

    let mut ws = Desktops::new_workspace();
    let root = ws.root();
    ws.submit_action_dyn(Action {
        dest: root,
        description: "add item".into(),
        body: Box::new(AddCanvasItem {
            child: Arc::new(Label::new("remember me".to_owned())),
            size: Vector { x: 200.0, y: 60.0 },
        }),
    });
    ws.process_pending();

    let canvas = ws
        .send_request(root.cast::<Desktops>(), ActiveCanvas)
        .expect("a canvas is active");
    let before = ws.send_request(canvas, CanvasChildren).unwrap_or_default();
    assert_eq!(before.len(), 1, "the item was added");

    ws.save_to(&path).expect("the workspace saves");
    assert!(
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > 0,
        "and wrote something"
    );

    // Into a workspace that knows nothing about the first.
    let mut other = Workspace::new_empty();
    let (loaded_root, registry) = Workspace::read_from(&path).expect("the file reads back");
    other.submit_action_dyn(Action {
        dest: NodeUid::nil(),
        description: "load".into(),
        body: Box::new(LoadWorkspace {
            root: loaded_root,
            registry,
        }),
    });
    other.process_pending();

    assert_eq!(other.root(), root, "the same root came back");
    let canvas = other
        .send_request(other.root().cast::<Desktops>(), ActiveCanvas)
        .expect("the loaded workspace has an active canvas");
    let after = other
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default();
    assert_eq!(
        after, before,
        "with the item still on it, under the same id"
    );

    // The history came too, which is what "everything" was chosen to mean.
    assert!(
        other.version_of(canvas.erase()) > 0,
        "the canvas remembers having been edited"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Saving says what went wrong rather than writing somewhere surprising.
#[test]
fn saving_without_a_folder_or_a_name_says_so() {
    dex_nodes::scripting::init_python();
    let dir = scratch("badsave");

    let mut ws = Workspace::new_empty();
    let sidebar = CanvasSidebar::build(ws.action_handle(), NodeUid::<Desktops>::nil());
    ws.process_pending();

    // Nothing written anywhere: there is no folder yet.
    ws.submit_action(
        sidebar,
        "save",
        dex_nodes::layouts::canvas::sidebar::SaveWorkspace,
    );
    ws.process_pending();
    assert_eq!(
        std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0),
        0,
        "nothing was written without a folder"
    );

    // With a folder but a blank name, still nothing.
    ws.submit_action(
        sidebar,
        "folder",
        SetSaveDir {
            path: dir.to_string_lossy().into_owned(),
        },
    );
    ws.process_pending();
    let name = owned_tree(&ws, sidebar.erase())
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid)
                .and_then(|n| {
                    (*n).as_any_ref()
                        .downcast_ref::<LabelEditable>()
                        .map(|l| l.value == "workspace.dex")
                })
                .unwrap_or(false)
        })
        .expect("the tab has a name field");
    ws.submit_action(
        name.cast::<LabelEditable>(),
        "blank it",
        SetText {
            value: "   ".to_owned(),
        },
    );
    ws.process_pending();
    ws.submit_action(
        sidebar,
        "save",
        dex_nodes::layouts::canvas::sidebar::SaveWorkspace,
    );
    ws.process_pending();
    assert_eq!(
        std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0),
        0,
        "nor with a blank name"
    );

    // Named, and it lands.
    ws.submit_action(
        name.cast::<LabelEditable>(),
        "name it",
        SetText {
            value: "here.dex".to_owned(),
        },
    );
    ws.process_pending();
    ws.submit_action(
        sidebar,
        "save",
        dex_nodes::layouts::canvas::sidebar::SaveWorkspace,
    );
    ws.process_pending();
    assert!(dir.join("here.dex").exists(), "the named file was written");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Folding a panel takes its space back, and unfolding returns it.
#[test]
fn the_sidebar_and_the_tab_row_fold_away() {
    let mut app = App::new();
    let root = app.root();

    // The sidebar's own tab strip is the thing that goes.
    assert!(
        app.shows_text("Prototypes"),
        "the sidebar is showing to begin with"
    );
    app.ws.submit_action(root, "fold", ToggleSidebar);
    app.ws.process_pending();
    assert!(
        !app.shows_text("Prototypes"),
        "folded, the sidebar is gone rather than merely narrow"
    );
    app.ws.submit_action(root, "unfold", ToggleSidebar);
    app.ws.process_pending();
    assert!(app.shows_text("Prototypes"), "and comes back");

    // The tab row: its canvas names are what disappear. Matched with the
    // number, since the sidebar has a "Canvas Lambda" button of its own.
    assert!(app.shows_text("Canvas 1"), "the tab row is showing");
    app.ws.submit_action(root, "fold tabs", ToggleTabBar);
    app.ws.process_pending();
    assert!(
        !app.shows_text("Canvas 1"),
        "folded, the tab row is gone too"
    );
    app.ws.submit_action(root, "unfold tabs", ToggleTabBar);
    app.ws.process_pending();
    assert!(app.shows_text("Canvas 1"), "and comes back");
}

/// Folding and unfolding twice over does what it says each time.
///
/// The collapse and reveal buttons swap in and out of being drawn, and a
/// button that stops being drawn keeps its last click cached. Read rather than
/// taken, that stale click fires again the moment the panel comes back and the
/// button is polled once more: the sidebar shuts itself the instant it is
/// opened — a bounce, rather than a fold.
#[test]
fn folding_twice_does_not_bounce_off_a_stale_click() {
    let mut app = App::new();

    // The sidebar starts 200 wide, so its edge — and the button on it — is
    // just past that.
    let collapse_at = egui::pos2(203.0, 400.0);
    app.click(collapse_at);
    assert!(
        !app.shows_text("Prototypes"),
        "the first click folded the sidebar"
    );

    // Back out, from the edge it went behind.
    app.click(egui::pos2(8.0, 400.0));
    assert!(app.shows_text("Prototypes"), "and the second unfolded it");

    // Settle. The collapse button is being polled again now, and the click
    // that folded it the first time must not still be sitting there.
    for _ in 0..3 {
        app.frame(vec![]);
    }
    assert!(
        app.shows_text("Prototypes"),
        "the sidebar stayed open rather than bouncing shut"
    );

    // And it still folds on the next honest click.
    app.click(collapse_at);
    assert!(
        !app.shows_text("Prototypes"),
        "a third click folds it again"
    );
}

/// A folded panel offers a way back only once the pointer comes near the edge
/// it went behind. A control you cannot find is a control you do not have, so
/// the one that folds a *showing* panel is always there.
#[test]
fn a_folded_panel_offers_its_way_back_near_the_edge() {
    let mut app = App::new();
    let root = app.root();
    app.ws.submit_action(root, "fold", ToggleSidebar);
    app.ws.process_pending();

    // Pointer far from the left edge: nothing offered.
    app.frame(vec![egui::Event::PointerMoved(egui::pos2(900.0, 400.0))]);
    assert!(
        !app.shows_text(">"),
        "no reveal button while the pointer is elsewhere"
    );

    // Near it, and the way back appears.
    app.frame(vec![egui::Event::PointerMoved(egui::pos2(10.0, 400.0))]);
    assert!(
        app.shows_text(">"),
        "the reveal button appears once the pointer is near the edge"
    );

    // Showing again, the collapse button stays put regardless of the pointer.
    app.ws.submit_action(root, "unfold", ToggleSidebar);
    app.ws.process_pending();
    app.frame(vec![egui::Event::PointerMoved(egui::pos2(900.0, 400.0))]);
    assert!(
        app.shows_text("<"),
        "a showing panel always says how to fold it"
    );
}

/// Left and right step through the tabs, and wrap.
#[test]
fn the_arrow_keys_step_through_the_tabs() {
    let mut app = App::new();
    let root = app.root();
    for _ in 0..2 {
        app.ws.submit_action(root, "add canvas", AddCanvas);
        app.ws.process_pending();
    }
    app.frame(vec![]);

    let canvases: Vec<NodeUid<Canvas>> = app
        .ws
        .send_request(root, Tabs)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|tab| app.ws.send_request(tab.cast::<DesktopTabView>(), TabCanvas))
        .collect();
    assert_eq!(canvases.len(), 3, "three tabs to step through");

    let active = |app: &App| app.ws.send_request(app.root(), ActiveCanvas).unwrap();
    let at = canvases.iter().position(|c| *c == active(&app)).unwrap();

    app.frame(vec![egui::Event::Key {
        key: egui::Key::ArrowRight,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }]);
    assert_eq!(
        active(&app),
        canvases[(at + 1) % 3],
        "right moves to the next tab"
    );

    app.frame(vec![egui::Event::Key {
        key: egui::Key::ArrowLeft,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }]);
    assert_eq!(active(&app), canvases[at], "and left comes back");

    // Wrapping: stepping back from the first lands on the last.
    app.ws
        .submit_action(root, "go first", StepTab { by: -(at as isize) });
    app.ws.process_pending();
    app.ws.submit_action(root, "wrap", StepTab { by: -1 });
    app.ws.process_pending();
    assert_eq!(
        active(&app),
        canvases[2],
        "the ends of the row are not dead"
    );
}

/// A focused text field keeps the arrow keys: they are the caret's.
#[test]
fn a_focused_editor_keeps_the_arrow_keys() {
    let mut app = App::new();
    let root = app.root();
    app.ws.submit_action(root, "add canvas", AddCanvas);
    app.ws.process_pending();
    app.frame(vec![]);

    let before = app.ws.send_request(root, ActiveCanvas).unwrap();

    // Anything focused at all is enough; the tab names are editable labels.
    let focus = egui::Id::new("some_focused_editor");
    app.ctx.memory_mut(|m| m.request_focus(focus));
    app.frame(vec![egui::Event::Key {
        key: egui::Key::ArrowRight,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }]);

    assert_eq!(
        app.ws.send_request(root, ActiveCanvas).unwrap(),
        before,
        "the tab did not change under the cursor"
    );
}

/// A canvas offers to fill the window with itself, which is the override stack.
#[test]
fn a_canvas_offers_to_go_fullscreen() {
    let mut app = App::new();
    let root = app.root();
    let canvas = app.ws.send_request(root, ActiveCanvas).unwrap();

    let inspector = app
        .ws
        .get_node(canvas.erase())
        .expect("the canvas is live")
        .build_inspector(NodeContext {
            id: canvas.erase(),
            workspace: &app.ws,
        })
        .expect("a canvas offers an inspector");
    app.ws.process_pending();

    let labels = button_labels(&app.ws, inspector);
    assert_eq!(
        labels,
        ["Fullscreen"],
        "the canvas menu offers exactly one thing"
    );

    // What the button does, checked through the action it submits.
    app.ws.submit_action(
        root,
        "fullscreen",
        PushOverride {
            node: canvas.erase(),
        },
    );
    app.ws.process_pending();
    let output = app.frame(vec![]);
    let closable = output.shapes.iter().any(|c| match &c.shape {
        egui::Shape::Text(t) => t.galley.text().contains("Close"),
        _ => false,
    });
    assert!(
        closable,
        "an opened override draws the way back out of itself"
    );
}
