//! A message class defined in a prelude — `class M(dex.Request)` — travels
//! through `send_request` to a script node and its Python answer comes back
//! whole. This is the one query protocol every visualization can speak: the
//! sender and the answerer never share a Rust type, only the message's own
//! `name` tag, so it survives the pickling and cloning that give a
//! script-defined class a fresh identity.

use dex_core::prelude::*;
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

/// Queue everything a finished script produced, then let the workspace apply it.
fn apply(ws: &mut Workspace, actions: std::sync::mpsc::Receiver<Action>) {
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();
}

/// A node whose `request` handler answers by the message's `name` tag, not by
/// any Rust type — exactly how a plot answers a query about its points.
const RESPONDER: &str = r#"
class Responder:
    def draw(self, ctx):
        return dex.DrawResult.Complete(region=None)

    def request(self, req, ctx):
        tag = getattr(req, "name", None)
        if tag == "double":
            return {"in": req.x, "out": req.x * 2}
        if tag == "keys":
            return [1, 2, 3]
        return NotImplemented

def transform():
    uid = dex.NodeUid.mint()
    dex.ws.insert_node_at_dyn(uid, Responder())
    return uid
"#;

/// The prelude defines the protocol; the transform speaks it.
const PRELUDE: &str = r#"
class Double(dex.Request):
    name = "double"
    def __init__(self, x):
        self.x = x

class Keys(dex.Request):
    name = "keys"

class Unknown(dex.Request):
    name = "unknown"
"#;

const SENDER: &str = r#"
def transform():
    ans = dex.snapshot.send_request(target, Double(21))
    assert ans == {"in": 21, "out": 42}, ans

    assert dex.snapshot.send_request(target, Keys()) == [1, 2, 3]

    # A message the node declines (returns NotImplemented) comes back as None.
    assert dex.snapshot.send_request(target, Unknown()) is None
    return "ok"
"#;

#[test]
fn a_prelude_message_round_trips_through_send_request() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();

    // Seat the responder in the workspace.
    let (handle, actions) = WorkspaceActionHandle::buffered();
    let target = match run_script(RESPONDER, "", &handle, &[], GraphSnapshot::capture(&ws)) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the responder script returns the id it minted"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    apply(&mut ws, actions);

    // Now query it with prelude-defined messages, over the fresh snapshot.
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("target".to_owned(), ScriptValue::Node(target))];
    let out = run_script(SENDER, PRELUDE, &handle, &args, GraphSnapshot::capture(&ws))
        .expect("the sender script runs and its assertions hold");

    let ScriptOutput::Node(node) = out else {
        panic!("the sender returns \"ok\"")
    };
    assert_eq!(
        dex_nodes::scripting::node_to_value(&*node).map(|v| v.display()),
        Some("ok".to_owned()),
    );
}
