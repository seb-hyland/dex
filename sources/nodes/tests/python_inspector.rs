//! A script-defined node can offer its own inspector, built from the same
//! pieces a Rust node uses.

use dex_core::prelude::*;
use dex_nodes::scripting::to_dyn_node_py;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// A node whose inspector is a column of its own commands, plus the shared
/// Copy/Mirror pair — composed entirely in Python.
const SOURCE: &str = r#"
class Widget:
    def __init__(self):
        self.greeting = "hello"

    def type_name(self):
        return "Widget"

    def build_inspector(self, ctx):
        ws = ctx.workspace
        handle = ws.action_handle()
        shout = dex.Button.build(handle, dex.Label.new("Shout"))
        # The shared placement pair, opted into the same way a Rust node does.
        placement = dex.PlacementCommands.build(handle, ctx.id, dex.Vector.new(120.0, 80.0))
        self.shout_button = shout
        return dex.VerticalLayout.build(handle, [placement, shout], 2.0)
"#;

#[test]
fn a_script_node_can_offer_an_inspector() {
    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();

    let node = Python::attach(|py| {
        let globals = PyDict::new(py);
        globals
            .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
            .unwrap();
        let src = std::ffi::CString::new(SOURCE).unwrap();
        py.run(src.as_c_str(), Some(&globals), Some(&globals))
            .expect("the module runs");
        let obj = py
            .eval(c"Widget()", Some(&globals), None)
            .expect("the widget constructs");
        to_dyn_node_py(&obj)
    });

    let uid = ws.insert_node_dyn(node);
    ws.process_pending();

    // The hook is reached, and what it returns is a live workspace node.
    let inspector = ws
        .get_node(uid)
        .expect("the node is live")
        .build_inspector(NodeContext {
            id: uid,
            workspace: &ws,
        })
        .expect("the script offered an inspector");
    ws.process_pending();

    assert!(
        ws.get_node(inspector).is_some(),
        "the inspector it built is registered"
    );

    // It is a real column, and it owns the rows the script put in it — so the
    // inspector can dispose of the whole thing when its menu closes.
    let mut owned = Vec::new();
    ws.get_node(inspector)
        .unwrap()
        .owned_refs(&mut |uid| owned.push(uid));
    assert_eq!(
        owned.len(),
        2,
        "the column owns the placement pair and the script's own button"
    );
    for row in &owned {
        assert!(ws.get_node(*row).is_some(), "each row is live");
    }
}

/// A script whose `build_inspector` raises still gets a menu — one saying so.
#[test]
fn a_failing_script_inspector_shows_the_error() {
    use dex_nodes::layouts::error::ErrorLayout;

    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();

    let node = Python::attach(|py| {
        let globals = PyDict::new(py);
        globals
            .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
            .unwrap();
        let src = std::ffi::CString::new(
            "class Broken:\n\
             \x20   def build_inspector(self, ctx):\n\
             \x20       raise ValueError('no menu for you')\n",
        )
        .unwrap();
        py.run(src.as_c_str(), Some(&globals), Some(&globals))
            .expect("the module runs");
        to_dyn_node_py(&py.eval(c"Broken()", Some(&globals), None).unwrap())
    });

    let uid = ws.insert_node_dyn(node);
    ws.process_pending();

    let inspector = ws
        .get_node(uid)
        .expect("the node is live")
        .build_inspector(NodeContext {
            id: uid,
            workspace: &ws,
        })
        .expect("a failure still yields something to show");
    ws.process_pending();

    let shown = ws
        .get_node(inspector)
        .expect("the error node is registered");
    assert!(
        shown.as_ref().as_any_ref().is::<ErrorLayout>(),
        "the menu shows an error rather than nothing"
    );
}
