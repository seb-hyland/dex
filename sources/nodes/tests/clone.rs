//! Deep cloning a subtree, and the `Mirror` that keeps a copy in step.

use dex_core::prelude::*;
use dex_nodes::{
    composites::{
        button::Button,
        lambda::{ConnectionPort, SetConnection},
    },
    layouts::{Mirror, canvas::nodes::CanvasNode},
    primitives::{
        interaction::InteractionBox,
        nothing::Nothing,
        text::{GetText, Label, LabelEditable, SetText},
    },
};

/// An empty workspace with a throwaway root, drained and ready.
fn workspace() -> Workspace {
    let mut ws = Workspace::new_empty();
    let root = ws.insert_node_now(Nothing);
    ws.set_root(root.erase());
    ws
}

#[test]
fn a_copy_gets_its_own_children() {
    let mut ws = workspace();
    let button = Button::build(ws.action_handle(), Label::new("Press".to_owned()));
    ws.process_pending();

    let copy = ws.deep_clone(button.erase());
    ws.process_pending();

    assert_ne!(copy, button.erase(), "the copy is a new node");
    let original = ws.get_node(button.erase()).expect("original is live");
    let copied = ws.get_node(copy).expect("copy is live");

    // The sensor is an owned child, so the copy must have its own.
    let sensor_of = |node: &Arc<dyn Node>| {
        let mut found = Vec::new();
        node.owned_refs(&mut |uid| found.push(uid));
        found
    };
    let original_children = sensor_of(&original);
    let copied_children = sensor_of(&copied);
    assert_eq!(original_children.len(), 1);
    assert_eq!(copied_children.len(), 1);
    assert_ne!(
        original_children[0], copied_children[0],
        "the copy must not share the original's sensor"
    );
    assert!(
        ws.get_node(copied_children[0]).is_some(),
        "the copied sensor is registered"
    );
}

#[test]
fn a_copy_reaches_the_whole_owned_subtree() {
    let mut ws = workspace();
    let label = ws.insert_node(LabelEditable::new("inner".to_owned()));
    let node = CanvasNode::build(
        ws.action_handle(),
        label.erase(),
        Vector { x: 0.0, y: 0.0 },
        Vector { x: 10.0, y: 10.0 },
    );
    ws.process_pending();

    let copy = ws.deep_clone(node.erase());
    ws.process_pending();

    // A canvas node owns its child plus ten sensors, and all of them are copied.
    let copied_child = ws
        .send_request(copy.cast::<CanvasNode>(), dex_nodes::layouts::canvas::nodes::CanvasNodeChild)
        .expect("the copy answers for its child");
    assert_ne!(copied_child, label.erase(), "the child was copied too");
    assert_eq!(
        ws.send_request(copied_child.cast::<LabelEditable>(), GetText)
            .as_deref(),
        Some("inner"),
        "the copied child carries the original's content"
    );

    // Editing the copy leaves the original alone.
    ws.submit_action(
        copied_child.cast::<LabelEditable>(),
        "test edit",
        SetText {
            value: "edited".to_owned(),
        },
    );
    ws.process_pending();
    assert_eq!(
        ws.send_request(label, GetText).as_deref(),
        Some("inner"),
        "the original is untouched by an edit to the copy"
    );
}

#[test]
fn a_reference_out_of_the_copied_set_is_left_alone() {
    let mut ws = workspace();
    let outside = ws.insert_node(Label::new("upstream".to_owned()));
    let port = ConnectionPort::build(ws.action_handle());
    ws.process_pending();
    ws.submit_action(
        port,
        "test wiring",
        SetConnection {
            target: Some(outside.erase()),
        },
    );
    ws.process_pending();

    let copy = ws.deep_clone(port.erase());
    ws.process_pending();

    // `connected` is a `#[uid_ref]`: the copy stays wired to the same upstream
    // node rather than dragging a copy of it along.
    assert_eq!(
        ws.send_request(
            copy.cast::<ConnectionPort>(),
            dex_nodes::composites::lambda::ConnectedTarget
        )
        .flatten(),
        Some(outside.erase()),
        "the wire still points at the original upstream node"
    );
    // The drag sensor is owned, so it was copied.
    let sensors = {
        let node = ws.get_node(copy).expect("copy is live");
        let mut found = Vec::new();
        node.owned_refs(&mut |uid| found.push(uid));
        found
    };
    assert_eq!(sensors.len(), 1, "only the sensor is owned");
    assert!(ws.get_node(sensors[0]).is_some());
}

#[test]
fn a_copy_starts_without_the_originals_interaction_state() {
    let mut ws = workspace();
    let sensor = ws.insert_node(InteractionBox::sensing(false, true, false));
    ws.process_pending();

    let copy = ws.deep_clone(sensor.erase());
    ws.process_pending();
    assert!(
        !ws.send_request(
            copy.cast::<InteractionBox>(),
            dex_nodes::primitives::interaction::WasClicked
        )
        .unwrap_or(false),
        "a fresh copy has not been clicked"
    );
}

