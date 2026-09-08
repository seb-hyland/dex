//! Nodes a workspace's prelude offers in the sidebar.

use std::time::Duration;

use dex_core::prelude::*;
use dex_nodes::composites::button::Button;
use dex_nodes::layouts::canvas::layout::{Canvas, CanvasChildren};
use dex_nodes::layouts::canvas::nodes::CanvasNodeChild;
use dex_nodes::layouts::canvas::sidebar::{CanvasSidebar, PreludeOffers};
use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
use dex_nodes::prelude_prototypes::{DEFAULT_SIZE, read};
use dex_nodes::primitives::shapes::Path;
use dex_nodes::primitives::text::{Label, SetText};

const OFFERS: &str = r#"
class Marker:
    pass

dex.prelude_prototypes.add("Greeting", "hello")
dex.prelude_prototypes.add(
    "Blob",
    dex.Path.polygon(
        [dex.Vector.new(0.0, 0.0), dex.Vector.new(60.0, 0.0), dex.Vector.new(0.0, 60.0)],
        dex.Color.rgb(200, 120, 60),
        dex.Stroke.none(),
    ),
    (90.0, 90.0),
)
"#;

/// A workspace to read a prelude against, and the actions it queued.
fn scanned(prelude: &str) -> (Vec<dex_nodes::prelude_prototypes::Offer>, Option<String>) {
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    read(prelude, &handle)
}

/// The list comes back in the order it was written, with what was written in it.
#[test]
fn a_prelude_offers_what_it_adds() {
    dex_nodes::scripting::init_python();
    let (offers, error) = scanned(OFFERS);

    assert_eq!(error, None, "the prelude ran");
    let names: Vec<&str> = offers.iter().map(|(name, _, _)| name.as_str()).collect();
    assert_eq!(names, ["Greeting", "Blob"], "in the order they were added");

    // A plain Python value becomes a node the same way a script's return does.
    assert!(
        offers[0].1.as_ref().as_any_ref().is::<Label>(),
        "a string is offered as a label"
    );
    assert!(
        (offers[0].2.x, offers[0].2.y) == (DEFAULT_SIZE.x, DEFAULT_SIZE.y),
        "and takes the default size, having named none"
    );
    assert!(
        offers[1].1.as_ref().as_any_ref().is::<Path>(),
        "a shape is offered as itself"
    );
    assert_eq!(
        (offers[1].2.x, offers[1].2.y),
        (90.0, 90.0),
        "at the size it named"
    );
}

/// Nothing offered is not an error, and neither is having no prelude at all.
#[test]
fn an_empty_prelude_offers_nothing_and_complains_about_nothing() {
    dex_nodes::scripting::init_python();
    assert_eq!(scanned("").0.len(), 0);
    assert_eq!(scanned("").1, None);
    assert_eq!(scanned("x = 1 + 1\n").1, None);
}

/// A prelude that will not run says why, rather than going quiet.
#[test]
fn a_broken_prelude_reports_itself() {
    dex_nodes::scripting::init_python();
    let (offers, error) = scanned("def broken(:\n");
    assert!(offers.is_empty(), "nothing was offered");
    let error = error.expect("the failure is reported");
    assert!(
        error.contains("SyntaxError"),
        "and says what went wrong: {error}"
    );
}

/// Tick and drain until `done`, or give up.
fn settle(ws: &mut Workspace, mut done: impl FnMut(&Workspace) -> bool) -> bool {
    // Drawn, not just ticked: the sidebar reads the prelude when it is about to
    // show what the prelude offers, so a workspace nobody looks at never runs
    // it. Which means a test that wants the offers has to look.
    let ctx = egui::Context::default();
    dex_nodes::fonts::install_fonts(&ctx);
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 900.0));
    for _ in 0..400 {
        let _ = ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
            },
        );
        ws.process_pending();
        if done(ws) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

