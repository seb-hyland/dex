//! Checking a buffer out to a file, and pulling edits back in.
//!
//! The helper is generic, so these exercise it directly; the `Lambda` tests
//! cover the one node that currently drives it.

use dex_nodes::primitives::checkout;

fn no_editor() {
    // Nothing should be launched by a test.
    unsafe {
        std::env::set_var("DEX_EDITOR", "true");
    }
}

fn which_checker() -> Result<String, ()> {
    for name in ["basedpyright", "pyright"] {
        if std::process::Command::new(name)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            return Ok(name.to_owned());
        }
    }
    Err(())
}

#[test]
fn a_checkout_holds_everything_a_language_server_needs() {
    no_editor();
    let out = checkout::write(
        "layout-test",
        "def transform():\n    pass\n",
        &[("count".to_owned(), "int".to_owned())],
    )
    .expect("written");

    for name in ["dex.pyi", "pyrightconfig.json", "main.py"] {
        assert!(out.dir.join(name).is_file(), "checkout is missing {name}");
    }
    // The stubs are rendered live, not copied from a checked-in file.
    let stubs = std::fs::read_to_string(out.dir.join("dex.pyi")).unwrap();
    assert!(stubs.contains("class Label"), "stubs look empty");

    let on_disk = std::fs::read_to_string(out.main_file()).unwrap();
    assert!(on_disk.starts_with("import dex"));
    assert!(on_disk.contains("count: int"));
    // Types must be qualified: nothing but the header's own imports is in scope.
    assert!(
        !on_disk.contains(": NodeUid"),
        "an unqualified type in the header will not resolve:\n{on_disk}"
    );

    let _ = std::fs::remove_dir_all(&out.dir);
}

#[test]
fn an_edit_is_pulled_back_without_the_header() {
    no_editor();
    let out = checkout::write("pull-test", "def transform():\n    pass\n", &[]).expect("written");

    // Nothing changed yet.
    assert!(
        checkout::poll(&out).is_none(),
        "polled a change with no edit"
    );

    std::fs::write(
        out.main_file(),
        format!(
            "import dex  {}\n\ndef transform():\n    return 'edited'\n",
            checkout::MARKER
        ),
    )
    .unwrap();

    let pulled = checkout::poll(&out).expect("edit detected");
    assert!(pulled.source.contains("return 'edited'"));
    assert!(
        !pulled.source.contains(checkout::MARKER),
        "the header leaked into the node: {:?}",
        pulled.source
    );
    // Polling again against the advanced checkout is quiet.
    assert!(checkout::poll(&pulled.checkout).is_none());

    let _ = std::fs::remove_dir_all(&out.dir);
}

/// A fresh checkout must typecheck, and `dex.` must actually complete — a
/// misrooted language server reports no errors *and* offers nothing, which
/// looks identical to the bindings being missing. Skipped without a checker.
#[test]
fn a_checkout_typechecks_and_completes() {
    no_editor();
    let Ok(checker) = which_checker() else {
        eprintln!("no language server on PATH; skipping");
        return;
    };

    let globals = [("count".to_owned(), "int".to_owned())];
    let out = checkout::write(
        "lsp-test",
        "def transform():\n    return dex.Label.new(f'{count}')\n",
        &globals,
    )
    .expect("written");

    let run = std::process::Command::new(&checker)
        .arg("--outputjson")
        .arg("main.py")
        .current_dir(&out.dir)
        .output()
        .expect("language server runs");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let errors = stdout
        .split("\"severity\": \"error\"")
        .count()
        .saturating_sub(1);
    assert_eq!(
        errors,
        0,
        "a fresh checkout does not typecheck:\n{}",
        &stdout[..stdout.len().min(1500)]
    );

    // Completions, driven through a real LSP session.
    let driver = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/lsp_complete.py");
    if driver.is_file() {
        std::fs::write(
            out.main_file(),
            format!(
                "import dex  {}\n\ndef transform():\n    dex.\n",
                checkout::MARKER
            ),
        )
        .unwrap();
        let run = std::process::Command::new("python3")
            .arg(&driver)
            .arg(&out.dir)
            .output()
            .expect("driver runs");
        let stdout = String::from_utf8_lossy(&run.stdout);
        let count: usize = stdout
            .split("non-dunder=")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        assert!(
            count > 50,
            "`dex.` offered {count} completions; the stubs are not being seen:\n{stdout}"
        );
        assert!(
            stdout.contains("AddChild"),
            "message classes missing:\n{stdout}"
        );
    }

    let _ = std::fs::remove_dir_all(&out.dir);
}