#[test]
fn a_mirror_follows_its_target_being_replaced() {
    let mut ws = workspace();
    let target = ws.insert_node(Label::new("first".to_owned()));
    let mirror = ws.insert_node(Mirror::new(target.erase()));
    ws.process_pending();

    // The first copy is taken on tick.
    ws.tick_all();
    ws.process_pending();

    let copy = ws
        .get_node(mirror.erase())
        .map(|n| {
            let mut found = Vec::new();
            n.owned_refs(&mut |uid| found.push(uid));
            found
        })
        .expect("mirror is live");
    assert_eq!(copy.len(), 1, "the mirror holds one copy");
    assert_ne!(copy[0], target.erase(), "the copy is its own node");

    // Replace the target's content, as a lambda re-run would.
    ws.action_handle()
        .insert_node_at_dyn(target.erase(), Arc::new(Label::new("second".to_owned())));
    ws.process_pending();

    ws.tick_all();
    ws.process_pending();

    let refreshed = ws
        .get_node(mirror.erase())
        .map(|n| {
            let mut found = Vec::new();
            n.owned_refs(&mut |uid| found.push(uid));
            found
        })
        .expect("mirror is live");
    assert_ne!(refreshed[0], copy[0], "the mirror retook its copy");
    assert!(
        ws.get_node(copy[0]).is_none(),
        "the superseded copy was deleted"
    );

    let shown = ws.get_node(refreshed[0]).expect("copy is live");
    let label = shown
        .as_ref()
        .as_any_ref()
        .downcast_ref::<Label>()
        .expect("the copy is a label");
    assert_eq!(label.text, "second", "the mirror shows the new content");
}

#[test]
fn a_copy_of_a_script_node_gets_its_own_python_state() {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;

    dex_nodes::scripting::init_python();
    let mut ws = workspace();

    let original = pyo3::Python::attach(|py| {
        let globals = PyDict::new(py);
        let src = std::ffi::CString::new(
            "class Counter:\n    def __init__(self):\n        self.hits = []\nmade = Counter()\n",
        )
        .unwrap();
        py.run(src.as_c_str(), Some(&globals), Some(&globals))
            .expect("class defines");
        let obj = globals.get_item("made").unwrap().expect("instance exists");
        dex_nodes::scripting::to_dyn_node_py(&obj)
    });

    let uid = ws.insert_node_dyn(original);
    ws.process_pending();
    let copy = ws.deep_clone(uid);
    ws.process_pending();

    let script_object = |ws: &Workspace, uid: NodeUid| {
        pyo3::Python::attach(|py| {
            ws.get_node(uid)
                .expect("node is live")
                .as_ref()
                .as_any_ref()
                .downcast_ref::<dex_nodes::primitives::dynamic::DynamicNode>()
                .and_then(|n| n.object().map(|o| o.clone_ref(py)))
                .expect("node wraps a script object")
        })
    };

    let original_obj = script_object(&ws, uid);
    let copied_obj = script_object(&ws, copy);

    pyo3::Python::attach(|py| {
        assert!(
            !original_obj.bind(py).is(copied_obj.bind(py)),
            "the copy wraps its own object, not a second reference to one"
        );

        // Mutate the copy's state; the original must not see it.
        copied_obj
            .bind(py)
            .getattr("hits")
            .unwrap()
            .call_method1("append", (1,))
            .unwrap();

        let original_hits: Vec<i64> = original_obj
            .bind(py)
            .getattr("hits")
            .unwrap()
            .extract()
            .unwrap();
        let copied_hits: Vec<i64> = copied_obj.bind(py).getattr("hits").unwrap().extract().unwrap();
        assert_eq!(original_hits, Vec::<i64>::new(), "the original is untouched");
        assert_eq!(copied_hits, vec![1], "the copy carries its own mutation");
    });
}

