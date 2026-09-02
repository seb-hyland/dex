//! The Settings tab: a global environment, and the command an editor is
//! launched with.
//!
//! The environment is the interesting one. It is picked with an ordinary
//! `FileBrowser` put into picker mode, and once taken it has to reach two
//! places that never see each other — the embedded interpreter's `sys.path`,
//! so imports work when a transform runs, and the checkout's pyright config,
//! so the editor does not underline those same imports in red.

use dex_core::prelude::*;
use dex_nodes::composites::button::Button;
use dex_nodes::layouts::canvas::sidebar::{CanvasSidebar, OpenSidebarTab, SetVenv};
use dex_nodes::primitives::checkout;
use dex_nodes::primitives::file_browser::{BrowseFor, FileBrowser, TakePickedPath};
use dex_nodes::primitives::text::GetText;
use dex_nodes::primitives::text::{LabelEditable, SetText};
use dex_nodes::settings;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The environment is process-global — there is one interpreter — so the tests
/// that set it take turns rather than reading each other's work.
static VENV: Mutex<()> = Mutex::new(());

const SCREEN: egui::Vec2 = egui::vec2(900.0, 700.0);

/// A scratch directory, emptied first so a rerun starts clean.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dex-settings-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

/// A directory shaped like a virtual environment.
fn fake_venv(at: &Path) -> PathBuf {
    let venv = at.join(".venv");
    std::fs::create_dir_all(venv.join("lib").join("python3.99").join("site-packages"))
        .expect("venv layout");
    venv
}

/// The editable field under `root` whose value is `value`.
///
/// By content, not by type: the Controls tab has several editable fields, and
/// "the only one" stopped being true the moment saving was added to it.
fn field_showing(ws: &Workspace, root: NodeUid, value: &str) -> NodeUid {
    owned_tree(ws, root)
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid)
                .and_then(|node| {
                    (*node)
                        .as_any_ref()
                        .downcast_ref::<LabelEditable>()
                        .map(|l| l.value == value)
                })
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("a field showing {value:?}"))
}

/// Every node reachable from `root` by ownership.
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

/// The labels of every button under `root`.
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

/// The sensor of the button under `root` labelled `label`, which is what a
/// click has to land on. A button owns exactly one.
fn button_sensor(ws: &Workspace, root: NodeUid, label: &str) -> NodeUid {
    let button = owned_tree(ws, root)
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid)
                .and_then(|node| {
                    (*node)
                        .as_any_ref()
                        .downcast_ref::<Button>()
                        .map(|b| b.label.text == label)
                })
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("a button labelled {label:?}"));
    let mut sensor = None;
    if let Some(node) = ws.get_node(button) {
        node.owned_refs(&mut |child| sensor = sensor.or(Some(child)));
    }
    sensor.expect("the button owns its sensor")
}

struct Harness {
    ws: Workspace,
    ctx: egui::Context,
}

impl Harness {
    fn new(root: NodeUid, ws: Workspace) -> Harness {
        let ctx = egui::Context::default();
        dex_nodes::fonts::install_fonts(&ctx);
        let mut h = Harness { ws, ctx };
        h.ws.set_root(root);
        h.ws.process_pending();
        h.frame(vec![]);
        h
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), SCREEN);
        let input = egui::RawInput {
            screen_rect: Some(rect),
            events,
            ..Default::default()
        };
        let ws = &mut self.ws;
        let _ = self.ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                ws.draw_frame(ui, rect);
            });
        });
        self.ws.tick_all();
        self.ws.process_pending();
    }

    /// Whether the button labelled `label` drew this frame. A button the tab
    /// is not offering still exists as a node — it is just not on the screen.
    fn shows(&self, root: NodeUid, label: &str) -> bool {
        let sensor = button_sensor(&self.ws, root, label);
        self.ctx.read_response(egui::Id::new(sensor)).is_some()
    }

    /// Where the button labelled `label` drew, this frame.
    fn rect_of(&self, sensor: NodeUid, label: &str) -> egui::Rect {
        self.ctx
            .read_response(egui::Id::new(sensor))
            .unwrap_or_else(|| panic!("the {label:?} button drew this frame"))
            .rect
    }

    /**
        Click the button labelled `label`, wherever it has ended up.

        Settled first, and its position re-read after the pointer moves onto it:
        a row whose neighbours are still resizing — a path field catching up
        with a folder change, say — slides out from under a position read a
        frame too early, and the click lands on nothing.
    */
    fn click(&mut self, root: NodeUid, label: &str) {
        let sensor = button_sensor(&self.ws, root, label);
        self.frame(vec![]);
        self.frame(vec![]);

        let at = self.rect_of(sensor, label).center();
        self.frame(vec![egui::Event::PointerMoved(at)]);
        let at = self.rect_of(sensor, label).center();
        self.frame(vec![egui::Event::PointerMoved(at)]);

        for pressed in [true, false] {
            self.frame(vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }]);
        }
        self.frame(vec![]);
    }
}

