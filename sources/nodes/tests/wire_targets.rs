//! A wire is drawn between the regions the inspector recorded, so a target it never
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

    // A wire is only drawn where the inspector recorded a region, so this is what
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

/// One wire per connection, and no more.
///
/// A sizing draw is invisible because the `Ui` it runs on is — but a port
/// paints its wire onto an explicit layer, taken from the context rather than
/// from that `Ui`, so it escaped and landed on screen a second time, from
/// wherever the measuring pass had put the port. Every wire drew twice, once
/// through the middle of nothing.
#[test]
fn a_connection_draws_exactly_one_wire() {
    use dex_nodes::composites::lambda::{AddArg, ConnectedTarget, LambdaArgs};

    dex_nodes::scripting::init_python();
    let mut ws = Desktops::new_workspace();
    let egui_ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 900.0));
    // A clock that moves, because a wire is drawn onto a child `Ui` and so
    // fades in with its parent: frozen at zero, every frame catches it partly
    // transparent and it never shows its own colour.
    let mut clock = 0.0;
    let mut run = |ws: &mut Workspace| {
        clock += 1.0 / 60.0;
        let input = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(clock),
            ..Default::default()
        };
        egui_ctx.clone().run_ui(input, |c| {
            egui::CentralPanel::default().show(c, |ui| ws.draw_frame(ui, screen));
        })
    };
    run(&mut ws);

    let canvas = ws
        .send_request(ws.root(), ActiveCanvas)
        .expect("the desktop has a canvas");
    for (label, size) in [
        ("Hello, ", Vector { x: 160.0, y: 60.0 }),
        ("world", Vector { x: 160.0, y: 60.0 }),
    ] {
        ws.submit_action(
            canvas,
            "add a text",
            AddCanvasItem {
                child: Arc::new(dex_nodes::primitives::text::Label::new(label.to_owned())),
                size,
            },
        );
    }
    ws.submit_action(
        canvas,
        "add a lambda",
        AddCanvasItem {
            child: Arc::new(Lambda::new(ws.action_handle())),
            size: Vector { x: 420.0, y: 340.0 },
        },
    );
    ws.process_pending();
    run(&mut ws);

    // A fresh lambda takes no arguments; give it two, as the `+` row does.
    let args_row = ws
        .live_ids()
        .into_iter()
        .find(|uid| {
            ws.get_node(*uid)
                .is_some_and(|n| n.as_ref().as_any_ref().is::<LambdaArgs>())
        })
        .expect("the lambda has an argument list");
    for _ in 0..2 {
        ws.submit_action(args_row, "add argument", AddArg);
        ws.process_pending();
    }
    run(&mut ws);

    // Wire every port the lambda has to the first text item.
    let items = ws
        .send_request(canvas, dex_nodes::layouts::canvas::layout::CanvasChildren)
        .unwrap_or_default();
    let text = items[0];
    assert!(
        ws.inspectable_rect(text).is_some(),
        "the text item is somewhere a wire can reach"
    );
    let ports: Vec<NodeUid> = ws
        .live_ids()
        .into_iter()
        .filter(|uid| ws.send_request(*uid, ConnectedTarget).is_some())
        .collect();
    assert!(!ports.is_empty(), "a fresh lambda has ports to connect");
    for port in &ports {
        ws.submit_action(
            *port,
            "connect",
            dex_nodes::composites::lambda::SetConnection { target: Some(text) },
        );
    }
    ws.process_pending();
    for _ in 0..12 {
        run(&mut ws);
    }
    let output = run(&mut ws);

    // The wire's own colour, so nothing else in the scene is counted.
    let wire = egui::Color32::from(Color::rgba(176, 202, 224, 150));
    for c in output.shapes.iter() {
        match &c.shape {
            egui::Shape::Path(p) if p.points.len() == 2 => {
                println!("PATH2 {:?} clip={:?}", p.stroke, c.clip_rect)
            }
            egui::Shape::LineSegment { stroke, .. } => println!("SEG {stroke:?}"),
            _ => {}
        }
    }
    let drawn = output
        .shapes
        .iter()
        .filter(|c| match &c.shape {
            egui::Shape::Path(p) => {
                p.points.len() == 2 && p.stroke.color == egui::epaint::ColorMode::Solid(wire)
            }
            egui::Shape::LineSegment { stroke, .. } => stroke.color == wire,
            _ => false,
        })
        .count();
    let connected = ports
        .iter()
        .filter(|uid| matches!(ws.send_request(**uid, ConnectedTarget), Some(Some(_))))
        .count();
    assert_eq!(
        drawn, connected,
        "{connected} connection(s) drew {drawn} wire(s)"
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
