//! A `CodeEditor` is `rows` tall regardless of how much height it is offered;
//! its own scroll area handles longer content.

use dex_core::prelude::*;
use dex_nodes::primitives::text::CodeEditor;

/// Draw `node` under `constraints` and report the height of the region it took.
fn drawn_height(node: CodeEditor, offered: AxisConstraint) -> f32 {
    let mut ws = Workspace::new_empty();
    let uid = ws.insert_node_now(node).erase();
    ws.set_root(uid);
    ws.process_pending();

    let egui_ctx = egui::Context::default();
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        egui_ctx.style_mut_of(theme, |style| style.animation_time = 0.0);
    }
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(900.0, 2000.0),
        )),
        ..Default::default()
    };

    let mut height = 0.0;
    for _ in 0..2 {
        let _ = egui_ctx.run_ui(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut ui = ui.new_child(egui::UiBuilder::new());
                let mut draw_ctx = DrawContext::root(
                    NodeContext {
                        id: uid,
                        workspace: &ws,
                    },
                    DrawConstraints {
                        pos: ScreenPos { x: 0.0, y: 0.0 },
                        x: Some(AxisConstraint::Exactly(600.0)),
                        y: Some(offered),
                        wrap: WrapConstraints::NotAllowed,
                        should_clip: true,
                    },
                    &mut ui,
                );
                let constraints = draw_ctx.constraints;
                if let Some(r) = draw_ctx.draw_workspace_node(uid, constraints) {
                    height = r.region().map(|reg| reg.size().y).unwrap_or(0.0);
                }
            });
        });
    }
    height
}

#[test]
fn an_editor_takes_its_row_count_not_the_whole_column() {
    dex_nodes::scripting::init_python();

    let mut editor = CodeEditor::new("one\ntwo\nthree".to_owned(), "python".to_owned());
    editor.rows = 6;
    let six_rows = drawn_height(editor.clone(), AxisConstraint::AtMost(1800.0));

    // Nowhere near the 1800 it was offered.
    assert!(
        six_rows < 200.0,
        "a 6-row editor claimed {six_rows}px of the 1800 offered"
    );

    // Content far longer than six rows does not make it grow: the internal
    // scroll area absorbs the overflow.
    let mut long = CodeEditor::new(
        (0..200).map(|i| format!("line {i}\n")).collect(),
        "python".to_owned(),
    );
    long.rows = 6;
    let with_long_content = drawn_height(long, AxisConstraint::AtMost(1800.0));
    // A few px of difference is chrome (a horizontal scrollbar for the longer
    // lines); 200 lines would be ~3400px if content actually drove the height.
    assert!(
        (with_long_content - six_rows).abs() < 10.0,
        "200 lines grew the editor from {six_rows} to {with_long_content}"
    );

    // Twelve rows is about twice six.
    let mut twelve = CodeEditor::new("x".to_owned(), "python".to_owned());
    twelve.rows = 12;
    let twelve_rows = drawn_height(twelve, AxisConstraint::AtMost(1800.0));
    assert!(
        twelve_rows > six_rows,
        "more rows should be taller: {six_rows} vs {twelve_rows}"
    );

    // `fill` is the explicit opt-in to claiming everything offered.
    let mut filling = CodeEditor::new("x".to_owned(), "python".to_owned());
    filling.rows = 6;
    filling.fill = true;
    let filled = drawn_height(filling, AxisConstraint::Exactly(1800.0));
    assert!(
        filled > 1000.0,
        "fill should claim the column, took {filled}"
    );
}

/// The reported symptom, in the shape a lambda builds: an editor stacked in a
/// column grew with its content until it swallowed the node. A fresh editor is
/// empty, so the buffer has to be long for this to bite.
#[test]
fn an_editor_in_a_column_does_not_swallow_it() {
    use dex_nodes::layouts::{LayoutChild, VerticalLayout};
    use dex_nodes::primitives::text::Label;

    dex_nodes::scripting::init_python();

    let mut long = CodeEditor::new(
        (0..200).map(|i| format!("line {i}\n")).collect(),
        "python".to_owned(),
    );
    long.rows = 6;

    // The stack a `Lambda` draws: editor, then more sections under it.
    let column = VerticalLayout {
        children: vec![
            LayoutChild::Node(std::sync::Arc::new(long)),
            LayoutChild::Node(std::sync::Arc::new(Label::new("under it".to_owned()))),
        ],
        spacing: 6.0,
        sizing: Vec::new(),
    };

    let ws = Workspace::new_empty();
    let egui_ctx = egui::Context::default();
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        egui_ctx.style_mut_of(theme, |style| style.animation_time = 0.0);
    }
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(900.0, 2000.0),
        )),
        ..Default::default()
    };

    let mut height = 0.0;
    for _ in 0..2 {
        let _ = egui_ctx.run_ui(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut ui = ui.new_child(egui::UiBuilder::new());
                let mut draw_ctx = DrawContext::root(
                    NodeContext {
                        id: NodeUid::nil(),
                        workspace: &ws,
                    },
                    DrawConstraints {
                        pos: ScreenPos { x: 0.0, y: 0.0 },
                        x: Some(AxisConstraint::Exactly(600.0)),
                        y: Some(AxisConstraint::AtMost(1800.0)),
                        wrap: WrapConstraints::NotAllowed,
                        should_clip: true,
                    },
                    &mut ui,
                );
                let constraints = draw_ctx.constraints;
                height = draw_ctx
                    .draw_node(&column, constraints)
                    .region()
                    .map(|r| r.size().y)
                    .unwrap_or(0.0);
            });
        });
    }

    assert!(
        height > 0.0 && height < 250.0,
        "a 6-row editor plus a label took {height}px of the 1800 offered"
    );
}