#[test]
fn a_script_node_declares_what_it_owns_and_its_handles_follow_the_copy() {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;

    dex_nodes::scripting::init_python();
    let mut ws = workspace();

    // A node the script owns, and one it merely points at.
    let owned = ws.insert_node(Label::new("owned".to_owned()));
    let upstream = ws.insert_node(Label::new("upstream".to_owned()));
    ws.process_pending();

    let node = pyo3::Python::attach(|py| {
        let globals = PyDict::new(py);
        globals
            .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
            .unwrap();
        let src = std::ffi::CString::new(
            "class Composite:\n\
             \x20   def __init__(self, mine, theirs):\n\
             \x20       self.mine = mine\n\
             \x20       self.buried = {'list': [theirs]}\n\
             \x20   def owned_nodes(self):\n\
             \x20       return [self.mine]\n",
        )
        .unwrap();
        py.run(src.as_c_str(), Some(&globals), Some(&globals))
            .expect("class defines");
        let cls = globals.get_item("Composite").unwrap().unwrap();
        let obj = cls
            .call1((
                dex_core::scripting::NodeHandle(owned.erase()),
                dex_core::scripting::NodeHandle(upstream.erase()),
            ))
            .expect("instance constructs");
        dex_nodes::scripting::to_dyn_node_py(&obj)
    });

    let uid = ws.insert_node_dyn(node);
    ws.process_pending();
    let copy = ws.deep_clone(uid);
    ws.process_pending();

    let attr = |ws: &Workspace, uid: NodeUid, path: &str| {
        pyo3::Python::attach(|py| {
            let node = ws.get_node(uid).expect("node is live");
            let obj = node
                .as_ref()
                .as_any_ref()
                .downcast_ref::<dex_nodes::primitives::dynamic::DynamicNode>()
                .and_then(|n| n.object().map(|o| o.clone_ref(py)))
                .expect("wraps a script object");
            let bound = obj.bind(py);
            let value = match path {
                "mine" => bound.getattr("mine").unwrap(),
                _ => bound
                    .getattr("buried")
                    .unwrap()
                    .get_item("list")
                    .unwrap()
                    .get_item(0)
                    .unwrap(),
            };
            value.extract::<dex_core::scripting::NodeHandle>().unwrap().0
        })
    };

    // The declared node was copied, and the handle follows the copy...
    let copied_owned = attr(&ws, copy, "mine");
    assert_ne!(copied_owned, owned.erase(), "the owned node was copied");
    let copied_text = ws
        .get_node(copied_owned)
        .expect("the copied node is registered")
        .as_ref()
        .as_any_ref()
        .downcast_ref::<Label>()
        .map(|l| l.text.clone());
    assert_eq!(
        copied_text.as_deref(),
        Some("owned"),
        "the copy of the owned node carries its content"
    );
    assert_eq!(
        attr(&ws, uid, "mine"),
        owned.erase(),
        "the original still points at its own node"
    );

    // ...while an undeclared handle, buried in a dict inside a list, is left
    // pointing where it did.
    assert_eq!(
        attr(&ws, copy, "buried"),
        upstream.erase(),
        "an undeclared reference is not copied"
    );
}

/// Closing a tab removes it, disposes of the canvas it owned, and hands the
/// active slot to a neighbour.
#[test]
fn closing_a_tab_removes_its_canvas_and_reactivates_a_neighbour() {
    use dex_nodes::layouts::desktops::{
        ActiveCanvas, AddCanvas, CloseCanvas, Desktops, Tabs,
    };

    let mut ws = Desktops::new_workspace();
    let root = ws.root().cast::<Desktops>();

    let first_canvas = ws
        .send_request(root, ActiveCanvas)
        .expect("the root answers with its active canvas");

    ws.submit_action(root, "Add canvas", AddCanvas);
    ws.process_pending();

    let second_canvas = ws
        .send_request(root, ActiveCanvas)
        .expect("the new canvas becomes active");
    assert_ne!(first_canvas, second_canvas);

    let tabs = ws.send_request(root, Tabs).unwrap_or_default();
    assert_eq!(tabs.len(), 2, "both canvases have a tab");

    // Close the active (second) tab.
    ws.submit_action(root, "Close canvas", CloseCanvas { tab: tabs[1] });
    ws.process_pending();

    let remaining = ws.send_request(root, Tabs).unwrap_or_default();
    assert_eq!(remaining, vec![tabs[0]], "only the first tab is left");
    assert_eq!(
        ws.send_request(root, ActiveCanvas),
        Some(first_canvas),
        "the neighbouring canvas takes over"
    );
    assert!(
        ws.get_node(tabs[1]).is_none(),
        "the closed tab is gone from the registry"
    );
    assert!(
        ws.get_node(second_canvas.erase()).is_none(),
        "deleting the tab disposed of the canvas it owned"
    );
}

/// There is always somewhere to work: closing the only tab opens a fresh
/// desktop rather than leaving an empty root.
#[test]
fn closing_the_last_tab_opens_an_empty_desktop() {
    use dex_nodes::layouts::desktops::{ActiveCanvas, CloseCanvas, Desktops, Tabs};

    let mut ws = Desktops::new_workspace();
    let root = ws.root().cast::<Desktops>();

    let tabs = ws.send_request(root, Tabs).unwrap_or_default();
    assert_eq!(tabs.len(), 1, "a fresh workspace has exactly one desktop");
    let only_canvas = ws
        .send_request(root, ActiveCanvas)
        .expect("the root answers with its active canvas");

    ws.submit_action(root, "Close canvas", CloseCanvas { tab: tabs[0] });
    ws.process_pending();

    let remaining = ws.send_request(root, Tabs).unwrap_or_default();
    assert_eq!(remaining.len(), 1, "a replacement desktop takes its place");
    assert_ne!(remaining[0], tabs[0], "and it is a new tab, not the closed one");

    let active = ws
        .send_request(root, ActiveCanvas)
        .expect("the root still has an active canvas");
    assert_ne!(active, only_canvas, "the replacement canvas is active");
    assert!(
        ws.get_node(only_canvas.erase()).is_none(),
        "the closed canvas is gone"
    );
    assert!(
        ws.get_node(active.erase()).is_some(),
        "the replacement canvas is live"
    );
}