/// The sidebar reads what the prelude offers whenever the prelude changes, and
/// keeps a template of each so one can be stamped out.
#[test]
fn the_sidebar_picks_up_what_the_prelude_offers() {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let sidebar = ws
        .live_ids()
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid)
                .is_some_and(|node| (*node).as_any_ref().is::<CanvasSidebar>())
        })
        .expect("the root has a sidebar")
        .cast::<CanvasSidebar>();

    // Nothing is offered until something is written.
    ws.tick_all();
    ws.process_pending();
    assert!(
        ws.send_request(sidebar, PreludeOffers)
            .unwrap_or_default()
            .is_empty(),
        "an empty prelude offers nothing"
    );

    ws.submit_action_dyn(Action {
        dest: prelude_editor(&ws),
        description: "write a prelude".into(),
        body: Box::new(SetText {
            value: OFFERS.to_owned(),
        }),
    });
    ws.process_pending();

    assert!(
        settle(&mut ws, |ws| ws
            .send_request(sidebar, PreludeOffers)
            .unwrap_or_default()
            .len()
            == 2),
        "the sidebar picked up both"
    );
    let offered = ws.send_request(sidebar, PreludeOffers).unwrap_or_default();
    assert_eq!(
        offered
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["Greeting", "Blob"]
    );

    // Each has a button of its own, labelled for it.
    let labels: Vec<String> = ws
        .live_ids()
        .into_iter()
        .filter_map(|uid| {
            ws.get_node(uid).and_then(|node| {
                (*node)
                    .as_any_ref()
                    .downcast_ref::<Button>()
                    .map(|b| b.label.text.clone())
            })
        })
        .collect();
    for name in ["Greeting", "Blob"] {
        assert!(labels.contains(&name.to_owned()), "a button reads {name:?}");
    }

    // Placing one leaves the template where it was: an offer is stamped, not spent.
    let (_, template, size) = offered[1].clone();
    let canvas = ws
        .send_request(ws.root().cast::<Desktops>(), ActiveCanvas)
        .expect("a desktop is showing");
    let copy = ws.deep_clone(template);
    ws.submit_action(
        canvas,
        "place it",
        dex_nodes::layouts::canvas::layout::PlaceOnCanvas { node: copy, size },
    );
    ws.process_pending();

    assert!(
        ws.get_node(template).is_some(),
        "the template outlives what was stamped from it"
    );
    let placed = ws
        .send_request(canvas.cast::<Canvas>(), CanvasChildren)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| ws.send_request(item, CanvasNodeChild))
        .any(|child| {
            ws.get_node(child)
                .is_some_and(|node| (*node).as_any_ref().is::<Path>())
        });
    assert!(placed, "and a copy of it landed on the desktop");

    // Rewriting the prelude replaces the list rather than adding to it.
    ws.submit_action_dyn(Action {
        dest: prelude_editor(&ws),
        description: "rewrite the prelude".into(),
        body: Box::new(SetText {
            value: "dex.prelude_prototypes.add(\"Only\", 1)\n".to_owned(),
        }),
    });
    ws.process_pending();
    assert!(
        settle(&mut ws, |ws| {
            let names: Vec<String> = ws
                .send_request(sidebar, PreludeOffers)
                .unwrap_or_default()
                .into_iter()
                .map(|(name, _, _)| name)
                .collect();
            names == ["Only"]
        }),
        "a prelude names its whole list every time it runs"
    );
}

/// The code editor the sidebar keeps its prelude in.
fn prelude_editor(ws: &Workspace) -> NodeUid {
    ws.live_ids()
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid).is_some_and(|node| {
                (*node)
                    .as_any_ref()
                    .is::<dex_nodes::primitives::text::CodeEditor>()
            })
        })
        .expect("the sidebar holds a code editor")
}