/// A picker lists dotfiles; an ordinary browser does not. `.venv` is the whole
/// reason — the environment you want to choose is usually named to be ignored.
#[test]
fn a_picker_lists_the_dotfiles_an_ordinary_browser_hides() {
    dex_nodes::scripting::init_python();
    let dir = scratch("dotfiles");
    fake_venv(&dir);
    std::fs::write(dir.join("plain.txt"), "hi").unwrap();
    let path = dir.to_string_lossy().into_owned();

    let mut ws = Workspace::new_empty();
    let plain = ws.insert_node_now(FileBrowser::new_at(ws.action_handle(), path.clone()));
    let picker = ws.insert_node_now(FileBrowser::picker(
        ws.action_handle(),
        path,
        BrowseFor::PickedDirectory,
    ));
    ws.process_pending();

    let plain_rows = button_labels(&ws, plain.erase());
    assert!(
        plain_rows.iter().any(|l| l.contains("plain.txt")),
        "an ordinary browser lists ordinary files: {plain_rows:?}"
    );
    assert!(
        !plain_rows.iter().any(|l| l.contains(".venv")),
        "and hides dotfiles: {plain_rows:?}"
    );

    let picker_rows = button_labels(&ws, picker.erase());
    assert!(
        picker_rows.iter().any(|l| l.contains(".venv")),
        "a picker lists them: {picker_rows:?}"
    );
    assert!(
        picker_rows.iter().any(|l| l.contains("Use this folder")),
        "and offers a way to choose the folder it is in: {picker_rows:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// "Use this folder" answers with the folder, once.
#[test]
fn choosing_a_folder_answers_the_asking_once() {
    dex_nodes::scripting::init_python();
    let dir = scratch("choose");
    let path = dir.to_string_lossy().into_owned();

    let mut ws = Workspace::new_empty();
    let picker = ws.insert_node_now(FileBrowser::picker(
        ws.action_handle(),
        path.clone(),
        BrowseFor::PickedDirectory,
    ));
    ws.process_pending();
    let mut h = Harness::new(picker.erase(), ws);

    assert_eq!(
        h.ws.send_request(picker, TakePickedPath).flatten(),
        None,
        "nothing has been chosen yet"
    );

    h.click(picker.erase(), "Use this folder");
    assert_eq!(
        h.ws.send_request(picker, TakePickedPath).flatten(),
        Some(path),
        "the folder it was showing is the answer"
    );
    // Taken, so whoever asked acts on one choice once.
    assert_eq!(
        h.ws.send_request(picker, TakePickedPath).flatten(),
        None,
        "the answer was consumed by the asking"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/**
    The environment reaches both places that need it, and a folder that is not
    one is refused rather than quietly accepted.

    All of it in one test: the setting is process-global — there is one
    interpreter — so two of these running at once would read each other's work.
*/
#[test]
fn the_global_environment_reaches_the_interpreter_and_the_editor() {
    let _guard = VENV.lock().unwrap_or_else(|e| e.into_inner());
    dex_nodes::scripting::init_python();
    let dir = scratch("venv");
    let venv = fake_venv(&dir);
    let site_packages = venv.join("lib").join("python3.99").join("site-packages");

    let mut ws = Workspace::new_empty();
    let sidebar = CanvasSidebar::build(
        ws.action_handle(),
        NodeUid::<dex_nodes::layouts::desktops::Desktops>::nil(),
    );
    ws.process_pending();
    ws.submit_action(sidebar, "settings", OpenSidebarTab { tab: 3 });
    ws.process_pending();

    // A folder that is not an environment says so, and changes nothing.
    ws.submit_action(
        sidebar,
        "bad venv",
        SetVenv {
            path: dir.to_string_lossy().into_owned(),
        },
    );
    ws.process_pending();
    assert_eq!(
        settings::venv(),
        None,
        "a folder with no site-packages is not taken as an environment"
    );

    // The real one is.
    ws.submit_action(
        sidebar,
        "venv",
        SetVenv {
            path: venv.to_string_lossy().into_owned(),
        },
    );
    ws.process_pending();
    assert_eq!(settings::venv().as_deref(), Some(venv.as_path()));

    // On the interpreter's path, at the front, which is what "always available
    // in Python" actually means.
    let on_path: bool = pyo3::Python::attach(|py| {
        use pyo3::prelude::*;
        let path: Vec<String> = py
            .import("sys")
            .unwrap()
            .getattr("path")
            .unwrap()
            .extract()
            .unwrap();
        path.first() == Some(&site_packages.to_string_lossy().into_owned())
    });
    assert!(on_path, "the environment's packages lead sys.path");

    // And named in the checkout config, or the editor would underline every
    // import that works perfectly well at runtime.
    let out = checkout::write("venv-config", "def transform():\n    pass\n", &[]).unwrap();
    let config = std::fs::read_to_string(out.dir.join("pyrightconfig.json")).unwrap();
    assert!(
        config.contains("\"venv\": \".venv\""),
        "the checkout names the environment: {config}"
    );
    assert!(
        config.contains(&format!(
            "\"venvPath\": \"{}\"",
            dir.to_string_lossy().replace('\\', "\\\\")
        )),
        "and where to find it: {config}"
    );

    // Cleared, and it goes from both.
    ws.submit_action(
        sidebar,
        "clear",
        SetVenv {
            path: String::new(),
        },
    );
    ws.process_pending();
    assert_eq!(settings::venv(), None, "clearing takes it back off");
    let gone: bool = pyo3::Python::attach(|py| {
        use pyo3::prelude::*;
        let path: Vec<String> = py
            .import("sys")
            .unwrap()
            .getattr("path")
            .unwrap()
            .extract()
            .unwrap();
        !path.contains(&site_packages.to_string_lossy().into_owned())
    });
    assert!(gone, "and off sys.path with it");

    let out = checkout::write("venv-config", "def transform():\n    pass\n", &[]).unwrap();
    let config = std::fs::read_to_string(out.dir.join("pyrightconfig.json")).unwrap();
    assert!(
        !config.contains("venvPath"),
        "and out of the checkout config: {config}"
    );

    let _ = std::fs::remove_dir_all(&out.dir);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Committing the editor field is what changes the command a checkout is
/// opened with, `$1` and `$2` and all.
#[test]
fn the_editor_field_sets_the_command_a_checkout_is_opened_with() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let sidebar = CanvasSidebar::build(
        ws.action_handle(),
        NodeUid::<dex_nodes::layouts::desktops::Desktops>::nil(),
    );
    ws.process_pending();

    let field = field_showing(&ws, sidebar.erase(), settings::DEFAULT_EDITOR);

    // A tick first, so the poll has a baseline and does not read the initial
    // value as an edit.
    ws.tick_all();
    ws.process_pending();
    assert_eq!(
        settings::editor_command(),
        settings::DEFAULT_EDITOR,
        "untouched, the field shows and means the default"
    );

    ws.submit_action(
        field.cast::<LabelEditable>(),
        "type a command",
        SetText {
            value: "myeditor --root $1".to_owned(),
        },
    );
    ws.process_pending();
    ws.tick_all();
    ws.process_pending();

    assert_eq!(settings::editor_command(), "myeditor --root $1");
    // The file was not named, so it is appended after the folder.
    assert_eq!(
        settings::editor_argv(
            &settings::editor_command(),
            Path::new("/tmp/co"),
            Path::new("/tmp/co/main.py")
        ),
        ["myeditor", "--root", "/tmp/co", "/tmp/co/main.py"]
    );

    // Blank means the default, not an empty command line.
    settings::set_editor_command(String::new());
    assert_eq!(settings::editor_command(), settings::DEFAULT_EDITOR);
}

/// The tab as it is actually used: open Settings, click Choose, steer the
/// browser to an environment, and take it. Nothing here reaches past the
/// buttons, which is the only way to know the tab draws what it polls.
#[test]
fn choosing_an_environment_through_the_tab_takes_it() {
    let _guard = VENV.lock().unwrap_or_else(|e| e.into_inner());
    dex_nodes::scripting::init_python();
    let _ = settings::set_venv(None);

    let dir = scratch("tab-flow");
    let venv = fake_venv(&dir);

    let mut ws = Workspace::new_empty();
    let sidebar = CanvasSidebar::build(
        ws.action_handle(),
        NodeUid::<dex_nodes::layouts::desktops::Desktops>::nil(),
    );
    ws.process_pending();
    ws.submit_action(sidebar, "settings", OpenSidebarTab { tab: 3 });
    ws.process_pending();

    let mut h = Harness::new(sidebar.erase(), ws);
    // Nothing to clear yet, so that button is built but not shown.
    assert!(
        h.shows(sidebar.erase(), "Choose…"),
        "the tab offers a choice"
    );
    assert!(
        !h.shows(sidebar.erase(), "Clear"),
        "and nothing to undo, with no environment set"
    );

    h.click(sidebar.erase(), "Choose…");
    let browser = owned_tree(&h.ws, sidebar.erase())
        .into_iter()
        .find(|uid| {
            h.ws.get_node(*uid)
                .is_some_and(|node| (*node).as_any_ref().is::<FileBrowser>())
        })
        .expect("choosing opens a browser");

    // Steer it by typing the path, which is what the path field is for.
    let path_field = owned_tree(&h.ws, browser)
        .into_iter()
        .find(|uid| {
            h.ws.get_node(*uid)
                .is_some_and(|node| (*node).as_any_ref().is::<LabelEditable>())
        })
        .expect("the browser shows where it is");
    h.ws.submit_action(
        path_field.cast::<LabelEditable>(),
        "go to the venv",
        SetText {
            value: venv.to_string_lossy().into_owned(),
        },
    );
    h.frame(vec![]);
    h.frame(vec![]);

    h.click(sidebar.erase(), "Use this folder");
    assert_eq!(
        settings::venv().as_deref(),
        Some(venv.as_path()),
        "the folder the browser was showing became the environment"
    );

    // The browser has done its job and is gone, and there is now something to
    // clear.
    assert!(
        h.ws.get_node(browser).is_none(),
        "the browser closes once it has been answered"
    );
    assert!(
        h.shows(sidebar.erase(), "Clear"),
        "and the tab now offers to undo it"
    );

    h.click(sidebar.erase(), "Clear");
    assert_eq!(settings::venv(), None, "cleared from the tab too");

    let _ = std::fs::remove_dir_all(&dir);
}

/// The browser is a toggle: the button that opens it closes it again, and says
/// which it will do. An opened-by-mistake browser fills the tab, so there has
/// to be a way back out that is not choosing something.
#[test]
fn the_environment_button_closes_the_browser_it_opened() {
    let _guard = VENV.lock().unwrap_or_else(|e| e.into_inner());
    dex_nodes::scripting::init_python();
    let _ = settings::set_venv(None);

    let mut ws = Workspace::new_empty();
    let sidebar = CanvasSidebar::build(
        ws.action_handle(),
        NodeUid::<dex_nodes::layouts::desktops::Desktops>::nil(),
    );
    ws.process_pending();
    ws.submit_action(sidebar, "settings", OpenSidebarTab { tab: 3 });
    ws.process_pending();
    let mut h = Harness::new(sidebar.erase(), ws);

    let browsing = |ws: &Workspace| {
        owned_tree(ws, sidebar.erase()).into_iter().any(|uid| {
            ws.get_node(uid)
                .is_some_and(|node| (*node).as_any_ref().is::<FileBrowser>())
        })
    };
    assert!(!browsing(&h.ws), "nothing is open to begin with");

    h.click(sidebar.erase(), "Choose…");
    assert!(browsing(&h.ws), "the button opened a browser");
    assert!(
        h.shows(sidebar.erase(), "Cancel"),
        "and now says how to get out of it"
    );

    h.click(sidebar.erase(), "Cancel");
    assert!(!browsing(&h.ws), "the same button closed it again");
    assert_eq!(settings::venv(), None, "closing chose nothing");
    assert!(
        h.shows(sidebar.erase(), "Choose…"),
        "and it offers to open one again"
    );
}

/// The command is typed into, not just displayed at.
#[test]
fn the_editor_command_is_a_field_you_can_type_in() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let sidebar = CanvasSidebar::build(
        ws.action_handle(),
        NodeUid::<dex_nodes::layouts::desktops::Desktops>::nil(),
    );
    ws.process_pending();
    ws.submit_action(sidebar, "settings", OpenSidebarTab { tab: 3 });
    ws.process_pending();

    let field = field_showing(&ws, sidebar.erase(), settings::DEFAULT_EDITOR);
    let editable = ws
        .get_node(field)
        .and_then(|node| {
            (*node)
                .as_any_ref()
                .downcast_ref::<LabelEditable>()
                .map(|l| (l.interactive, l.shrink_to_text))
        })
        .expect("it is an editable label");
    assert!(
        editable.0,
        "the field takes typing without having to be woken up first"
    );
    assert!(
        !editable.1,
        "and fills its row, so there is a box to click into"
    );

    let mut h = Harness::new(sidebar.erase(), ws);
    h.ws.submit_action(
        field.cast::<LabelEditable>(),
        "type",
        SetText {
            value: "myeditor $2".to_owned(),
        },
    );
    h.frame(vec![]);
    h.frame(vec![]);
    assert_eq!(
        h.ws.send_request(field.cast::<LabelEditable>(), GetText)
            .as_deref(),
        Some("myeditor $2"),
        "and shows what was typed"
    );

    settings::set_editor_command(String::new());
}

/// The "Use this folder" button sits under the listing, not beside the path.
/// A path is as long as it is: put the button after it and a deep enough
/// folder pushes it off the edge, which is where the last version of this went
/// wrong.
#[test]
fn the_choose_button_stays_put_however_long_the_path_is() {
    dex_nodes::scripting::init_python();
    let dir = scratch("long-path");
    // Deep enough that the path field is wider than the panel.
    let deep = dir.join("a-fairly-long-folder-name/and-another-one-here/and-a-third-for-luck");
    std::fs::create_dir_all(&deep).unwrap();

    let mut ws = Workspace::new_empty();
    let shallow = ws.insert_node_now(FileBrowser::picker(
        ws.action_handle(),
        dir.to_string_lossy().into_owned(),
        BrowseFor::PickedDirectory,
    ));
    ws.process_pending();
    let mut h = Harness::new(shallow.erase(), ws);

    let sensor = button_sensor(&h.ws, shallow.erase(), "Use this folder");
    let at_root = h.rect_of(sensor, "Use this folder");
    let path_field = owned_tree(&h.ws, shallow.erase())
        .into_iter()
        .find(|uid| {
            h.ws.get_node(*uid)
                .is_some_and(|node| (*node).as_any_ref().is::<LabelEditable>())
        })
        .expect("the browser shows where it is");

    // Now walk somewhere deep, so the path field grows a long way.
    h.ws.submit_action(
        path_field.cast::<LabelEditable>(),
        "go deep",
        SetText {
            value: deep.to_string_lossy().into_owned(),
        },
    );
    h.frame(vec![]);
    h.frame(vec![]);
    h.frame(vec![]);
    let at_depth = h.rect_of(sensor, "Use this folder");

    assert!(
        (at_depth.min.x - at_root.min.x).abs() < 0.5,
        "the button did not move sideways: {} then {}",
        at_root.min.x,
        at_depth.min.x
    );
    assert!(
        at_depth.max.x <= SCREEN.x + 0.5,
        "and is still on the screen: it ends at {}",
        at_depth.max.x
    );

    // Under the listing, not level with the path.
    let path_rect = h
        .ctx
        .read_response(egui::Id::new(path_field))
        .map(|r| r.rect);
    if let Some(path_rect) = path_rect {
        assert!(
            at_depth.min.y > path_rect.max.y,
            "the button sits below the path row: {} vs {}",
            at_depth.min.y,
            path_rect.max.y
        );
    }

    // And still answers with the folder it is showing.
    h.click(shallow.erase(), "Use this folder");
    assert_eq!(
        h.ws.send_request(shallow, TakePickedPath)
            .flatten()
            .as_deref(),
        Some(deep.to_string_lossy().as_ref()),
        "the deep folder is what it chose"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The footer button fits inside the browser, whole. Reserving a fixed band
/// for it is what cut the label in half: a button is as tall as its font and
/// padding make it, which is not a number this file gets to choose.
#[test]
fn the_choose_button_is_not_clipped_by_the_band_kept_for_it() {
    dex_nodes::scripting::init_python();
    let dir = scratch("footer-fit");
    // Enough entries that the list wants every pixel it can get.
    for i in 0..40 {
        std::fs::create_dir_all(dir.join(format!("folder-{i:02}"))).unwrap();
    }

    let mut ws = Workspace::new_empty();
    let browser = ws.insert_node_now(FileBrowser::picker(
        ws.action_handle(),
        dir.to_string_lossy().into_owned(),
        BrowseFor::PickedDirectory,
    ));
    ws.process_pending();

    // Drawn into a fixed box, the way the settings tab hands it one.
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 500.0));
    let box_h = 420.0;
    let mut region = None;
    // Twice: the first frame is what tells the browser how tall its button is.
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let ws = &ws;
        let _ = ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| {
                let mut ui = ui.new_child(egui::UiBuilder::new());
                let constraints = DrawConstraints {
                    pos: ScreenPos { x: 0.0, y: 0.0 },
                    x: Some(AxisConstraint::Exactly(300.0)),
                    y: Some(AxisConstraint::Exactly(box_h)),
                    wrap: WrapConstraints::NotAllowed,
                    should_clip: true,
                };
                let mut draw = DrawContext::for_ui(
                    NodeContext {
                        id: browser.erase(),
                        workspace: ws,
                    },
                    constraints,
                    &mut ui,
                );
                region = draw
                    .draw_workspace_node(browser.erase(), constraints)
                    .and_then(|r| r.region());
            });
        });
    }

    let region = region.expect("the browser drew");
    assert!(
        (region.size().y - box_h).abs() < 0.5,
        "the browser fills the box it was given: {}",
        region.size().y
    );

    let sensor = button_sensor(&ws, browser.erase(), "Use this folder");
    let rect = ctx
        .read_response(egui::Id::new(sensor))
        .expect("the button drew")
        .rect;
    assert!(
        rect.height() > 20.0,
        "the button is a whole button, not a sliver: {}",
        rect.height()
    );
    assert!(
        rect.max.y <= box_h + 0.5,
        "and fits inside the box: it ends at {} of {box_h}",
        rect.max.y
    );
    assert!(
        rect.max.y > box_h - 12.0,
        "sitting near the bottom, with a little room under it: it ends at {}",
        rect.max.y
    );

    let _ = std::fs::remove_dir_all(&dir);
}
