//! Introspects the generated `dex` module, so the bound surface is checked
//! rather than assumed.

fn bound_names(py: pyo3::Python<'_>, class: &str) -> Vec<String> {
    use pyo3::prelude::*;
    let module = dex_dynamic::build_python_module(py).expect("module builds");
    let cls = module.getattr(class).expect("class is registered");
    let mut names: Vec<String> = cls
        .dir()
        .unwrap()
        .extract::<Vec<String>>()
        .unwrap()
        .into_iter()
        .filter(|n| !n.starts_with('_'))
        .collect();
    names.sort();
    names
}

#[test]
fn contexts_expose_their_rust_apis() {
    dex_nodes::scripting::init_python();
    pyo3::Python::attach(|py| {
        let draw = bound_names(py, "DrawContext");
        println!("DrawContext -> {draw:?}");
        // Mirrors Rust's `DrawContext`: `node`, `constraints`, and the draw calls.
        for expected in [
            "constraints",
            "draw_node",
            "draw_workspace_node",
            "get_workspace_node",
            "node",
            "request_skip_frame",
        ] {
            assert!(draw.contains(&expected.to_string()), "missing {expected}");
        }

        // `NodeContext` reached through `.node`, as in Rust — not flattened onto
        // the draw context.
        for flattened in ["id", "workspace"] {
            assert!(
                !draw.contains(&flattened.to_string()),
                "{flattened} should live on `ctx.node`, as it does in Rust"
            );
        }
        let node_ctx = bound_names(py, "NodeContext");
        println!("NodeContext -> {node_ctx:?}");
        for expected in ["id", "workspace"] {
            assert!(
                node_ctx.contains(&expected.to_string()),
                "missing {expected}"
            );
        }
        // egui-facing methods must not leak into scripts.
        for banned in ["host_widgets", "for_ui"] {
            assert!(!draw.contains(&banned.to_string()), "leaked {banned}");
        }

        // Nor should Python-only conveniences: positioning goes in the
        // constraints, as it does in Rust, and the constraints are readable.
        for crutch in ["draw_node_at", "avail_width", "avail_height"] {
            assert!(
                !draw.contains(&crutch.to_string()),
                "{crutch} is a Python-only shim for something Rust expresses with DrawConstraints"
            );
        }

        // The pieces that make that possible must be bound.
        let constraints = bound_names(py, "DrawConstraints");
        println!("DrawConstraints -> {constraints:?}");
        for expected in [
            "pos",
            "x",
            "y",
            "wrap",
            "should_clip",
            "fits",
            "shrunk_by_per_side",
        ] {
            assert!(
                constraints.contains(&expected.to_string()),
                "missing {expected}"
            );
        }
        let axis = bound_names(py, "AxisConstraint");
        assert!(axis.contains(&"provided_value".to_string()));

        let ws = bound_names(py, "Workspace");
        println!("Workspace -> {ws:?}");
        for expected in ["root", "get_node", "version_of", "delete_node"] {
            assert!(ws.contains(&expected.to_string()), "missing {expected}");
        }
        // Host lifecycle must stay out of reach.
        for banned in [
            "draw_frame",
            "set_root",
            "process_pending",
            "insert_node_now",
        ] {
            assert!(!ws.contains(&banned.to_string()), "leaked {banned}");
        }
    });
}

#[test]
fn every_message_is_reachable_by_name() {
    dex_nodes::scripting::init_python();
    let (mut requests, mut actions) = dex_core::messages::registered_messages();
    requests.sort_unstable();
    actions.sort_unstable();
    println!("requests ({}) -> {requests:?}", requests.len());
    println!("actions  ({}) -> {actions:?}", actions.len());

    // A sample spanning several defining modules.
    for expected in ["Selected", "GetText", "ArgBindings", "ConnectedTarget"] {
        assert!(
            requests.contains(&expected),
            "request {expected} unregistered"
        );
    }
    assert!(!actions.is_empty(), "no actions registered");

    // Dispatch is by class identity, but two same-named messages would still
    // collide as `dex` attributes — `build_python_module` rejects that.
    pyo3::Python::attach(|py| {
        dex_dynamic::build_python_module(py).expect("no two bindings share a name");
    });

    // Each message is reachable as a class you can construct.
    pyo3::Python::attach(|py| {
        use pyo3::prelude::*;
        let module = dex_dynamic::build_python_module(py).unwrap();
        for name in requests.iter().chain(actions.iter()) {
            assert!(
                module.getattr(*name).is_ok(),
                "message {name} is not exposed as a class"
            );
        }
    });
}

