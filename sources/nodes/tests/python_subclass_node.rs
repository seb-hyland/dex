//! A script node built from a class *hierarchy* survives everything a node has
//! to survive.
//!
//! The default prelude is a library: one `Plot` spine and a layout per subclass,
//! so a scatter and a phylogeny answer the same messages by inheriting the same
//! code. That only works if a base class comes through a deep clone (which goes
//! via `copy.deepcopy`) and a save/load round trip (which goes via cloudpickle)
//! intact — including methods the subclass never overrides, and `super()` calls
//! that have to find the base at the other end.

use std::sync::Arc;

use dex_core::prelude::*;
use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

/// A two-level hierarchy with state at each level, an override that calls
/// `super()`, and an inherited method the subclass does not mention.
const LIBRARY: &str = r#"
class Plot:
    """The spine: state, an inherited answer, and a hook to extend."""

    def __init__(self, rows):
        self.rows = list(rows)
        self.marks = {}

    def kind(self):
        return "plot"

    def label(self):
        return "%s of %d" % (self.kind(), len(self.rows))

    def request(self, req, ctx):
        tag = getattr(req, "name", None)
        if tag == "lib.label":
            return self.label()
        if tag == "lib.rows":
            return list(self.rows)
        return NotImplemented


class Scatter(Plot):
    def __init__(self, rows, marker):
        super().__init__(rows)
        self.marker = marker

    def kind(self):
        return "scatter[%s]" % self.marker

    def type_name(self):
        return self.label()
"#;

/// The prelude the asking scripts run under.
const ASK: &str = r#"
class Label(dex.Request):
    name = "lib.label"


class Rows(dex.Request):
    name = "lib.rows"
"#;

/// Build a `Scatter` and hand it over as a node.
fn scatter_node() -> Arc<dyn Node> {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;
    pyo3::Python::attach(|py| {
        let globals = PyDict::new(py);
        globals
            .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
            .unwrap();
        let src = std::ffi::CString::new(LIBRARY).unwrap();
        py.run(src.as_c_str(), Some(&globals), Some(&globals))
            .expect("the library runs");
        let obj = py
            .eval(c"Scatter([1, 2, 3], 'dot')", Some(&globals), None)
            .expect("the subclass constructs");
        dex_nodes::scripting::to_dyn_node_py(&obj)
    })
}

/// Ask a node for the label its *base class* computes from the override.
fn label_of(ws: &Workspace, target: NodeUid) -> Option<String> {
    let script = "def transform():\n    \
                      return dex.snapshot.send_request(target, Label()) or \"\"\n";
    let (handle, _actions) = WorkspaceActionHandle::buffered();
    let args = [("target".to_owned(), ScriptValue::Node(target))];
    let out = run_script(script, ASK, &handle, &args, GraphSnapshot::capture(ws)).ok()?;
    let ScriptOutput::Node(node) = out else {
        return None;
    };
    dex_nodes::scripting::node_to_value(&*node)
        .map(|v| v.display())
        .filter(|s| !s.is_empty())
}

/// The inherited method runs, and finds the subclass's override through it.
#[test]
fn an_inherited_method_answers_through_the_override() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let uid = ws.insert_node_dyn(scatter_node());
    ws.process_pending();

    assert_eq!(
        label_of(&ws, uid).as_deref(),
        Some("scatter[dot] of 3"),
        "`Plot.label` called `Scatter.kind`"
    );
    assert_eq!(
        ws.get_node(uid).unwrap().type_name(NodeContext {
            id: uid,
            workspace: &ws,
        }),
        "scatter[dot] of 3",
        "and the node names itself with it"
    );
}

/// A deep clone goes through `copy.deepcopy`; the copy is still a `Scatter`
/// with a working `Plot` behind it, and shares no state with the original.
#[test]
fn a_deep_clone_keeps_the_hierarchy() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let uid = ws.insert_node_dyn(scatter_node());
    ws.process_pending();

    let copy = ws.deep_clone(uid);
    ws.process_pending();
    assert_ne!(copy, uid, "a fresh id");
    assert_eq!(
        label_of(&ws, copy).as_deref(),
        Some("scatter[dot] of 3"),
        "the copy answers the same way"
    );
}

/// Saving and loading goes through cloudpickle. The class hierarchy has to be
/// pickled *by value* — the loading workspace has never seen this prelude.
#[test]
fn a_save_and_load_keeps_the_hierarchy() {
    dex_nodes::scripting::init_python();
    let dir = std::env::temp_dir().join("dex-subclass-node-test");
    std::fs::create_dir_all(&dir).expect("a place to save");
    let path = dir.join("workspace.dex");

    let mut ws = Workspace::new_empty();
    let uid = ws.insert_node_dyn(scatter_node());
    ws.set_root(uid);
    ws.process_pending();
    ws.save_to(&path).expect("the workspace saves");

    let mut other = Workspace::new_empty();
    let (root, registry) = Workspace::read_from(&path).expect("the file reads back");
    other.submit_action_dyn(Action {
        dest: NodeUid::nil(),
        description: "load".into(),
        body: Box::new(LoadWorkspace { root, registry }),
    });
    other.process_pending();

    assert_eq!(
        label_of(&other, root).as_deref(),
        Some("scatter[dot] of 3"),
        "the base class came back with the subclass"
    );
    let _ = std::fs::remove_file(&path);
}
