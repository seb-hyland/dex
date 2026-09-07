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

/// The list comes back in the order it was written, with what was written in it.
#[test]
fn a_prelude_offers_what_it_adds() {
    dex_nodes::scripting::init_python();
    let (offers, error) = read(OFFERS);

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
    assert_eq!(read("").0.len(), 0);
    assert_eq!(read("").1, None);
    assert_eq!(read("x = 1 + 1\n").1, None);
}

/// A prelude that will not run says why, rather than going quiet.
#[test]
fn a_broken_prelude_reports_itself() {
    dex_nodes::scripting::init_python();
    let (offers, error) = read("def broken(:\n");
    assert!(offers.is_empty(), "nothing was offered");
    let error = error.expect("the failure is reported");
    assert!(
        error.contains("SyntaxError"),
        "and says what went wrong: {error}"
    );
}

/// Tick and drain until `done`, or give up.
fn settle(ws: &mut Workspace, mut done: impl FnMut(&Workspace) -> bool) -> bool {
    for _ in 0..400 {
        ws.tick_all();
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
