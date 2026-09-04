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

/// An edit pulled back reaches the editor, not just the value behind it.
///
/// `GetText` prefers the live edit buffer, and the editor draws from it, so a
/// `SetText` that wrote only the committed value left the old text both on
/// screen and running. The bare-lambda test above never caught it because an
/// editor that has not drawn has no buffer to shadow anything.
#[test]
fn an_edit_reaches_an_editor_that_has_already_drawn() {
    use dex_core::prelude::*;
    use dex_nodes::composites::lambda::{ActiveScript, Lambda};
    use dex_nodes::layouts::canvas::layout::{AddCanvasItem, CanvasChildren};
    use dex_nodes::layouts::canvas::nodes::CanvasNodeChild;
    use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
    use dex_nodes::primitives::text::{CodeEditor, GetText, RequestExternalEdit};

    no_editor();
    dex_nodes::scripting::init_python();

    let mut ws = Desktops::new_workspace();
    let egui_ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 900.0));
    let frame = |ws: &mut Workspace| {
        let _ = egui_ctx.clone().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
            },
        );
    };
    frame(&mut ws);

    let canvas = ws
        .send_request(ws.root(), ActiveCanvas)
        .expect("the desktop has a canvas");
    ws.submit_action(
        canvas,
        "add a lambda",
        AddCanvasItem {
            child: Arc::new(Lambda::new(ws.action_handle())),
            size: Vector { x: 420.0, y: 340.0 },
        },
    );
    ws.process_pending();
    frame(&mut ws);

    let item = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the lambda was placed");
    let lambda = ws
        .send_request(item, CanvasNodeChild)
        .expect("the item wraps the lambda");

    // This lambda's own editor, not the sidebar's prelude.
    fn editor_under(ws: &Workspace, node: NodeUid, depth: usize) -> Option<NodeUid> {
        if depth > 6 {
            return None;
        }
        let held = ws.get_node(node)?;
        if held.as_ref().as_any_ref().is::<CodeEditor>() {
            return Some(node);
        }
        let mut found = None;
        held.owned_refs(&mut |child| {
            if found.is_none() {
                found = editor_under(ws, child, depth + 1);
            }
        });
        found
    }
    let editor = editor_under(&ws, lambda, 0).expect("the lambda has an editor");

    // Exactly what the "Open in IDE" row submits.
    ws.submit_action(editor.cast::<CodeEditor>(), "open", RequestExternalEdit);
    ws.process_pending();
    for _ in 0..3 {
        frame(&mut ws);
    }

    let dir = std::env::temp_dir().join(format!("dex-checkout-{}", lambda.key()));
    assert!(dir.is_dir(), "the button's request checked nothing out");

    std::fs::write(
        dir.join("main.py"),
        format!(
            "import dex  {}\n\ndef transform():\n    return 'from the ide'\n",
            checkout::MARKER
        ),
    )
    .unwrap();
    for _ in 0..3 {
        frame(&mut ws);
    }

    let shown = ws
        .send_request(editor.cast::<CodeEditor>(), GetText)
        .unwrap_or_default();
    assert!(
        shown.contains("from the ide"),
        "the editor still shows the text it had: {shown:?}"
    );
    let script = ws.send_request(lambda, ActiveScript).unwrap_or_default();
    assert!(
        script.contains("from the ide"),
        "the lambda would still run the old script: {script:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A checkout's environment follows the setting, rather than being frozen at
/// the moment it was opened.
#[test]
fn changing_the_environment_rewrites_an_open_checkout() {
    use dex_nodes::settings;

    no_editor();
    dex_nodes::scripting::init_python();

    // A directory shaped like a virtual environment, since that is all the
    // setting checks before it will take one.
    let venv = std::env::temp_dir().join("dex-test-venv/lib/python3.99/site-packages");
    std::fs::create_dir_all(&venv).unwrap();
    let venv = std::env::temp_dir().join("dex-test-venv");

    // With nothing configured, an activated environment is still found: the
    // embedded interpreter imports from it, so a checkout that ignored it would
    // call every package in it missing.
    let restore_active = std::env::var_os("VIRTUAL_ENV");
    unsafe { std::env::set_var("VIRTUAL_ENV", &venv) };
    if settings::venv().is_none() {
        assert_eq!(
            settings::effective_venv().as_deref(),
            Some(venv.as_path()),
            "an activated environment is what a checkout is read against"
        );
    }
    match restore_active {
        Some(v) => unsafe { std::env::set_var("VIRTUAL_ENV", v) },
        None => unsafe { std::env::remove_var("VIRTUAL_ENV") },
    }

    let key = "config-refresh";
    let dir = std::env::temp_dir().join(format!("dex-checkout-{key}"));
    let _ = std::fs::remove_dir_all(&dir);
    let before = settings::venv();
    let checkout = checkout::write(key, "x = 1\n", &[]).expect("checked out");

    settings::set_venv(Some(venv.clone())).expect("the test environment is taken");
    let refreshed = checkout::refresh_config(&checkout).expect("the environment moved");
    let config = std::fs::read_to_string(dir.join("pyrightconfig.json")).unwrap();
    assert!(
        config.contains("dex-test-venv"),
        "the config still describes the old environment: {config}"
    );

    // And it settles: nothing rewrites the file until the setting moves again.
    assert!(
        checkout::refresh_config(&refreshed).is_none(),
        "the config is rewritten every frame"
    );

    settings::set_venv(before).expect("restored");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&venv);
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

    for name in [
        "tiled_layout.py",
        "shape_gallery.py",
        "scatterplot.py",
        "phylo_tree.py",
        "circos.py",
        "circos2.py",
        "circos3.py",
    ] {
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
