//! A wire is drawn between the regions the halo recorded, so a target it never
//! recorded gets no wire. A lambda's output holds `Nothing` until it computes,
//! and `Nothing` draws no region — which left freshly built graphs with no
//! visible connections at all.

use dex_core::prelude::*;
use dex_nodes::composites::lambda::Lambda;
use dex_nodes::layouts::canvas::layout::AddCanvasItem;
use dex_nodes::layouts::desktops::{ActiveCanvas, Desktops};
use dex_nodes::scripting::DataflowOutput;

/// Run a frame, so the probe has something settled to answer from.
fn frame(ws: &mut Workspace) {
    let egui_ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 900.0));
    let input = egui::RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    // Two passes: the first settles layout, the second is what the probe keeps.
    for _ in 0..2 {
        let _ = egui_ctx.run_ui(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ws.draw_frame(ui, screen);
            });
        });
    }
}

#[test]
fn an_output_that_has_not_computed_can_still_be_wired_to() {
    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    frame(&mut ws);

    let canvas = ws
        .send_request(ws.root(), ActiveCanvas)
        .expect("the desktop has a canvas");
    let lambda = Arc::new(Lambda::new(ws.action_handle()));
    ws.submit_action(
        canvas,
        "add a lambda",
        AddCanvasItem {
            child: lambda,
            size: Vector { x: 420.0, y: 340.0 },
        },
    );
    ws.process_pending();
    frame(&mut ws);

    // The lambda has never run, so its output still holds `Nothing`.
    let item = ws
        .send_request(canvas, dex_nodes::layouts::canvas::layout::CanvasChildren)
        .unwrap_or_default()
        .pop()
        .expect("the lambda was placed");
    let inner = ws
        .send_request(item, dex_nodes::layouts::canvas::nodes::CanvasNodeChild)
        .expect("the item wraps the lambda");
    let output = ws
        .send_request(inner, DataflowOutput)
        .flatten()
        .expect("the lambda reports an output");
    assert!(
        ws.get_node(output).is_some_and(|n| n
            .as_ref()
            .as_any_ref()
            .is::<dex_nodes::primitives::nothing::Nothing>()),
        "the output has not computed yet"
    );

    // A wire is only drawn where the halo recorded a region, so this is what
    // decides whether the connection is visible.
    let rect = ws.inspectable_rect(output);
    assert!(
        rect.is_some(),
        "an empty output is still somewhere a wire can reach"
    );
    let size = rect.unwrap().size();
    assert!(
        size.x > 0.0 && size.y > 0.0,
        "it occupies the space the layout gave it, not a point: {} x {}",
        size.x,
        size.y
    );
}

/// Every wire in a generated equation resolves to somewhere on screen.
///
/// A port drawing filled means it is connected; it says nothing about whether
/// the wire is drawn. That needs the target to have been recorded, which needs
/// it to have been *drawn* — and a lambda laid out too short never reaches its
/// output row at all, because `VerticalLayout` stops once it runs out of height.
#[test]
fn every_connection_in_a_generated_equation_can_be_drawn() {
    use dex_nodes::composites::lambda::{ComputeCanvasNode, ConnectedTarget};
    use dex_nodes::layouts::desktops::PushOverride;
    use dex_nodes::scripting::{ScriptOutput, ScriptValue, run_script};

    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    frame(&mut ws);

    let (handle, actions) = WorkspaceActionHandle::buffered();
    let args = [
        (
            "equation".to_owned(),
            ScriptValue::Str("a*x**2 + b*x + c".to_owned()),
        ),
        ("params".to_owned(), ScriptValue::Str("x a b c".to_owned())),
    ];
    let built = match run_script(
        include_str!("../../../examples/gen_eq.py"),
        "",
        &handle,
        &args,
        GraphSnapshot::capture(&ws),
    ) {
        Ok(ScriptOutput::Handle(uid)) => uid,
        Ok(_) => panic!("the generator returns a lambda"),
        Err(e) => panic!("{e}"),
    };
    drop(handle);
    for action in actions.try_iter() {
        ws.submit_action_dyn(action);
    }
    ws.process_pending();

    // Open its canvas fullscreen, as the Open button does, so the body draws.
    let canvas = ws
        .send_request(built, ComputeCanvasNode)
        .expect("the lambda has a canvas");
    ws.submit_action(
        ws.root().cast::<Desktops>(),
        "open",
        PushOverride { node: canvas },
    );
    ws.process_pending();
    frame(&mut ws);

    let mut wires = 0;
    let mut unreachable = Vec::new();
    for uid in ws.live_ids() {
        // Only a connection port answers this.
        let Some(Some(target)) = ws.send_request(uid, ConnectedTarget) else {
            continue;
        };
        wires += 1;
        if ws.inspectable_rect(target).is_none() {
            unreachable.push(target);
        }
    }

    assert!(wires >= 8, "the equation has several connections: {wires}");
    assert!(
        unreachable.is_empty(),
        "{} of {wires} connections have no target region, so no wire is drawn",
        unreachable.len()
    );
}

/// A container's version follows its contents.
///
/// The registry bumps only the uid an action was addressed to, so a consumer
/// holding a canvas lambda by reference never noticed anything inside it
/// change. `Node::version` folds the whole owned subtree.
#[test]
fn a_containers_version_moves_when_something_inside_it_does() {
    use dex_nodes::composites::lambda::{CanvasLambda, LambdaName, LambdaNameNode};
    use dex_nodes::primitives::text::SetText;

    dex_nodes::scripting::init_python();
    let mut ws = Workspace::new_empty();
    let lambda = ws
        .action_handle()
        .insert_node(CanvasLambda::new(ws.action_handle()));
    ws.process_pending();
    ws.set_root(lambda.erase());

    let before = ws.version_of(lambda.erase());

    // Rename something *inside* it: its own registry entry is untouched.
    let name_node = ws
        .send_request(lambda, LambdaNameNode)
        .expect("the lambda has a name node");
    ws.submit_action(
        name_node.cast::<dex_nodes::primitives::text::LabelEditable>(),
        "rename",
        SetText {
            value: "Renamed".to_owned(),
        },
    );
    ws.process_pending();

    assert_eq!(
        ws.send_request(lambda, LambdaName).as_deref(),
        Some("Renamed"),
        "the change landed"
    );
    assert_ne!(
        ws.version_of(lambda.erase()),
        before,
        "and the container's version moved with it"
    );
}