/// The whole path end to end: a script names a request, the registry builds it,
/// the workspace answers, and the response comes back as a Python value.
#[test]
fn a_script_can_query_the_workspace_by_name() {
    use dex_core::prelude::*;
    use dex_nodes::primitives::text::LabelEditable;
    use pyo3::prelude::*;
    use pyo3::types::PyDict;

    dex_nodes::scripting::init_python();

    let mut ws = Workspace::new_empty();
    let label = ws.insert_node_now(LabelEditable::new("hello from rust".to_owned()));
    ws.set_root(label.erase());
    ws.process_pending();

    Python::attach(|py| {
        PyWorkspace::enter(py, &ws, |pyws| {
            let globals = PyDict::new(py);
            globals.set_item("ws", pyws).unwrap();
            globals
                .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
                .unwrap();
            globals
                .set_item("node", Py::new(py, NodeHandle(label.erase())).unwrap())
                .unwrap();

            let text: String = py
                .eval(
                    c"ws.send_request(node, dex.GetText())",
                    Some(&globals),
                    None,
                )
                .expect("request dispatches")
                .extract()
                .expect("response is a string");
            assert_eq!(text, "hello from rust");

            // Constructed by keyword, like any Python class.
            py.run(
                c"ws.submit_action(node, dex.SetText(value='changed'))",
                Some(&globals),
                None,
            )
            .expect("action dispatches");

            // Fields read back through `IntoDynamic`.
            let field: String = py
                .eval(c"dex.SetText(value='x').value", Some(&globals), None)
                .expect("getter works")
                .extract()
                .unwrap();
            assert_eq!(field, "x");

            // Passing a non-message is a clear type error.
            let err = py
                .eval(c"ws.send_request(node, 'GetText')", Some(&globals), None)
                .expect_err("a bare string is not a request");
            assert!(err.to_string().contains("expected a request message"));

            // A request passed where an action belongs is caught too.
            let kind_err = py
                .run(
                    c"ws.submit_action(node, dex.GetText())",
                    Some(&globals),
                    None,
                )
                .expect_err("a request is not an action");
            assert!(kind_err.to_string().contains("expected a action message"));
        })
        .unwrap();
    });

    // The queued action was really enqueued: draining applies it.
    ws.process_pending();
    let text = ws.send_request(label, dex_nodes::primitives::text::GetText);
    assert_eq!(text.as_deref(), Some("changed"));
}

/// The invariant the raw pointer behind `Scoped` rests on: a handle a script
/// stashes away is dead once its call returns, rather than dangling.
#[test]
fn a_stashed_handle_expires_with_its_scope() {
    use dex_core::prelude::*;
    use dex_nodes::primitives::text::LabelEditable;
    use pyo3::prelude::*;
    use pyo3::types::PyDict;

    dex_nodes::scripting::init_python();

    let mut ws = Workspace::new_empty();
    let label = ws.insert_node_now(LabelEditable::new("scoped".to_owned()));
    ws.set_root(label.erase());
    ws.process_pending();

    Python::attach(|py| {
        let globals = PyDict::new(py);
        globals
            .set_item("node", Py::new(py, NodeHandle(label.erase())).unwrap())
            .unwrap();
        globals
            .set_item("dex", dex_dynamic::build_python_module(py).unwrap())
            .unwrap();

        PyWorkspace::enter(py, &ws, |pyws| {
            globals.set_item("stashed", pyws).unwrap();
            // Live inside the scope.
            let live: bool = py
                .eval(c"stashed.live", Some(&globals), None)
                .unwrap()
                .extract()
                .unwrap();
            assert!(live, "handle should be live inside its scope");
        })
        .unwrap();

        // Dead outside it.
        let live: bool = py
            .eval(c"stashed.live", Some(&globals), None)
            .unwrap()
            .extract()
            .unwrap();
        assert!(!live, "handle should be dead outside its scope");

        // And using it raises rather than reading freed memory.
        let err = py
            .eval(
                c"stashed.send_request(node, dex.GetText())",
                Some(&globals),
                None,
            )
            .expect_err("an expired handle must refuse to run");
        assert!(
            err.to_string().contains("no longer valid"),
            "unexpected error: {err}"
        );
    });
}