/// The one node that currently drives a checkout. The helper is generic, so a
/// prelude editor would wire up the same way.
#[test]
fn a_lambda_checks_its_script_out_and_pulls_edits_back() {
    use dex_core::prelude::*;
    use dex_nodes::composites::lambda::Lambda;
    use dex_nodes::primitives::text::GetText;

    no_editor();
    dex_nodes::scripting::init_python();

    let mut ws = Workspace::new_empty();
    let lambda = ws.insert_node_now(Lambda::new(ws.action_handle()));
    ws.process_pending();
    let lambda = lambda.erase();

    // The checkout is keyed off the node id, so it is stable and collision-free.
    let dir = std::env::temp_dir().join(format!("dex-checkout-{}", lambda.key()));
    assert!(!dir.exists(), "checked out before being asked");

    // Drive the same path the button does.
    let node = ws.get_node(lambda).unwrap();
    let lam = node
        .as_ref()
        .as_any_ref()
        .downcast_ref::<Lambda>()
        .expect("is a Lambda");
    lam.edit_externally(NodeContext {
        id: lambda,
        workspace: &ws,
    });
    assert!(dir.is_file() || dir.is_dir(), "no checkout at {dir:?}");

    // Edit it the way an external editor would.
    let main = dir.join("main.py");
    std::fs::write(
        &main,
        format!(
            "import dex  {}\n\ndef transform():\n    return 'from the ide'\n",
            checkout::MARKER
        ),
    )
    .unwrap();

    for _ in 0..3 {
        if let Some(n) = ws.get_node(lambda) {
            n.tick(NodeContext {
                id: lambda,
                workspace: &ws,
            });
        }
        ws.process_pending();
    }

    let script = ws
        .send_request(lambda, dex_nodes::composites::lambda::ActiveScript)
        .unwrap_or_default();
    assert!(
        script.contains("from the ide"),
        "the lambda did not pull the edit: {script:?}"
    );
    assert!(
        !script.contains(checkout::MARKER),
        "header leaked: {script:?}"
    );
    let _ = ws.send_request(lambda, GetText);

    let _ = std::fs::remove_dir_all(&dir);
}

/// Every kind of injected global must resolve.
///
/// A header sits in the script's own namespace, where the only thing in scope
/// is what the header imports — a bare `NodeUid` or `Any` is an undefined name.
/// Driven off `ScriptValue::python_type` so it cannot drift from what a lambda
/// actually injects.
#[test]
fn every_injected_global_type_resolves() {
    use dex_nodes::scripting::ScriptValue;

    no_editor();
    let Ok(checker) = which_checker() else {
        eprintln!("no language server on PATH; skipping");
        return;
    };

    // One global per ScriptValue variant, named after its type.
    let kinds = [
        ScriptValue::Str(String::new()),
        ScriptValue::Int(0),
        ScriptValue::Float(0.0),
        ScriptValue::Bool(false),
        ScriptValue::Node(dex_core::NodeUid::nil()),
        ScriptValue::Nothing,
    ];
    let mut globals: Vec<(String, String)> = kinds
        .iter()
        .enumerate()
        .map(|(i, k)| (format!("g{i}"), k.python_type().to_owned()))
        .collect();
    // `Table` needs arrow to construct; its declared type is what matters.
    globals.push(("table".to_owned(), "typing.Any".to_owned()));

    let out =
        checkout::write("global-types", "def transform():\n    pass\n", &globals).expect("written");

    let run = std::process::Command::new(&checker)
        .arg("--outputjson")
        .arg("main.py")
        .current_dir(&out.dir)
        .output()
        .expect("language server runs");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let errors = stdout
        .split("\"severity\": \"error\"")
        .count()
        .saturating_sub(1);

    let header = std::fs::read_to_string(out.main_file()).unwrap();
    assert_eq!(
        errors,
        0,
        "an injected global does not resolve.\nheader:\n{}\n\n{}",
        header.lines().take(10).collect::<Vec<_>>().join("\n"),
        &stdout[..stdout.len().min(1200)]
    );

    let _ = std::fs::remove_dir_all(&out.dir);
}

/// The worked examples must typecheck against the real bindings, so the stubs
/// and the examples cannot drift apart.
#[test]
fn the_example_typechecks_against_the_stubs() {
    no_editor();
    let Ok(checker) = which_checker() else {
        eprintln!("no language server on PATH; skipping");
        return;
    };

    for name in ["tiled_layout.py", "shape_gallery.py", "scatterplot.py"] {
        let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name);
        let source = std::fs::read_to_string(&example).expect("example exists");

        let out = checkout::write("example-check", &source, &[]).expect("written");
        let run = std::process::Command::new(&checker)
            .arg("--outputjson")
            .arg("main.py")
            .current_dir(&out.dir)
            .output()
            .expect("language server runs");
        let stdout = String::from_utf8_lossy(&run.stdout);
        let errors = stdout
            .split("\"severity\": \"error\"")
            .count()
            .saturating_sub(1);
        assert_eq!(
            errors,
            0,
            "examples/{name} does not typecheck against the stubs:\n{}",
            &stdout[..stdout.len().min(1500)]
        );

        let _ = std::fs::remove_dir_all(&out.dir);
    }
}