/// A prelude that offers a factory: `dex.ws` is live while it is scanned, so a
/// prototype can build the children a real view is made of.
const FACTORY: &str = r#"
class Widget:
    """A node with a child of its own, built the way an example's `build` does."""

    def __init__(self, dropdown):
        self.dropdown = dropdown

    def type_name(self):
        return "A Widget"

    def owned_nodes(self):
        return [self.dropdown]

    def on_delete(self, ctx):
        ctx.workspace.delete_node(self.dropdown)

    def draw(self, ctx):
        return ctx.draw_node(self.dropdown, ctx.constraints)


built = []


def build_widget(ws):
    built.append(1)
    return Widget(dex.Dropdown.build(ws, ["one", "two"]))


dex.prelude_prototypes.add("Widget", build_widget, (200.0, 30.0))
"#;

/// The factory is handed a workspace, and what it inserts arrives with the offer.
#[test]
fn a_factory_is_called_with_a_workspace() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();

    let (handle, actions) = WorkspaceActionHandle::buffered();
    let (offers, error) = read(FACTORY, &handle);
    assert_eq!(error, None, "the prelude ran and the factory returned");
    assert_eq!(offers.len(), 1, "one offer, from one `add`");
    assert_eq!(offers[0].0, "Widget");

    // The dropdown the factory built was queued on the same handle.
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
    let dropdowns = ws
        .live_ids()
        .into_iter()
        .filter(|uid| {
            ws.get_node(*uid).is_some_and(|node| {
                (*node)
                    .as_any_ref()
                    .is::<dex_nodes::primitives::dropdown::Dropdown>()
            })
        })
        .count();
    assert_eq!(dropdowns, 1, "the factory's child landed in the workspace");
}

/// The whole point of a factory: the prelude runs before every lambda, and a
/// prototype must not build itself — or seed a fresh set of children — then.
#[test]
fn a_factory_is_not_called_when_a_lambda_runs() {
    use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};
    dex_nodes::scripting::init_python();
    let ws = Workspace::new_empty();

    let (handle, actions) = WorkspaceActionHandle::buffered();
    let script = "def transform():\n    return \"%d\" % len(built)\n";
    let args: [(String, ScriptValue); 0] = [];
    let out = run_script(script, FACTORY, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("the prelude and script run");
    let ScriptOutput::Node(node) = out else {
        panic!("returns a count")
    };
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node).map(|v| v.display()),
        Some("0".to_owned()),
        "the factory was registered but never called"
    );
    drop(handle);
    assert_eq!(
        actions.try_iter().count(),
        0,
        "and so nothing was inserted by the run"
    );
}

/// A factory that raises says so, in the same place a syntax error does.
#[test]
fn a_factory_that_raises_is_reported() {
    dex_nodes::scripting::init_python();
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let (offers, error) = read(
        "def bad(ws):\n    raise ValueError('no')\n\n\
         dex.prelude_prototypes.add(\"Bad\", bad)\n",
        &handle,
    );
    assert!(offers.is_empty(), "the offer could not be built");
    let error = error.expect("the failure is reported");
    assert!(
        error.contains("Bad") && error.contains("no"),
        "and names both the offer and the cause: {error}"
    );
}

