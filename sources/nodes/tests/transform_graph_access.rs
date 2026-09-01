//! A transform reads the graph through `dex.snapshot` and writes to it through
//! `dex.ws`, the same way Rust does — the foundation the symbolic tooling sits on.

use dex_core::prelude::*;
use dex_nodes::composites::lambda::{ActiveScript, LambdaEditor};
use dex_nodes::primitives::text::{GetText, LabelEditable};
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

/// Queue everything a finished script produced, then let the workspace apply it.
fn apply(ws: &mut Workspace, actions: std::sync::mpsc::Receiver<Action>) {
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
}

/// A workspace holding a lambda editor and two editable labels.
fn fixture() -> (Workspace, NodeUid, NodeUid, NodeUid) {
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();

    let editor = LambdaEditor::build(handle.clone()).erase();
    let first = handle.insert_node(LabelEditable::new("first".to_owned()));
    let second = handle.insert_node(LabelEditable::new("second".to_owned()));
    ws.process_pending();
    ws.set_root(editor);

    (ws, editor, first.erase(), second.erase())
}

const SCRIPT: &str = r#"
def transform():
    snap = dex.snapshot

    # Read: a request answered by following the target's own children.
    inner = snap.send_request(editor, dex.ActiveEditor())
    kind = snap.type_name(editor)

    # Ownership, as a deep clone would follow it.
    assert snap.owner_of(inner) == editor, "the editor owns its code editor"
    assert snap.owner_of(editor) is None, "nothing owns the root"

    # Write: an action, addressed by uid, exactly as Rust submits one.
    dex.ws.submit_action(first, dex.SetText(kind))

    # Write: several actions as one undo step.
    dex.ws.batch(
        [
            (inner, dex.SetText("from the script")),
            (second, dex.SetText("batched " + kind)),
        ],
        "Test batch",
    )

    # A minted id is usable before the node it names exists.
    fresh = dex.NodeUid.mint()
    dex.ws.insert_node_at_dyn(fresh, "minted")
    return fresh
"#;

#[test]
fn a_transform_reads_and_writes_the_live_graph() {
    dex_nodes::scripting::init_python();
    let (mut ws, editor, first, second) = fixture();

    let graph = GraphSnapshot::capture(&ws);
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let args = [
        ("editor".to_owned(), ScriptValue::Node(editor)),
        ("first".to_owned(), ScriptValue::Node(first)),
        ("second".to_owned(), ScriptValue::Node(second)),
    ];

    let output = match run_script(SCRIPT, "", &handle, &args, graph) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the script returns the id it minted"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    apply(&mut ws, actions);

    // The single action landed, carrying a value the script read from the graph.
    assert_eq!(
        ws.send_request(first, GetText).as_deref(),
        Some("A Lambda Editor")
    );
    // Both halves of the batch landed.
    assert_eq!(
        ws.send_request(editor, ActiveScript).as_deref(),
        Some("from the script")
    );
    assert_eq!(
        ws.send_request(second, GetText).as_deref(),
        Some("batched A Lambda Editor")
    );
    // The minted id names the node the script filled it with.
    assert_eq!(ws.send_request(output, GetText).as_deref(), Some("minted"));
}

/// The snapshot describes the graph as it stood when the script was scheduled,
/// not as the script leaves it.
#[test]
fn a_snapshot_does_not_see_the_scripts_own_writes() {
    dex_nodes::scripting::init_python();
    let (ws, _editor, first, _second) = fixture();

    let source = r#"
def transform():
    dex.ws.submit_action(target, dex.SetText("changed"))
    return dex.snapshot.send_request(target, dex.GetText())
"#;

    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("target".to_owned(), ScriptValue::Node(first))];
    let out = run_script(source, "", &handle, &args, GraphSnapshot::capture(&ws)).unwrap();

    let ScriptOutput::Node(node) = out else {
        panic!("the script returns the text it read")
    };
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node)
            .map(|v| v.display())
            .as_deref(),
        Some("first"),
        "the snapshot is fixed at capture"
    );
}

/// A request the target does not answer comes back as `None`, not an error, and
/// a value that is not a message is rejected by name.
#[test]
fn the_snapshot_reports_what_it_cannot_answer() {
    dex_nodes::scripting::init_python();
    let (ws, _editor, first, _second) = fixture();

    let source = r#"
def transform():
    assert dex.snapshot.send_request(target, dex.CanvasChildren()) is None
    assert dex.snapshot.get_node(dex.NodeUid.mint()) is None
    try:
        dex.snapshot.send_request(target, "not a message")
    except TypeError as e:
        assert "known requests" in str(e), str(e)
    else:
        raise AssertionError("a non-message should be refused")
    return None
"#;

    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("target".to_owned(), ScriptValue::Node(first))];
    run_script(source, "", &handle, &args, GraphSnapshot::capture(&ws)).expect("script runs");
}

/// Numbers cross the script boundary as numbers, in both directions, the way
/// text crosses as text.
#[test]
fn numbers_are_seen_as_numbers() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();
    let count = handle.insert_node(dex_nodes::primitives::number::Integer::new(7));
    let ratio = handle.insert_node(dex_nodes::primitives::number::Float::new(0.5));
    ws.process_pending();
    ws.set_root(count.erase());

    let source = r#"
def transform():
    assert isinstance(count, int), type(count)
    assert isinstance(ratio, float), type(ratio)
    return count * 2 + ratio
"#;
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [
        (
            "count".to_owned(),
            dex_nodes::scripting::resolve_arg(&ws, count.erase()).value,
        ),
        (
            "ratio".to_owned(),
            dex_nodes::scripting::resolve_arg(&ws, ratio.erase()).value,
        ),
    ];
    let out = run_script(source, "", &handle, &args, GraphSnapshot::capture(&ws)).unwrap();

    let ScriptOutput::Node(node) = out else {
        panic!("the script returns a number")
    };
    // A returned float comes back as a `Float`, not stringified into a label.
    assert!(
        node.as_ref()
            .as_any_ref()
            .is::<dex_nodes::primitives::number::Float>(),
        "a returned float becomes a Float node"
    );
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node).map(|v| v.display()),
        Some("14.5".to_owned())
    );
}

/// Text that is not a number is dropped: the field snaps back to the value.
#[test]
fn a_number_refuses_text_that_is_not_one() {
    use dex_nodes::primitives::number::Integer;
    use dex_nodes::primitives::text::{GetText, SetText};

    let mut ws = Workspace::new_empty();
    let handle = ws.action_handle();
    let count = handle.insert_node(Integer::new(7));
    ws.process_pending();
    ws.set_root(count.erase());

    ws.submit_action(
        count,
        "Edit",
        SetText {
            value: " 12 ".to_owned(),
        },
    );
    ws.process_pending();
    assert_eq!(
        ws.send_request(count.erase(), GetText).as_deref(),
        Some("12"),
        "a number, surrounding space and all, is accepted"
    );

    ws.submit_action(
        count,
        "Edit",
        SetText {
            value: "twelve".to_owned(),
        },
    );
    ws.process_pending();
    assert_eq!(
        ws.send_request(count.erase(), GetText).as_deref(),
        Some("12"),
        "text that is not a number leaves the value alone"
    );
}