/// Rewriting the prelude takes the old templates away *with their children*, so
/// editing it repeatedly does not silt the workspace up with orphans.
#[test]
fn replacing_the_offers_deletes_what_they_owned() {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let sidebar = ws
        .live_ids()
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid)
                .is_some_and(|node| (*node).as_any_ref().is::<CanvasSidebar>())
        })
        .expect("the root has a sidebar")
        .cast::<CanvasSidebar>();

    let dropdowns = |ws: &Workspace| {
        ws.live_ids()
            .into_iter()
            .filter(|uid| {
                ws.get_node(*uid).is_some_and(|node| {
                    (*node)
                        .as_any_ref()
                        .is::<dex_nodes::primitives::dropdown::Dropdown>()
                })
            })
            .count()
    };
    // A fresh workspace opens with the *default* prelude in its editor, which
    // offers a data explorer — and that has dropdowns of its own. So the
    // baseline is taken after this prelude has replaced it and settled, not
    // before, or it would be counting somebody else's controls.
    ws.submit_action_dyn(Action {
        dest: prelude_editor(&ws),
        description: "write a prelude".into(),
        body: Box::new(SetText {
            value: FACTORY.to_owned(),
        }),
    });
    ws.process_pending();
    assert!(
        settle(&mut ws, |ws| {
            let names: Vec<String> = ws
                .send_request(sidebar, PreludeOffers)
                .unwrap_or_default()
                .into_iter()
                .map(|(name, _, _)| name)
                .collect();
            names == ["Widget"]
        }),
        "the widget is what is on offer"
    );
    let with_widget = dropdowns(&ws);
    let before = with_widget - 1;
    assert!(
        with_widget >= 1,
        "the offered widget's dropdown is live ({with_widget})"
    );

    // Rewrite it to offer something else entirely.
    ws.submit_action_dyn(Action {
        dest: prelude_editor(&ws),
        description: "rewrite the prelude".into(),
        body: Box::new(SetText {
            value: "dex.prelude_prototypes.add(\"Only\", 1)\n".to_owned(),
        }),
    });
    ws.process_pending();
    assert!(
        settle(&mut ws, |ws| {
            let names: Vec<String> = ws
                .send_request(sidebar, PreludeOffers)
                .unwrap_or_default()
                .into_iter()
                .map(|(name, _, _)| name)
                .collect();
            names == ["Only"] && dropdowns(ws) == before
        }),
        "the old template went, and took its dropdown with it \
         (before {before}, now {})",
        dropdowns(&ws)
    );
}

/// Typing in the prelude does not re-run it; a finished edit does.
///
/// Reading the prelude means *running* it, which is not something to do between
/// one keystroke and the next. So the scan watches the committed value, and the
/// editor commits when focus leaves it — which is now noticed on `tick` rather
/// than while drawing, so an editor whose tab has been closed still commits
/// what was typed instead of losing it.
#[test]
fn typing_does_not_re_run_the_prelude_but_finishing_does() {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let sidebar = ws
        .live_ids()
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid)
                .is_some_and(|node| (*node).as_any_ref().is::<CanvasSidebar>())
        })
        .expect("the root has a sidebar")
        .cast::<CanvasSidebar>();
    let editor = prelude_editor(&ws).cast::<dex_nodes::primitives::text::CodeEditor>();

    let offers = |ws: &Workspace| -> Vec<String> {
        ws.send_request(sidebar, PreludeOffers)
            .unwrap_or_default()
            .into_iter()
            .map(|(name, _, _)| name)
            .collect()
    };

    ws.submit_action_dyn(Action {
        dest: editor.erase(),
        description: "write a prelude".into(),
        body: Box::new(SetText {
            value: "dex.prelude_prototypes.add(\"First\", 1)\n".to_owned(),
        }),
    });
    ws.process_pending();
    assert!(
        settle(&mut ws, |ws| offers(ws) == ["First"]),
        "the first prelude was read"
    );

    // Typing: the buffer changes, the committed value does not.
    let typed = "dex.prelude_prototypes.add(\"Second\", 2)\n";
    let node = ws.get_node(editor.erase()).expect("the editor is live");
    (*node)
        .as_any_ref()
        .downcast_ref::<dex_nodes::primitives::text::CodeEditor>()
        .expect("a code editor")
        .set_buffer(typed.to_owned());

    for _ in 0..40 {
        ws.tick_all();
        ws.process_pending();
    }
    assert_eq!(offers(&ws), ["First"], "typing did not re-run the prelude");

    // Finishing the edit is what a commit is, and that is read.
    ws.submit_action_dyn(Action {
        dest: editor.erase(),
        description: "finish the edit".into(),
        body: Box::new(SetText {
            value: typed.to_owned(),
        }),
    });
    ws.process_pending();
    assert!(
        settle(&mut ws, |ws| offers(ws) == ["Second"]),
        "a finished edit is read"
    );
}
